//! VCraft's end-to-end encryption, byte for byte as the Fabric mod does it
//! (`E2ee`, `Secure`): X25519 identities in Java's X.509 / PKCS#8 encodings,
//! pairwise keys from HKDF-SHA256, AES-256-GCM, trust-on-first-use pinning,
//! and the per-call / per-room voice keys. Plus the addons' Ed25519 signing key.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use curve25519_dalek::montgomery::MontgomeryPoint;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// X.509 SubjectPublicKeyInfo prefix of an X25519 key (Java's `getEncoded`).
const X25519_SPKI: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00];
/// PKCS#8 prefix of an X25519 private key.
const X25519_PKCS8: [u8; 16] = [0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x04, 0x22, 0x04, 0x20];
/// The same two for Ed25519.
const ED25519_SPKI: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
const ED25519_PKCS8: [u8; 16] = [0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];

pub const NONCE: usize = 12;
pub const TAG: usize = 16;
const PREFIX: &str = "e2:";

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    let _ = SystemRandom::new().fill(&mut b);
    b
}

fn raw_key(der: &[u8]) -> Option<[u8; 32]> {
    (der.len() >= 32).then(|| der[der.len() - 32..].try_into().ok()).flatten()
}

pub fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

/// `Arrays.compare` (signed bytes) then concatenation, smaller first.
fn sorted(a: &[u8], b: &[u8]) -> Vec<u8> {
    let sa: Vec<i8> = a.iter().map(|&x| x as i8).collect();
    let sb: Vec<i8> = b.iter().map(|&x| x as i8).collect();
    if sa <= sb { concat(&[a, b]) } else { concat(&[b, a]) }
}

/// HKDF-SHA256 with an all-zero salt, one 32-byte block (`E2ee.hkdf`).
fn hkdf(ikm: &[u8], info: &[u8]) -> [u8; 32] {
    use ring::hmac;
    let prk = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &[0u8; 32]), ikm);
    let mut ctx = hmac::Context::with_key(&hmac::Key::new(hmac::HMAC_SHA256, prk.as_ref()));
    ctx.update(info);
    ctx.update(&[1]);
    ctx.sign().as_ref().try_into().unwrap()
}

/// AES-256-GCM with a given nonce: ciphertext and tag.
pub fn seal_with(key: &[u8], nonce: [u8; NONCE], plaintext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut buf = plaintext.to_vec();
    k.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(aad), &mut buf).ok()?;
    Some(buf)
}

/// A random nonce, then the ciphertext and tag (`E2ee.seal`).
pub fn seal(key: &[u8], plaintext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let nonce: [u8; NONCE] = random();
    Some(concat(&[&nonce, &seal_with(key, nonce, plaintext, aad)?]))
}

pub fn open_with(key: &[u8], nonce: [u8; NONCE], data: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).ok()?);
    let mut buf = data.to_vec();
    let plain = k.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(aad), &mut buf).ok()?;
    Some(plain.to_vec())
}

pub fn open(key: &[u8], sealed: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < NONCE + TAG {
        return None;
    }
    open_with(key, sealed[..NONCE].try_into().ok()?, &sealed[NONCE..], aad)
}

/// This account's X25519 identity.
pub struct Identity {
    private: [u8; 32],
    public_der: Vec<u8>,
    public_b64: String,
    pairwise: HashMap<String, [u8; 32]>,
}

impl Identity {
    pub fn generate() -> Self {
        let private: [u8; 32] = random();
        Self::from_private(private)
    }

    fn from_private(private: [u8; 32]) -> Self {
        let public = MontgomeryPoint::mul_base_clamped(private).to_bytes();
        let public_der = concat(&[&X25519_SPKI, &public]);
        let public_b64 = B64.encode(&public_der);
        Self { private, public_der, public_b64, pairwise: HashMap::new() }
    }

    pub fn decode(_public_b64: &str, private_b64: &str) -> Option<Self> {
        let der = B64.decode(private_b64.trim()).ok()?;
        Some(Self::from_private(raw_key(&der)?))
    }

    pub fn private_b64(&self) -> String {
        B64.encode(concat(&[&X25519_PKCS8, &self.private]))
    }

