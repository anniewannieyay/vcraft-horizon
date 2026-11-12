# Writing a VCraft addon (Horizon)

On Fabric, VCraft addons are separate mods that talk to VCraft through its
`VCraftApi`. Horizon native mods can't call into each other, so on Horizon
an addon is a module **inside** VCraft's engine crate, wired into a handful
of hook points. Proximity Chat, Announcements and Group Rules all work this
way; see `mixins/vcraft/engine/src/addons.rs`.

[`docs/addon-template.rs`](docs/addon-template.rs) is a complete, small addon
("Ping": a button in group chats that pings everyone online in the group) that
you can copy as a starting point. It isn't compiled until you add it.

## Steps

1. Copy `docs/addon-template.rs` to `mixins/vcraft/engine/src/addon_<name>.rs`
   and add `pub mod addon_<name>;` to `engine/src/lib.rs`.
2. Give it an id (`vcraft-<name>`) and a switch: add `addon_<name>: bool` to
   `Config` in `config.rs` (load it with `bo("addon<Name>", true)` and save it
   in `save()`), and a button in `settings_buttons` / `settings_click` in
   `lib.rs`.
3. Wire the hooks you need in `addons.rs`. Each is one line that calls your
   module when your switch is on:

| Hook in `addons.rs` | Called when | Use it to |
|---|---|---|
| `loaded_ids` | connecting | tell the relay your addon id (the relay may enable features per addon) |
| `addons_connected` | the relay says welcome | fetch your state from the relay |
| `addons_dispatch(t, o)` | a relay message VCraft doesn't know arrives | handle your own message types (`<name>.something`) |
| `addons_filter(conv, m)` | a message arrives or history loads | hide or rewrite messages (return `None` to hide) |
| `addons_check_send(conv, text)` | the player sends a message | block it with a reason (`Some("...")`) |
| `addons_tick` | every client tick | timers, polling |
| `addons_call(had, has)` | the call starts or ends | react to calls |
| `pages` / `render_page` / `page_opened` | the VCraft screen | a page of your own, in the sidebar or opened from a button |
| `chat_buttons(c, conv)` | a chat is drawn | an icon button in the chat header |
| `hud(c, g, x, y)` | the HUD is drawn | lines under the call |
| `run(c, AddonAction)` | one of your buttons is clicked | add a variant to `AddonAction` for each |

## Rules

- **Relay message types need a dot** (`ping.send`), and `room.*` is VCraft's.
  The relay only forwards types it knows, so a brand-new type needs the relay
  to support it. Without that, the `activity` message (see `social.rs`) can
  carry small encrypted payloads between friends: add a `req` type there.
- **Encrypt what's private.** `addon_seal` / `addon_open` in `addons.rs`
  encrypt end to end for one friend; `sign` signs with the player's key.
- **Store files in `config/vcraft/addons/<id>/`** (`Addons::write`).
- **Translate what you show**: wrap text in `crate::i18n::t("...")` and add a
  row to the table in `i18n.rs` (English is the key).
- **Log, but never content**: `crate::dlog::line("ping: sent")` writes to
  `config/vcraft/debug.log`. Never log what people type or say.
- **Test the logic**: put pure functions in your module with `#[cfg(test)]`
  tests and run `cargo test` in `engine/`.
