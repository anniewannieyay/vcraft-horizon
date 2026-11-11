//! Player faces (`Skins`): each skin fetched once from Mojang's session
//! server, its face and hat layer cut out at 8x8 and packed into one small
//! atlas the glue uploads as the game's mod image.

use base64::Engine;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

pub const FACE: u32 = 8;
pub const COLUMNS: u32 = 16;
pub const SIZE: u32 = FACE * COLUMNS;
const SLOTS: usize = (COLUMNS * COLUMNS) as usize;

struct Atlas {
    pixels: Vec<u8>,
    slots: HashMap<String, usize>,
    requested: HashSet<String>,
    dirty: bool,
    jobs: Option<Sender<String>>,
}

static ATLAS: LazyLock<Mutex<Atlas>> = LazyLock::new(|| {
    Mutex::new(Atlas { pixels: vec![0; (SIZE * SIZE * 4) as usize], slots: HashMap::new(), requested: HashSet::new(), dirty: false, jobs: None })
});

/// Where `uuid`'s face is in the atlas, once loaded; asks for it otherwise.
pub fn slot(uuid: &str) -> Option<usize> {
    let mut a = ATLAS.lock().ok()?;
    if let Some(s) = a.slots.get(uuid) {
        return Some(*s);
    }
    if crate::util::parse_uuid(uuid).is_some() && a.requested.insert(uuid.to_owned()) && a.requested.len() <= SLOTS {
        if a.jobs.is_none() {
            let (tx, rx) = channel::<String>();
            a.jobs = Some(tx);
            let _ = std::thread::Builder::new().name("VCraft Skins".into()).spawn(move || {
                for uuid in rx {
                    match fetch_face(&uuid) {
                        Ok(face) => store(&uuid, &face),
                        Err(e) => eprintln!("VCraft: no skin for {uuid}: {e}"),
                    }
                }
            });
        }
        if let Some(tx) = &a.jobs {
            let _ = tx.send(uuid.to_owned());
        }
    }
    None
}

/// The source rectangle of `slot` in the atlas, in its own pixels.
pub fn source(slot: usize) -> [f32; 4] {
    let (col, row) = (slot as u32 % COLUMNS, slot as u32 / COLUMNS);
    [(col * FACE) as f32, (row * FACE) as f32, FACE as f32, FACE as f32]
}

pub fn tick() {}

/// The whole atlas when a face was added since the last call.
pub fn take_image() -> Option<(u32, u32, Vec<u8>)> {
    let mut a = ATLAS.lock().ok()?;
    if !a.dirty {
        return None;
    }
    a.dirty = false;
    Some((SIZE, SIZE, a.pixels.clone()))
}

fn store(uuid: &str, face: &[u8]) {
    let Ok(mut a) = ATLAS.lock() else { return };
    let slot = a.slots.len();
    if slot >= SLOTS {
        return;
    }
    let (col, row) = (slot as u32 % COLUMNS, slot as u32 / COLUMNS);
    for y in 0..FACE {
        for x in 0..FACE {
            let src = ((y * FACE + x) * 4) as usize;
            let dst = (((row * FACE + y) * SIZE + col * FACE + x) * 4) as usize;
            a.pixels[dst..dst + 4].copy_from_slice(&face[src..src + 4]);
        }
    }
    a.slots.insert(uuid.to_owned(), slot);
    a.dirty = true;
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout(Duration::from_secs(15)).build()
}

/// The 8x8 RGBA face (head front, hat layer over it) of `uuid`'s skin.
fn fetch_face(uuid: &str) -> Result<Vec<u8>, String> {
    let id: String = uuid.chars().filter(|c| *c != '-').collect();
    let body = agent()
        .get(&format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let profile: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let textures = profile["properties"]
        .as_array()
        .and_then(|p| p.iter().find(|x| x["name"] == "textures"))
        .and_then(|x| x["value"].as_str())
        .ok_or("no textures")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(textures).map_err(|e| e.to_string())?;
    let tex: Value = serde_json::from_slice(&decoded).map_err(|e| e.to_string())?;
    let url = tex["textures"]["SKIN"]["url"].as_str().ok_or("no skin")?.replacen("http://", "https://", 1);
    let mut png = Vec::new();
    std::io::Read::read_to_end(
        &mut agent().get(&url).call().map_err(|e| e.to_string())?.into_reader(),
        &mut png,
    )
    .map_err(|e| e.to_string())?;
    let (w, h, rgba) = decode_png(&png)?;
    if w < 64 || h < 32 {
        return Err("odd skin size".into());
    }
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let mut face = vec![0u8; (FACE * FACE * 4) as usize];
    for y in 0..8 {
        for x in 0..8 {
            let mut c = px(8 + x, 8 + y);
            c[3] = 255;
            let hat = px(40 + x, 8 + y);
            if hat[3] > 0 {
                let a = hat[3] as u32;
                for k in 0..3 {
                    c[k] = ((hat[k] as u32 * a + c[k] as u32 * (255 - a)) / 255) as u8;
                }
            }
            let i = ((y * FACE + x) * 4) as usize;
            face[i..i + 4].copy_from_slice(&c);
        }
    }
    Ok(face)
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or("too big")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed png".into()),
    };
    Ok((w, h, rgba))
}
