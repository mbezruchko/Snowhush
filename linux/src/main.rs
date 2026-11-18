mod chores;
mod fight;
mod hyprland;
mod kwin;
mod light;
mod log;
mod niri;
mod picture;
mod prefs;
mod render;
mod screen;
mod snow;
mod source;
mod sway;
mod tray;
mod wallpaper;
mod windows;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.get(1).map(String::as_str) == Some(wallpaper::READ) {
        std::process::exit(wallpaper::read_for_another_run(&arguments[2..]));
    }
    snowhush_core::text::choose(&languages());
    if let Err(error) = snow::run() {
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
