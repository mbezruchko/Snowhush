mod kwin;
mod light;
mod log;
mod prefs;
mod render;
mod screen;
mod snow;
mod tray;
mod windows;

fn main() {
    if let Err(error) = snow::run() {
        log!("failed: {error}");
        eprintln!("Snowhush couldn't start: {error}");
        std::process::exit(1);
    }
}
