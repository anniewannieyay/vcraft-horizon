//! VCraft's addons, built in: on Fabric each is its own mod
//! (vcraft-proximity, vcraft-announcements, vcraft-grouprules) plugged into
//! VCraft's addon API; Horizon mods can't call into each other, so here they
//! are part of VCraft, each with a switch in Options > Mods > VCraft. Their
//! relay messages, signatures, encryption and files are the Fabric addons'.

use crate::crypto::{self, Trust};
use crate::gfx::{self, Gfx, Icon};
use crate::screen::{self, Action, Ctx, Style};
use crate::state::{LOBBY, Message};
use crate::util::{b, i, now_ms, s};
use crate::{Core, Effect};
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const PROXIMITY: &str = "vcraft-proximity";
pub const ANNOUNCEMENTS: &str = "vcraft-announcements";
pub const GROUPRULES: &str = "vcraft-grouprules";

const PROX_ICON: Icon = ["...#.....", "..##..#..", "###.#..#.", "###.#.#.#", "###.#.#.#", "###.#.#.#", "###.#..#.", "..##..#..", "...#....."];
const ANN_ICON: Icon = ["......##.", "....####.", "##.#####.", "########.", "########.", "##.#####.", ".#..####.", ".#....##.", ".##......"];
const RULES_ICON: Icon = [".#######.", "##.....##", "#..###..#", "#.......#", "#..###..#", "#.......#", ".#..#..#.", "..#...#..", "...###..."];

const MIN_RANGE: i32 = 8;
const MAX_RANGE: i32 = 128;
const FULL_VOLUME_BLOCKS: f64 = 6.0;
const SLOW_STEPS: [i64; 7] = [0, 5, 10, 30, 60, 300, 900];

#[derive(Clone, Debug)]
pub enum AddonAction {
    ProxEnabled(bool),
    ProxRange(i32),
    AnnPopups(bool),
    AnnPost,
    AnnDelete(String),
    AnnRsvp(String, bool),
    AnnTrust(String),
    AnnInviteHere,
    AnnJoin(Option<String>, String),
    RulesBack,
    RulesAccept(String),
    RulesMode(Mode),
    RulesSlow(i32),
    RulesAdminsOnly,
    RulesRequireAccept,
    RulesAdmin(String),
    RulesSave(String),
    RulesMute(String, String, i64),
    RulesKick(String, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hide,
    Mask,
    Block,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Hide => "HIDE",
            Mode::Mask => "MASK",
            Mode::Block => "BLOCK",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "HIDE" => Some(Mode::Hide),
            "MASK" => Some(Mode::Mask),
            "BLOCK" => Some(Mode::Block),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Nearby {
    uuid: String,
    name: String,
    distance: f64,
    gain: f64,
}

#[derive(Default)]
pub struct Prox {
    enabled: bool,
    range: i32,
    gains: HashMap<String, f64>,
    nearby: Vec<Nearby>,
    ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnnTrust {
    Verified,
    KeyChanged,
    Invalid,
}

#[derive(Clone, Debug)]
struct AnnItem {
    id: String,
    author: String,
    author_name: Option<String>,
    title: String,
    body: String,
    when: Option<i64>,
    server_name: Option<String>,
    server_address: Option<String>,
    created: i64,
    pub_key: Option<String>,
    going: i64,
    me: bool,
    trust: AnnTrust,
}

#[derive(Default)]
pub struct Ann {
    popups: bool,
    last_seen: i64,
    pins: HashMap<String, String>,
    items: Vec<AnnItem>,
    loaded: bool,
    invite_here: bool,
}

#[derive(Clone, Debug)]
struct Policy {
    version: i64,
    accept_version: i64,
    slow_seconds: i64,
    admins_only: bool,
    require_accept: bool,
    admins: Vec<String>,
    mutes: HashMap<String, i64>,
    accepted: bool,
}

#[derive(Clone, Debug)]
struct Content {
    group: String,
    version: i64,
    accept_version: i64,
    owner: String,
    mode: Mode,
    keywords: Vec<String>,
    text: String,
    pub_key: String,
    sig: String,
}

#[derive(Default, Clone)]
struct GroupRules {
    policy: Option<Policy>,
    content: Option<Content>,
    signed_json: Option<String>,
    problem: Option<String>,
}

#[derive(Default)]
pub struct Rules {
    rules: HashMap<String, GroupRules>,
    owner_keys: HashMap<String, String>,
    fetched: HashSet<String>,
    refreshed: HashSet<String>,
    conv: Option<String>,
    editor_loaded: bool,
    mode: Option<Mode>,
    slow_seconds: i64,
    admins_only: bool,
    require_accept: bool,
    admins: Vec<String>,
}

pub struct Addons {
    dir: PathBuf,
    pub prox: Prox,
    pub ann: Ann,
    pub rules: Rules,
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn string_map(v: Option<Value>) -> HashMap<String, String> {
    v.and_then(|v| v.as_object().cloned())
        .map(|m| m.into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned()))).collect())
        .unwrap_or_default()
}

impl Addons {
    pub fn load(config_dir: &Path) -> Self {
        let dir = config_dir.join("vcraft").join("addons");
        let ps = read_json(&dir.join(PROXIMITY).join("settings.json")).unwrap_or(Value::Null);
        let prox = Prox {
            enabled: ps["enabled"].as_bool().unwrap_or(false),
            range: (ps["range"].as_i64().unwrap_or(48) as i32).clamp(MIN_RANGE, MAX_RANGE),
            ..Prox::default()
        };
        let asettings = read_json(&dir.join(ANNOUNCEMENTS).join("settings.json")).unwrap_or(Value::Null);
        let ann = Ann {
            popups: asettings["popups"].as_bool().unwrap_or(true),
            last_seen: asettings["lastSeen"].as_i64().unwrap_or(0),
            pins: string_map(read_json(&dir.join(ANNOUNCEMENTS).join("pins.json"))),
            ..Ann::default()
        };
        let rules = Rules { owner_keys: string_map(read_json(&dir.join(GROUPRULES).join("owner-keys.json"))), ..Rules::default() };
        Self { dir, prox, ann, rules }
    }

    fn write(&self, addon: &str, file: &str, v: &Value) {
        let d = self.dir.join(addon);
        let _ = std::fs::create_dir_all(&d).and_then(|()| std::fs::write(d.join(file), v.to_string()));
    }

    /// Proximity Chat's volumes, when the addon is on.
    pub fn room_gains(&self) -> Option<HashMap<String, f64>> {
        Some(self.prox.gains.clone())
    }
}

fn group_id(conv: Option<&str>) -> Option<String> {
    conv?.strip_prefix("g:").map(str::to_owned)
}

fn addon_aad(context: &str, from: &str, to: &str) -> String {
    format!("addon|{context}|{from}|{to}")
}

fn addon_seal(c: &Core, to: &str, context: &str, plain: &str) -> Option<String> {
    let me = c.me()?;
    crate::with_secure(|sec| sec.seal(to, &addon_aad(context, &me, to), plain))
}

fn addon_open(c: &Core, from: &str, context: &str, sealed: &str) -> Option<String> {
    let me = c.me()?;
    crate::with_secure(|sec| sec.open(from, &addon_aad(context, from, &me), Some(sealed)))
}

fn sign(c: &Core, text: &str) -> Option<(String, String)> {
    let s = c.signing.as_ref()?;
    Some((s.public_key().to_owned(), s.sign(text)))
}

fn relay_send(t: &str, body: Value) {
    let mut o = body.as_object().cloned().unwrap_or_default();
    o.insert("t".into(), t.into());
    crate::relay().send(&Value::Object(o));
}

impl Core {
    fn loaded_ids(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.cfg.addon_proximity {
            v.push(PROXIMITY);
        }
        if self.cfg.addon_announcements {
            v.push(ANNOUNCEMENTS);
        }
        if self.cfg.addon_grouprules {
            v.push(GROUPRULES);
        }
        v
    }

