//! VCraft 2.5's social features, on top of what the relay already carries:
//!
//! - **What friends are doing**: the server they're on and whether they're in
//!   a call ride along in the end-to-end encrypted `activity` message that
//!   already carries "Now listening". Fabric clients read only its `song` and
//!   ignore the rest, so both keep working together.
//! - **Asking to join a call** and **rejoining after a dropped connection**:
//!   a request in that same `activity` message; the friend in the call
//!   rings you in (automatically for someone who was just in it), and your
//!   client answers that ring by itself.
//! - **Group invite codes**: a code names the group and the member who made
//!   it; pasting it asks that member (who must be your friend) to add you.
//! - **Last seen**, **pins** and **nicknames and notes** stay on your computer.

use crate::state::{Invite, LOBBY, Message};
use crate::util::{b, now_ms, s};
use crate::{Core, crypto, dlog, i18n};
use base64::Engine;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// How long someone who was in your call can rejoin it without asking.
const REJOIN_WINDOW_MS: i64 = 180_000;
/// How long after a drop VCraft keeps trying to get back in.
const REJOIN_TRY_MS: i64 = 120_000;
const REQUEST_MS: i64 = 60_000;
const CODE_PREFIX: &str = "vc1.";
const MAX_PINS: usize = 50;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {
    pub server: Option<String>,
    pub in_call: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RequestKind {
    /// They want to join your call.
    Call,
    /// They want to be added to this group (its id).
    Group(String),
}

#[derive(Clone, Debug)]
pub struct Request {
    pub from: String,
    pub name: String,
    pub kind: RequestKind,
    pub at: i64,
}

#[derive(Clone, Debug)]
pub struct Pin {
    pub id: String,
    pub from_name: String,
    pub text: String,
    pub ts: i64,
}

#[derive(Clone, Debug)]
pub struct Rejoin {
    /// The others who were in the call, not asked yet.
    pub members: Vec<String>,
    pub dropped: i64,
    pub next_try: i64,
}

#[derive(Default)]
pub struct Social {
    pub activity: HashMap<String, Activity>,
    pub last_seen: HashMap<String, i64>,
    pub pins: HashMap<String, Vec<Pin>>,
    pub requests: Vec<Request>,
    /// Friends whose call you asked to join: their ring is answered by itself until then.
    pub waiting_join: HashMap<String, i64>,
    /// Who was in your call, and when last.
    pub recent_members: HashMap<String, i64>,
    pub rejoin: Option<Rejoin>,
    sent: Option<String>,
    next_seen_save: i64,
}

impl Social {
    pub fn load(cfg: &crate::config::Config) -> Self {
        let seen = cfg.read_data("last-seen.json");
        let last_seen = seen.as_object().map(|m| m.iter().filter_map(|(k, v)| v.as_i64().map(|t| (k.clone(), t))).collect()).unwrap_or_default();
        let mut pins = HashMap::new();
        if let Some(m) = cfg.read_data("pins.json").as_object() {
            for (conv, list) in m {
                let list: Vec<Pin> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| Pin {
                        id: s(p, "id").unwrap_or_default(),
                        from_name: s(p, "fromName").unwrap_or_default(),
                        text: s(p, "text").unwrap_or_default(),
                        ts: p["ts"].as_i64().unwrap_or(0),
                    })
                    .filter(|p| !p.id.is_empty())
                    .collect();
                if !list.is_empty() {
                    pins.insert(conv.clone(), list);
                }
            }
        }
        Self { last_seen, pins, ..Self::default() }
    }
}

/// "5m ago", "3h ago", "2d ago".
pub fn ago(ms: i64, now: i64) -> String {
    let secs = ((now - ms) / 1000).max(0);
    if secs < 60 {
        i18n::t("just now").to_owned()
    } else if secs < 3600 {
        i18n::tf("{0}m ago", &[&(secs / 60).to_string()])
    } else if secs < 86_400 {
        i18n::tf("{0}h ago", &[&(secs / 3600).to_string()])
    } else {
        i18n::tf("{0}d ago", &[&(secs / 86_400).to_string()])
    }
}