    pub fn public_key(&self) -> &str {
        &self.public_b64
    }

    pub fn pairwise_key(&mut self, theirs_b64: &str) -> Option<[u8; 32]> {
        if let Some(k) = self.pairwise.get(theirs_b64) {
            return Some(*k);
        }
        let theirs_der = B64.decode(theirs_b64).ok()?;
        let theirs = raw_key(&theirs_der)?;
        let shared = MontgomeryPoint(theirs).mul_clamped(self.private).to_bytes();
        let info = concat(&[b"crosstalk-pairwise-v1", &sorted(&self.public_der, &theirs_der)]);
        let key = hkdf(&shared, &info);
        self.pairwise.insert(theirs_b64.to_owned(), key);
        Some(key)
    }

    pub fn safety_number(&self, theirs_b64: &str) -> String {
        let Ok(theirs) = B64.decode(theirs_b64) else { return "?????".into() };
        let digest = ring::digest::digest(&ring::digest::SHA256, &sorted(&self.public_der, &theirs));
        let n = u64::from_be_bytes(digest.as_ref()[..8].try_into().unwrap()) & (i64::MAX as u64);
        let digits = format!("{:015}", n % 1_000_000_000_000_000);
        format!("{} {} {}", &digits[..5], &digits[5..10], &digits[10..])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    Changed,
    None,
}

struct Rx {
    key: [u8; 32],
    epoch: u8,
    last_counter: u64,
}

/// A call's or a room's voice key session (`Secure.Session`).
pub struct Session {
    key_message: &'static str,
    aad_label: &'static str,
    rotate_on_join: bool,
    id: Option<String>,
    key: Option<[u8; 32]>,
    epoch: u8,
    counter: u64,
    sent_to: HashSet<String>,
    last_members: HashSet<String>,
    rx: HashMap<String, Rx>,
}

impl Session {
    fn new(key_message: &'static str, aad_label: &'static str, rotate_on_join: bool) -> Self {
        Self {
            key_message,
            aad_label,
            rotate_on_join,
            id: None,
            key: None,
            epoch: 0,
            counter: 0,
            sent_to: HashSet::new(),
            last_members: HashSet::new(),
            rx: HashMap::new(),
        }
    }

    fn aad(&self, session: &str, from: &str, to: &str, e: u8) -> Vec<u8> {
        format!("{}|{session}|{from}|{to}|{e}", self.aad_label).into_bytes()
    }

