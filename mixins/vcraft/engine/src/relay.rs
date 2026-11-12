//! The connection to the VCraft relay (`RelayConnection`): a WebSocket on a
//! thread of its own, the challenge / Mojang session login, JSON messages to
//! and from the game thread and voice frames straight to the voice engine.

use base64::Engine;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::protocol::{CloseFrame, Role, WebSocket, frame::coding::CloseCode};
use tungstenite::{Error as WsError, Message};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disconnected,
    Connecting,
    Authenticating,
    Connected,
}

/// What the relay thread hands the game thread.
pub enum Event {
    Message(Value),
    /// The connection ended (or was dropped): the state resets.
    Reset,
}

/// What logging in needs from the game.
#[derive(Clone)]
pub struct Login {
    pub username: String,
    pub account_id: String,
    pub config_dir: PathBuf,
    pub status: String,
    pub url: String,
    /// The Mojang session join for the relay's server id (`joinServer`).
    pub authenticate: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
}

enum Out {
    Text(String),
    Voice(Vec<u8>),
}

struct Shared {
    status: Status,
    last_error: Option<String>,
    auth_error: Option<String>,
    wanted: bool,
    generation: u64,
    outgoing: VecDeque<Out>,
    queued_voice: usize,
    events: VecDeque<Event>,
    login: Option<Login>,
    retry_ms: u64,
}

pub type VoiceSink = Arc<dyn Fn(&[u8]) + Send + Sync>;

pub struct Relay {
    shared: Arc<Mutex<Shared>>,
    voice: Mutex<Option<VoiceSink>>,
}

const MAX_QUEUED_VOICE: usize = 8;

impl Relay {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                status: Status::Disconnected,
                last_error: None,
                auth_error: None,
                wanted: false,
                generation: 0,
                outgoing: VecDeque::new(),
                queued_voice: 0,
                events: VecDeque::new(),
                login: None,
                retry_ms: 2000,
            })),
            voice: Mutex::new(None),
        }
    }

    pub fn set_voice_sink(&self, sink: VoiceSink) {
        *self.voice.lock().unwrap() = Some(sink);
    }

    pub fn status(&self) -> Status {
        self.shared.lock().unwrap().status
    }

    pub fn connected(&self) -> bool {
        self.status() == Status::Connected
    }

    /// The player wants to be connected (it wasn't turned off).
    pub fn wanted(&self) -> bool {
        self.shared.lock().unwrap().wanted
    }

    pub fn last_error(&self) -> Option<String> {
        self.shared.lock().unwrap().last_error.clone()
    }

    pub fn drain(&self) -> Vec<Event> {
        self.shared.lock().unwrap().events.drain(..).collect()
    }

    pub fn connect(&self, login: Login) {
        {
            let mut s = self.shared.lock().unwrap();
            s.wanted = true;
            s.login = Some(login);
            if s.status != Status::Disconnected {
                return;
            }
            s.status = Status::Connecting;
            s.generation += 1;
        }
        let run_id = self.shared.lock().unwrap().generation;
        let shared = self.shared.clone();
        let sink = self.voice.lock().unwrap().clone();
        let _ = std::thread::Builder::new().name("VCraft Relay".into()).spawn(move || run(shared, run_id, sink));
    }

    pub fn disconnect(&self) {
        let mut s = self.shared.lock().unwrap();
        s.wanted = false;
        s.generation += 1;
        s.status = Status::Disconnected;
        s.outgoing.clear();
        s.events.push_back(Event::Reset);
    }

    pub fn reconnect(&self, login: Login) {
        self.disconnect();
        self.shared.lock().unwrap().retry_ms = 2000;
        self.connect(login);
    }

    pub fn send(&self, message: &Value) {
        let mut s = self.shared.lock().unwrap();
        let login = message["t"].as_str() == Some("login");
        if s.status == Status::Connected || (login && s.status == Status::Authenticating) {
            s.outgoing.push_back(Out::Text(message.to_string()));
        }
    }

    pub fn send_voice(&self, frame: Vec<u8>) {
        let mut s = self.shared.lock().unwrap();
        if s.status == Status::Connected && s.queued_voice < MAX_QUEUED_VOICE {
            s.queued_voice += 1;
            s.outgoing.push_back(Out::Voice(frame));
        }
    }
}

