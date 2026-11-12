//! A template VCraft addon: "Ping". A button in group chats posts a ping to
//! the group, at most once a minute, and the HUD shows when you last pinged.
//!
//! To use it: copy this file to `mixins/vcraft/engine/src/addon_ping.rs`,
//! add `pub mod addon_ping;` to `lib.rs`, then wire the three marked hooks in
//! `addons.rs` (see ADDONS.md). Rename everything "ping" to your addon's name.

use crate::gfx::{self, Gfx, Icon};
use crate::i18n::t;
use crate::screen::Action;
use crate::util::now_ms;
use crate::{Core, addons, dlog};

pub const ID: &str = "vcraft-ping";
pub const ICON: Icon = ["....#....", "...###...", "..#####..", "..#####..", "..#####..", ".#######.", "#########", "....#....", "...###..."];
const COOLDOWN_MS: i64 = 60_000;

/// The addon's state, kept on `Core` (add `pub ping: addon_ping::Ping` to `Addons`).
#[derive(Default)]
pub struct Ping {
    pub last: i64,
}

/// Whether a ping may go out now.
pub fn ready(last: i64, now: i64) -> bool {
    now - last >= COOLDOWN_MS
}

/// Hook: `chat_buttons` in addons.rs — `v.extend(addon_ping::chat_buttons(c, conv));`
pub fn chat_buttons(c: &Core, conv: &str) -> Vec<addons::ChatButton> {
    if !conv.starts_with("g:") {
        return Vec::new();
    }
    let tip = if ready(c.addons.ping.last, now_ms()) { t("Ping everyone in this group") } else { t("You pinged recently") };
    vec![addons::ChatButton { icon: ICON, tooltip: tip.to_owned(), action: Action::Addon(addons::AddonAction::Ping(conv.to_owned())) }]
}

/// Hook: `run` in addons.rs — `AddonAction::Ping(conv) => addon_ping::ping(c, &conv),`
pub fn ping(c: &mut Core, conv: &str) {
    let now = now_ms();
    if !ready(c.addons.ping.last, now) {
        c.notice(t("Wait a minute before pinging again."), true);
        return;
    }
    if c.send_message(conv, "\u{1F4E3} Ping! Hop on?", None, None) {
        c.addons.ping.last = now;
        dlog::line("ping: sent to a group");
    }
}

/// Hook: `hud` in addons.rs — `y = addon_ping::hud(c, g, x, y);`
pub fn hud(c: &Core, g: &mut Gfx, x: i32, y: i32) -> i32 {
    let left = COOLDOWN_MS - (now_ms() - c.addons.ping.last);
    if c.addons.ping.last == 0 || left <= 0 {
        return y;
    }
    let s = format!("Ping ready in {}s", left / 1000 + 1);
    let w = g.width(&s);
    g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
    g.text(&s, x + 1, y, gfx::MUTED);
    y + 15
}

#[cfg(test)]
mod tests {
    #[test]
    fn cooldown() {
        assert!(super::ready(0, 60_000));
        assert!(!super::ready(10_000, 60_000));
    }
}
