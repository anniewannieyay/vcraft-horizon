// The mod's row in Options > Mods, with its keys.
fn vcraft_list(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use vcraft_engine as vc;
    mods.push(crate::game::native_hooks::NativeMod {
        id: vc::ID.into(),
        title: format!("{} {}", vc::NAME, vc::VERSION),
        description: vc::DESCRIPTION.into(),
        keys: vc::KEYS.iter().map(|(n, l, d)| [n.to_string(), l.to_string(), d.to_string()]).collect(),
    });
}