    pub fn addons_connected(&mut self) {
        let ids = self.loaded_ids();
        if ids.is_empty() {
            return;
        }
        self.send_json(json!({ "t": "addons", "list": ids }));
        if self.cfg.addon_announcements {
            relay_send("ann.list", json!({}));
        }
        if self.cfg.addon_grouprules {
            self.addons.rules.fetched.clear();
            self.addons.rules.refreshed.clear();
        }
    }

    pub fn addons_message(&mut self, _conv: &str, _m: &Message) {}

    pub fn addons_call(&mut self, _had: bool, _has: bool) {}

    pub fn addons_dispatch(&mut self, t: &str, o: &Value) {
        match t {
            "ann.list" | "ann.new" | "ann.deleted" | "ann.rsvp" if self.cfg.addon_announcements => self.ann_handle(t, o),
            "grule.update" if self.cfg.addon_grouprules => self.rules_update(o),
            "grule.need" if self.cfg.addon_grouprules => self.rules_need(o),
            _ => {}
        }
    }

    pub fn addons_filter(&mut self, conv: &str, m: Message) -> Option<Message> {
        if !self.cfg.addon_grouprules || m.system || m.id.is_none() {
            return Some(m);
        }
        let Some(gid) = group_id(Some(conv)) else { return Some(m) };
        let Some(content) = self.addons.rules.rules.get(&gid).and_then(|r| r.content.clone()) else { return Some(m) };
        if content.keywords.is_empty() {
            return Some(m);
        }
        let re = pattern(&content.keywords);
        if !re.is_match(&m.text) {
            return Some(m);
        }
        if content.mode != Mode::Mask {
            return None;
        }
        let text = re.replace_all(&m.text, |caps: &regex::Captures| "*".repeat(caps[0].chars().count().max(3))).into_owned();
        Some(Message { text, ..m })
    }

    pub fn addons_check_send(&mut self, conv: &str, text: &str) -> Option<String> {
        if !self.cfg.addon_grouprules {
            return None;
        }
        let gid = group_id(Some(conv))?;
        let r = self.addons.rules.rules.get(&gid)?.clone();
        let me = self.me().unwrap_or_default();
        if let Some(p) = &r.policy {
            let owner = r.content.as_ref().map(|c| c.owner.clone());
            if p.require_accept && !p.accepted && owner.as_deref() != Some(me.as_str()) {
                screen::open_addon_page(self, GROUPRULES, Some(conv.to_owned()));
                return Some("Accept this group's rules before posting.".into());
            }
        }
        let content = r.content?;
        if content.keywords.is_empty() || content.mode == Mode::Mask {
            return None;
        }
        pattern(&content.keywords).find(text).map(|m| format!("Not sent: this group doesn't allow \"{}\".", m.as_str()))
    }

    pub fn addons_tick(&mut self) {
        if self.cfg.addon_proximity {
            self.prox_tick();
        }
        if self.cfg.addon_grouprules && crate::relay().connected() && self.st.my_uuid.is_some() {
            let ids: Vec<String> = self.st.groups.iter().map(|g| g.id.clone()).collect();
            for id in ids {
                if self.addons.rules.fetched.insert(id.clone()) {
                    relay_send("grule.get", json!({ "group": id }));
                }
            }
        }
    }

    // ---- Proximity Chat ----

    pub fn proximity_set_enabled(&mut self, on: bool) {
        self.addons.prox.enabled = on;
        self.prox_save();
        if !on {
            self.leave_room();
            self.addons.prox.gains.clear();
            self.addons.prox.nearby.clear();
        }
    }

    fn prox_save(&self) {
        let p = &self.addons.prox;
        self.addons.write(PROXIMITY, "settings.json", &json!({ "enabled": p.enabled, "range": p.range }));
    }

    fn prox_tick(&mut self) {
        self.addons.prox.ticks += 1;
        if self.addons.prox.ticks % 5 != 0 {
            return;
        }
        let connected = crate::relay().connected() && self.st.my_uuid.is_some();
        let address = if self.addons.prox.enabled && connected { self.server.as_ref().map(|s| s.1.clone()) } else { None };
        match address {
            None => {
                if self.st.room.is_some() || self.wanted_room().is_some() {
                    self.leave_room();
                }
                self.addons.prox.gains.clear();
                self.addons.prox.nearby.clear();
            }
            Some(a) => {
                self.join_room(&room_key(&a));
                self.prox_gains();
            }
        }
    }

    fn prox_gains(&mut self) {
        let me = self.world.me.clone();
        let players: HashMap<String, [f64; 3]> = self.world.players.iter().cloned().collect();
        let my_uuid = self.me();
        let range = self.addons.prox.range;
        let mut gains = HashMap::new();
        let mut list = Vec::new();
        for m in self.st.room.as_ref().map(|r| r.members.clone()).unwrap_or_default() {
            if Some(&m.uuid) == my_uuid.as_ref() {
                continue;
            }
            let d = match (&me, players.get(&m.uuid)) {
                (Some((a, _)), Some(b)) => ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt(),
                _ => f64::INFINITY,
            };
            let g = gain(d, range);
            gains.insert(m.uuid.clone(), g);
            list.push(Nearby { uuid: m.uuid, name: m.name, distance: d, gain: g });
        }
        list.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        self.addons.prox.gains = gains;
        self.addons.prox.nearby = list;
    }

    // ---- Announcements ----

    fn ann_parse(&mut self, o: &Value) -> AnnItem {
        let author = s(o, "author").unwrap_or_default();
        let title = s(o, "title").unwrap_or_default();
        let body = s(o, "body");
        let when = o.get("when").and_then(Value::as_i64);
        let (server_name, server_address) = (s(o, "serverName"), s(o, "serverAddress"));
        let created = i(o, "created");
        let (pub_key, sig) = (s(o, "pub"), s(o, "sig"));
        let text = ann_signed_text(&author, created, &title, body.as_deref().unwrap_or(""), when, server_name.as_deref(), server_address.as_deref(), body.is_none());
        let trust = match (&pub_key, &sig) {
            (Some(p), Some(sg)) if crypto::verify_ed25519(p, &text, sg) => match self.addons.ann.pins.get(&author) {
                None => {
                    self.addons.ann.pins.insert(author.clone(), p.clone());
                    self.ann_save_pins();
                    AnnTrust::Verified
                }
                Some(pinned) if pinned == p => AnnTrust::Verified,
                Some(_) => AnnTrust::KeyChanged,
            },
            _ => AnnTrust::Invalid,
        };
        AnnItem {
            id: s(o, "id").unwrap_or_default(),
            author,
            author_name: s(o, "authorName"),
            title,
            body: body.unwrap_or_default(),
            when,
            server_name,
            server_address,
            created,
            pub_key,
            going: i(o, "going"),
            me: b(o, "me"),
            trust,
        }
    }

