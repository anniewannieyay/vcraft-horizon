# Submit form: copy-paste values

For https://playhorizon.dev/mods/submit. Remove this file from the repo after
you submit, if you like.

| Field | Value |
|---|---|
| Kind | Native mod |
| File | `vcraft-2.4.0.hnmod` |
| Id | `vcraft` |
| Version | `2.4.0` |
| Minecraft version | `26.3` |
| Name | VCraft |
| Licence (SPDX) | `MIT` |
| Source | `https://github.com/anniewannieyay/vcraft-horizon` |
| Icon | `icon.png` (384x384 PNG, 154 KiB) |
| Categories (1 to 3) | Social, Utility (pick the closest from the form's list) |
| Tags (optional) | `voice-chat`, `chat`, `encryption`, `friends`, `proximity-chat` |

## Summary (160 characters or fewer)

Voice calls, group chats and direct messages with your friends from anywhere in Minecraft, end-to-end encrypted.

## What changed (first release)

First release on Horizon: a native port of VCraft with voice calls, DMs, group chats, and the Proximity, Announcements and Group rules addons. New in 2.4.0: microphone noise suppression (on by default, switch in Settings > Voice).

## Description (Markdown, at least 50 characters)

VCraft lets you talk to your friends from anywhere in Minecraft, whether
you're in single player or on any server.

**Features**
- 1:1 and group **voice calls**, with voice activity or push to talk
- **Noise suppression** that removes fan, hum and hiss from your mic
- **Direct messages** and **group chats**, plus a community lobby
- **End-to-end encrypted**: X25519 key agreement, Ed25519 signatures and a
  safety number for each friend
- Built-in addons: **Proximity** (volume follows distance on the same
  server), **Announcements** and **Group rules** (keyword filters, slow mode,
  moderation)
- Optional **Now listening** sharing when the Now Playing mod is on
- Press the VCraft key or type `/vc`

**What it does on your computer.** This is a native mod and isn't sandboxed,
so here is everything it touches:
- Connects to the VCraft relay (`wss://vcraft.2-31-104-109.sslip.io` by
  default, which you can change in settings) to deliver encrypted messages
  and voice. The relay can't read them. It can see your IP address, your
  username and public key, your status, and who you talk to and when.
- Uses your microphone and speakers only during a call.
- Fetches player skins from Mojang's session server.
- Only while the Now Playing mod is on, it runs a small hidden PowerShell,
  `osascript` or `playerctl` helper that reads the current song from your
  OS media controls.
- Uses the clipboard only when you copy or paste, and opens Ko-fi only if you
  click Support.
- Stores settings and keys in `config/vcraft.json` and `config/vcraft/`, and
  reads Now Playing's sharing switch from `config/nowplaying.properties`.

It doesn't download or run other code, or collect analytics.
