// A click on the VCraft screen.
fn vcraft_mouse(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        vcraft_engine::screen_click(screen.mouse, button);
    }
}
