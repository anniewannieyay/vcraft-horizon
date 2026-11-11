// Characters typed on the VCraft screen go into the focused field.
fn vcraft_chars(game: &crate::game::Game<'_>, c: char, canceled: &mut bool) {
    if crate::game::mod_vcraft_glue::ours(game) {
        *canceled = true;
        vcraft_engine::screen_char(c);
    }
}
