// Keys over the VCraft screen: typing keys, Enter, Escape, paste, the open key closes it.
fn vcraft_keys(game: &crate::game::Game<'_>, key: &str, modifiers: u32, canceled: &mut bool) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        let open = crate::game::mod_vcraft_glue::open_key(game);
        vcraft_engine::screen_key(key, modifiers, &open);
    }
}
