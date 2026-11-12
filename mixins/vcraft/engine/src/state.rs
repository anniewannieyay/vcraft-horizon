//! What the relay says, kept for the UI (`State`): friends, groups,
//! conversations, invites, the call and the room, presence and songs; and
//! everything the client sends back.

use crate::crypto::{self, Trust};
use crate::util::{b, clean_song, i, now_ms, s};
use crate::{Core, Effect};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const STATUSES: [&str; 4] = ["online", "idle", "dnd", "invisible"];
pub const LOBBY: &str = "lobby";

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub uuid: String,
    pub name: String,
    pub status: String,
}
impl Profile {
    pub fn online(&self) -> bool {
        self.status != "offline"
    }
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub members: Vec<Profile>,
}
impl Group {
    pub fn conv(&self) -> String {
        format!("g:{}", self.id)
    }
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ServerInvite {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: Option<String>,
    pub from: String,
    pub from_name: String,
    pub text: String,
    pub ts: i64,
    pub secure: bool,
    pub reply: Option<Reply>,
    pub invite: Option<ServerInvite>,
    pub system: bool,
    pub song: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Invite {
    pub call_id: String,
    pub from: String,
    pub from_name: String,
    pub expires: i64,
}

#[derive(Clone, Debug)]
pub struct CallMember {
    pub uuid: String,
    pub name: String,
    pub muted: bool,
    pub deafened: bool,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub id: String,
    pub members: Vec<CallMember>,
    pub ringing: Vec<Profile>,
}

#[derive(Clone, Debug)]
pub struct Room {
    pub key: String,
    pub id: String,
    pub members: Vec<CallMember>,
}

#[derive(Default)]
pub struct State {
    pub my_uuid: Option<String>,
    pub my_name: Option<String>,
    pub my_status: String,
    pub my_rank: Option<String>,
    pub ranks: HashMap<String, String>,
    pub lobby_online: i64,
    pub lobby_muted: bool,
    pub friends: Vec<Profile>,
    pub incoming: Vec<Profile>,
    pub outgoing: Vec<Profile>,
    pub blocked: Vec<Profile>,
    pub groups: Vec<Group>,
    pub conversations: HashMap<String, Vec<Message>>,
    pub unread: HashMap<String, i64>,
    pub invites: BTreeMap<u64, Invite>,
    invite_seq: u64,
    pub listening: HashMap<String, String>,
    shared_song: Option<String>,
    listening_synced: bool,
    next_listening_check: i64,
    pub call: Option<Call>,
    pub room: Option<Room>,
    wanted_room: Option<String>,
    pub viewing: Option<String>,
    pub focus_request: Option<String>,
    pub muted: bool,
    pub deafened: bool,
    pub notice: Option<(String, bool, i64)>,
    ringing: bool,
    pub revision: u64,
}

fn profiles(core: &mut Core, arr: Option<&Value>) -> Vec<Profile> {
    let mut out = Vec::new();
    for j in arr.and_then(Value::as_array).into_iter().flatten() {
        let uuid = s(j, "uuid").unwrap_or_default();
        core.set_rank(Some(&uuid), s(j, "rank").as_deref());
        crate::with_secure(|sec| sec.observe(Some(&uuid), s(j, "key").as_deref()));
        let status = if j.get("status").is_some() { s(j, "status").unwrap_or_else(|| "offline".into()) } else { "offline".into() };
        out.push(Profile { uuid, name: s(j, "name").unwrap_or_default(), status });
    }
    out
}

fn sorted(mut list: Vec<Profile>) -> Vec<Profile> {
    list.sort_by(|a, b| (!a.online(), a.name.to_lowercase()).cmp(&(!b.online(), b.name.to_lowercase())));
    list
}

fn members(arr: Option<&Value>) -> Vec<(CallMember, Option<String>, Option<String>)> {
    arr.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| {
            (
                CallMember { uuid: s(m, "uuid").unwrap_or_default(), name: s(m, "name").unwrap_or_default(), muted: b(m, "muted"), deafened: b(m, "deafened") },
                s(m, "key"),
                s(m, "rank"),
            )
        })
        .collect()
}

impl Core {
    pub fn me(&self) -> Option<String> {
        self.st.my_uuid.clone()
    }

    pub fn changed(&mut self) {
        self.st.revision += 1;
    }

    pub fn reset(&mut self) {
        self.prepare_rejoin();
        self.social.activity.clear();
        self.social.requests.clear();
        self.social.waiting_join.clear();
        let st = &mut self.st;
        st.my_uuid = None;
        st.my_name = None;
        st.friends.clear();
        st.incoming.clear();
        st.outgoing.clear();
        st.blocked.clear();
        st.groups.clear();
        st.conversations.clear();
        st.unread.clear();
        st.invites.clear();
        st.ranks.clear();
        st.listening.clear();
        st.listening_synced = false;
        st.my_rank = None;
        st.lobby_online = 0;
        self.set_call(None);
        self.secure_on_call();
        self.st.room = None;
        self.secure_on_room();
        self.update_voice();
        self.update_ringing();
        self.changed();
    }

