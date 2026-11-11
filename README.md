# VCraft for Horizon

Voice calls, group chats and direct messages with your friends from anywhere
in Minecraft, end-to-end encrypted. This is the native Horizon port of the
VCraft Fabric mod.

- **Mod id:** `vcraft`
- **Version:** 2.3.0
- **Game:** Minecraft 26.3 (Horizon, pinned to commit `fdc87aec`)
- **Kind:** native mod (`.hnmod`)
- **Licence:** MIT

## Features

- 1:1 and group voice calls, with voice activity or push to talk
- Direct messages and group chats, plus a community lobby chat
- End-to-end encryption: X25519 key agreement, Ed25519 signatures, and
  per-message AEAD. Each friend has a safety number you can compare.
- Built-in addons:
  - **Proximity**: voice volume follows the distance between players on the
    same server.
  - **Announcements**: signed announcements from a group's owner.
  - **Group rules**: keyword filters, slow mode and moderation for groups.
- Optional "Now listening" sharing. This only works while the separate
  Now Playing mod is turned on.
- The `/vc` command, a HUD and a full in-game screen

## What it does on your computer

Native mods aren't sandboxed, so here is everything VCraft touches outside
the game:

| What | When | Why |
|---|---|---|
| Connects to the relay (default `wss://vcraft.2-31-104-109.sslip.io`, can be changed in settings) | While the game runs | Delivers encrypted messages and voice between friends |
| Opens your microphone and speakers (`cpal`) | Only during a call | Voice |
| `https://sessionserver.mojang.com` and Mojang's skin texture server | When showing a friend's head | Player skins |
| Runs a hidden PowerShell helper (Windows), `osascript` (macOS) or `playerctl` (Linux) | Only while the Now Playing mod is on | Reads the song title from the OS media controls. The script is in `engine/src/media.rs` and quits when the game quits. |
| Clipboard (`arboard`) | Only when you copy or paste in a VCraft text field | Copy and paste |
| Opens your browser at `https://ko-fi.com/annestinksbad` | Only when you click the Support button | Donations |
| Reads and writes `config/vcraft.json` and `config/vcraft/` | Always | Settings, keys and addon data |
| Reads `config/nowplaying.properties` | While the Now Playing mod is on | Respects Now Playing's own sharing switch |

It doesn't download or run any other code, or send anything to analytics or
telemetry services.

### What the relay can see

Message text, group content and voice are encrypted end to end, so the relay
can't read them. Like any server, it can see your IP address, your Minecraft
username and public key, your online status, and who you message or call and
when. You can run your own relay and point VCraft at it with the relay
address field in VCraft's settings.

## Layout

```
compatibility.json          Horizon and horizon-mixins commits this builds against
mixins/vcraft/
  horizon-mixin.toml        the bundle manifest: hooks, glue module, engine crate
  *.rs                      one small helper per hook point
  glue.rs                   reads the connection and players from `Game`
  engine/                   the engine crate: relay, crypto, voice, UI, addons
```

## Build

You need the Rust stable toolchain, Git, Python 3.11 or later, and `rsync`.
On Windows, use WSL for the check and package steps. From a checkout of
[horizon-rust/horizon](https://github.com/horizon-rust/horizon):

```bash
python3 native/tools/native.py check /path/to/vcraft-horizon
python3 native/tools/native.py check --horizon latest /path/to/vcraft-horizon
python3 native/tools/native.py package /path/to/vcraft-horizon
cargo horizon-mod check native/target/vcraft-2.3.0.hnmod
```

`package` writes `native/target/vcraft-2.3.0.hnmod` and prints its SHA-256.

## Licence

MIT, see [LICENSE](LICENSE).
