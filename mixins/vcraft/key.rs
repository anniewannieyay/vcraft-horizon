// Open VCraft (O), push to talk (V, held), mute (M), deafen (N).
fn vcraft_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::mod_vcraft_glue::on_key(game, key, pressed);
}