    pub fn send_json(&self, o: Value) {
        crate::relay().send(&o);
    }

    pub fn send(&self, t: &str, key: Option<&str>, value: Option<&str>) {
        let mut o = Map::new();
        o.insert("t".into(), t.into());
        if let Some(k) = key {
            o.insert(k.into(), value.map_or(Value::Null, |v| Value::String(v.into())));
        }
        self.send_json(Value::Object(o));
    }

    fn set_rank(&mut self, uuid: Option<&str>, rank: Option<&str>) {
        let Some(uuid) = uuid else { return };
        match rank.filter(|r| !r.is_empty()) {
            Some(r) => {
                self.st.ranks.insert(uuid.to_owned(), r.to_owned());
            }
            None => {
                self.st.ranks.remove(uuid);
            }
        }
    }

    pub fn rank(&self, uuid: Option<&str>) -> Option<String> {
        self.st.ranks.get(uuid?).cloned()
    }

    pub fn is_staff(&self) -> bool {
        matches!(self.st.my_rank.as_deref(), Some("owner" | "mod"))
    }

    pub fn handle(&mut self, o: &Value) {
        let t = s(o, "t").unwrap_or_default();
        match t.as_str() {
            "welcome" => {
                self.st.my_uuid = s(o, "uuid");
                self.st.my_name = s(o, "name");
                self.st.my_status = s(o, "status").unwrap_or_else(|| "online".into());
                let me = self.st.my_uuid.clone();
                crate::with_secure(|sec| {
                    let k = sec.public_key();
                    sec.observe(me.as_deref(), k.as_deref())
                });
                self.st.my_rank = s(o, "rank");
                let rank = self.st.my_rank.clone();
                self.set_rank(me.as_deref(), rank.as_deref());
                if self.st.viewing.as_deref() == Some(LOBBY) {
                    self.send("lobby_join", None, None);
                }
                self.send_voice_state();
                self.st.listening_synced = false;
                if let Some(r) = self.st.wanted_room.clone() {
                    self.send("room.join", Some("key"), Some(&r));
                }
                self.addons_connected();
            }
            "status" => self.st.my_status = s(o, "status").unwrap_or_else(|| "online".into()),
            "rank" => {
                self.st.my_rank = s(o, "rank");
                let (me, rank) = (self.st.my_uuid.clone(), self.st.my_rank.clone());
                self.set_rank(me.as_deref(), rank.as_deref());
            }
            "lobby_history" => {
                let mut list = Vec::new();
                for e in o["messages"].as_array().into_iter().flatten() {
                    let m = self.lobby_message(e);
                    if let Some(m) = self.addons_filter(LOBBY, m) {
                        list.push(m);
                    }
                }
                self.st.conversations.insert(LOBBY.into(), list);
                self.st.lobby_muted = b(o, "muted");
            }
            "lobby_msg" => {
                let m = self.lobby_message(o);
                let Some(m) = self.addons_filter(LOBBY, m) else { return };
                self.addons_message(LOBBY, &m);
                let from_me = Some(&m.from) == self.st.my_uuid.as_ref();
                let list = self.st.conversations.entry(LOBBY.into()).or_default();
                list.push(m);
                if list.len() > 300 {
                    let excess = list.len() - 300;
                    list.drain(..excess);
                }
                if !from_me && self.st.viewing.as_deref() != Some(LOBBY) {
                    *self.st.unread.entry(LOBBY.into()).or_default() += 1;
                }
            }
            "lobby_deleted" => {
                let id = s(o, "id");
                if let Some(list) = self.st.conversations.get_mut(LOBBY) {
                    list.retain(|m| m.id != id);
                }
            }
            "lobby_count" => self.st.lobby_online = i(o, "online"),
            "friends" => {
                let f = profiles(self, o.get("friends"));
                let gone: Vec<String> = f.iter().filter(|n| !n.online() && self.friend(&n.uuid).is_some_and(|p| p.online())).map(|n| n.uuid.clone()).collect();
                for u in gone {
                    self.note_presence(&u, true, "offline");
                }
                self.st.friends = sorted(f);
                self.st.incoming = profiles(self, o.get("incoming"));
                self.st.outgoing = profiles(self, o.get("outgoing"));
                self.st.blocked = profiles(self, o.get("blocked"));
                if !self.st.listening_synced {
                    self.st.listening_synced = true;
                    self.st.shared_song = self.shared_song();
                    let _ = self.activity_changed();
                    let to = self.online_friends();
                    self.send_listening(&to);
                }
            }
            "groups" => {
                let mut list = Vec::new();
                for g in o["groups"].as_array().into_iter().flatten() {
                    let members = profiles(self, g.get("members"));
                    list.push(Group { id: s(g, "id").unwrap_or_default(), name: s(g, "name").unwrap_or_default(), owner: s(g, "owner").unwrap_or_default(), members });
                }
                list.sort_by_key(|g| g.name.to_lowercase());
                self.st.groups = list;
            }
            "presence" => self.update_presence(&s(o, "uuid").unwrap_or_default(), &s(o, "status").unwrap_or_else(|| "offline".into())),
            "activity" => {
                let from = s(o, "from").unwrap_or_default();
                let me = self.me().unwrap_or_default();
                let opened = crate::with_secure(|sec| sec.open(&from, &crypto::activity_aad(&from, &me), s(o, "body").as_deref()));
                let (Some(json), true) = (opened, self.friend(&from).is_some()) else { return };
                let env = serde_json::from_str::<Value>(&json).unwrap_or(Value::Null);
                let song = clean_song(s(&env, "song").as_deref(), 120);
                match song {
                    Some(song) => self.st.listening.insert(from.clone(), song),
                    None => self.st.listening.remove(&from),
                };
                self.on_activity(&from, &env);
            }
            "friend_request" => self.notify_user("Friend request", &format!("{} wants to be friends", s(o, "name").unwrap_or_default()), None),
            "friend_added" => self.notify_user("New friend", &format!("{} accepted your request", s(o, "name").unwrap_or_default()), None),
            "dm" => {
                let from = s(o, "from").unwrap_or_default();
                let to = s(o, "to").unwrap_or_default();
                let other = if Some(&from) == self.st.my_uuid.as_ref() { to.clone() } else { from.clone() };
                let opened = crate::with_secure(|sec| sec.open(&other, &crypto::dm_aad(&from, &to), s(o, "text").as_deref()));
                let m = self.decode(o, opened);
                self.add(&other, m);
            }
            "gmsg" => {
                let group = s(o, "group").unwrap_or_default();
                let from = s(o, "from").unwrap_or_default();
                let me = self.me().unwrap_or_default();
                let opened = crate::with_secure(|sec| sec.open(&from, &crypto::group_aad(&group, &from, &me), s(o, "text").as_deref()));
                let m = self.decode(o, opened);
                self.add(&format!("g:{group}"), m);
            }
            "history" => {
                let group = s(o, "group");
                let conv = match &group {
                    Some(g) => format!("g:{g}"),
                    None => s(o, "with").unwrap_or_default(),
                };
                let me = self.me().unwrap_or_default();
                let mut list = Vec::new();
                for mx in o["messages"].as_array().into_iter().flatten() {
                    let from = s(mx, "from").unwrap_or_default();
                    let opened = match &group {
                        Some(g) => crate::with_secure(|sec| sec.open(&from, &crypto::group_aad(g, &from, &me), s(mx, "text").as_deref())),
                        None => {
                            let to = s(mx, "to").unwrap_or_default();
                            let peer = if from == me { to.clone() } else { from.clone() };
                            crate::with_secure(|sec| sec.open(&peer, &crypto::dm_aad(&from, &to), s(mx, "text").as_deref()))
                        }
                    };
                    let m = self.decode(mx, opened);
                    if let Some(m) = self.addons_filter(&conv, m) {
                        list.push(m);
                    }
                }
                if let Some(old) = self.st.conversations.get(&conv) {
                    list.extend(old.iter().filter(|m| m.system).cloned());
                }
                list.sort_by_key(|m| m.ts);
                self.st.conversations.insert(conv, list);
            }
            "deleted" => {
                let conv = match s(o, "group") {
                    Some(g) => format!("g:{g}"),
                    None => s(o, "with").unwrap_or_default(),
                };
                let id = s(o, "id");
                if let Some(list) = self.st.conversations.get_mut(&conv) {
                    list.retain(|m| m.id != id);
                }
            }
            "unread" => {
                if let Some(counts) = o["counts"].as_object() {
                    for (k, v) in counts {
                        *self.st.unread.entry(k.clone()).or_default() += v.as_i64().unwrap_or(0);
                    }
                }
                let total: i64 = self.st.unread.values().sum();
                if total > 0 {
                    self.notify_user("VCraft", &format!("{total} unread message{}", if total == 1 { "" } else { "s" }), None);
                }
            }
            "group_notice" => {
                let conv = format!("g:{}", s(o, "group").unwrap_or_default());
                self.st.conversations.entry(conv).or_default().push(Message {
                    id: None,
                    from: String::new(),
                    from_name: String::new(),
                    text: s(o, "text").unwrap_or_default(),
                    ts: i(o, "ts"),
                    secure: true,
                    reply: None,
                    invite: None,
                    system: true,
                    song: None,
                });
            }
            "group_created" => self.st.focus_request = Some(format!("g:{}", s(o, "group").unwrap_or_default())),
            "call_invite" => {
                let inv = Invite {
                    call_id: s(o, "callId").unwrap_or_default(),
                    from: s(o, "from").unwrap_or_default(),
                    from_name: s(o, "fromName").unwrap_or_default(),
                    expires: now_ms() + 60_000,
                };
                if !self.is_blocked(&inv.from) && !self.auto_accept(&inv) {
                    let quiet = self.quiet(Some(&inv.from));
                    let name = self.cfg.display_name(&inv.from, &inv.from_name);
                    let who = crate::dlog::who(&inv.from);
                    self.st.invites.retain(|_, v| v.call_id != inv.call_id);
                    self.st.invite_seq += 1;
                    let seq = self.st.invite_seq;
                    self.st.invites.insert(seq, inv);
                    if !quiet {
                        self.toast(crate::i18n::t("Incoming call"), &crate::i18n::tf("{0} is calling you", &[&name]));
                        self.chat("Call", 0xFF55_FF55, &crate::i18n::tf("{0} is calling you. Type /vc accept or open VCraft.", &[&name]));
                    }
                    crate::dlog::line(&format!("call: invited by {who}"));
                }
            }
            "call" => {
                match o.get("call").filter(|c| !c.is_null()) {
                    Some(j) => {
                        let mut list = Vec::new();
                        for (m, key, rank) in members(j.get("members")) {
                            self.set_rank(Some(&m.uuid), rank.as_deref());
                            crate::with_secure(|sec| sec.observe(Some(&m.uuid), key.as_deref()));
                            list.push(m);
                        }
                        let id = s(j, "id").unwrap_or_default();
                        self.st.invites.retain(|_, v| v.call_id != id);
                        let ringing = profiles(self, j.get("ringing"));
                        if self.st.call.is_none() {
                            crate::dlog::line(&format!("call: joined ({} in it)", list.len()));
                        }
                        self.set_call(Some(Call { id, members: list, ringing }));
                        self.note_call_members();
                    }
                    None => {
                        if self.st.call.is_some() {
                            crate::dlog::line("call: left");
                        }
                        self.set_call(None);
                    }
                }
                self.secure_on_call();
            }
            "export" => self.save_export(o.get("data").cloned().unwrap_or(Value::Null)),
            "account_deleted" => {
                self.cfg.auto_connect = false;
                self.cfg.save();
                self.notice("Your VCraft account and its data were deleted from the relay.", false);
            }
            "admin_result" => {
                let text = s(o, "text").unwrap_or_default();
                self.chat("Admin", 0xFFF0_B232, &text);
                self.notice(&text, false);
            }
            "call_rekey" => {
                let me = self.me();
                let out = crate::with_secure(|sec| sec.rekey(false, false, me.as_deref()));
                out.into_iter().for_each(|m| self.send_json(m));
            }
            "call_cancel" => {
                let id = s(o, "callId");
                self.st.invites.retain(|_, v| Some(&v.call_id) != id.as_ref());
            }
            "call_key" => {
                let me = self.me();
                crate::with_secure(|sec| {
                    sec.on_key(false, &s(o, "from").unwrap_or_default(), s(o, "callId").as_deref(), i(o, "epoch") as u8, &s(o, "key").unwrap_or_default(), me.as_deref())
                });
            }
            "room.state" => {
                match o.get("room").filter(|r| !r.is_null()) {
                    Some(j) => {
                        let mut list = Vec::new();
                        for (m, key, _) in members(j.get("members")) {
                            crate::with_secure(|sec| sec.observe(Some(&m.uuid), key.as_deref()));
                            list.push(m);
                        }
                        self.st.room = Some(Room { key: s(j, "key").unwrap_or_default(), id: s(j, "id").unwrap_or_default(), members: list });
                    }
                    None => self.st.room = None,
                }
                self.secure_on_room();
                self.update_voice();
            }
            "room.key" => {
                let me = self.me();
                crate::with_secure(|sec| {
                    sec.on_key(true, &s(o, "from").unwrap_or_default(), s(o, "room").as_deref(), i(o, "epoch") as u8, &s(o, "key").unwrap_or_default(), me.as_deref())
                });
            }
            "room.rekey" => {
                let me = self.me();
                let out = crate::with_secure(|sec| sec.rekey(true, false, me.as_deref()));
                out.into_iter().for_each(|m| self.send_json(m));
            }
            "info" => self.notice(&s(o, "text").unwrap_or_default(), false),
            "error" => {
                let text = s(o, "text").unwrap_or_default();
                self.notice(&text, true);
                self.toast("VCraft", &text);
            }
            other => self.addons_dispatch(other, o),
        }
        self.update_ringing();
        self.changed();
    }

