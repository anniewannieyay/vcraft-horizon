// Glue between Horizon's `Game` and the VCraft engine: what the game knows
// (the server, players, the account), drawing the engine's draw lists, and
// carrying out its effects (chat lines, sounds, the screen, joining servers).
use crate::game::Game;
use horizon_client::hud::HudBatch;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vcraft_engine as vc;

static PTT_HELD: AtomicBool = AtomicBool::new(false);
static WAS_ON: AtomicBool = AtomicBool::new(false);
static SCREEN_OPEN: AtomicBool = AtomicBool::new(false);
static RING: Mutex<Option<Instant>> = Mutex::new(None);
static PENDING_JOIN: Mutex<Option<(String, String)>> = Mutex::new(None);

pub(super) fn config_dir() -> Option<std::path::PathBuf> {
    Game::native_config_dir().ok()
}

pub(super) fn account() -> Option<vc::Account> {
    let session = horizon_client::auth::current()?;
    let s = session.clone();
    Some(vc::Account {
        name: session.name.clone(),
        uuid: Some(vc::util::uuid_string(session.uuid)),
        authenticate: Arc::new(move |server_id: &str| {
            horizon_client::auth::join_server(&s, server_id).map_err(|_| "the Mojang session join failed".to_owned())
        }),
    })
}

fn world(game: &Game<'_>) -> vc::World {
    let server = game
        .remote
        .as_ref()
        .filter(|r| r.joined && !game.has_singleplayer_server())
        .map(|r| (r.session.name.clone(), r.session.address.clone()));
    let me = game.native_player().map(|p| ([p.x, p.y, p.z], p.dimension.to_string()));
    let players = game
        .native_client_entities()
        .into_iter()
        .filter_map(|e| Some((vc::util::uuid_string(e.player_uuid?), e.position)))
        .collect();
    vc::World { server, me, players, now_playing_on: game.native_on("nowplaying") }
}

/// Draws an engine draw list onto `batch`.
pub(super) fn render(game: &Game<'_>, batch: &mut HudBatch, draws: Vec<vc::Draw>) {
    let mut clips: Vec<(usize, [f32; 4])> = Vec::new();
    for d in draws {
        match d {
            vc::Draw::Fill(rect, argb) => game.native_fill(batch, rect, argb),
            vc::Draw::Text(text, at, argb) => {
                let [a, r, g, b] = argb.to_be_bytes();
                batch.text_styled(
                    &game.gui.atlas,
                    &horizon_client::text::Text::literal(text.as_str()),
                    at[0],
                    at[1],
                    horizon_client::hud::rgba(r, g, b, a),
                    false,
                );
            }
            vc::Draw::Face(slot, dst) => {
                if !game.native_sprite(batch, horizon_client::hud::MOD_IMAGE_SPRITE, vc::face_source(slot), dst) {
                    game.native_fill(batch, dst, 0xFF58_65F2);
                }
            }
            vc::Draw::PushClip(rect) => clips.push((batch.vertices.len(), rect)),
            vc::Draw::PopClip => {
                if let Some((start, [x0, y0, x1, y1])) = clips.pop() {
                    batch.clip_since(start, x0, y0, x1, y1);
                }
            }
        }
    }
    while let Some((start, [x0, y0, x1, y1])) = clips.pop() {
        batch.clip_since(start, x0, y0, x1, y1);
    }
}

fn chime(game: &mut Game<'_>, volume: f32, pitch: f32, delay: i32) {
    if let Some(p) = game.native_player() {
        let _ = game.native_play_local_sound("minecraft:block.note_block.bell", [p.x, p.y + 1.6, p.z], volume, pitch, delay);
    }
}

