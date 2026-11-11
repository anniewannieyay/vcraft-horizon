// Draws the VCraft screen while it's open.
fn vcraft_screen(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
) {
    crate::game::mod_vcraft_glue::on_screen(game, batch, screen.mouse);
}