    fn add(&mut self, conv: &str, m: Message) {
        let Some(m) = self.addons_filter(conv, m) else { return };
        self.addons_message(conv, &m);
        let from_me = Some(&m.from) == self.st.my_uuid.as_ref();
        let (from, from_name) = (m.from.clone(), self.cfg.display_name(&m.from, &m.from_name));
        let group_muted = self.group_muted(conv);
        let preview = match (&m.invite, &m.song) {
            (Some(inv), _) => format!("invited you to {}", inv.name),
            (None, Some(song)) => format!("is listening to {song}"),
            _ => m.text.clone(),
        };
        self.st.conversations.entry(conv.to_owned()).or_default().push(m);
        if !from_me && self.st.viewing.as_deref() != Some(conv) {
            *self.st.unread.entry(conv.to_owned()).or_default() += 1;
            let where_ = conv.starts_with("g:").then(|| self.conv_name(conv));
            let title = match &where_ {
                Some(w) => crate::i18n::tf("{0} in {1}", &[&from_name, w]),
                None => from_name.clone(),
            };
            if group_muted {
                return;
            }
            self.notify_user(&title, &preview, Some(&from));
            if !self.quiet(Some(&from)) {
                self.chat(where_.as_deref().unwrap_or("DM"), 0xFFFF_77FF, &format!("{from_name}: {preview}"));
                if self.cfg.message_sound {
                    self.fx.push(Effect::Ding);
                }
            }
        }
    }

