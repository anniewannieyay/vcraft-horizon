// Each frame: the engine's effects, the screen's state, ringing, a pending join.
fn vcraft_frame(game: &mut crate::game::Game<'_>) {
    crate::game::mod_vcraft_glue::on_frame(game);
}
