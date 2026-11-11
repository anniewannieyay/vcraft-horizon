// The settings page in Options > Mods > VCraft (more inside VCraft itself).
fn vcraft_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    let _ = &game;
    if mod_id == vcraft_engine::ID {
        buttons.extend(vcraft_engine::settings_buttons());
    }
}