    fn decode(&mut self, o: &Value, opened: Option<String>) -> Message {
        if o.get("fromRank").is_some() {
            self.set_rank(s(o, "from").as_deref(), s(o, "fromRank").as_deref());
        }
        let (mut reply, mut invite, mut song) = (None, None, None);
        let ok = opened.is_some();
        let text = match opened.and_then(|j| serde_json::from_str::<Value>(&j).ok()) {
            None => "This message couldn't be decrypted.".to_owned(),
            Some(env) => {
                if let Some(r) = env.get("reply").filter(|r| r.is_object()) {
                    reply = Some(Reply { id: s(r, "id").unwrap_or_default(), name: s(r, "name").unwrap_or_default(), text: s(r, "text").unwrap_or_default() });
                }
                if let Some(v) = env.get("invite").filter(|r| r.is_object()) {
                    invite = Some(ServerInvite { name: s(v, "name").unwrap_or_default(), address: s(v, "address").unwrap_or_default() });
                }
                if env.get("song").is_some() {
                    song = clean_song(s(&env, "song").as_deref(), 120);
                }
                s(&env, "text").unwrap_or_default()
            }
        };
        Message {
            id: s(o, "id"),
            from: s(o, "from").unwrap_or_default(),
            from_name: s(o, "fromName").unwrap_or_default(),
            text,
            ts: i(o, "ts"),
            secure: ok,
            reply,
            invite,
            system: false,
            song,
        }
    }