    fn ann_save_pins(&self) {
        let m: Map<String, Value> = self.addons.ann.pins.iter().map(|(k, v)| (k.clone(), v.clone().into())).collect();
        self.addons.write(ANNOUNCEMENTS, "pins.json", &Value::Object(m));
    }

    fn ann_save_settings(&self) {
        let a = &self.addons.ann;
        self.addons.write(ANNOUNCEMENTS, "settings.json", &json!({ "popups": a.popups, "lastSeen": a.last_seen }));
    }

    fn ann_handle(&mut self, t: &str, o: &Value) {
        match t {
            "ann.list" => {
                let items: Vec<Value> = o["items"].as_array().cloned().unwrap_or_default();
                let list: Vec<AnnItem> = items.iter().map(|e| self.ann_parse(e)).collect();
                let first = !self.addons.ann.loaded && self.addons.ann.last_seen == 0 && !list.is_empty();
                self.addons.ann.items = list;
                if first {
                    self.ann_mark_seen();
                }
                self.addons.ann.loaded = true;
            }
            "ann.new" => {
                let it = self.ann_parse(&o["item"]);
                self.addons.ann.items.retain(|i| i.id != it.id);
                self.addons.ann.items.insert(0, it.clone());
                if it.trust != AnnTrust::Invalid {
                    self.chat("Announcement", gfx::GOLD, &it.title);
                    if self.addons.ann.popups {
                        self.toast("VCraft announcement", &it.title);
                    }
                }
            }
            "ann.deleted" => {
                let id = s(o, "id");
                self.addons.ann.items.retain(|i| Some(&i.id) != id.as_ref());
            }
            "ann.rsvp" => {
                let id = s(o, "id").unwrap_or_default();
                for it in &mut self.addons.ann.items {
                    if it.id == id {
                        it.going = i(o, "going");
                        it.me = b(o, "me");
                    }
                }
            }
            _ => {}
        }
    }

    fn ann_unseen(&self) -> i64 {
        let a = &self.addons.ann;
        a.items.iter().filter(|i| i.created > a.last_seen && i.trust != AnnTrust::Invalid).count() as i64
    }

    fn ann_mark_seen(&mut self) {
        let newest = self.addons.ann.items.iter().map(|i| i.created).max().unwrap_or(0);
        if newest > self.addons.ann.last_seen {
            self.addons.ann.last_seen = newest;
            self.ann_save_settings();
        }
    }

    fn ann_post(&mut self, title: &str, body: &str, when: Option<i64>, invite_here: bool) {
        let (sn, sa) = match (invite_here, self.server.clone()) {
            (true, Some((n, a))) => (Some(n), Some(a)),
            _ => (None, None),
        };
        let created = now_ms();
        let me = self.me().unwrap_or_default();
        let text = ann_signed_text(&me, created, title, body, when, sn.as_deref(), sa.as_deref(), false);
        let Some((pub_key, sig)) = sign(self, &text) else {
            self.notice("Couldn't sign the announcement.", true);
            return;
        };
        let mut o = json!({ "title": title, "body": body, "created": created, "pub": pub_key, "sig": sig });
        if let Some(w) = when {
            o["when"] = w.into();
        }
        if let (Some(n), Some(a)) = (sn, sa) {
            o["serverName"] = n.into();
            o["serverAddress"] = a.into();
        }
        relay_send("ann.post", o);
    }

    // ---- Group Rules ----

    fn rules_update(&mut self, o: &Value) {
        let group = s(o, "group").unwrap_or_default();
        let Some(p) = o.get("rules").filter(|r| !r.is_null()) else {
            self.addons.rules.rules.remove(&group);
            return;
        };
        let policy = Policy {
            version: i(p, "version"),
            accept_version: i(p, "acceptVersion"),
            slow_seconds: i(p, "slowSeconds"),
            admins_only: b(p, "adminsOnly"),
            require_accept: b(p, "requireAccept"),
            admins: p["admins"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
            mutes: p["mutes"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0))).collect()).unwrap_or_default(),
            accepted: b(p, "accepted"),
        };
        let mut r = self.addons.rules.rules.get(&group).cloned().unwrap_or_default();
        let stale = r.content.as_ref().is_none_or(|c| c.version != policy.version);
        r.policy = Some(policy.clone());
        match s(p, "blob") {
            Some(blob) if stale => {
                let problem = self.rules_open_blob(&group, &policy, &blob, &mut r);
                r.problem = problem.clone();
                if problem.is_some() && self.addons.rules.refreshed.insert(format!("{group}|{}", policy.version)) {
                    relay_send("grule.get", json!({ "group": group, "refresh": true }));
                }
            }
            None if stale => {
                r.problem = (policy.version != 0).then(|| "Waiting for a member to share the rules with you...".to_owned());
            }
            _ => {}
        }
        self.addons.rules.rules.insert(group, r);
    }

    fn rules_open_blob(&mut self, group: &str, policy: &Policy, blob: &str, r: &mut GroupRules) -> Option<String> {
        let Some((sealer, sealed)) = blob.split_once(':').filter(|(a, _)| !a.is_empty()) else {
            return Some("The rules couldn't be read.".into());
        };
        let Some(json) = addon_open(self, sealer, &format!("grule|{group}|{}", policy.version), sealed) else {
            return Some("The rules couldn't be decrypted. Check the encryption keys of the group's members.".into());
        };
        let Some(c) = serde_json::from_str::<Value>(&json).ok().and_then(|v| parse_content(&v)) else {
            return Some("The rules couldn't be read.".into());
        };
        let owner = self.group(&format!("g:{group}")).map(|g| g.owner.clone());
        if owner.as_deref() != Some(c.owner.as_str()) || c.group != group || c.version != policy.version {
            return Some("These rules weren't made by the group's owner.".into());
        }
        if !crypto::verify_ed25519(&c.pub_key, &rules_signed_text(&c), &c.sig) {
            return Some("The rules' signature doesn't match.".into());
        }
        match self.addons.rules.owner_keys.get(&c.owner) {
            Some(pinned) if *pinned != c.pub_key => return Some("The owner's signing key changed. Ask them if they moved computers.".into()),
            Some(_) => {}
            None => {
                self.addons.rules.owner_keys.insert(c.owner.clone(), c.pub_key.clone());
                self.rules_save_owner_keys();
            }
        }
        r.content = Some(c);
        r.signed_json = Some(json);
        None
    }

    fn rules_save_owner_keys(&self) {
        let m: Map<String, Value> = self.addons.rules.owner_keys.iter().map(|(k, v)| (k.clone(), v.clone().into())).collect();
        self.addons.write(GROUPRULES, "owner-keys.json", &Value::Object(m));
    }

    fn rules_need(&mut self, o: &Value) {
        let group = s(o, "group").unwrap_or_default();
        let to = s(o, "uuid").unwrap_or_default();
        let Some(r) = self.addons.rules.rules.get(&group).cloned() else { return };
        let (Some(c), Some(json), Some(p)) = (r.content, r.signed_json, r.policy) else { return };
        if c.version != p.version {
            return;
        }
        if let Some(sealed) = addon_seal(self, &to, &format!("grule|{group}|{}", c.version), &json) {
            let me = self.me().unwrap_or_default();
            relay_send("grule.share", json!({ "group": group, "to": to, "blob": format!("{me}:{sealed}") }));
        }
    }

    fn rules_is_admin(&self, g: &crate::state::Group, uuid: Option<&str>) -> bool {
        let Some(uuid) = uuid else { return false };
        uuid == g.owner || self.addons.rules.rules.get(&g.id).and_then(|r| r.policy.as_ref()).is_some_and(|p| p.admins.iter().any(|a| a == uuid))
    }

    fn rules_save(&mut self, gid: &str, text: &str, words: &str) {
        let Some(g) = self.group(&format!("g:{gid}")).cloned() else { return };
        let old = self.addons.rules.rules.get(gid).cloned();
        let version = old.as_ref().and_then(|o| o.policy.as_ref()).map_or(0, |p| p.version) + 1;
        let same_text = old.as_ref().and_then(|o| o.content.as_ref()).is_some_and(|c| c.text == text);
        let accept_version = if same_text { old.as_ref().and_then(|o| o.content.as_ref()).map_or(version, |c| c.accept_version) } else { version };
        let me = self.me().unwrap_or_default();
        let Some(pub_key) = self.signing.as_ref().map(|s| s.public_key().to_owned()) else {
            self.notice("Couldn't sign the rules.", true);
            return;
        };
        let ed = &self.addons.rules;
        let mut c = Content {
            group: g.id.clone(),
            version,
            accept_version,
            owner: me.clone(),
            mode: ed.mode.unwrap_or(Mode::Mask),
            keywords: parse_keywords(words),
            text: text.to_owned(),
            pub_key: pub_key.clone(),
            sig: String::new(),
        };
        let Some((_, sig)) = sign(self, &rules_signed_text(&c)) else {
            self.notice("Couldn't sign the rules.", true);
            return;
        };
        c.sig = sig;
        let json = content_json(&c).to_string();
        let mut blobs = Map::new();
        let mut missed = Vec::new();
        for m in &g.members {
            match addon_seal(self, &m.uuid, &format!("grule|{}|{version}", g.id), &json) {
                Some(sealed) => {
                    blobs.insert(m.uuid.clone(), format!("{me}:{sealed}").into());
                }
                None => missed.push(m.name.clone()),
            }
        }
        let ed = &self.addons.rules;
        relay_send(
            "grule.set",
            json!({
                "group": g.id, "version": version, "acceptVersion": accept_version,
                "slowSeconds": ed.slow_seconds, "adminsOnly": ed.admins_only, "requireAccept": ed.require_accept,
                "admins": ed.admins, "blobs": blobs,
            }),
        );
        self.addons.rules.owner_keys.insert(me, pub_key);
        self.rules_save_owner_keys();
        if missed.is_empty() {
            self.notice("Group rules saved.", false);
        } else {
            self.notice(&format!("Saved. {} will get the rules once their encryption key is trusted.", missed.join(", ")), true);
        }
    }
}

