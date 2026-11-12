//! VCraft for Horizon: voice calls, group chats and direct messages with your
//! friends from anywhere in Minecraft, end-to-end encrypted. A port of the
//! Fabric mod `vcraft` 2.3.0 and its addons (Proximity Chat, Announcements,
//! Group Rules), speaking the same relay protocol and encryption, so Horizon
//! and Fabric players talk to each other.
//!
//! The bundle's helpers (the glue compiled into the game) call this crate:
//! they hand it ticks, keys, clicks and what the game knows, draw what it
//! returns, and carry out its [`Effect`]s.

pub mod adpcm;
pub mod addons;
pub mod commands;
pub mod config;
pub mod crypto;
pub mod denoise;
pub mod dlog;
pub mod echo;
pub mod effects;
pub mod gfx;
pub mod hud;
pub mod i18n;
pub mod media;
pub mod social;
pub mod relay;
pub mod screen;
pub mod skins;
pub mod state;
pub mod util;
pub mod voice;

use config::Config;
use crypto::{Secure, Signing};
use relay::{Login, Relay};
use state::State;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use voice::Voice;

pub use gfx::Draw;

pub const ID: &str = "vcraft";
pub const NAME: &str = "VCraft";
pub const VERSION: &str = "2.5.0";
pub const DESCRIPTION: &str =
    "Voice calls, group chats and direct messages with your friends from anywhere in Minecraft. End-to-end encrypted.";
pub const COMMAND: &str = "vc";

/// Keys: (name, label, default binding), as the Fabric mod binds them, then
/// Horizon's own (unbound until the player picks a key).
pub const KEYS: [(&str, &str, &str); 7] = [
    ("open", "Open VCraft", "key.keyboard.o"),
    ("push_to_talk", "Push to Talk", "key.keyboard.v"),
    ("mute", "Toggle Mute", "key.keyboard.m"),
    ("deafen", "Toggle Deafen", "key.keyboard.n"),
    ("accept_call", "Accept Call", "key.keyboard.unknown"),
    ("hang_up", "Hang Up / Decline", "key.keyboard.unknown"),
    ("compact_hud", "Toggle Compact HUD", "key.keyboard.unknown"),
];

/// "key.keyboard.left.shift" -> "Left Shift", "key.mouse.4" -> "Mouse 4".
pub fn key_label(key: &str) -> String {
    if key.is_empty() || key == "key.keyboard.unknown" {
        return i18n::t("Not bound").to_owned();
    }
    let (mouse, rest) = match key.strip_prefix("key.mouse.") {
        Some(r) => (true, r),
        None => (false, key.strip_prefix("key.keyboard.").unwrap_or(key)),
    };
    let words: Vec<String> = rest
        .split('.')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let label = words.join(" ");
    if mouse { format!("Mouse {label}") } else { label }
}

/// What the game should do, drained by the glue each tick.
#[derive(Clone, Debug)]
pub enum Effect {
    Chat { tag: String, tag_rgb: u32, body: String },
    Ding,
    RingStart,
    RingStop,
    /// Join this server (after the player confirmed).
    Connect { name: String, address: String },
    OpenScreen,
    CloseScreen,
}

/// What the game tells the engine each tick.
#[derive(Clone, Default)]
pub struct World {
    /// The multiplayer server played on: (name, address); None in single player or the menus.
    pub server: Option<(String, String)>,
    /// The player's position and dimension.
    pub me: Option<([f64; 3], String)>,
    /// Other players in view: UUID to position (same dimension as `me`).
    pub players: Vec<(String, [f64; 3])>,
    /// The Now Playing mod is turned on.
    pub now_playing_on: bool,
    /// The pause menu is open.
    pub paused: bool,
    /// The game window has the focus.
    pub focused: bool,
    /// The game's language code (`en_us`, `fr_fr`...).
    pub language: String,
    /// The VCraft keys as bound: (name, key).
    pub keys: Vec<(String, String)>,
}

/// Who is signed in, from the launcher's account.
#[derive(Clone)]
pub struct Account {
    pub name: String,
    pub uuid: Option<String>,
    pub authenticate: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
}

pub struct Core {
    pub cfg: Config,
    pub st: State,
    pub fx: Vec<Effect>,
    pub toasts: Vec<(String, String, i64)>,
    pub server: Option<(String, String)>,
    pub world: World,
    pub account: Option<Account>,
    pub addons: addons::Addons,
    pub ui: screen::Ui,
    pub signing: Option<Signing>,
    np_sharing: (bool, i64),
    started: bool,
    ptt_held: bool,
    /// Muted by the pause menu or the window losing focus (not by the player).
    pub auto_muted: bool,
    /// The friends, calls, pins and requests of the 2.5 social features.
    pub social: social::Social,
    last_logged: (Option<relay::Status>, Option<String>),
}

