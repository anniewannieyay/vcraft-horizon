# Submit form: copy-paste values

For https://playhorizon.dev/mods/submit. Remove this file from the repo after
you submit, if you like.

| Field | Value |
|---|---|
| Kind | Native mod |
| File | `vcraft-2.5.1.hnmod` |
| Id | `vcraft` |
| Version | `2.5.1` |
| Minecraft version | `26.3` |
| Name | VCraft |
| Licence (SPDX) | `MIT` |
| Source | `https://github.com/anniewannieyay/vcraft-horizon` |
| Icon | `icon.png` (384x384 PNG, 154 KiB) |
| Categories (1 to 3) | Social, Utility (pick the closest from the form's list) |
| Tags (optional) | `voice-chat`, `chat`, `encryption`, `friends`, `proximity-chat` |

## Summary (160 characters or fewer)

Voice calls, group chats and direct messages with your friends from anywhere in Minecraft, end-to-end encrypted.

## What changed (2.5.1)

Safer sign-in: the Mojang session id is now computed by VCraft from the relay's host and a fresh nonce, so a relay can't reuse your sign-in on a real server. Needs the updated relay. Also from 2.5.0: ask to join a friend's call, rejoin calls after a dropped connection, auto-mute when paused or tabbed out, echo cancellation for speakers, voice effects. Friend nicknames and notes, last seen, and what friends are playing. Mute groups, group invite codes, personal pinned messages. Unread badge and compact call HUD, new keys, seven new languages, and a debug log for bug reports.

## Description (Markdown, at least 50 characters)

VCraft lets you talk to your friends from anywhere in Minecraft, whether
you're in single player or on any server. This is the Horizon port of the
[VCraft Fabric mod](https://modrinth.com/mod/vcraftimac) (MIT), by the same
author: I made the original and maintain both.

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
  and voice. The relay can't read them. I (the author) run the default relay
  on a Hetzner server. It keeps your username, UUID, public key, friends,
  groups and status until you delete your account; encrypted messages with
  their sender and time for up to 365 days (300 per conversation); and
  username-only connect/disconnect log lines. It uses your IP address only
  while you're connected, to limit abuse, and doesn't store it. Voice is
  never stored. Settings → Privacy downloads or deletes your data.
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