fn effects(game: &mut Game<'_>, fx: Vec<vc::Effect>) {
    for e in fx {
        match e {
            vc::Effect::Chat { tag, body, .. } => game.native_chat(&format!("[{tag}] {body}")),
            vc::Effect::Ding => chime(game, 1.5, 0.9, 0),
            vc::Effect::RingStart => *RING.lock().unwrap() = Some(Instant::now() - Duration::from_secs(10)),
            vc::Effect::RingStop => *RING.lock().unwrap() = None,
            vc::Effect::OpenScreen => {
                if game.native_mod_screen() == Some(vc::ID) || game.native_mod_screen_open(vc::ID) {
                    SCREEN_OPEN.store(true, Ordering::SeqCst);
                    vc::screen_opened();
                } else {
                    vc::notice("Close the menu you're in, then open VCraft again.", true);
                }
            }
            vc::Effect::CloseScreen => game.native_mod_screen_close(vc::ID),
            vc::Effect::Connect { name, address } => {
                *PENDING_JOIN.lock().unwrap() = Some((name, address));
                if game.remote.is_some() || game.player.is_some() {
                    if let Err(e) = game.leave_server(None) {
                        log::warn!("VCraft: couldn't leave the world to join a server: {e}");
                    }
                }
            }
        }
    }
}

/// Each frame: effects, the screen's state, ringing, a pending join.
pub(super) fn on_frame(game: &mut Game<'_>) {
    if !game.native_on(vc::ID) {
        if WAS_ON.swap(false, Ordering::SeqCst) {
            vc::shutdown();
            game.native_mod_screen_close(vc::ID);
        }
        return;
    }
    WAS_ON.store(true, Ordering::SeqCst);
    let fx = vc::take_effects();
    effects(game, fx);
    let open = game.native_mod_screen() == Some(vc::ID);
    if SCREEN_OPEN.swap(open, Ordering::SeqCst) && !open {
        vc::screen_closed();
    }
    let ring_due = {
        let mut r = RING.lock().unwrap();
        match *r {
            Some(t) if t.elapsed() >= Duration::from_millis(1800) => {
                *r = Some(Instant::now());
                true
            }
            _ => false,
        }
    };
    if ring_due {
        chime(game, 0.8, 1.0, 0);
        chime(game, 0.8, 1.26, 4);
    }
    let pending = PENDING_JOIN.lock().unwrap().clone();
    if let Some((name, address)) = pending {
        if game.remote.is_none() && game.player.is_none() {
            *PENDING_JOIN.lock().unwrap() = None;
            if let Err(e) = game.connect_server(&address, &name) {
                vc::notice(&format!("Couldn't join {address}: {e}"), true);
            }
        }
    }
}

/// Each client tick on the window's game.
pub(super) fn on_tick(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(vc::ID) {
        return;
    }
    let w = world(game);
    let ptt = PTT_HELD.load(Ordering::SeqCst) && game.native_playing();
    let fx = vc::tick(config_dir, account, w, ptt);
    effects(game, fx);
    if let Some((width, height, rgba)) = vc::faces_image() {
        game.native_mod_image(width, height, &rgba);
    }
}

pub(super) fn on_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !game.native_on(vc::ID) {
        return;
    }
    for (name, _, default) in vc::KEYS {
        let bound = game.native_mod_key(vc::ID, name, default);
        if bound != key || bound == "key.keyboard.unknown" {
            continue;
        }
        if name == "push_to_talk" {
            PTT_HELD.store(pressed, Ordering::SeqCst);
        } else if pressed && game.native_playing() {
            vc::key(name);
        }
    }
}

pub(super) fn on_hud(game: &mut Game<'_>, batch: &mut HudBatch) {
    if !game.native_on(vc::ID) {
        return;
    }
    let draws = {
        let g = &*game;
        vc::hud_draw(batch.width, batch.height, &|t: &str| g.native_text_width(t))
    };
    render(game, batch, draws);
}

pub(super) fn on_screen(game: &Game<'_>, batch: &mut HudBatch, mouse: [f32; 2]) {
    if game.native_mod_screen() != Some(vc::ID) {
        return;
    }
    let draws = vc::screen_draw(batch.width, batch.height, mouse, &|t: &str| game.native_text_width(t));
    render(game, batch, draws);
}

pub(super) fn ours(game: &Game<'_>) -> bool {
    game.native_mod_screen() == Some(vc::ID)
}

pub(super) fn open_key(game: &Game<'_>) -> String {
    game.native_mod_key(vc::ID, "open", "key.keyboard.o")
}

pub(super) fn on_command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    if *ran || !game.native_on(vc::ID) {
        return;
    }
    if let Some(lines) = vc::command(command) {
        *ran = true;
        for l in lines {
            game.native_chat(&l);
        }
    }
}
