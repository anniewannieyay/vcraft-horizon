//! The HUD (`CallHud`): muted or deafened, who is in the call and talking
//! (or just their faces, in compact mode), unread messages, the addons'
//! lines (Proximity Chat), and VCraft's pop-up notifications.

use crate::gfx::{self, Draw, Gfx};
use crate::i18n::{t, tf};
use crate::util::now_ms;
use crate::{Core, addons};

const TOAST_MS: i64 = 5000;

pub fn draw(c: &mut Core, width: f32, _height: f32, measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    let mut g = Gfx::new(measure);
    let x = 4;
    let mut y = 4;
    if c.st.deafened || c.st.muted || c.auto_muted {
        let s = if c.st.deafened {
            t("Deafened").to_owned()
        } else if c.st.muted {
            t("Muted").to_owned()
        } else if c.world.paused {
            t("Muted while paused").to_owned()
        } else {
            t("Muted while in another window").to_owned()
        };
        let w = g.width(&s);
        g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
        g.text(&s, x + 1, y, gfx::RED);
        y += 15;
    }
    if let Some(call) = c.st.call.clone() {
        if c.cfg.hud_compact {
            y = compact_call(c, &mut g, x, y, &call);
        } else {
            for m in &call.members {
                let speaking = crate::voice().is_speaking(&m.uuid);
                let muted_for_me = c.cfg.muted_users.contains(&m.uuid);
                let tag = if m.deafened {
                    format!("  {}", t("deafened"))
                } else if m.muted {
                    format!("  {}", t("muted"))
                } else if muted_for_me {
                    format!("  {}", t("muted for you"))
                } else {
                    String::new()
                };
                let name = c.cfg.display_name(&m.uuid, &m.name);
                let w = g.width(&name) + g.width(&tag) + 18;
                g.panel(x - 2, y - 2, w, 13, 0xA000_0000);
                if speaking {
                    g.outline(x - 1, y - 1, 11, 11, gfx::GREEN);
                }
                g.face(&m.uuid, &m.name, x, y, 9);
                g.text(&name, x + 13, y, if speaking { gfx::WHITE } else { 0xFFC4_C9CE });
                let nw = g.width(&name);
                g.text(&tag, x + 13 + nw, y, gfx::MUTED);
                y += 14;
            }
            for p in &call.ringing {
                let line = tf("Ringing {0}...", &[&c.cfg.display_name(&p.uuid, &p.name)]);
                let w = g.width(&line);
                g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
                g.text(&line, x + 1, y, gfx::MUTED);
                y += 14;
            }
        }
    }
    if c.cfg.hud_unread && !c.ui.open {
        let unread = c.unread_total();
        if unread > 0 {
            let s = if unread == 1 { t("1 unread message").to_owned() } else { tf("{0} unread messages", &[&unread.min(999).to_string()]) };
            let w = g.width(&s);
            g.panel(x - 2, y - 2, w + 16, 13, 0xA000_0000);
            g.fill(x, y + 2, 5, 5, gfx::RED);
            g.text(&s, x + 10, y, gfx::WHITE);
            y += 15;
        }
    }
    if let Some(r) = c.social.requests.last().cloned() {
        let s = match r.kind {
            crate::social::RequestKind::Call => tf("{0} wants to join your call", &[&r.name]),
            crate::social::RequestKind::Group(_) => tf("{0} wants to join a group", &[&r.name]),
        };
        let w = g.width(&s);
        g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
        g.text(&s, x + 1, y, gfx::GOLD);
        y += 15;
    }
    addons::hud(c, &mut g, x, y);
    draw_toasts(c, &mut g, width as i32);
    g.finish()
}

/// The call as a row of faces: green ring while talking, red corner when muted.
fn compact_call(c: &Core, g: &mut Gfx, x: i32, y: i32, call: &crate::state::Call) -> i32 {
    let n = call.members.len() as i32 + call.ringing.len() as i32;
    g.panel(x - 2, y - 2, n * 14 + 2, 15, 0xA000_0000);
    let mut fx = x;
    for m in &call.members {
        if crate::voice().is_speaking(&m.uuid) {
            g.outline(fx - 1, y - 1, 13, 13, gfx::GREEN);
        }
        g.face(&m.uuid, &m.name, fx, y, 11);
        if m.muted || m.deafened || c.cfg.muted_users.contains(&m.uuid) {
            g.fill(fx + 8, y + 8, 4, 4, gfx::RED);
        }
        fx += 14;
    }
    for p in &call.ringing {
        g.face(&p.uuid, &p.name, fx, y, 11);
        g.fill(fx, y, 11, 11, 0x9023_2428);
        fx += 14;
    }
    y + 17
}

fn draw_toasts(c: &mut Core, g: &mut Gfx, width: i32) {
    let now = now_ms();
    c.toasts.retain(|t| now - t.2 < TOAST_MS);
    let mut y = 4;
    for (title, body, _) in &c.toasts {
        let tw = 160.max(g.width(title).max(g.width(body)) + 14).min(width / 2);
        let x = width - tw - 4;
        g.panel(x, y, tw, 30, 0xE023_2428);
        g.fill(x, y + 1, 2, 28, gfx::GREEN);
        let t = g.clip(title, tw - 12);
        g.text(&t, x + 8, y + 5, gfx::WHITE);
        let b = g.clip(body, tw - 12);
        g.text(&b, x + 8, y + 17, gfx::MUTED);
        y += 34;
    }
}
