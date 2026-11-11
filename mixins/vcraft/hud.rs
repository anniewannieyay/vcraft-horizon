// Muted/deafened, the call, Proximity Chat, VCraft's notifications.
fn vcraft_hud(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    crate::game::mod_vcraft_glue::on_hud(game, batch);
}