pub fn gain(distance: f64, range: i32) -> f64 {
    if distance <= FULL_VOLUME_BLOCKS {
        return 1.0;
    }
    if distance >= range as f64 {
        return 0.0;
    }
    let t = (distance - FULL_VOLUME_BLOCKS) / (range as f64 - FULL_VOLUME_BLOCKS);
    1.0 - t * t * (3.0 - 2.0 * t)
}

pub fn room_key(address: &str) -> String {
    let mut a = address.trim().to_lowercase();
    if let Some(x) = a.strip_suffix(":25565") {
        a = x.to_owned();
    }
    if let Some(x) = a.strip_suffix('.') {
        a = x.to_owned();
    }
    let h = ring::digest::digest(&ring::digest::SHA256, format!("vcraft-proximity|{a}").as_bytes());
    format!("mc:{}", &hex::encode(h.as_ref())[..40])
}

#[allow(clippy::too_many_arguments)]
fn ann_signed_text(author: &str, created: i64, title: &str, body: &str, when: Option<i64>, sn: Option<&str>, sa: Option<&str>, body_null: bool) -> String {
    // Java concatenates a null body as "null".
    let body = if body_null { "null" } else { body };
    format!(
        "vcraft-announcement\n{author}\n{created}\n{title}\n{body}\n{}\n{}\n{}",
        when.map(|w| w.to_string()).unwrap_or_default(),
        sn.unwrap_or(""),
        sa.unwrap_or("")
    )
}

fn rules_signed_text(c: &Content) -> String {
    format!(
        "vcraft-grouprules\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        c.group,
        c.version,
        c.accept_version,
        c.owner,
        c.mode.name(),
        c.keywords.join("\u{1}"),
        c.text,
        c.pub_key
    )
}

fn content_json(c: &Content) -> Value {
    json!({
        "group": c.group, "version": c.version, "acceptVersion": c.accept_version, "owner": c.owner,
        "mode": c.mode.name(), "keywords": c.keywords, "text": c.text, "pub": c.pub_key, "sig": c.sig,
    })
}

fn parse_content(o: &Value) -> Option<Content> {
    Some(Content {
        group: s(o, "group")?,
        version: o.get("version")?.as_i64()?,
        accept_version: o.get("acceptVersion")?.as_i64()?,
        owner: s(o, "owner")?,
        mode: Mode::parse(&s(o, "mode")?)?,
        keywords: o["keywords"].as_array()?.iter().filter_map(|v| v.as_str().map(str::to_owned)).take(100).collect(),
        text: s(o, "text")?,
        pub_key: s(o, "pub")?,
        sig: s(o, "sig")?,
    })
}

pub fn parse_keywords(typed: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in typed.split(',') {
        let t = w.trim().to_lowercase();
        if !t.is_empty() && t.chars().count() <= 40 && !out.contains(&t) && out.len() < 100 {
            out.push(t);
        }
    }
    out
}

fn pattern(keywords: &[String]) -> Regex {
    let parts: Vec<String> = keywords.iter().filter(|w| !w.trim().is_empty()).map(|w| regex::escape(w.trim())).collect();
    let src = if parts.is_empty() { "[^\\s\\S]".to_owned() } else { parts.join("|") };
    RegexBuilder::new(&src).case_insensitive(true).build().unwrap_or_else(|_| Regex::new("[^\\s\\S]").unwrap())
}

fn slow(s: i64) -> String {
    if s < 60 { format!("{s}s") } else { format!("{} min", s / 60) }
}

fn step(current: i64, dir: i32) -> i64 {
    let mut i = 0usize;
    while i < SLOW_STEPS.len() - 1 && SLOW_STEPS[i] < current {
        i += 1;
    }
    SLOW_STEPS[(i as i32 + dir).clamp(0, SLOW_STEPS.len() as i32 - 1) as usize]
}

pub struct PageInfo {
    pub id: &'static str,
    pub title: String,
    pub icon: Icon,
    pub in_sidebar: bool,
    pub badge: i64,
}