    fn lobby_message(&mut self, o: &Value) -> Message {
        self.set_rank(s(o, "from").as_deref(), s(o, "fromRank").as_deref());
        let reply = o.get("reply").filter(|r| r.is_object()).map(|r| Reply {
            id: s(r, "id").unwrap_or_default(),
            name: s(r, "name").unwrap_or_default(),
            text: s(r, "text").unwrap_or_default(),
        });
        Message {
            id: s(o, "id"),
            from: s(o, "from").unwrap_or_default(),
            from_name: s(o, "fromName").unwrap_or_default(),
            text: s(o, "text").unwrap_or_default(),
            ts: i(o, "ts"),
            secure: true,
            reply,
            invite: None,
            system: false,
            song: None,
        }
    }

    fn update_presence(&mut self, uuid: &str, status: &str) {
        let before = self.friend(uuid).cloned();
        self.note_presence(uuid, before.as_ref().is_some_and(Profile::online), status);
        if status == "offline" {
            self.st.listening.remove(uuid);
        } else if before.as_ref().is_some_and(|b| !b.online()) && (self.st.shared_song.is_some() || self.cfg.share_server) {
            self.send_listening(&[uuid.to_owned()]);
        }
        let fix = |p: &Profile| if p.uuid == uuid { Profile { uuid: uuid.to_owned(), name: p.name.clone(), status: status.to_owned() } } else { p.clone() };
        let f: Vec<Profile> = self.st.friends.iter().map(fix).collect();
        self.st.friends = sorted(f);
        for g in &mut self.st.groups {
            g.members = g.members.iter().map(fix).collect();
        }
    }

    fn set_call(&mut self, new_call: Option<Call>) {
        let before = self.st.call.take();
        let had = before.is_some();
        let has = new_call.is_some();
        self.st.call = new_call;
        self.update_voice();
        self.addons_call(had, has);
        if had && !has && self.st.room.is_some() {
            let me = self.me();
            let out = crate::with_secure(|sec| sec.rekey(true, true, me.as_deref()));
            out.into_iter().for_each(|m| self.send_json(m));
            self.send("room.rekey", None, None);
        }
    }

    fn secure_on_call(&mut self) {
        let me = self.me();
        let (id, members): (Option<String>, HashSet<String>) = match &self.st.call {
            Some(c) => (Some(c.id.clone()), c.members.iter().map(|m| m.uuid.clone()).collect()),
            None => (None, HashSet::new()),
        };
        let out = crate::with_secure(|sec| sec.update(false, id.as_deref(), members, me.as_deref()));
        out.into_iter().for_each(|m| self.send_json(m));
    }

    fn secure_on_room(&mut self) {
        let me = self.me();
        let (id, members): (Option<String>, HashSet<String>) = match &self.st.room {
            Some(r) => (Some(r.id.clone()), r.members.iter().map(|m| m.uuid.clone()).collect()),
            None => (None, HashSet::new()),
        };
        let out = crate::with_secure(|sec| sec.update(true, id.as_deref(), members, me.as_deref()));
        out.into_iter().for_each(|m| self.send_json(m));
    }

