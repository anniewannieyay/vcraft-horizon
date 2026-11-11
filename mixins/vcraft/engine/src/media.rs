//! The song playing on this computer, as "Title - Artist", for sharing with
//! friends. On Fabric VCraft reads it from the Now Playing mod; Horizon mods
//! can't call each other, so this is Now Playing's own watcher, run only
//! while the Now Playing mod is on. Windows' media controls through a
//! PowerShell helper, osascript on macOS, playerctl on Linux.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

static CURRENT: Mutex<String> = Mutex::new(String::new());
static STARTED: std::sync::Once = std::sync::Once::new();

const POLL_MS: u64 = 2000;

pub fn current() -> String {
    CURRENT.lock().map(|s| s.clone()).unwrap_or_default()
}

fn set(s: String) {
    if let Ok(mut c) = CURRENT.lock() {
        *c = s;
    }
}

fn format(line: &str) -> String {
    let mut parts = line.split('\t');
    let title = parts.next().unwrap_or("").trim();
    let artist = parts.next().unwrap_or("").trim();
    if title.is_empty() {
        String::new()
    } else if artist.is_empty() {
        title.to_owned()
    } else {
        format!("{title} - {artist}")
    }
}

/// Starts the watcher thread once.
pub fn start() {
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("NowPlaying-MediaWatcher".into()).spawn(|| {
            if cfg!(windows) {
                run_windows();
            } else if cfg!(target_os = "macos") {
                poll_loop(&["osascript", "-e", MAC_SCRIPT]);
            } else {
                poll_loop(&["playerctl", "metadata", "--format", "{{status}}\t{{title}}\t{{artist}}"]);
            }
        });
    });
}

const MAC_SCRIPT: &str = "if application \"Spotify\" is running then\n\ttell application \"Spotify\"\n\t\tif player state is playing then return (name of current track) & tab & (artist of current track)\n\tend tell\nend if\nif application \"Music\" is running then\n\ttell application \"Music\"\n\t\tif player state is playing then return (name of current track) & tab & (artist of current track)\n\tend tell\nend if\nreturn \"\"\n";

const WINDOWS_SCRIPT: &str = "$ErrorActionPreference = 'Stop'\n[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\nAdd-Type -AssemblyName System.Runtime.WindowsRuntime\n$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {\n\t$_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and\n\t$_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]\nfunction Await($op, [Type]$type) {\n\t$task = $asTask.MakeGenericMethod($type).Invoke($null, @($op))\n\t$task.Wait(-1) | Out-Null\n\t$task.Result\n}\n$null = [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager, Windows.Media.Control, ContentType = WindowsRuntime]\n$mgrType = [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]\n$propsType = [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties]\n$mgr = Await ($mgrType::RequestAsync()) $mgrType\nwhile ($true) {\n\tif (-not (Get-Process -Id __PARENT__ -ErrorAction SilentlyContinue)) { exit }\n\t$line = ''\n\ttry {\n\t\t$sessions = @()\n\t\t$cur = $mgr.GetCurrentSession()\n\t\tif ($cur) { $sessions += $cur }\n\t\t$sessions += @($mgr.GetSessions())\n\t\tforeach ($s in $sessions) {\n\t\t\tif ($s.GetPlaybackInfo().PlaybackStatus -eq 'Playing') {\n\t\t\t\t$p = Await ($s.TryGetMediaPropertiesAsync()) $propsType\n\t\t\t\tif ($p.Title) { $line = $p.Title + \"`t\" + $p.Artist; break }\n\t\t\t}\n\t\t}\n\t} catch {}\n\t[Console]::Out.WriteLine(($line -replace '[\\r\\n]', ' '))\n\t[Console]::Out.Flush()\n\tStart-Sleep -Milliseconds __POLL__\n}\n";

fn run_windows() {
    use base64::Engine;
    let script = WINDOWS_SCRIPT
        .replace("__PARENT__", &std::process::id().to_string())
        .replace("__POLL__", &POLL_MS.to_string());
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    loop {
        // The script is WINDOWS_SCRIPT above, verbatim; -EncodedCommand only
        // avoids quoting it on the command line (execution policy does not
        // apply to it). Its window is hidden so no console flashes, and it
        // exits by itself when the game does (the __PARENT__ check).
        let mut cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_window(&mut cmd);
        match cmd.spawn() {
            Ok(mut child) => {
                if let Some(out) = child.stdout.take() {
                    for line in BufReader::new(out).lines() {
                        let Ok(line) = line else { break };
                        set(format(&line));
                    }
                }
                let status = child.wait();
                eprintln!("Now Playing media helper exited ({status:?}), restarting");
            }
            Err(e) => eprintln!("Now Playing could not start PowerShell media helper: {e}"),
        }
        set(String::new());
        std::thread::sleep(Duration::from_secs(10));
    }
}

fn poll_loop(argv: &[&str]) {
    let mut warned = false;
    loop {
        match Command::new(argv[0]).args(&argv[1..]).stderr(Stdio::null()).output() {
            Ok(out) => {
                let mut s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                if let Some(rest) = s.strip_prefix("Playing\t") {
                    s = rest.to_owned();
                } else if s.starts_with("Paused\t") || s.starts_with("Stopped\t") {
                    s.clear();
                }
                set(format(&s));
            }
            Err(e) => {
                if !warned {
                    eprintln!("Now Playing could not query media ({}): {e}", argv[0]);
                    warned = true;
                }
                set(String::new());
            }
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_title_and_artist() {
        assert_eq!(super::format("Song\tArtist"), "Song - Artist");
        assert_eq!(super::format("Song\t"), "Song");
        assert_eq!(super::format("\tArtist"), "");
    }
}