pub fn pages(c: &Core) -> Vec<PageInfo> {
    let mut v = Vec::new();
    if c.cfg.addon_proximity {
        v.push(PageInfo { id: PROXIMITY, title: "Proximity Chat".into(), icon: PROX_ICON, in_sidebar: true, badge: 0 });
    }
    if c.cfg.addon_announcements {
        v.push(PageInfo { id: ANNOUNCEMENTS, title: "Announcements".into(), icon: ANN_ICON, in_sidebar: true, badge: c.ann_unseen() });
    }
    if c.cfg.addon_grouprules {
        let title = match c.addons.rules.conv.as_deref().and_then(|cv| c.group(cv)) {
            Some(g) => format!("Rules: {}", g.name),
            None => "Group rules".into(),
        };
        v.push(PageInfo { id: GROUPRULES, title, icon: RULES_ICON, in_sidebar: false, badge: 0 });
    }
    v
}

pub fn page_exists(c: &Core, id: &str) -> bool {
    pages(c).iter().any(|p| p.id == id)
}

pub fn page_opened(c: &mut Core, id: &str, arg: Option<String>) {
    if id == GROUPRULES {
        if let Some(a) = arg {
            if c.addons.rules.conv.as_deref() != Some(a.as_str()) {
                c.addons.rules.conv = Some(a);
                c.addons.rules.editor_loaded = false;
            }
        }
        if let Some(g) = group_id(c.addons.rules.conv.as_deref()) {
            relay_send("grule.get", json!({ "group": g }));
        }
    }
}

pub struct ChatButton {
    pub icon: Icon,
    pub tooltip: String,
    pub action: Action,
}

pub fn chat_buttons(c: &Core, conv: &str) -> Vec<ChatButton> {
    if !c.cfg.addon_grouprules || !conv.starts_with("g:") {
        return Vec::new();
    }
    let needs = group_id(Some(conv))
        .and_then(|g| c.addons.rules.rules.get(&g))
        .and_then(|r| r.policy.as_ref())
        .is_some_and(|p| p.require_accept && !p.accepted);
    vec![ChatButton {
        icon: RULES_ICON,
        tooltip: if needs { "Group rules: accept them to post".into() } else { "Group rules".into() },
        action: Action::OpenAddonPage(GROUPRULES.into(), Some(conv.to_owned())),
    }]
}

/// The addons' HUD lines below the call (Proximity Chat's talkers).
pub fn hud(c: &Core, g: &mut Gfx, x: i32, mut y: i32) -> i32 {
    let p = &c.addons.prox;
    if !c.cfg.addon_proximity || !p.enabled || c.st.room.is_none() {
        return y;
    }
    if c.st.call.is_some() {
        let s = "Proximity paused during your call";
        let w = g.width(s);
        g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
        g.text(s, x + 1, y, gfx::MUTED);
        return y + 15;
    }
    let in_range = p.nearby.iter().filter(|n| n.gain > 0.0).count();
    let head = format!("Proximity: {in_range} nearby");
    let w = g.width(&head);
    g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
    g.text(&head, x + 1, y, gfx::LIGHT_GREEN);
    y += 14;
    for n in &p.nearby {
        if n.gain <= 0.0 || !crate::voice().is_speaking(&n.uuid) {
            continue;
        }
        let dist = format!("{}m", n.distance.round() as i64);
        let w = g.width(&n.name) + g.width(&dist) + 24;
        g.panel(x - 2, y - 2, w, 13, 0xA000_0000);
        g.fill(x - 1, y - 1, 11, 11, gfx::GREEN);
        g.face(&n.uuid, &n.name, x, y, 9);
        g.text(&n.name, x + 13, y, gfx::WHITE);
        let nw = g.width(&n.name);
        g.text(&dist, x + 17 + nw, y, gfx::MUTED);
        y += 14;
    }
    y
}

fn input(ctx: &mut Ctx, id: &str, x: i32, y: i32, w: i32, hint: &str, max: usize) -> String {
    ctx.g.panel(x, y, w, 18, gfx::BG);
    let [_, py, _, ph] = ctx.page_area;
    if y + 4 >= py && y + 14 <= py + ph {
        ctx.field(id, x + 5, y + 5, w - 10, hint, max)
    } else {
        ctx.fields.get(id).map(|f| f.value.clone()).unwrap_or_default()
    }
}

pub fn field_enter(_c: &mut Core, _id: &str) {}

pub fn render_page(c: &mut Core, ctx: &mut Ctx, id: &str, x: i32, y: i32, w: i32, h: i32) {
    match id {
        PROXIMITY => prox_page(c, ctx, x, y, w),
        ANNOUNCEMENTS => ann_page(c, ctx, x, y, w),
        GROUPRULES => rules_page(c, ctx, x, y, w),
        _ => {}
    }
    let _ = h;
}

fn prox_page(c: &mut Core, ctx: &mut Ctx, x: i32, mut y: i32, w: i32) {
    let top = y;
    let (enabled, range) = (c.addons.prox.enabled, c.addons.prox.range);
    y = ctx.section("PROXIMITY CHAT", x, y, w);
    ctx.g.text("Talk to nearby players", x, y + 4, gfx::TEXT);
    ctx.toggle(x + w - 30, y + 1, enabled, Action::Addon(AddonAction::ProxEnabled(!enabled)));
    y += 22;
    y = ctx.g.para("Hear other VCraft players on the same Minecraft server without starting a call. Voices get quieter with distance and go silent past your range. Mute, deafen and push-to-talk work as usual, and proximity pauses while you're in a call.", x, y, w, gfx::MUTED) + 6;
    y = ctx.g.para("Privacy: while this is on, the relay is told a code made from the server's address so it can put you in the same voice room as other players there. That means the relay knows which server you're on (anyone who knows an address can work out its code). Voice stays end-to-end encrypted: the relay can't hear it.", x, y, w, gfx::GOLD) + 10;
    ctx.g.text("Range", x, y + 5, gfx::TEXT);
    let bx = x + w - 120;
    ctx.styled(bx, y, 22, 18, "-", Style::Normal, Some(Action::Addon(AddonAction::ProxRange(range - 8))));
    ctx.g.centered(&format!("{range} blocks"), bx + 60, y + 5, gfx::WHITE);
    ctx.styled(bx + 98, y, 22, 18, "+", Style::Normal, Some(Action::Addon(AddonAction::ProxRange(range + 8))));
    y += 30;
    y = ctx.section("STATUS", x, y, w);
    let connected = crate::relay().connected() && c.st.my_uuid.is_some();
    let (status, color) = if !enabled {
        ("Off.".to_owned(), gfx::MUTED)
    } else if !connected {
        ("Waiting for VCraft to connect...".to_owned(), gfx::MUTED)
    } else if c.server.is_none() {
        ("Join a multiplayer server to use proximity chat.".to_owned(), gfx::MUTED)
    } else if c.st.room.is_none() {
        (format!("Joining the voice room for {}...", c.server.as_ref().map(|s| s.0.clone()).unwrap_or_default()), gfx::MUTED)
    } else if c.st.call.is_some() {
        ("Paused while you're in a call.".to_owned(), gfx::GOLD)
    } else {
        let others = c.st.room.as_ref().map_or(0, |r| r.members.len() as i64) - 1;
        let t = if others <= 0 {
            "On. No other VCraft players with proximity chat here yet.".to_owned()
        } else {
            format!("On. {others} other VCraft player{} on this server.", if others == 1 { "" } else { "s" })
        };
        (t, gfx::GREEN)
    };
    y = ctx.g.para(&status, x, y, w, color) + 8;
    if enabled && c.st.room.is_some() {
        y = ctx.section("PLAYERS", x, y, w);
        let nearby = c.addons.prox.nearby.clone();
        if nearby.is_empty() {
            ctx.g.text("Nobody else yet.", x, y + 2, gfx::FAINT);
            y += 16;
        }
        for n in nearby {
            if crate::voice().is_speaking(&n.uuid) && n.gain > 0.0 {
                ctx.g.fill(x - 1, y - 1, 18, 18, gfx::GREEN);
            }
            ctx.g.face(&n.uuid, &n.name, x, y, 16);
            let nm = ctx.g.clip(&n.name, w - 150);
            ctx.g.text(&nm, x + 24, y + 4, gfx::WHITE);
            let where_ = if n.distance.is_infinite() {
                "not nearby".to_owned()
            } else {
                format!("{} blocks{}", n.distance.round() as i64, if n.gain <= 0.0 { " (out of range)" } else { "" })
            };
            let ww = ctx.g.width(&where_);
            ctx.g.text(&where_, x + w - ww, y + 4, if n.gain > 0.0 { gfx::MUTED } else { gfx::FAINT });
            y += 22;
        }
    }
    ctx.content = ctx.content.max(y - top);
}