    pub fn voice_active(&self) -> bool {
        self.st.call.is_some() || self.st.room.as_ref().is_some_and(|r| r.members.len() > 1)
    }

    pub fn update_voice(&mut self) {
        self.push_voice_params();
        if self.voice_active() {
            crate::voice().start();
        } else {
            crate::voice().stop();
        }
    }

    pub fn trust_new_key(&mut self, uuid: &str) {
        if !crate::with_secure(|sec| sec.accept_presented_key(uuid)) {
            return;
        }
        self.secure_on_call();
        self.secure_on_room();
        if self.st.call.as_ref().is_some_and(|c| c.members.iter().any(|m| m.uuid == uuid)) {
            self.send("call_rekey", None, None);
        }
        if self.st.room.as_ref().is_some_and(|r| r.members.iter().any(|m| m.uuid == uuid)) {
            self.send("room.rekey", None, None);
        }
        self.changed();
    }

    pub fn join_room(&mut self, key: &str) {
        if self.st.wanted_room.as_deref() != Some(key) {
            self.st.wanted_room = Some(key.to_owned());
            self.send("room.join", Some("key"), Some(key));
        }
    }

    pub fn leave_room(&mut self) {
        if self.st.wanted_room.is_some() || self.st.room.is_some() {
            self.st.wanted_room = None;
            self.send("room.leave", None, None);
            self.st.room = None;
            self.secure_on_room();
            self.update_voice();
            self.changed();
        }
    }

    pub fn wanted_room(&self) -> Option<&str> {
        self.st.wanted_room.as_deref()
    }

    /// Each client tick (`State.tick`).
    pub fn state_tick(&mut self) {
        let now = now_ms();
        let before = self.st.invites.len();
        self.st.invites.retain(|_, i| i.expires >= now);
        if self.st.invites.len() != before {
            self.changed();
        }
        self.update_ringing();
        self.addons_tick();
        self.social_tick();
        if now >= self.st.next_listening_check && self.st.listening_synced && crate::relay().connected() {
            self.st.next_listening_check = now + 2000;
            self.st.shared_song = self.shared_song();
            if self.activity_changed() {
                let to = self.online_friends();
                self.send_listening(&to);
            }
        }
    }

    /// The song being shared with friends right now.
    pub fn shared_song_now(&self) -> Option<String> {
        self.st.shared_song.clone()
    }

    fn shared_song(&self) -> Option<String> {
        if self.cfg.share_listening && self.st.my_status != "invisible" && self.now_playing_sharing() {
            self.now_playing_current()
        } else {
            None
        }
    }

    fn online_friends(&self) -> Vec<String> {
        self.st.friends.iter().filter(|p| p.online()).map(|p| p.uuid.clone()).collect()
    }

    /// The song, and on Horizon where you play and whether you're in a call.
    fn send_listening(&mut self, to: &[String]) {
        self.send_activity(to, None);
    }

    fn update_ringing(&mut self) {
        let should = self.st.invites.values().any(|i| !self.quiet(Some(&i.from)));
        if should != self.st.ringing {
            self.st.ringing = should;
            self.fx.push(if should && self.cfg.ringtone != "none" { Effect::RingStart } else { Effect::RingStop });
        }
    }

    pub fn ringing(&self) -> bool {
        self.st.ringing
    }

    fn quiet(&self, from: Option<&str>) -> bool {
        self.st.my_status == "dnd" || from.is_some_and(|f| self.cfg.muted_users.contains(f))
    }

    pub fn send_message(&mut self, conv: &str, text: &str, reply: Option<Reply>, invite: Option<ServerInvite>) -> bool {
        self.send_message_song(conv, text, reply, invite, None)
    }

    pub fn send_song(&mut self, conv: &str) {
        match self.now_playing_current() {
            None => {
                let installed = self.now_playing_installed();
                self.notice(if installed { "Nothing is playing right now." } else { "Turn on the Now Playing mod to share songs." }, true);
            }
            Some(song) => {
                self.send_message_song(conv, &format!("\u{266A} Listening to {song}"), None, None, Some(song.clone()));
            }
        }
    }

