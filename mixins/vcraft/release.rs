// The mouse let go over the VCraft screen (a slider stops).
fn vcraft_release(game: &crate::game::Game<'_>, canceled: &mut bool) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        vcraft_engine::screen_release();
    }
}