fn fmt_time(ms: i64, f: &str) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_millis_opt(ms).single().map(|t| t.format(f).to_string()).unwrap_or_default()
}

fn ann_page(c: &mut Core, ctx: &mut Ctx, x: i32, mut y: i32, w: i32) {
    let top = y;
    c.ann_mark_seen();
    ctx.g.fill(x - 16, y - 10, w + 32, 16, 0x40F0_B232);
    let note = ctx.g.clip("Public: not end-to-end encrypted. Posted and signed by the VCraft owner.", w);
    ctx.g.text(&note, x, y - 6, 0xFFF5_D58A);
    y += 14;
    ctx.g.text("Pop-up for new announcements", x, y + 4, gfx::TEXT);
    let popups = c.addons.ann.popups;
    ctx.toggle(x + w - 30, y + 1, popups, Action::Addon(AddonAction::AnnPopups(!popups)));
    y += 24;
    if c.st.my_rank.as_deref() == Some("owner") {
        y = ann_composer(c, ctx, x, y, w) + 6;
    }
    y = ctx.section("EVENTS", x, y, w);
    let items = c.addons.ann.items.clone();
    if items.is_empty() {
        ctx.g.text("No announcements yet.", x, y + 2, gfx::FAINT);
        y += 16;
    }
    for it in &items {
        y = ann_item(c, ctx, it, x, y, w) + 8;
    }
    ctx.content = ctx.content.max(y - top);
}

fn ann_when(typed: &str) -> Result<Option<i64>, ()> {
    use chrono::{Local, NaiveDateTime, TimeZone};
    let t = typed.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let n = NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M").map_err(|_| ())?;
    Local.from_local_datetime(&n).earliest().map(|d| Some(d.timestamp_millis())).ok_or(())
}

fn ann_composer(c: &mut Core, ctx: &mut Ctx, x: i32, mut y: i32, w: i32) -> i32 {
    y = ctx.section("NEW ANNOUNCEMENT", x, y, w);
    let title = input(ctx, "ann.title", x, y, w, "Title", 80);
    y += 22;
    input(ctx, "ann.body", x, y, w, "What's happening?", 1000);
    y += 22;
    let when = input(ctx, "ann.when", x, y, 200.min(w), "When (optional): 2026-11-02 19:00", 16);
    y += 24;
    if let Some((name, _)) = c.server.clone() {
        let t = ctx.g.clip(&format!("Invite people to {name}"), w - 40);
        ctx.g.text(&t, x, y + 4, gfx::TEXT);
        ctx.toggle(x + w - 30, y + 1, c.addons.ann.invite_here, Action::Addon(AddonAction::AnnInviteHere));
        y += 22;
    }
    let error = ann_when(&when).is_err();
    if error {
        ctx.g.text("Write the time like 2026-11-02 19:00", x, y + 5, gfx::RED);
    }
    let ok = !title.trim().is_empty() && !error;
    ctx.styled(x + w - 80, y, 80, 18, "Post", if ok { Style::Primary } else { Style::Normal }, ok.then_some(Action::Addon(AddonAction::AnnPost)));
    y + 24
}

fn ann_item(c: &Core, ctx: &mut Ctx, it: &AnnItem, x: i32, mut y: i32, w: i32) -> i32 {
    let start = y;
    let accent = if it.trust == AnnTrust::Verified { gfx::GOLD } else { gfx::RED };
    let (tx, tw) = (x + 10, w - 10);
    let t = ctx.g.clip(&it.title, tw - 70);
    ctx.g.text(&t, tx, y, gfx::WHITE);
    if c.st.my_rank.as_deref() == Some("owner") {
        ctx.styled(x + w - 54, y - 3, 54, 14, "Delete", Style::Subtle, Some(Action::Addon(AddonAction::AnnDelete(it.id.clone()))));
    }
    y += 12;
    let by = format!("by {}  \u{b7}  {}", it.author_name.clone().unwrap_or_else(|| "the owner".into()), fmt_time(it.created, "%b %-d, %-I:%M %p"));
    let by = ctx.g.clip(&by, tw);
    ctx.g.text(&by, tx, y, gfx::FAINT);
    y += 12;
    match it.trust {
        AnnTrust::Verified => ctx.g.text("\u{2714} Signed by the VCraft owner", tx, y, gfx::LIGHT_GREEN),
        AnnTrust::KeyChanged => {
            let s = ctx.g.clip("\u{26a0} Signed with a different key than before", tw - 90);
            ctx.g.text(&s, tx, y, gfx::GOLD);
            ctx.styled(x + w - 84, y - 3, 84, 14, "Trust new key", Style::Normal, Some(Action::Addon(AddonAction::AnnTrust(it.id.clone()))));
        }
        AnnTrust::Invalid => ctx.g.text("\u{2716} Signature doesn't match: this may not be from the owner", tx, y, gfx::RED),
    }
    y += 14;
    if !it.body.trim().is_empty() {
        y = ctx.g.para(&it.body, tx, y, tw, gfx::TEXT) + 4;
    }
    if let Some(when) = it.when {
        ctx.g.text(&format!("When: {}", fmt_time(when, "%a, %b %-d at %-I:%M %p")), tx, y, gfx::GOLD);
        y += 14;
    }
    if let (Some(addr), true) = (&it.server_address, it.trust != AnnTrust::Invalid) {
        let cw = 260.min(tw);
        ctx.g.panel(tx, y, cw, 26, gfx::PANEL);
        let n = ctx.g.clip(it.server_name.as_deref().unwrap_or(addr), cw - 60);
        ctx.g.text(&n, tx + 6, y + 4, gfx::WHITE);
        let a = ctx.g.clip(addr, cw - 60);
        ctx.g.text(&a, tx + 6, y + 15, gfx::MUTED);
        ctx.styled(tx + cw - 46, y + 4, 40, 18, "Join", Style::Primary, Some(Action::Addon(AddonAction::AnnJoin(it.server_name.clone(), addr.clone()))));
        y += 30;
    }
    if it.trust != AnnTrust::Invalid {
        let label = if it.me {
            format!("\u{2714} Going ({})", it.going)
        } else if it.going > 0 {
            format!("I'm going ({})", it.going)
        } else {
            "I'm going".into()
        };
        ctx.styled(tx, y, 100, 16, &label, if it.me { Style::Primary } else { Style::Normal }, Some(Action::Addon(AddonAction::AnnRsvp(it.id.clone(), !it.me))));
        y += 20;
    }
    ctx.g.fill(x, start - 2, 2, y - start, accent);
    y
}