    fn send_message_song(&mut self, conv: &str, text: &str, reply: Option<Reply>, invite: Option<ServerInvite>, song: Option<String>) -> bool {
        if let Some(blocked) = self.addons_check_send(conv, text) {
            self.notice(&blocked, true);
            return false;
        }
        if conv == LOBBY {
            let mut o = json!({ "t": "lobby_msg", "text": text });
            if let Some(r) = reply {
                o["reply"] = json!({ "id": r.id });
            }
            self.send_json(o);
            return true;
        }
        let mut env = json!({ "text": text });
        if let Some(r) = reply {
            let short: String = r.text.chars().take(120).collect();
            env["reply"] = json!({ "id": r.id, "name": r.name, "text": short });
        }
        if let Some(inv) = invite {
            env["invite"] = json!({ "name": inv.name, "address": inv.address });
        }
        if let Some(song) = song {
            env["song"] = song.into();
        }
        let plain = env.to_string();
        let me = self.me().unwrap_or_default();
        if conv.starts_with("g:") {
            let Some(g) = self.group(conv).cloned() else { return false };
            let mut payloads = Map::new();
            let mut missed = Vec::new();
            for p in &g.members {
                match crate::with_secure(|sec| sec.seal(&p.uuid, &crypto::group_aad(&g.id, &me, &p.uuid), &plain)) {
                    Some(sealed) => {
                        payloads.insert(p.uuid.clone(), sealed.into());
                    }
                    None if p.uuid != me => missed.push(p.name.clone()),
                    None => {}
                }
            }
            if !missed.is_empty() {
                self.notice(&format!("Not sent to {} (encryption key not trusted).", missed.join(", ")), true);
            }
            self.send_json(json!({ "t": "gmsg", "group": g.id, "payloads": payloads }));
        } else {
            let Some(sealed) = crate::with_secure(|sec| sec.seal(conv, &crypto::dm_aad(&me, conv), &plain)) else {
                let changed = crate::with_secure(|sec| sec.trust(conv)) == Trust::Changed;
                self.notice(
                    if changed {
                        "Not sent: their encryption key changed. Check the safety number, then Trust."
                    } else {
                        "Not sent: no encryption key for them yet."
                    },
                    true,
                );
                return false;
            };
            self.send_json(json!({ "t": "dm", "to": conv, "text": sealed }));
        }
        true
    }