trait Stream: Read + Write + Send {
    fn set_timeout(&self, d: Option<Duration>);
}
impl Stream for TcpStream {
    fn set_timeout(&self, d: Option<Duration>) {
        let _ = self.set_read_timeout(d);
    }
}
impl Stream for rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    fn set_timeout(&self, d: Option<Duration>) {
        let _ = self.sock.set_read_timeout(d);
    }
}

struct Url {
    tls: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Option<Url> {
    let url = url.trim();
    let (tls, rest) = if let Some(r) = url.strip_prefix("wss://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("ws://") {
        (false, r)
    } else {
        return None;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_owned()),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.contains(']') || h.ends_with(']') => (h.to_owned(), p.parse().ok()?),
        _ => (authority.to_owned(), if tls { 443 } else { 80 }),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']').to_owned();
    (!host.is_empty()).then_some(Url { tls, host, port, path })
}

fn open_stream(url: &Url) -> Result<Box<dyn Stream>, String> {
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("no address")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    let _ = tcp.set_nodelay(true);
    if !url.tls {
        return Ok(Box::new(tcp));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(url.host.clone()).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

/// The HTTP upgrade by hand (Horizon builds tungstenite without its handshake).
fn handshake(mut stream: Box<dyn Stream>, url: &Url) -> Result<WebSocket<Box<dyn Stream>>, String> {
    let key = base64::engine::general_purpose::STANDARD.encode(crate::crypto::random::<16>());
    let default_port = if url.tls { 443 } else { 80 };
    let host = if url.port == default_port { url.host.clone() } else { format!("{}:{}", url.host, url.port) };
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nUser-Agent: VCraft/{} (Horizon)\r\n\r\n",
        url.path,
        crate::VERSION
    );
    stream.set_timeout(Some(Duration::from_secs(10)));
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let end = loop {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed during handshake".into());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 16 * 1024 {
            return Err("handshake too long".into());
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]);
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 101") {
        return Err(format!("relay refused the connection ({})", status_line.trim()));
    }
    let rest = buf[end..].to_vec();
    Ok(WebSocket::from_partially_read(stream, rest, Role::Client, None))
}

fn alive(shared: &Mutex<Shared>, run_id: u64) -> bool {
    shared.lock().unwrap().generation == run_id
}

fn run(shared: Arc<Mutex<Shared>>, mut run_id: u64, voice: Option<VoiceSink>) {
    loop {
        let login = shared.lock().unwrap().login.clone();
        let Some(login) = login else { return };
        let reason = session(&shared, run_id, &login, voice.as_ref());
        // `failed`
        let delay = {
            let mut s = shared.lock().unwrap();
            if s.generation != run_id {
                return;
            }
            s.generation += 1;
            run_id = s.generation;
            s.status = Status::Disconnected;
            if s.auth_error.is_none() {
                s.last_error = Some(reason.clone());
            }
            s.auth_error = None;
            s.outgoing.clear();
            s.queued_voice = 0;
            s.events.push_back(Event::Reset);
            if reason == "Banned" || reason == "Account deleted" {
                s.wanted = false;
            }
            if !s.wanted {
                return;
            }
            let d = s.retry_ms;
            s.retry_ms = (s.retry_ms * 2).min(60_000);
            d
        };
        let until = Instant::now() + Duration::from_millis(delay);
        while Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
            if !alive(&shared, run_id) || !shared.lock().unwrap().wanted {
                return;
            }
        }
        let mut s = shared.lock().unwrap();
        if s.status != Status::Disconnected || !s.wanted {
            return;
        }
        s.status = Status::Connecting;
    }
}

