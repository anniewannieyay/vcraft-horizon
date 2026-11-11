// A click on the settings page.
fn vcraft_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    let _ = &game;
    if mod_id == vcraft_engine::ID {
        vcraft_engine::settings_click(button);
    }
}
