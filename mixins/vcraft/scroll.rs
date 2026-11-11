// The wheel over the VCraft screen scrolls the list under the pointer.
fn vcraft_scroll(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    scroll: [f64; 2],
    canceled: &mut bool,
) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        vcraft_engine::screen_scroll(screen.mouse, scroll[1]);
    }
}