fn rules_page(c: &mut Core, ctx: &mut Ctx, x: i32, mut y: i32, w: i32) {
    let top = y;
    let conv = c.addons.rules.conv.clone();
    let Some(g) = conv.as_deref().and_then(|cv| c.group(cv)).cloned() else {
        ctx.g.text("Open a group chat, then click the rules button.", x, y, gfx::MUTED);
        return;
    };
    let r = c.addons.rules.rules.get(&g.id).cloned();
    let me = c.me();
    let owner = Some(&g.owner) == me.as_ref();
    let admin = c.rules_is_admin(&g, me.as_deref());
    ctx.styled(x, y - 2, 70, 16, "< Back", Style::Subtle, Some(Action::Addon(AddonAction::RulesBack)));
    y += 20;
    match r.as_ref().and_then(|r| r.policy.clone()).filter(|p| p.version != 0) {
        Some(p) => {
            y = rules_summary(c, ctx, r.as_ref().unwrap(), &p, x, y, w, owner || admin);
            if p.require_accept && !p.accepted && !owner {
                ctx.styled(x, y, 140, 20, "I accept the rules", Style::Primary, Some(Action::Addon(AddonAction::RulesAccept(g.id.clone()))));
                y += 28;
            }
        }
        None => {
            y = ctx.g.para(if owner { "This group has no rules yet. Set them up below." } else { "This group has no rules." }, x, y, w, gfx::MUTED) + 8;
        }
    }
    if owner {
        y = rules_editor(c, ctx, &g, r.as_ref(), x, y + 4, w);
    }
    if admin {
        y = rules_moderation(c, ctx, &g, r.as_ref(), x, y + 4, w, owner);
    }
    ctx.content = ctx.content.max(y - top);
}

#[allow(clippy::too_many_arguments)]
fn rules_summary(c: &Core, ctx: &mut Ctx, r: &GroupRules, p: &Policy, x: i32, mut y: i32, w: i32, staff: bool) -> i32 {
    y = ctx.section("RULES", x, y, w);
    if let Some(problem) = &r.problem {
        y = ctx.g.para(problem, x, y, w, gfx::RED) + 4;
    }
    if let Some(content) = &r.content {
        ctx.g.text("\u{2714} Signed by the group owner", x, y, gfx::LIGHT_GREEN);
        y += 14;
        if !content.text.trim().is_empty() {
            y = ctx.g.para(&content.text, x, y, w, gfx::TEXT) + 6;
        }
        let n = content.keywords.len();
        if n > 0 {
            let how = match content.mode {
                Mode::Hide => "Messages using them are hidden.",
                Mode::Mask => "They're replaced with ***.",
                Mode::Block => "Messages using them can't be sent.",
            };
            let words = if staff { content.keywords.join(", ") } else { format!("{n} word{}", if n == 1 { "" } else { "s" }) };
            y = ctx.g.para(&format!("Filtered: {words}. {how}"), x, y, w, gfx::MUTED) + 4;
        }
    }
    let mut sb = String::new();
    if p.slow_seconds > 0 {
        sb.push_str(&format!("Slow mode: one message every {}. ", slow(p.slow_seconds)));
    }
    if p.admins_only {
        sb.push_str("Only admins can post. ");
    }
    if p.require_accept {
        sb.push_str(if p.accepted { "You've accepted the rules. " } else { "You need to accept the rules to post. " });
    }
    let now = now_ms();
    if let Some(until) = c.me().and_then(|me| p.mutes.get(&me).copied()).filter(|u| *u > now) {
        sb.push_str(&format!("You're muted for {} more min. ", (until - now) / 60_000 + 1));
    }
    if !sb.is_empty() {
        y = ctx.g.para(sb.trim(), x, y, w, gfx::GOLD) + 4;
    }
    y + 6
}

fn rules_editor(c: &mut Core, ctx: &mut Ctx, g: &crate::state::Group, r: Option<&GroupRules>, x: i32, mut y: i32, w: i32) -> i32 {
    if !c.addons.rules.editor_loaded {
        c.addons.rules.editor_loaded = true;
        let content = r.and_then(|r| r.content.clone());
        let policy = r.and_then(|r| r.policy.clone());
        ctx.set_field("grule.text", content.as_ref().map_or("", |c| c.text.as_str()));
        ctx.set_field("grule.words", &content.as_ref().map_or(String::new(), |c| c.keywords.join(", ")));
        let ed = &mut c.addons.rules;
        ed.mode = Some(content.as_ref().map_or(Mode::Mask, |c| c.mode));
        ed.slow_seconds = policy.as_ref().map_or(0, |p| p.slow_seconds);
        ed.admins_only = policy.as_ref().is_some_and(|p| p.admins_only);
        ed.require_accept = policy.as_ref().is_some_and(|p| p.require_accept);
        ed.admins = policy.map(|p| p.admins).unwrap_or_default();
    }
    y = ctx.section("EDIT RULES", x, y, w);
    ctx.g.text("Rules text (members see this)", x, y, gfx::MUTED);
    input(ctx, "grule.text", x, y + 11, w, "Be kind. No spam.", 500);
    y += 34;
    ctx.g.text("Filtered words, separated by commas", x, y, gfx::MUTED);
    input(ctx, "grule.words", x, y + 11, w, "word, another word", 1500);
    y += 34;
    ctx.g.text("Filtered words are", x, y + 5, gfx::TEXT);
    let bx = x + w - 210;
    let mode = c.addons.rules.mode.unwrap_or(Mode::Mask);
    for (i, (label, m)) in [("Hidden", Mode::Hide), ("Masked ***", Mode::Mask), ("Blocked", Mode::Block)].into_iter().enumerate() {
        ctx.styled(bx + 70 * i as i32, y, 66, 18, label, if mode == m { Style::Primary } else { Style::Normal }, Some(Action::Addon(AddonAction::RulesMode(m))));
    }
    y += 24;
    ctx.g.text("Slow mode", x, y + 5, gfx::TEXT);
    ctx.styled(x + w - 120, y, 22, 18, "-", Style::Normal, Some(Action::Addon(AddonAction::RulesSlow(-1))));
    let ss = c.addons.rules.slow_seconds;
    ctx.g.centered(&if ss == 0 { "Off".to_owned() } else { slow(ss) }, x + w - 60, y + 5, gfx::WHITE);
    ctx.styled(x + w - 22, y, 22, 18, "+", Style::Normal, Some(Action::Addon(AddonAction::RulesSlow(1))));
    y += 24;
    ctx.g.text("Only admins can post", x, y + 3, gfx::TEXT);
    ctx.toggle(x + w - 30, y, c.addons.rules.admins_only, Action::Addon(AddonAction::RulesAdminsOnly));
    y += 20;
    ctx.g.text("Members must accept the rules to post", x, y + 3, gfx::TEXT);
    ctx.toggle(x + w - 30, y, c.addons.rules.require_accept, Action::Addon(AddonAction::RulesRequireAccept));
    y += 24;
    ctx.g.text("Admins", x, y, gfx::MUTED);
    y += 12;
    for m in &g.members {
        if m.uuid == g.owner {
            continue;
        }
        ctx.g.face(&m.uuid, &m.name, x, y, 12);
        let n = ctx.g.clip(&m.name, w - 60);
        ctx.g.text(&n, x + 18, y + 2, gfx::TEXT);
        let on = c.addons.rules.admins.contains(&m.uuid);
        ctx.toggle(x + w - 30, y, on, Action::Addon(AddonAction::RulesAdmin(m.uuid.clone())));
        y += 18;
    }
    y += 6;
    ctx.styled(x + w - 100, y, 100, 20, "Save rules", Style::Primary, Some(Action::Addon(AddonAction::RulesSave(g.id.clone()))));
    y = ctx.g.para(
        "Words are filtered by every member's VCraft, because messages are end-to-end encrypted and the relay can't read them. Members without the Group Rules addon don't filter.",
        x,
        y + 26,
        w - 4,
        gfx::FAINT,
    );
    y + 8
}

