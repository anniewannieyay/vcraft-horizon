// Dragging on the VCraft screen (the volume and threshold sliders).
fn vcraft_drag(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    canceled: &mut bool,
) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        vcraft_engine::screen_drag(screen.mouse);
    }
}