/// One connection, until it ends; returns why.
fn session(shared: &Arc<Mutex<Shared>>, run_id: u64, login: &Login, voice: Option<&VoiceSink>) -> String {
    let Some(url) = parse_url(&login.url) else {
        return "Invalid relay address".into();
    };
    let stream = match open_stream(&url) {
        Ok(s) => s,
        Err(e) => return format!("Can't reach relay: {e}"),
    };
    let mut ws = match handshake(stream, &url) {
        Ok(ws) => ws,
        Err(e) => return format!("Can't reach relay: {e}"),
    };
    ws.get_ref().set_timeout(Some(Duration::from_millis(20)));
    {
        let mut s = shared.lock().unwrap();
        if s.generation != run_id {
            let _ = ws.close(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() }));
            return "Disconnected".into();
        }
        s.status = Status::Authenticating;
    }
    loop {
        if !alive(shared, run_id) {
            let _ = ws.close(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() }));
            let _ = ws.flush();
            return "Disconnected".into();
        }
        match ws.read() {
            Ok(Message::Text(t)) => match serde_json::from_str::<Value>(t.as_str()) {
                Ok(o) => handle(shared, &o, login),
                Err(e) => eprintln!("VCraft: bad relay message: {e}"),
            },
            Ok(Message::Binary(b)) => {
                if let Some(v) = voice {
                    v(&b);
                }
            }
            Ok(Message::Close(frame)) => {
                let reason = frame.map(|f| f.reason.as_str().to_owned()).filter(|r| !r.is_empty());
                return reason.unwrap_or_else(|| "Disconnected from relay".into());
            }
            Ok(_) => {}
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(WsError::ConnectionClosed | WsError::AlreadyClosed) => return "Disconnected from relay".into(),
            Err(e) => return format!("Relay error: {e}"),
        }
        let out: Vec<Out> = {
            let mut s = shared.lock().unwrap();
            s.queued_voice = 0;
            s.outgoing.drain(..).collect()
        };
        for o in out {
            let msg = match o {
                Out::Text(t) => Message::text(t),
                Out::Voice(v) => Message::binary(v),
            };
            if let Err(e) = ws.write(msg) {
                return format!("Relay error: {e}");
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return format!("Relay error: {e}"),
        }
    }
}

fn handle(shared: &Arc<Mutex<Shared>>, o: &Value, login: &Login) {
    let t = o["t"].as_str().unwrap_or("");
    if t == "challenge" {
        let server_id = o["serverId"].as_str().unwrap_or("").to_owned();
        let offline = o["offline"].as_bool().unwrap_or(false);
        let (shared, login) = (shared.clone(), login.clone());
        let _ = std::thread::Builder::new().name("VCraft Login".into()).spawn(move || {
            if let Err(e) = crate::with_secure(|sec| sec.init(&login.config_dir, &login.account_id)) {
                eprintln!("VCraft could not load its encryption keys: {e}");
            }
            if !offline {
                if let Err(e) = (login.authenticate)(&server_id) {
                    eprintln!("VCraft: Mojang session join failed: {e}");
                }
            }
            let key = crate::with_secure(|sec| sec.public_key());
            let msg = json!({ "t": "login", "name": login.username, "key": key, "status": login.status });
            let mut s = shared.lock().unwrap();
            if s.status == Status::Authenticating {
                s.outgoing.push_back(Out::Text(msg.to_string()));
            }
        });
        return;
    }
    let mut s = shared.lock().unwrap();
    if t == "welcome" {
        s.status = Status::Connected;
        s.last_error = None;
        s.retry_ms = 2000;
    }
    if t == "error" && s.status == Status::Authenticating {
        let text = o["text"].as_str().map(str::to_owned);
        s.auth_error = text.clone();
        s.last_error = text;
    }
    s.events.push_back(Event::Message(o.clone()));
}

#[cfg(test)]
mod tests {
    use super::parse_url;

    #[test]
    fn parses_relay_urls() {
        let u = parse_url("wss://vcraft.2-31-104-109.sslip.io").unwrap();
        assert!(u.tls && u.port == 443 && u.path == "/" && u.host == "vcraft.2-31-104-109.sslip.io");
        let u = parse_url("ws://localhost:24470/ws").unwrap();
        assert!(!u.tls && u.port == 24470 && u.path == "/ws" && u.host == "localhost");
        assert!(parse_url("http://x").is_none());
    }
}