/// An invite code: the group, who made it (uuid and name) and the group's name.
pub fn encode_code(group: &str, issuer: &str, issuer_name: &str, group_name: &str) -> String {
    let body = json!({ "g": group, "o": issuer, "n": issuer_name, "gn": group_name }).to_string();
    format!("{CODE_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(body))
}

pub struct Code {
    pub group: String,
    pub issuer: String,
    pub issuer_name: String,
    pub group_name: String,
}

pub fn decode_code(code: &str) -> Option<Code> {
    let raw = code.trim().strip_prefix(CODE_PREFIX)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    Some(Code {
        group: s(&v, "g").filter(|g| !g.is_empty())?,
        issuer: s(&v, "o").filter(|o| !o.is_empty())?,
        issuer_name: s(&v, "n").unwrap_or_default(),
        group_name: s(&v, "gn").unwrap_or_default(),
    })
}

impl Core {
    // ---- What you share with friends ----

    /// Where you're playing, when you share it: the server's name, or single player.
    fn my_place(&self) -> Option<String> {
        match &self.server {
            Some((name, _)) => Some(name.clone()),
            None if self.world.me.is_some() => Some("Singleplayer".into()),
            None => None,
        }
    }

    fn sharing_activity(&self) -> bool {
        self.cfg.share_server && self.st.my_status != "invisible"
    }

    /// The `activity` envelope: the song (as before), and on Horizon where you
    /// play and whether you're in a call, plus an optional request.
    fn activity_env(&self, req: Option<Value>) -> String {
        let mut env = Map::new();
        if let Some(song) = self.shared_song_now() {
            env.insert("song".into(), song.into());
        }
        if self.sharing_activity() {
            if let Some(place) = self.my_place() {
                env.insert("server".into(), place.into());
            }
            env.insert("inCall".into(), self.st.call.is_some().into());
        }
        if let Some(r) = req {
            env.insert("req".into(), r);
        }
        Value::Object(env).to_string()
    }

    /// What the last `activity` said, to resend only on a change.
    pub fn activity_signature(&self) -> String {
        let place = if self.sharing_activity() { self.my_place() } else { None };
        let call = self.sharing_activity() && self.st.call.is_some();
        format!("{:?}|{place:?}|{call}", self.shared_song_now())
    }

    pub fn activity_changed(&mut self) -> bool {
        let sig = self.activity_signature();
        if self.social.sent.as_deref() == Some(sig.as_str()) {
            return false;
        }
        self.social.sent = Some(sig);
        true
    }

    /// Sends `activity` to these friends, with a request when given.
    pub fn send_activity(&mut self, to: &[String], req: Option<Value>) {
        let env = self.activity_env(req);
        let me = self.me().unwrap_or_default();
        let mut payloads = Map::new();
        for uuid in to {
            if let Some(sealed) = crate::with_secure(|sec| sec.seal(uuid, &crypto::activity_aad(&me, uuid), &env)) {
                payloads.insert(uuid.clone(), sealed.into());
            }
        }
        if !payloads.is_empty() {
            self.send_json(json!({ "t": "activity", "payloads": payloads }));
        }
    }

    /// A friend's `activity`, opened: what they're doing, and their request.
    pub fn on_activity(&mut self, from: &str, env: &Value) {
        let a = Activity { server: s(env, "server").map(|p| p.chars().filter(|c| !c.is_control()).take(48).collect()), in_call: b(env, "inCall") };
        if a == Activity::default() {
            self.social.activity.remove(from);
        } else {
            self.social.activity.insert(from.to_owned(), a);
        }
        if let Some(req) = env.get("req").filter(|r| r.is_object()) {
            self.on_request(from, req);
        }
    }

    fn on_request(&mut self, from: &str, req: &Value) {
        let Some(friend) = self.friend(from).cloned() else { return };
        let name = self.cfg.display_name(&friend.uuid, &friend.name);
        let quiet = self.st.my_status == "dnd" || self.cfg.muted_users.contains(from);
        match s(req, "type").as_deref() {
            Some("rejoin") if self.st.call.is_some() => {
                let recent = self.social.recent_members.get(from).is_some_and(|t| now_ms() - t < REJOIN_WINDOW_MS);
                if recent {
                    dlog::line(&format!("call: letting {} back in after a dropped connection", dlog::who(from)));
                    self.call(from);
                    self.notice(&i18n::tf("{0} is rejoining the call", &[&name]), false);
                } else {
                    self.add_request(from, &name, RequestKind::Call, quiet);
                }
            }
            Some("join") if self.st.call.is_some() => self.add_request(from, &name, RequestKind::Call, quiet),
            Some("group") => {
                let Some(gid) = s(req, "group") else { return };
                let Some(g) = self.group(&format!("g:{gid}")).cloned() else { return };
                if g.members.iter().any(|m| m.uuid == from) {
                    return;
                }
                self.add_request(from, &name, RequestKind::Group(gid), quiet);
            }
            _ => {}
        }
    }

    fn add_request(&mut self, from: &str, name: &str, kind: RequestKind, quiet: bool) {
        self.social.requests.retain(|r| !(r.from == from && r.kind == kind));
        let body = match &kind {
            RequestKind::Call => i18n::tf("{0} wants to join your call", &[name]),
            RequestKind::Group(g) => {
                let gname = self.group(&format!("g:{g}")).map(|g| g.name.clone()).unwrap_or_default();
                i18n::tf("{0} wants to join {1}", &[name, &gname])
            }
        };
        self.social.requests.push(Request { from: from.to_owned(), name: name.to_owned(), kind, at: now_ms() });
        if !quiet {
            self.toast(i18n::t("Request"), &body);
            self.chat("VCraft", 0xFF55_FF55, &format!("{body}. {}", i18n::t("Open VCraft to let them in.")));
            if self.cfg.message_sound {
                self.fx.push(crate::Effect::Ding);
            }
        }
        self.changed();
    }

    pub fn accept_request(&mut self, from: &str, kind: &RequestKind) {
        match kind {
            RequestKind::Call => self.call(from),
            RequestKind::Group(g) => self.group_action("group_add", g, Some("uuid"), Some(from)),
        }
        self.social.requests.retain(|r| !(r.from == from && &r.kind == kind));
        self.changed();
    }

    pub fn dismiss_request(&mut self, from: &str, kind: &RequestKind) {
        self.social.requests.retain(|r| !(r.from == from && &r.kind == kind));
        self.changed();
    }

    /// Asks a friend who is in a call to ring you in.
    pub fn ask_to_join(&mut self, uuid: &str) {
        let Some(f) = self.friend(uuid).cloned() else { return };
        if !self.social.activity.get(uuid).is_some_and(|a| a.in_call) {
            self.notice(&i18n::tf("{0} isn't in a call.", &[&self.cfg.display_name(&f.uuid, &f.name)]), true);
            return;
        }
        self.social.waiting_join.insert(uuid.to_owned(), now_ms() + REQUEST_MS);
        self.send_activity(&[uuid.to_owned()], Some(json!({ "type": "join" })));
        self.notice(&i18n::tf("Asked {0} to let you into their call.", &[&self.cfg.display_name(&f.uuid, &f.name)]), false);
    }

    /// A call invite that answers your own request is accepted by itself.
    pub fn auto_accept(&mut self, inv: &Invite) -> bool {
        let waiting = self.social.waiting_join.get(&inv.from).is_some_and(|t| *t > now_ms());
        if !waiting || self.st.call.is_some() {
            return false;
        }
        self.social.waiting_join.remove(&inv.from);
        self.social.rejoin = None;
        dlog::line(&format!("call: joining {}'s call", dlog::who(&inv.from)));
        self.send("call_accept", Some("callId"), Some(&inv.call_id));
        true
    }

    // ---- Calls: who was in it, and getting back in ----

    /// Remembers who is in the call, so they can rejoin it after a drop.
    pub fn note_call_members(&mut self) {
        let now = now_ms();
        let me = self.me();
        if let Some(call) = &self.st.call {
            for m in &call.members {
                if Some(&m.uuid) != me.as_ref() {
                    self.social.recent_members.insert(m.uuid.clone(), now);
                }
            }
        }
    }

    /// The relay connection dropped during a call: try to get back in.
    pub fn prepare_rejoin(&mut self) {
        let Some(call) = &self.st.call else { return };
        let me = self.me();
        let members: Vec<String> = call.members.iter().map(|m| m.uuid.clone()).filter(|u| Some(u) != me.as_ref()).collect();
        if !self.cfg.auto_rejoin || members.is_empty() || !crate::relay().wanted() {
            return;
        }
        dlog::line(&format!("call: connection dropped with {} others in the call, will try to rejoin", members.len()));
        let now = now_ms();
        self.social.rejoin = Some(Rejoin { members, dropped: now, next_try: now });
    }

    pub fn social_tick(&mut self) {
        let now = now_ms();
        self.social.requests.retain(|r| now - r.at < REQUEST_MS);
        self.social.waiting_join.retain(|_, t| *t > now);
        self.social.recent_members.retain(|_, t| now - *t < REJOIN_WINDOW_MS);
        if self.st.call.is_some() {
            self.note_call_members();
        }
        self.try_rejoin(now);
        // Friends seen online: when we last saw them, saved now and then.
        if now >= self.social.next_seen_save && crate::relay().connected() {
            self.social.next_seen_save = now + 60_000;
            let online: Vec<String> = self.st.friends.iter().filter(|f| f.online()).map(|f| f.uuid.clone()).collect();
            for u in online {
                self.social.last_seen.insert(u, now);
            }
            self.save_last_seen();
        }
    }

    fn try_rejoin(&mut self, now: i64) {
        let Some(mut r) = self.social.rejoin.clone() else { return };
        if self.st.call.is_some() || now - r.dropped > REJOIN_TRY_MS {
            if self.st.call.is_none() {
                dlog::line("call: couldn't rejoin in time");
                self.notice(i18n::t("Couldn't get back into the call."), true);
            }
            self.social.rejoin = None;
            return;
        }
        if now < r.next_try || !crate::relay().connected() || self.st.my_uuid.is_none() {
            return;
        }
        let pick = r.members.iter().position(|u| self.friend(u).is_some_and(|f| f.online()));
        match pick {
            Some(i) => {
                let uuid = r.members.remove(i);
                self.social.waiting_join.insert(uuid.clone(), now + 20_000);
                self.send_activity(&[uuid.clone()], Some(json!({ "type": "rejoin" })));
                dlog::line(&format!("call: asked {} to let us back in", dlog::who(&uuid)));
                self.notice(i18n::t("Reconnected. Getting you back into the call..."), false);
                r.next_try = now + 12_000;
                self.social.rejoin = Some(r);
            }
            None if r.members.iter().all(|u| self.friend(u).is_none()) && !self.st.friends.is_empty() => {
                // Nobody left to ask is a friend: they can't hear us ask.
                self.notice(i18n::t("Reconnected. Ask someone in the call to invite you again."), false);
                self.social.rejoin = None;
            }
            None => {
                r.next_try = now + 2000;
                self.social.rejoin = Some(r);
            }
        }
    }

    // ---- Last seen ----

    pub fn note_presence(&mut self, uuid: &str, was_online: bool, status: &str) {
        if status == "offline" {
            self.social.activity.remove(uuid);
            if was_online {
                self.social.last_seen.insert(uuid.to_owned(), now_ms());
                self.save_last_seen();
            }
        }
    }

    fn save_last_seen(&self) {
        let m: Map<String, Value> = self.social.last_seen.iter().map(|(k, v)| (k.clone(), (*v).into())).collect();
        self.cfg.write_data("last-seen.json", &Value::Object(m));
    }

    /// The second line under a friend: "Playing on X", "In a call", "Last seen 2h ago".
    pub fn friend_detail(&self, uuid: &str, online: bool) -> Option<String> {
        if !online {
            let t = self.social.last_seen.get(uuid)?;
            return Some(i18n::tf("Last seen {0}", &[&ago(*t, now_ms())]));
        }
        let a = self.social.activity.get(uuid)?;
        match (&a.server, a.in_call) {
            (Some(p), true) if p == "Singleplayer" => Some(i18n::t("In a call, in single player").to_owned()),
            (Some(p), true) => Some(i18n::tf("In a call, on {0}", &[p])),
            (None, true) => Some(i18n::t("In a call").to_owned()),
            (Some(p), false) if p == "Singleplayer" => Some(i18n::t("Playing single player").to_owned()),
            (Some(p), false) => Some(i18n::tf("Playing on {0}", &[p])),
            (None, false) => None,
        }
    }

    pub fn friend_in_call(&self, uuid: &str) -> bool {
        self.social.activity.get(uuid).is_some_and(|a| a.in_call) && !self.in_call(uuid)
    }

    // ---- Pins (yours, on this computer) ----

    pub fn is_pinned(&self, conv: &str, id: &str) -> bool {
        self.social.pins.get(conv).is_some_and(|l| l.iter().any(|p| p.id == id))
    }

    pub fn toggle_pin(&mut self, conv: &str, m: &Message) {
        let Some(id) = m.id.clone() else { return };
        let list = self.social.pins.entry(conv.to_owned()).or_default();
        if let Some(i) = list.iter().position(|p| p.id == id) {
            list.remove(i);
        } else {
            let text = match (&m.invite, &m.song) {
                (Some(inv), _) => format!("Server invite: {}", inv.name),
                (None, Some(song)) => format!("\u{266A} {song}"),
                _ => m.text.clone(),
            };
            list.push(Pin { id, from_name: m.from_name.clone(), text, ts: m.ts });
            list.sort_by_key(|p| p.ts);
            if list.len() > MAX_PINS {
                list.remove(0);
            }
        }
        if list.is_empty() {
            self.social.pins.remove(conv);
        }
        self.save_pins();
        self.changed();
    }

    pub fn unpin(&mut self, conv: &str, id: &str) {
        if let Some(list) = self.social.pins.get_mut(conv) {
            list.retain(|p| p.id != id);
            if list.is_empty() {
                self.social.pins.remove(conv);
            }
        }
        self.save_pins();
        self.changed();
    }

    fn save_pins(&self) {
        let mut m = Map::new();
        for (conv, list) in &self.social.pins {
            let arr: Vec<Value> = list.iter().map(|p| json!({ "id": p.id, "fromName": p.from_name, "text": p.text, "ts": p.ts })).collect();
            m.insert(conv.clone(), Value::Array(arr));
        }
        self.cfg.write_data("pins.json", &Value::Object(m));
    }

    pub fn pins(&self, conv: &str) -> Vec<Pin> {
        if conv == LOBBY {
            return Vec::new();
        }
        self.social.pins.get(conv).cloned().unwrap_or_default()
    }

    // ---- Group invite codes ----

    pub fn group_code(&self, conv: &str) -> Option<String> {
        let g = self.group(conv)?;
        let me = self.me()?;
        Some(encode_code(&g.id, &me, &self.my_name(), &g.name))
    }

    /// Pasting a code: asks its maker to add you (you need to be friends).
    pub fn join_with_code(&mut self, code: &str) {
        let Some(c) = decode_code(code) else {
            self.notice(i18n::t("That isn't a VCraft group code."), true);
            return;
        };
        if self.group(&format!("g:{}", c.group)).is_some() {
            crate::screen::run(self, crate::screen::Action::OpenChat(format!("g:{}", c.group)));
            return;
        }
        if Some(&c.issuer) == self.st.my_uuid.as_ref() {
            self.notice(i18n::t("That's your own code."), true);
            return;
        }
        let group_name = if c.group_name.is_empty() { i18n::t("the group").to_owned() } else { c.group_name.clone() };
        match self.friend(&c.issuer).cloned() {
            Some(f) if f.online() => {
                self.send_activity(&[f.uuid.clone()], Some(json!({ "type": "group", "group": c.group })));
                self.notice(&i18n::tf("Asked {0} to add you to {1}.", &[&self.cfg.display_name(&f.uuid, &f.name), &group_name]), false);
            }
            Some(f) => {
                self.notice(&i18n::tf("{0} is offline. Try again when they're online.", &[&self.cfg.display_name(&f.uuid, &f.name)]), true);
            }
            None if !c.issuer_name.is_empty() => {
                self.send("friend_request", Some("name"), Some(&c.issuer_name));
                self.notice(&i18n::tf("Sent {0} a friend request. Paste the code again once they accept.", &[&c.issuer_name]), false);
            }
            None => self.notice(i18n::t("Add the person who sent the code as a friend first."), true),
        }
    }

    // ---- Nicknames and notes ----

    pub fn set_nickname(&mut self, uuid: &str, nick: &str, note: &str) {
        let (nick, note) = (nick.trim(), note.trim());
        if nick.is_empty() {
            self.cfg.nicknames.remove(uuid);
        } else {
            self.cfg.nicknames.insert(uuid.to_owned(), nick.chars().take(24).collect());
        }
        if note.is_empty() {
            self.cfg.notes.remove(uuid);
        } else {
            self.cfg.notes.insert(uuid.to_owned(), note.chars().take(160).collect());
        }
        self.cfg.save();
        self.changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        let code = encode_code("grp-1", "0123", "Steve", "Builders");
        let c = decode_code(&format!("  {code} ")).unwrap();
        assert_eq!((c.group.as_str(), c.issuer.as_str(), c.issuer_name.as_str(), c.group_name.as_str()), ("grp-1", "0123", "Steve", "Builders"));
        assert!(decode_code("hello").is_none());
        assert!(decode_code("vc1.!!!").is_none());
    }

    #[test]
    fn ago_reads_well() {
        let _l = crate::i18n::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::i18n::set("en_us");
        let now = 10_000_000_000;
        assert_eq!(ago(now - 5_000, now), "just now");
        assert_eq!(ago(now - 5 * 60_000, now), "5m ago");
        assert_eq!(ago(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(ago(now - 2 * 86_400_000, now), "2d ago");
    }
}
