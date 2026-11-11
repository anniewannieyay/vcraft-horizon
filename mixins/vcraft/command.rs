// `/vc ...`, answered in chat.
fn vcraft_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    crate::game::mod_vcraft_glue::on_command(game, command, ran);
}
