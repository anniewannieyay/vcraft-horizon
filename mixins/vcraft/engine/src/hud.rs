//! The HUD (`CallHud`): muted or deafened, who is in the call and talking,
//! the addons' lines (Proximity Chat), and VCraft's pop-up notifications.

use crate::gfx::{self, Draw, Gfx};
use crate::util::now_ms;
use crate::{Core, addons};

const TOAST_MS: i64 = 5000;

pub fn draw(c: &mut Core, width: f32, _height: f32, measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    let mut g = Gfx::new(measure);
    let x = 4;
    let mut y = 4;
    if c.st.deafened || c.st.muted {
        let s = if c.st.deafened { "Deafened" } else { "Muted" };
        let w = g.width(s);
        g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
        g.text(s, x + 1, y, gfx::RED);
        y += 15;
    }
    if let Some(call) = c.st.call.clone() {
        for m in &call.members {
            let speaking = crate::voice().is_speaking(&m.uuid);
            let muted_for_me = c.cfg.muted_users.contains(&m.uuid);
            let tag = if m.deafened {
                "  deafened"
            } else if m.muted {
                "  muted"
            } else if muted_for_me {
                "  muted for you"
            } else {
                ""
            };
            let w = g.width(&m.name) + g.width(tag) + 18;
            g.panel(x - 2, y - 2, w, 13, 0xA000_0000);
            if speaking {
                g.outline(x - 1, y - 1, 11, 11, gfx::GREEN);
            }
            g.face(&m.uuid, &m.name, x, y, 9);
            g.text(&m.name, x + 13, y, if speaking { gfx::WHITE } else { 0xFFC4_C9CE });
            let nw = g.width(&m.name);
            g.text(tag, x + 13 + nw, y, gfx::MUTED);
            y += 14;
        }
        for p in &call.ringing {
            let line = format!("Ringing {}...", p.name);
            let w = g.width(&line);
            g.panel(x - 2, y - 2, w + 6, 13, 0xA000_0000);
            g.text(&line, x + 1, y, gfx::MUTED);
            y += 14;
        }
    }
    addons::hud(c, &mut g, x, y);
    draw_toasts(c, &mut g, width as i32);
    g.finish()
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