    fn rotate(&mut self) {
        self.key = Some(random());
        self.epoch = self.epoch.wrapping_add(1);
        self.counter = 0;
        self.sent_to.clear();
    }
}

fn nonce(counter: u64) -> [u8; NONCE] {
    let mut n = [0u8; NONCE];
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n
}

fn voice_aad(sender: &str, epoch: u8) -> Option<Vec<u8>> {
    let u = crate::util::parse_uuid(sender)?;
    let mut out = u.to_be_bytes().to_vec();
    out.push(epoch);
    Some(out)
}

/// `Secure`: the identity, the trust store and the voice sessions.
pub struct Secure {
    identity: Option<Identity>,
    trust_file: Option<PathBuf>,
    pinned: HashMap<String, String>,
    presented: HashMap<String, String>,
    pub call: Session,
    pub room: Session,
}

impl Default for Secure {
    fn default() -> Self {
        Self {
            identity: None,
            trust_file: None,
            pinned: HashMap::new(),
            presented: HashMap::new(),
            call: Session::new("call_key", "callkey", false),
            room: Session::new("room.key", "roomkey", true),
        }
    }
}

pub fn safe_name(account: &str) -> String {
    account.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_private(path: &Path, text: &str) {
    let _ = std::fs::write(path, text);
}

impl Secure {
    /// Loads (or makes) `config/vcraft/identity-<account>.json` and the trust store.
    pub fn init(&mut self, config_dir: &Path, account: &str) -> Result<(), String> {
        let dir = config_dir.join("vcraft");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let safe = safe_name(account);
        let id_file = dir.join(format!("identity-{safe}.json"));
        let trust_file = dir.join(format!("trust-{safe}.json"));
        let identity = match read_json(&id_file) {
            Some(o) => Identity::decode(o["public"].as_str().unwrap_or(""), o["private"].as_str().unwrap_or(""))
                .ok_or("bad identity file")?,
            None => {
                let id = Identity::generate();
                let o = json!({ "public": id.public_key(), "private": id.private_b64() });
                write_private(&id_file, &serde_json::to_string_pretty(&o).unwrap_or_default());
                id
            }
        };
        self.identity = Some(identity);
        self.pinned.clear();
        self.presented.clear();
        if let Some(Value::Object(m)) = read_json(&trust_file) {
            for (k, v) in m {
                if let Some(s) = v.as_str() {
                    self.pinned.insert(k, s.to_owned());
                }
            }
        }
        self.trust_file = Some(trust_file);
        Ok(())
    }

    pub fn public_key(&self) -> Option<String> {
        self.identity.as_ref().map(|i| i.public_key().to_owned())
    }

    pub fn observe(&mut self, uuid: Option<&str>, key: Option<&str>) {
        let (Some(uuid), Some(key)) = (uuid, key) else { return };
        if key.is_empty() {
            return;
        }
        self.presented.insert(uuid.to_owned(), key.to_owned());
        if !self.pinned.contains_key(uuid) {
            self.pinned.insert(uuid.to_owned(), key.to_owned());
            self.save_trust();
        }
    }

    pub fn trust(&self, uuid: &str) -> Trust {
        match self.presented.get(uuid) {
            None => Trust::None,
            Some(k) if self.pinned.get(uuid) == Some(k) => Trust::Trusted,
            Some(_) => Trust::Changed,
        }
    }

    /// Pins the key `uuid` presents now. The caller re-keys the call and room.
    pub fn accept_presented_key(&mut self, uuid: &str) -> bool {
        let Some(key) = self.presented.get(uuid).cloned() else { return false };
        self.pinned.insert(uuid.to_owned(), key);
        self.save_trust();
        true
    }

    pub fn safety_number(&self, uuid: &str) -> Option<String> {
        let key = self.presented.get(uuid)?;
        Some(self.identity.as_ref()?.safety_number(key))
    }

    fn pair_key(&mut self, uuid: &str) -> Option<[u8; 32]> {
        if self.trust(uuid) != Trust::Trusted {
            return None;
        }
        let key = self.presented.get(uuid)?.clone();
        self.identity.as_mut()?.pairwise_key(&key)
    }

    fn save_trust(&self) {
        if let Some(f) = &self.trust_file {
            let m: Map<String, Value> = self.pinned.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
            let _ = std::fs::write(f, Value::Object(m).to_string());
        }
    }

    pub fn seal(&mut self, recipient: &str, aad: &str, envelope: &str) -> Option<String> {
        let key = self.pair_key(recipient)?;
        Some(format!("{PREFIX}{}", B64.encode(seal(&key, envelope.as_bytes(), aad.as_bytes())?)))
    }

    pub fn open(&mut self, peer: &str, aad: &str, wire: Option<&str>) -> Option<String> {
        let sealed = B64.decode(wire?.strip_prefix(PREFIX)?).ok()?;
        let key = self.pair_key(peer)?;
        String::from_utf8(open(&key, &sealed, aad.as_bytes())?).ok()
    }

    /// Updates a voice session's members; returns the key messages to send.
    pub fn update(&mut self, room: bool, new_id: Option<&str>, members: HashSet<String>, me: Option<&str>) -> Vec<Value> {
        let mut out = Vec::new();
        let (Some(new_id), Some(me)) = (new_id, me) else {
            let s = if room { &mut self.room } else { &mut self.call };
            s.id = None;
            s.key = None;
            s.sent_to.clear();
            s.last_members.clear();
            s.rx.clear();
            return out;
        };
        let mut members = members;
        members.remove(me);
        {
            let s = if room { &mut self.room } else { &mut self.call };
            if s.id.as_deref() != Some(new_id) {
                s.id = Some(new_id.to_owned());
                s.rx.clear();
                s.last_members.clear();
                s.rotate();
            }
            let left = !s.last_members.is_subset(&members);
            let joined = !members.is_subset(&s.last_members);
            if left || (joined && s.rotate_on_join && !s.last_members.is_empty()) {
                s.rotate();
                s.rx.retain(|k, _| members.contains(k));
            }
            s.last_members = members.clone();
        }
        let mut sorted: Vec<&String> = members.iter().collect();
        sorted.sort();
        for m in sorted {
            let already = if room { self.room.sent_to.contains(m) } else { self.call.sent_to.contains(m) };
            if already || self.trust(m) != Trust::Trusted {
                continue;
            }
            let Some(pair) = self.pair_key(m) else { continue };
            let s = if room { &mut self.room } else { &mut self.call };
            let (Some(key), Some(id)) = (s.key, s.id.clone()) else { continue };
            let aad = s.aad(&id, me, m, s.epoch);
            if let Some(sealed) = seal(&pair, &key, &aad) {
                out.push(json!({ "t": s.key_message, "to": m, "epoch": s.epoch, "key": B64.encode(sealed) }));
                s.sent_to.insert(m.clone());
            }
        }
        out
    }

    pub fn rekey(&mut self, room: bool, clear_received: bool, me: Option<&str>) -> Vec<Value> {
        let s = if room { &mut self.room } else { &mut self.call };
        let Some(id) = s.id.clone() else { return Vec::new() };
        if clear_received {
            s.rx.clear();
        }
        s.rotate();
        let mut members = s.last_members.clone();
        if let Some(me) = me {
            members.insert(me.to_owned());
        }
        self.update(room, Some(&id), members, me)
    }

    pub fn on_key(&mut self, room: bool, from: &str, for_id: Option<&str>, epoch: u8, key_b64: &str, me: Option<&str>) {
        let (Some(me), Some(for_id)) = (me, for_id) else { return };
        let matches = { let s = if room { &self.room } else { &self.call }; s.id.as_deref() == Some(for_id) };
        if !matches || self.trust(from) != Trust::Trusted {
            return;
        }
        let Some(pair) = self.pair_key(from) else { return };
        let Ok(sealed) = B64.decode(key_b64) else { return };
        let s = if room { &mut self.room } else { &mut self.call };
        let aad = s.aad(for_id, from, me, epoch);
        match open(&pair, &sealed, &aad).and_then(|k| <[u8; 32]>::try_from(k.as_slice()).ok()) {
            Some(key) => {
                s.rx.insert(from.to_owned(), Rx { key, epoch, last_counter: 0 });
            }
            None => eprintln!("VCraft: rejected a voice key from {from}"),
        }
    }

    fn active(&mut self, in_call: bool) -> &mut Session {
        if in_call { &mut self.call } else { &mut self.room }
    }

    pub fn encrypt_voice(&mut self, in_call: bool, frame: &[u8], me: &str) -> Option<Vec<u8>> {
        let s = self.active(in_call);
        let key = s.key?;
        let e = s.epoch;
        s.counter += 1;
        let n = s.counter;
        let ct = seal_with(&key, nonce(n), frame, &voice_aad(me, e)?)?;
        let mut out = Vec::with_capacity(9 + ct.len());
        out.push(e);
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(&ct);
        Some(out)
    }

    pub fn decrypt_voice(&mut self, in_call: bool, sender: &str, data: &[u8]) -> Option<Vec<u8>> {
        let s = self.active(in_call);
        let r = s.rx.get_mut(sender)?;
        if data.len() < 25 {
            return None;
        }
        let e = data[0];
        let n = u64::from_be_bytes(data[1..9].try_into().ok()?);
        if e != r.epoch || n <= r.last_counter {
            return None;
        }
        let plain = open_with(&r.key, nonce(n), &data[9..], &voice_aad(sender, e)?)?;
        r.last_counter = n;
        Some(plain)
    }

    pub fn has_voice_key_from(&self, in_call: bool, uuid: &str) -> bool {
        if in_call { self.call.rx.contains_key(uuid) } else { self.room.rx.contains_key(uuid) }
    }
}

pub fn dm_aad(from: &str, to: &str) -> String {
    format!("dm|{from}|{to}")
}
pub fn activity_aad(from: &str, to: &str) -> String {
    format!("act|{from}|{to}")
}
pub fn group_aad(group: &str, from: &str, to: &str) -> String {
    format!("gm|{group}|{from}|{to}")
}

/// The addons' Ed25519 key (`config/vcraft/signing-<account>.json`).
pub struct Signing {
    pair: ring::signature::Ed25519KeyPair,
    public_b64: String,
}

impl Signing {
    pub fn load(config_dir: &Path, account: &str) -> Option<Self> {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let dir = config_dir.join("vcraft");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(format!("signing-{}.json", safe_name(account)));
        let seed: [u8; 32] = match read_json(&file) {
            Some(o) => raw_key(&B64.decode(o["private"].as_str()?).ok()?)?,
            None => {
                let seed: [u8; 32] = random();
                let pair = Ed25519KeyPair::from_seed_unchecked(&seed).ok()?;
                let o = json!({
                    "public": B64.encode(concat(&[&ED25519_SPKI, pair.public_key().as_ref()])),
                    "private": B64.encode(concat(&[&ED25519_PKCS8, &seed])),
                });
                write_private(&file, &o.to_string());
                seed
            }
        };
        let pair = Ed25519KeyPair::from_seed_unchecked(&seed).ok()?;
        let public_b64 = B64.encode(concat(&[&ED25519_SPKI, pair.public_key().as_ref()]));
        Some(Self { pair, public_b64 })
    }

    pub fn public_key(&self) -> &str {
        &self.public_b64
    }

    pub fn sign(&self, text: &str) -> String {
        B64.encode(self.pair.sign(text.as_bytes()).as_ref())
    }
}

/// Ed25519 `signature` over `text` by the X.509-encoded `public_key`.
pub fn verify_ed25519(public_key: &str, text: &str, signature: &str) -> bool {
    let (Ok(der), Ok(sig)) = (B64.decode(public_key), B64.decode(signature)) else { return false };
    let Some(raw) = raw_key(&der) else { return false };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, raw).verify(text.as_bytes(), &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairwise_keys_agree_and_messages_round_trip() {
        let mut a = Identity::generate();
        let mut b = Identity::generate();
        let (pa, pb) = (a.public_key().to_owned(), b.public_key().to_owned());
        let ka = a.pairwise_key(&pb).unwrap();
        let kb = b.pairwise_key(&pa).unwrap();
        assert_eq!(ka, kb);
        let sealed = seal(&ka, b"hello", b"dm|a|b").unwrap();
        assert_eq!(open(&kb, &sealed, b"dm|a|b").unwrap(), b"hello");
        assert!(open(&kb, &sealed, b"dm|a|c").is_none());
        assert_eq!(a.safety_number(&pb), b.safety_number(&pa));
        let decoded = Identity::decode(&pa, &a.private_b64()).unwrap();
        assert_eq!(decoded.public_key(), pa);
        assert_eq!(B64.decode(&pa).unwrap().len(), 44);
    }

    #[test]
    fn x25519_rfc7748_vector() {
        // RFC 7748 section 6.1.
        let alice: [u8; 32] = hex::decode("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a").unwrap().try_into().unwrap();
        let bob_pub: [u8; 32] = hex::decode("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f").unwrap().try_into().unwrap();
        let shared = MontgomeryPoint(bob_pub).mul_clamped(alice).to_bytes();
        assert_eq!(hex::encode(shared), "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    }

    #[test]
    fn ed25519_sign_verify() {
        let dir = std::env::temp_dir().join(format!("vc-sign-{}", std::process::id()));
        let s = Signing::load(&dir, "acct").unwrap();
        let sig = s.sign("hi");
        assert!(verify_ed25519(s.public_key(), "hi", &sig));
        assert!(!verify_ed25519(s.public_key(), "ho", &sig));
        let s2 = Signing::load(&dir, "acct").unwrap();
        assert_eq!(s.public_key(), s2.public_key());
        let _ = std::fs::remove_dir_all(dir);
    }
}