#[allow(clippy::too_many_arguments)]
fn rules_moderation(c: &Core, ctx: &mut Ctx, g: &crate::state::Group, r: Option<&GroupRules>, x: i32, mut y: i32, w: i32, owner: bool) -> i32 {
    y = ctx.section("MEMBERS", x, y, w);
    let now = now_ms();
    let me = c.me();
    for m in &g.members {
        if Some(&m.uuid) == me.as_ref() || m.uuid == g.owner || (!owner && c.rules_is_admin(g, Some(&m.uuid))) {
            continue;
        }
        ctx.g.face(&m.uuid, &m.name, x, y, 14);
        let n = ctx.g.clip(&m.name, w - 170);
        ctx.g.text(&n, x + 20, y + 3, gfx::TEXT);
        let muted = r.and_then(|r| r.policy.as_ref()).and_then(|p| p.mutes.get(&m.uuid)).is_some_and(|u| *u > now);
        let (label, mins) = if muted { ("Unmute", 0) } else { ("Mute 10 min", 10) };
        ctx.styled(x + w - 150, y, 90, 16, label, Style::Normal, Some(Action::Addon(AddonAction::RulesMute(g.id.clone(), m.uuid.clone(), mins))));
        ctx.styled(x + w - 54, y, 54, 16, "Kick", Style::Danger, Some(Action::Addon(AddonAction::RulesKick(g.id.clone(), m.uuid.clone()))));
        y += 20;
    }
    y + 6
}

pub fn run(c: &mut Core, a: AddonAction) {
    let field = |c: &Core, id: &str| c.ui.fields.get(id).map(|f| f.value.clone()).unwrap_or_default();
    match a {
        AddonAction::ProxEnabled(on) => c.proximity_set_enabled(on),
        AddonAction::ProxRange(r) => {
            c.addons.prox.range = r.clamp(MIN_RANGE, MAX_RANGE);
            c.prox_save();
        }
        AddonAction::AnnPopups(on) => {
            c.addons.ann.popups = on;
            c.ann_save_settings();
        }
        AddonAction::AnnPost => {
            let (title, body, when) = (field(c, "ann.title"), field(c, "ann.body"), field(c, "ann.when"));
            let Ok(when) = ann_when(&when) else { return };
            if title.trim().is_empty() {
                return;
            }
            let here = c.addons.ann.invite_here && c.server.is_some();
            c.ann_post(title.trim(), body.trim(), when, here);
            for id in ["ann.title", "ann.body", "ann.when"] {
                c.ui.fields.entry(id.into()).or_default().value.clear();
            }
            c.addons.ann.invite_here = false;
        }
        AddonAction::AnnDelete(id) => relay_send("ann.delete", json!({ "id": id })),
        AddonAction::AnnRsvp(id, going) => relay_send("ann.rsvp", json!({ "id": id, "going": going })),
        AddonAction::AnnTrust(id) => {
            if let Some(it) = c.addons.ann.items.iter().find(|i| i.id == id).cloned() {
                if let Some(p) = it.pub_key {
                    c.addons.ann.pins.insert(it.author, p);
                    c.ann_save_pins();
                    relay_send("ann.list", json!({}));
                }
            }
        }
        AddonAction::AnnInviteHere => c.addons.ann.invite_here = !c.addons.ann.invite_here,
        AddonAction::AnnJoin(name, address) => screen::confirm_join_server(c, name, address),
        AddonAction::RulesBack => {
            if let Some(conv) = c.addons.rules.conv.clone() {
                screen::run(c, Action::OpenChat(conv));
            }
        }
        AddonAction::RulesAccept(g) => {
            if let Some(p) = c.addons.rules.rules.get(&g).and_then(|r| r.policy.clone()) {
                relay_send("grule.accept", json!({ "group": g, "version": p.accept_version }));
            }
        }
        AddonAction::RulesMode(m) => c.addons.rules.mode = Some(m),
        AddonAction::RulesSlow(d) => c.addons.rules.slow_seconds = step(c.addons.rules.slow_seconds, d),
        AddonAction::RulesAdminsOnly => c.addons.rules.admins_only = !c.addons.rules.admins_only,
        AddonAction::RulesRequireAccept => c.addons.rules.require_accept = !c.addons.rules.require_accept,
        AddonAction::RulesAdmin(uuid) => {
            let admins = &mut c.addons.rules.admins;
            match admins.iter().position(|a| *a == uuid) {
                Some(i) => {
                    admins.remove(i);
                }
                None => admins.push(uuid),
            }
        }
        AddonAction::RulesSave(g) => {
            let (text, words) = (field(c, "grule.text"), field(c, "grule.words"));
            c.rules_save(&g, text.trim(), &words);
        }
        AddonAction::RulesMute(g, uuid, minutes) => relay_send("grule.mute", json!({ "group": g, "uuid": uuid, "minutes": minutes })),
        AddonAction::RulesKick(g, uuid) => relay_send("grule.kick", json!({ "group": g, "uuid": uuid })),
    }
    let _ = (LOBBY, Trust::None, Effect::Ding);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proximity_math() {
        assert_eq!(gain(3.0, 48), 1.0);
        assert_eq!(gain(48.0, 48), 0.0);
        let mid = gain(27.0, 48);
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        assert_eq!(room_key("Play.Example.com:25565"), room_key("play.example.com."));
        assert!(room_key("x").starts_with("mc:") && room_key("x").len() == 43);
    }

    #[test]
    fn keywords_and_masking() {
        assert_eq!(parse_keywords(" Foo, bar ,foo,, "), vec!["foo", "bar"]);
        let re = pattern(&["bad word".into(), "x.y".into()]);
        assert!(re.is_match("a BAD WORD here"));
        assert!(!re.is_match("xzy"));
        assert_eq!(step(0, 1), 5);
        assert_eq!(step(900, 1), 900);
        assert_eq!(step(7, -1), 5);
    }
}
