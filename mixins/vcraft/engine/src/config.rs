//! `config/vcraft.json`, the Fabric mod's file and fields.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const DEFAULT_RELAY: &str = "wss://vcraft.2-31-104-109.sslip.io";
const OLD_DEFAULT_RELAY: &str = "ws://localhost:24470";

#[derive(Clone, Debug)]
pub struct Config {
    pub server_url: String,
    pub auto_connect: bool,
    pub auto_update: bool,
    pub status: String,
    pub push_to_talk: bool,
    pub voice_threshold_db: f64,
    pub mic_volume: f64,
    pub noise_suppression: bool,
    pub output_volume: f64,
    pub notifications: bool,
    pub message_sound: bool,
    pub share_listening: bool,
    pub now_playing_key_moved: bool,
    pub ringtone: String,
    pub muted_users: BTreeSet<String>,
    /// The built-in addons (Proximity Chat, Announcements, Group Rules), each
    /// its own mod on Fabric: here a switch each.
    pub addon_proximity: bool,
    pub addon_announcements: bool,
    pub addon_grouprules: bool,
    file: PathBuf,
}

impl Config {
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("vcraft.json");
        let o: Value = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let st = |k: &str, d: &str| o.get(k).and_then(Value::as_str).unwrap_or(d).to_owned();
        let bo = |k: &str, d: bool| o.get(k).and_then(Value::as_bool).unwrap_or(d);
        let fl = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
        let mut server_url = st("serverUrl", DEFAULT_RELAY);
        if server_url.trim().is_empty() || server_url.trim() == OLD_DEFAULT_RELAY {
            server_url = DEFAULT_RELAY.into();
        }
        let muted_users = o
            .get("mutedUsers")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let c = Self {
            server_url,
            auto_connect: bo("autoConnect", true),
            auto_update: bo("autoUpdate", true),
            status: st("status", "online"),
            push_to_talk: bo("pushToTalk", false),
            voice_threshold_db: fl("voiceThresholdDb", -42.0),
            mic_volume: fl("micVolume", 1.0),
            noise_suppression: bo("noiseSuppression", true),
            output_volume: fl("outputVolume", 1.0),
            notifications: bo("notifications", true),
            message_sound: bo("messageSound", true),
            share_listening: bo("shareListening", true),
            now_playing_key_moved: bo("nowPlayingKeyMoved", false),
            ringtone: st("ringtone", "minecraft:music_disc.pigstep"),
            muted_users,
            addon_proximity: bo("addonProximity", true),
            addon_announcements: bo("addonAnnouncements", true),
            addon_grouprules: bo("addonGroupRules", true),
            file,
        };
        c.save();
        c
    }

    pub fn dir(&self) -> PathBuf {
        self.file.parent().map(Path::to_path_buf).unwrap_or_default()
    }

    pub fn save(&self) {
        let v = json!({
            "serverUrl": self.server_url,
            "autoConnect": self.auto_connect,
            "autoUpdate": self.auto_update,
            "status": self.status,
            "pushToTalk": self.push_to_talk,
            "voiceThresholdDb": self.voice_threshold_db,
            "micVolume": self.mic_volume,
            "noiseSuppression": self.noise_suppression,
            "outputVolume": self.output_volume,
            "notifications": self.notifications,
            "messageSound": self.message_sound,
            "shareListening": self.share_listening,
            "nowPlayingKeyMoved": self.now_playing_key_moved,
            "ringtone": self.ringtone,
            "mutedUsers": self.muted_users.iter().collect::<Vec<_>>(),
            "addonProximity": self.addon_proximity,
            "addonAnnouncements": self.addon_announcements,
            "addonGroupRules": self.addon_grouprules,
        });
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&self.file, serde_json::to_string_pretty(&v).unwrap_or_default()) {
            eprintln!("VCraft: could not save {}: {e}", self.file.display());
        }
    }
}
