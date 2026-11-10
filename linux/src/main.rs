mod chores;
mod gnome;
mod hyprland;
mod kwin;
mod light;
mod log;
mod niri;
mod overlay;
mod prefs;
mod render;
mod screen;
mod snow;
mod source;
mod sway;
mod tray;
mod windows;

fn main() {
    snowhush_core::text::choose(&languages());
    let result = if gnome::should_launch() { gnome::launch() } else { snow::run() };
    if let Err(error) = result {
        log!("failed: {error}");
        eprintln!("Snowhush couldn't start: {error}");
        std::process::exit(1);
    }
}

/// The user's languages as gettext ranks them: `LANGUAGE`'s list, then `LC_ALL`, `LC_MESSAGES` and `LANG`.
fn languages() -> Vec<String> {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let mut tags: Vec<String> = var("LANGUAGE").map(|list| list.split(':').map(str::to_string).collect()).unwrap_or_default();
    tags.extend(["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().filter_map(var));
    tags
}
