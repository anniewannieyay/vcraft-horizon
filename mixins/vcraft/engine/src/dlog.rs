//! The debug log, `config/vcraft/debug.log`: what VCraft did (connecting,
//! disconnecting and why, audio devices, calls joined and left), never what
//! anyone said or typed. Each line is written and flushed on its own, so the
//! log survives the game crashing. Users can attach it to bug reports.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_BYTES: u64 = 512 * 1024;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
static ON: AtomicBool = AtomicBool::new(true);

/// Where the log goes: `<config>/vcraft/debug.log`.
pub fn init(config_dir: &std::path::Path, on: bool) {
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(config_dir.join("vcraft").join("debug.log"));
    set_enabled(on);
}

pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::SeqCst);
}

pub fn path() -> Option<PathBuf> {
    FILE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Writes one line, moving a full log to `debug.old.log` first.
pub fn line(text: &str) {
    if !ON.load(Ordering::SeqCst) {
        return;
    }
    let Some(file) = path() else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&file, file.with_file_name("debug.old.log"));
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let clean: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "[{stamp}] {clean}");
        let _ = f.flush();
    }
}

/// A short, stable stand-in for a player's UUID in the log.
pub fn who(uuid: &str) -> String {
    uuid.chars().filter(|c| *c != '-').take(8).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn shortens_uuids() {
        assert_eq!(super::who("0123abcd-ef01-2345-6789-abcdef012345"), "0123abcd");
    }
}