static CORE: LazyLock<Mutex<Option<Core>>> = LazyLock::new(|| Mutex::new(None));
static SECURE: LazyLock<Mutex<Secure>> = LazyLock::new(|| Mutex::new(Secure::default()));
static RELAY: LazyLock<Relay> = LazyLock::new(Relay::new);
static VOICE: LazyLock<Arc<Voice>> = LazyLock::new(|| Arc::new(Voice::new()));

pub fn with_secure<R>(f: impl FnOnce(&mut Secure) -> R) -> R {
    f(&mut SECURE.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn relay() -> &'static Relay {
    &RELAY
}

pub fn voice() -> &'static Arc<Voice> {
    &VOICE
}

fn core() -> MutexGuard<'static, Option<Core>> {
    CORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` on the engine, making it the first time with `config_dir`
/// (the game's shared `config/` folder) and the account.
pub fn with<R>(config_dir: impl FnOnce() -> Option<PathBuf>, account: impl FnOnce() -> Option<Account>, f: impl FnOnce(&mut Core) -> R) -> Option<R> {
    let mut g = core();
    if g.is_none() {
        let dir = config_dir()?;
        let cfg = Config::load(&dir);
        dlog::init(&dir, cfg.debug_log);
        dlog::line(&format!("VCraft {VERSION} for Horizon started"));
        let account = account();
        let account_id = account.as_ref().map(|a| a.uuid.clone().unwrap_or_else(|| a.name.clone())).unwrap_or_default();
        let signing = Signing::load(&dir, &account_id);
        let mut c = Core {
            st: {
                let mut st = State::default();
                st.my_status = cfg.status.clone();
                st
            },
            addons: addons::Addons::load(&dir),
            social: social::Social::load(&cfg),
            cfg,
            fx: Vec::new(),
            toasts: Vec::new(),
            server: None,
            world: World::default(),
            account,
            ui: screen::Ui::default(),
            signing,
            np_sharing: (true, 0),
            started: false,
            ptt_held: false,
            auto_muted: false,
            last_logged: (None, None),
        };
        let v = VOICE.clone();
        RELAY.set_voice_sink(Arc::new(move |data: &[u8]| v.receive(data)));
        c.push_voice_params();
        *g = Some(c);
    }
    g.as_mut().map(f)
}

impl Core {
    pub fn login(&self) -> Option<Login> {
        let a = self.account.clone()?;
        Some(Login {
            username: a.name.clone(),
            account_id: a.uuid.clone().unwrap_or_else(|| a.name.clone()),
            config_dir: self.cfg.dir(),
            status: self.cfg.status.clone(),
            url: self.cfg.server_url.clone(),
            authenticate: a.authenticate.clone(),
        })
    }

    pub fn connect(&mut self) {
        match self.login() {
            Some(l) => relay().connect(l),
            None => self.notice("Sign in to Minecraft in the Horizon launcher to use VCraft.", true),
        }
    }

    pub fn reconnect(&mut self) {
        match self.login() {
            Some(l) => relay().reconnect(l),
            None => self.notice("Sign in to Minecraft in the Horizon launcher to use VCraft.", true),
        }
    }

    pub fn my_name(&self) -> String {
        self.st.my_name.clone().or_else(|| self.account.as_ref().map(|a| a.name.clone())).unwrap_or_default()
    }

    /// The voice threads' view of the state and settings.
    pub fn push_voice_params(&mut self) {
        let mut p = voice().params.lock().unwrap();
        p.muted = self.st.muted || self.auto_muted;
        p.echo_cancellation = self.cfg.echo_cancellation;
        p.effect = self.cfg.voice_effect.clone();
        p.deafened = self.st.deafened;
        p.active = self.voice_active();
        p.in_call = self.st.call.is_some();
        p.me = self.st.my_uuid.clone();
        p.push_to_talk = self.cfg.push_to_talk;
        p.push_to_talk_held = self.ptt_held;
        p.threshold_db = self.cfg.voice_threshold_db;
        p.mic_volume = self.cfg.mic_volume;
        p.noise_suppression = self.cfg.noise_suppression;
        p.output_volume = self.cfg.output_volume;
        p.muted_users = self.cfg.muted_users.iter().cloned().collect();
        p.room_gains = self.addons.room_gains();
    }

    /// The Now Playing mod is on (the Fabric mod checks it's installed).
    pub fn now_playing_installed(&self) -> bool {
        self.world.now_playing_on
    }

    pub fn now_playing_current(&self) -> Option<String> {
        if !self.now_playing_installed() {
            return None;
        }
        media::start();
        util::clean_song(Some(&media::current()), 120)
    }

    /// Now Playing's own "sharing" switch (`config/nowplaying.properties`).
    pub fn now_playing_sharing(&self) -> bool {
        self.now_playing_installed() && self.np_sharing.0
    }

    fn refresh_np_sharing(&mut self) {
        let now = util::now_ms();
        if now - self.np_sharing.1 < 5000 {
            return;
        }
        self.np_sharing.1 = now;
        let text = std::fs::read_to_string(self.cfg.dir().join("nowplaying.properties")).unwrap_or_default();
        self.np_sharing.0 = !text.lines().any(|l| {
            let l = l.trim();
            l.starts_with("sharing") && l.split(['=', ':']).nth(1).is_some_and(|v| v.trim().eq_ignore_ascii_case("false"))
        });
    }

    /// Logs the relay's and the audio's state when they change.
    fn log_status(&mut self) {
        let st = relay().status();
        let err = relay().last_error().or_else(|| voice().error());
        if self.last_logged.0 != Some(st) {
            let why = if st == relay::Status::Disconnected { relay().last_error().map(|e| format!(" ({e})")).unwrap_or_default() } else { String::new() };
            dlog::line(&format!("relay: {st:?}{why}"));
        }
        if err != self.last_logged.1 {
            if let Some(e) = voice().error().filter(|e| Some(e) != self.last_logged.1.as_ref()) {
                dlog::line(&format!("audio: {e}"));
            }
        }
        self.last_logged = (Some(st), err);
    }

    /// The bound key for one of [`KEYS`], as the game reported it.
    pub fn key_bound(&self, name: &str) -> String {
        self.world
            .keys
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, k)| k.clone())
            .or_else(|| KEYS.iter().find(|k| k.0 == name).map(|k| k.2.to_owned()))
            .unwrap_or_default()
    }

    /// Unread messages, without muted groups.
    pub fn unread_total(&self) -> i64 {
        self.st.unread.iter().filter(|(conv, _)| !self.group_muted(conv)).map(|(_, n)| *n).sum()
    }

    pub fn group_muted(&self, conv: &str) -> bool {
        conv.strip_prefix("g:").is_some_and(|g| self.cfg.muted_groups.contains(g))
    }
}

fn account_or_none() -> Option<Account> {
    None
}

/// Each client tick, on the window's game.
pub fn tick(config_dir: impl FnOnce() -> Option<PathBuf>, account: impl FnOnce() -> Option<Account>, world: World, ptt_held: bool) -> Vec<Effect> {
    with(config_dir, account, |c| {
        c.server = world.server.clone();
        i18n::set(if c.cfg.language == "game" { &world.language } else { &c.cfg.language });
        let auto = (c.cfg.auto_mute_paused && world.paused) || (c.cfg.auto_mute_unfocused && !world.focused);
        c.world = world;
        c.ptt_held = ptt_held;
        if auto != c.auto_muted {
            c.auto_muted = auto;
            c.send_voice_state();
            c.changed();
        }
        c.log_status();
        c.refresh_np_sharing();
        if !c.started {
            c.started = true;
            if c.cfg.auto_connect {
                c.connect();
            }
        }
        for e in relay().drain() {
            match e {
                relay::Event::Message(o) => c.handle(&o),
                relay::Event::Reset => c.reset(),
            }
        }
        c.state_tick();
        c.push_voice_params();
        skins::tick();
        std::mem::take(&mut c.fx)
    })
    .unwrap_or_default()
}

/// The mod was turned off in Options > Mods, or the game is quitting.
pub fn shutdown() {
    relay().disconnect();
    voice().stop();
    if let Some(c) = core().as_mut() {
        c.started = false;
        c.reset();
        c.social.rejoin = None;
        dlog::line("stopped");
    }
}

/// Effects queued by keys, clicks, commands and settings, for the glue to carry out.
pub fn take_effects() -> Vec<Effect> {
    with(|| None, account_or_none, |c| std::mem::take(&mut c.fx)).unwrap_or_default()
}

/// A bound key went down while playing.
pub fn key(name: &str) {
    let _ = with(|| None, account_or_none, |c| match name {
        "open" => c.fx.push(Effect::OpenScreen),
        "mute" => c.toggle_mute(),
        "deafen" => c.toggle_deafen(),
        "accept_call" => match c.latest_invite() {
            Some(inv) => c.accept_invite(&inv.call_id),
            None => c.toast("VCraft", i18n::t("Nobody is calling you.")),
        },
        "hang_up" => {
            if let Some(inv) = c.latest_invite() {
                c.decline_invite(&inv.call_id);
            } else if c.st.call.is_some() {
                c.send("call_leave", None, None);
            }
        }
        "compact_hud" => {
            c.cfg.hud_compact = !c.cfg.hud_compact;
            c.cfg.save();
        }
        _ => {}
    });
}

/// Tells the engine what to say in chat, for the glue's own messages.
pub fn notice(text: &str, error: bool) {
    let _ = with(|| None, account_or_none, |c| c.notice(text, error));
}
/// The HUD: muted/deafened, the call, the addons' lines, toasts and the screen-less notice.
pub fn hud_draw(width: f32, height: f32, measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    with(|| None, account_or_none, |c| hud::draw(c, width, height, measure)).unwrap_or_default()
}

/// The VCraft screen, drawn over the world while it's open.
pub fn screen_draw(width: f32, height: f32, mouse: [f32; 2], measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    with(|| None, account_or_none, |c| screen::draw(c, width, height, mouse, measure)).unwrap_or_default()
}

pub fn screen_opened() {
    let _ = with(|| None, account_or_none, screen::opened);
}

pub fn screen_closed() {
    let _ = with(|| None, account_or_none, screen::closed);
}

pub fn screen_click(mouse: [f32; 2], button: i32) {
    let _ = with(|| None, account_or_none, |c| screen::click(c, mouse, button));
}

pub fn screen_is_open() -> bool {
    with(|| None, account_or_none, |c| c.ui.open).unwrap_or(false)
}

pub fn screen_release() {
    let _ = with(|| None, account_or_none, screen::release);
}

pub fn screen_drag(mouse: [f32; 2]) {
    let _ = with(|| None, account_or_none, |c| screen::drag(c, mouse));
}

pub fn screen_scroll(mouse: [f32; 2], amount: f64) {
    let _ = with(|| None, account_or_none, |c| screen::scroll(c, mouse, amount));
}

/// A key on the open screen; `open_key` is the bound "Open VCraft" key.
pub fn screen_key(key: &str, modifiers: u32, open_key: &str) {
    let _ = with(|| None, account_or_none, |c| screen::key(c, key, modifiers, open_key));
}

pub fn screen_char(ch: char) {
    let _ = with(|| None, account_or_none, |c| screen::typed(c, ch));
}

/// `/vc ...`: None when it isn't ours; else the lines to show.
pub fn command(line: &str) -> Option<Vec<String>> {
    with(|| None, account_or_none, |c| commands::run(c, line)).flatten()
}

/// The faces atlas when it changed: (width, height, RGBA).
pub fn faces_image() -> Option<(u32, u32, Vec<u8>)> {
    skins::take_image()
}

/// The faces atlas's source rectangle for a face slot.
pub fn face_source(slot: usize) -> [f32; 4] {
    skins::source(slot)
}

pub fn settings_buttons() -> Vec<String> {
    with(|| None, account_or_none, |c| {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Connect on startup: {}", on(c.cfg.auto_connect)),
            format!("Input: {}", if c.cfg.push_to_talk { "Push to Talk" } else { "Voice Activity" }),
            format!("Pop-up notifications: {}", on(c.cfg.notifications)),
            format!("Message sound: {}", on(c.cfg.message_sound)),
            format!("Show friends my song: {}", on(c.cfg.share_listening)),
            format!("Addon - Proximity Chat: {}", on(c.cfg.addon_proximity)),
            format!("Addon - Announcements: {}", on(c.cfg.addon_announcements)),
            format!("Addon - Group Rules: {}", on(c.cfg.addon_grouprules)),
            format!("Echo cancellation: {}", on(c.cfg.echo_cancellation)),
            format!("Compact HUD: {}", on(c.cfg.hud_compact)),
            "Open VCraft (more settings inside)".to_owned(),
        ]
    })
    .unwrap_or_default()
}

pub fn settings_click(button: usize) {
    let _ = with(|| None, account_or_none, |c| {
        match button {
            0 => c.cfg.auto_connect = !c.cfg.auto_connect,
            1 => c.cfg.push_to_talk = !c.cfg.push_to_talk,
            2 => c.cfg.notifications = !c.cfg.notifications,
            3 => c.cfg.message_sound = !c.cfg.message_sound,
            4 => c.cfg.share_listening = !c.cfg.share_listening,
            5 => {
                c.cfg.addon_proximity = !c.cfg.addon_proximity;
                if !c.cfg.addon_proximity {
                    c.proximity_set_enabled(false);
                }
            }
            6 => c.cfg.addon_announcements = !c.cfg.addon_announcements,
            7 => c.cfg.addon_grouprules = !c.cfg.addon_grouprules,
            8 => {
                c.cfg.echo_cancellation = !c.cfg.echo_cancellation;
                c.push_voice_params();
            }
            9 => c.cfg.hud_compact = !c.cfg.hud_compact,
            10 => c.fx.push(Effect::OpenScreen),
            _ => {}
        }
        c.cfg.save();
    });
}