    fn save_export(&mut self, data: Value) {
        let dir = self.cfg.dir().join("vcraft");
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let file = dir.join(format!("data-export-{stamp}.json"));
        let r = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, serde_json::to_string_pretty(&data).unwrap_or_default()));
        match r {
            Ok(()) => self.notice(&format!("Saved your data to config/vcraft/{}", file.file_name().unwrap_or_default().to_string_lossy()), false),
            Err(e) => self.notice(&format!("Couldn't save your data export: {e}"), true),
        }
    }

    pub fn admin(&self, cmd: &str, arg: Option<&str>) {
        let mut o = json!({ "t": "admin", "cmd": cmd });
        if let Some(a) = arg {
            o["arg"] = a.into();
        }
        self.send_json(o);
    }

    pub fn lobby_mute(&self, uuid: &str, mute: bool) {
        self.send_json(json!({ "t": "lobby_mute", "uuid": uuid, "muted": mute }));
    }

    pub fn delete_message(&self, conv: &str, id: &str) {
        if conv == LOBBY {
            self.send("lobby_delete", Some("id"), Some(id));
        } else if let Some(g) = conv.strip_prefix("g:") {
            self.send_json(json!({ "t": "delete", "id": id, "group": g }));
        } else {
            self.send_json(json!({ "t": "delete", "id": id, "with": conv }));
        }
    }

    pub fn invite_to_server(&mut self, conv: &str) -> bool {
        match self.server.clone() {
            None => {
                self.notice("Join a multiplayer server first, then invite people to it.", true);
                false
            }
            Some((name, address)) => {
                self.send_message(conv, &format!("Come play with me on {name}!"), None, Some(ServerInvite { name, address }));
                true
            }
        }
    }

    pub fn open_conversation(&mut self, conv: Option<&str>) {
        self.st.viewing = conv.map(str::to_owned);
        let Some(conv) = conv else { return };
        self.st.unread.remove(conv);
        if conv == LOBBY {
            self.send("lobby_join", None, None);
        } else if let Some(g) = conv.strip_prefix("g:") {
            self.send_json(json!({ "t": "history", "group": g }));
        } else {
            self.send_json(json!({ "t": "history", "with": conv }));
        }
        self.changed();
    }

    pub fn set_status(&mut self, status: &str) {
        self.st.my_status = status.to_owned();
        self.cfg.status = status.to_owned();
        self.cfg.save();
        self.send("status", Some("status"), Some(status));
        self.update_ringing();
        self.changed();
    }

    pub fn create_group(&self, name: &str, members: Vec<String>) {
        self.send_json(json!({ "t": "group_create", "name": name, "members": members }));
    }

    pub fn group_action(&self, t: &str, group: &str, key: Option<&str>, value: Option<&str>) {
        let mut o = json!({ "t": t, "group": group });
        if let Some(k) = key {
            o[k] = value.map_or(Value::Null, |v| v.into());
        }
        self.send_json(o);
    }

    pub fn call(&self, uuid: &str) {
        self.send("call_invite", Some("to"), Some(uuid));
    }

    pub fn call_group(&mut self, g: &Group) {
        let me = self.me();
        let mut rung = 0;
        for p in &g.members {
            if Some(&p.uuid) != me.as_ref() && p.online() && !self.in_call(&p.uuid) {
                self.call(&p.uuid);
                rung += 1;
            }
        }
        if rung == 0 {
            self.notice(&format!("Nobody else in {} is online.", g.name), true);
        }
    }

    pub fn in_call(&self, uuid: &str) -> bool {
        self.st.call.as_ref().is_some_and(|c| c.members.iter().any(|m| m.uuid == uuid) || c.ringing.iter().any(|p| p.uuid == uuid))
    }

    pub fn accept_invite(&mut self, call_id: &str) {
        self.send("call_accept", Some("callId"), Some(call_id));
        self.st.invites.retain(|_, v| v.call_id != call_id);
        self.update_ringing();
        self.changed();
    }

    pub fn decline_invite(&mut self, call_id: &str) {
        self.send("call_decline", Some("callId"), Some(call_id));
        self.st.invites.retain(|_, v| v.call_id != call_id);
        self.update_ringing();
        self.changed();
    }

    pub fn live_invites(&self) -> Vec<Invite> {
        let now = now_ms();
        self.st.invites.values().filter(|i| i.expires > now).cloned().collect()
    }

    pub fn latest_invite(&self) -> Option<Invite> {
        self.st.invites.values().last().cloned()
    }

    pub fn reconnect_voice(&mut self) {
        if self.st.call.is_none() && self.st.room.is_none() {
            return;
        }
        crate::voice().stop();
        self.update_voice();
        crate::voice().start();
        let me = self.me();
        let in_call = self.st.call.is_some();
        let out = crate::with_secure(|sec| sec.rekey(!in_call, true, me.as_deref()));
        out.into_iter().for_each(|m| self.send_json(m));
        self.send(if in_call { "call_rekey" } else { "room.rekey" }, None, None);
        self.send_voice_state();
        self.notice("Reconnecting voice...", false);
    }

    pub fn toggle_mute(&mut self) {
        if self.st.deafened {
            self.st.deafened = false;
            self.st.muted = false;
        } else {
            self.st.muted = !self.st.muted;
        }
        self.send_voice_state();
        self.push_voice_params();
        self.changed();
    }

    pub fn toggle_deafen(&mut self) {
        self.st.deafened = !self.st.deafened;
        self.send_voice_state();
        self.push_voice_params();
        self.changed();
    }

    pub fn send_voice_state(&self) {
        self.send_json(json!({ "t": "state", "muted": self.st.muted || self.st.deafened || self.auto_muted, "deafened": self.st.deafened }));
    }

    pub fn toggle_user_mute(&mut self, uuid: &str) {
        if !self.cfg.muted_users.remove(uuid) {
            self.cfg.muted_users.insert(uuid.to_owned());
        }
        self.cfg.save();
        self.push_voice_params();
        self.update_ringing();
        self.changed();
    }

    pub fn friend(&self, uuid: &str) -> Option<&Profile> {
        self.st.friends.iter().find(|f| f.uuid == uuid)
    }

    pub fn friend_by_name(&self, name: &str) -> Option<Profile> {
        self.st.friends.iter().find(|f| f.name.eq_ignore_ascii_case(name)).cloned()
    }

    pub fn group(&self, conv: &str) -> Option<&Group> {
        let id = conv.strip_prefix("g:")?;
        self.st.groups.iter().find(|g| g.id == id)
    }

    pub fn is_blocked(&self, uuid: &str) -> bool {
        self.st.blocked.iter().any(|p| p.uuid == uuid)
    }

    pub fn conv_name(&self, conv: &str) -> String {
        if conv == LOBBY {
            return "VCraft Community".into();
        }
        if let Some(g) = self.group(conv) {
            return g.name.clone();
        }
        self.friend(conv).map_or_else(|| "Unknown".into(), |f| self.cfg.display_name(&f.uuid, &f.name))
    }

    pub fn conversation_exists(&self, conv: Option<&str>) -> bool {
        match conv {
            Some(LOBBY) => true,
            Some(c) if c.starts_with("g:") => self.group(c).is_some(),
            Some(c) => self.friend(c).is_some(),
            None => false,
        }
    }

    pub fn notice(&mut self, text: &str, error: bool) {
        self.st.notice = Some((text.to_owned(), error, now_ms()));
        self.changed();
    }

    fn notify_user(&mut self, title: &str, body: &str, from: Option<&str>) {
        if !self.quiet(from) && self.cfg.notifications {
            self.toast(title, body);
        }
    }

    pub fn toast(&mut self, title: &str, body: &str) {
        self.toasts.push((title.to_owned(), body.to_owned(), now_ms()));
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn chat(&mut self, tag: &str, tag_argb: u32, body: &str) {
        self.fx.push(Effect::Chat { tag: tag.to_owned(), tag_rgb: tag_argb & 0xFF_FFFF, body: body.to_owned() });
    }
}

pub fn status_label(status: Option<&str>) -> &'static str {
    match status.unwrap_or("offline") {
        "online" => "Online",
        "idle" => "Idle",
        "dnd" => "Do Not Disturb",
        "invisible" => "Offline (invisible)",
        _ => "Offline",
    }
}

pub fn status_color(status: Option<&str>) -> u32 {
    match status.unwrap_or("offline") {
        "online" => 0xFF3B_A55C,
        "idle" => 0xFFFA_A61A,
        "dnd" => 0xFFED_4245,
        _ => 0xFF74_7F8D,
    }
}
