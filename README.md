# VCraft for Horizon

Voice calls, group chats and direct messages with your friends from anywhere
in Minecraft, end-to-end encrypted. This is the native Horizon port of the
[VCraft Fabric mod](https://modrinth.com/mod/vcraftimac) (MIT licence), by
the same author: I made the original VCraft and maintain both.

- **Mod id:** `vcraft`
- **Version:** 2.5.1
- **Game:** Minecraft 26.3 (Horizon, pinned to commit `fdc87aec`)
- **Kind:** native mod (`.hnmod`)
- **Licence:** MIT

## Features

- 1:1 and group voice calls, with voice activity or push to talk
- Noise suppression for your microphone (on by default; switch it off in
  VCraft's settings under Voice). It removes steady background noise such as
  fans, hum and hiss, and adds 20 ms of delay.
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

### New in 2.5.0

- **Calls:** ring a whole group with one button; **ask to join** a friend's
  call when they're in one (`/vc join <friend>`); **rejoin by yourself**
  after the relay connection drops; optional **auto-mute** in the pause menu
  or when the game window isn't focused; **echo cancellation** for speakers
  (off by default); **voice effects** (higher, deeper, radio).
- **Friends:** your own **nicknames and notes** (`/vc nick`), **last seen**,
  and **what friends are doing** ("Playing on Hypixel", "In a call"), shared
  end to end encrypted and turned off with "Show friends my server and calls".
- **Groups:** **mute a group** without leaving it, **invite codes** (copy one
  from a group's menu; friends paste it on the Friends page), and **your own
  pinned messages** with a pins panel.
- **HUD and controls:** an **unread badge** while playing, a **compact call
  HUD** (faces only), new keys (accept call, hang up, compact HUD; unbound
  until you pick one), and the bound keys listed in VCraft's settings.
- **Languages:** French, German, Spanish, Russian, Chinese, Ukrainian and
  Polish, following the game's language or picked in VCraft's settings.
- **Debug log:** `config/vcraft/debug.log` (connections, audio devices,
  calls; never what anyone says or types), with an "Open log folder" button.
- **Addons:** see [ADDONS.md](ADDONS.md) and the template in
  [docs/addon-template.rs](docs/addon-template.rs).

What friends are doing, call join requests and group code requests travel
in the encrypted `activity` message that already carries "Now listening".
Fabric clients only read its song, so Fabric and Horizon players still talk
to each other; the new features just work between Horizon players. Pins,
nicknames, notes and last seen stay on your computer.

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
| Reads and writes `config/vcraft.json` and `config/vcraft/` | Always | Settings, keys, addon data, your pins (`pins.json`), when you last saw friends (`last-seen.json`) and the debug log (`debug.log`, can be turned off) |
| Reads `config/nowplaying.properties` | While the Now Playing mod is on | Respects Now Playing's own sharing switch |

It doesn't download or run any other code, or send anything to analytics or
telemetry services.

With "Show friends my server and calls" on (the default), your friends'
VCraft can see the name of the server you're on (or that you're in single
player) and whether you're in a call. It's end-to-end encrypted, so the relay
can't read it, and it's never sent while you're Invisible.

### The default relay: who runs it and what it keeps

The default relay, `vcraft.2-31-104-109.sslip.io`, is run by me, the author
of VCraft (GitHub: `anniewannieyay`), on a rented Hetzner server. Its source
is the same relay the Fabric mod uses.

Message text, group content and voice are encrypted end to end, so the relay
can't read them. What it does keep:

| Data | How long |
|---|---|
| Minecraft username and UUID, public encryption key | Until you delete your account |
| Friends, friend requests, blocks, groups | Until you delete your account |
| Online status and when you were last online | Until you delete your account |
| Direct and group messages (encrypted), with sender, recipient and time | Up to 300 per conversation, deleted after 365 days |
| VCraft Community (public lobby) messages, not encrypted | Latest 500, deleted after 365 days |
| Server log lines when you connect or disconnect (username only) | As long as the server's system log keeps them |

Your IP address is used while you're connected, to limit abuse (connections
and sign-ins per address), and isn't stored. Voice is relayed live and never
stored. Your Microsoft access token never leaves your computer: you sign in
with a Mojang session check bound to the relay (see
[docs/relay-auth.md](docs/relay-auth.md)).

You can download or delete everything the relay has about you in VCraft →
Settings → Privacy. You can also run your own relay and point VCraft at it
with the relay address field in VCraft's settings.

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
cargo horizon-mod check native/target/vcraft-2.5.1.hnmod
```

`package` writes `native/target/vcraft-2.5.1.hnmod` and prints its SHA-256.

The engine's own tests run without Horizon: `cargo test` in
`mixins/vcraft/engine` (with Horizon's `Cargo.lock` copied in, so the same
crate versions are used).

## Licence

MIT, see [LICENSE](LICENSE).
