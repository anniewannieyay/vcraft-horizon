//! `/vc`: msg, call, invite, accept, decline, hangup, mute, deafen, status,
//! add, admin (the Fabric mod's client commands), and Horizon's join and nick.

use crate::Core;
use crate::i18n::{t, tf};
use crate::state::{self, STATUSES};

/// Runs `/vc ...` (`line` without its slash); None when it isn't `/vc`.
pub fn run(c: &mut Core, line: &str) -> Option<Vec<String>> {
    let mut parts = line.trim().splitn(2, ' ');
    if parts.next()? != crate::COMMAND {
        return None;
    }
    let rest = parts.next().unwrap_or("").trim();
    let mut words = rest.splitn(2, ' ');
    let sub = words.next().unwrap_or("").to_lowercase();
    let arg = words.next().unwrap_or("").trim();
    let usage = || vec![t("Usage: /vc msg|call|join|invite <friend>, accept, decline, hangup, mute, deafen, status <status>, add <player>, nick <friend> [nickname]").to_owned()];
    let friend = |c: &Core, name: &str| -> Result<state::Profile, Vec<String>> {
        c.friend_by_name(name)
            .or_else(|| c.st.friends.iter().find(|f| c.cfg.nicknames.get(&f.uuid).is_some_and(|n| n.eq_ignore_ascii_case(name))).cloned())
            .ok_or_else(|| vec![tf("{0} isn't on your VCraft friends list.", &[name])])
    };
    let out = match sub.as_str() {
        "msg" => {
            let mut p = arg.splitn(2, ' ');
            let (name, msg) = (p.next().unwrap_or(""), p.next().unwrap_or("").trim());
            if name.is_empty() || msg.is_empty() {
                return Some(vec!["Usage: /vc msg <friend> <message>".into()]);
            }
            match friend(c, name) {
                Ok(f) => {
                    c.send_message(&f.uuid, msg, None, None);
                    Vec::new()
                }
                Err(e) => e,
            }
        }
        "call" => match friend(c, arg) {
            Ok(f) => {
                c.call(&f.uuid);
                Vec::new()
            }
            Err(e) => e,
        },
        "join" => match friend(c, arg) {
            Ok(f) => {
                c.ask_to_join(&f.uuid);
                Vec::new()
            }
            Err(e) => e,
        },
        "nick" => {
            let mut p = arg.splitn(2, ' ');
            let (name, nick) = (p.next().unwrap_or(""), p.next().unwrap_or("").trim());
            if name.is_empty() {
                return Some(vec![t("Usage: /vc nick <friend> [nickname] (no nickname clears it)").into()]);
            }
            match friend(c, name) {
                Ok(f) => {
                    let note = c.cfg.notes.get(&f.uuid).cloned().unwrap_or_default();
                    c.set_nickname(&f.uuid, nick, &note);
                    vec![if nick.is_empty() { tf("Cleared your nickname for {0}.", &[&f.name]) } else { tf("{0} is now {1} for you.", &[&f.name, nick]) }]
                }
                Err(e) => e,
            }
        }
        "invite" => match friend(c, arg) {
            Ok(f) => {
                if c.invite_to_server(&f.uuid) {
                    vec![tf("Invited {0} to this server.", &[&f.name])]
                } else {
                    vec![t("Join a multiplayer server first, then invite people to it.").into()]
                }
            }
            Err(e) => e,
        },
        "accept" => match c.latest_invite() {
            Some(inv) => {
                c.accept_invite(&inv.call_id);
                Vec::new()
            }
            None => vec![t("Nobody is calling you.").into()],
        },
        "decline" => {
            if let Some(inv) = c.latest_invite() {
                c.decline_invite(&inv.call_id);
            }
            Vec::new()
        }
        "hangup" => {
            c.send("call_leave", None, None);
            Vec::new()
        }
        "mute" => {
            c.toggle_mute();
            vec![if c.st.muted { t("Microphone muted") } else { t("Microphone unmuted") }.into()]
        }
        "deafen" => {
            c.toggle_deafen();
            vec![if c.st.deafened { t("Deafened") } else { t("Undeafened") }.into()]
        }
        "status" => {
            let s = if arg == "offline" { "invisible" } else { arg };
            if STATUSES.contains(&s) {
                c.set_status(s);
                vec![format!("{}: {}", t("Status"), t(state::status_label(Some(s))))]
            } else {
                vec![t("Use online, idle, dnd or offline.").into()]
            }
        }
        "add" if !arg.is_empty() => {
            c.send("friend_request", Some("name"), Some(arg));
            Vec::new()
        }
        "admin" if !arg.is_empty() => {
            let mut p = arg.splitn(2, char::is_whitespace);
            let cmd = p.next().unwrap_or("").to_lowercase();
            let a = p.next().map(str::trim).filter(|s| !s.is_empty());
            c.admin(&cmd, a);
            Vec::new()
        }
        "" => {
            c.fx.push(crate::Effect::OpenScreen);
            Vec::new()
        }
        _ => usage(),
    };
    Some(out)
}
