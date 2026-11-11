// Each client tick: the relay's messages, presence, songs, the addons.
fn vcraft_tick(game: &mut crate::game::Game<'_>) {
    crate::game::mod_vcraft_glue::on_tick(game);
}
