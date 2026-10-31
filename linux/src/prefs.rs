//! Settings in `~/.config/snowhush/settings`, Open at Login as an autostart entry, and KDE's animation speed.

use std::path::PathBuf;

use snowhush_core::simulation::Snowfall;

#[derive(Clone, Copy)]
pub struct Settings {
    pub snowfall: Snowfall,
    pub accumulate: bool,
    pub melt: bool,
    pub snow_on_windows: bool,
    pub behind_windows: bool,
    pub show_in_screenshots: bool,
    pub halloween: bool,
}

impl Settings {
    pub fn load() -> Settings {
        let text = config_dir().and_then(|folder| std::fs::read_to_string(folder.join("settings")).ok()).unwrap_or_default();
        let read = |key: &str| {
            text.lines().find_map(|line| line.split_once('=').filter(|(k, _)| k.trim() == key).and_then(|(_, v)| v.trim().parse::<u32>().ok()))
        };
        let calm = !animations_enabled();
        let snowfall = read("snowfall").and_then(|i| Snowfall::ALL.get(i as usize).copied());
        Settings {
            snowfall: snowfall.unwrap_or(if calm { Snowfall::Light } else { Snowfall::Steady }),
            accumulate: read("accumulate").is_none_or(|v| v != 0),
            melt: read("meltSnow").is_none_or(|v| v != 0),
            snow_on_windows: read("snowOnWindows").is_none_or(|v| v != 0),
            behind_windows: read("behindWindows").is_some_and(|v| v != 0),
            show_in_screenshots: read("showInScreenshots").is_some_and(|v| v != 0),
            halloween: read("halloween").is_none_or(|v| v != 0),
        }
    }

    pub fn save(&self) {
        let Some(folder) = config_dir() else { return };
        let snowfall = Snowfall::ALL.iter().position(|&s| s == self.snowfall).unwrap_or(2);
        let text = format!(
            "snowfall={snowfall}\naccumulate={}\nmeltSnow={}\nsnowOnWindows={}\nbehindWindows={}\nshowInScreenshots={}\nhalloween={}\n",
            self.accumulate as u32,
            self.melt as u32,
            self.snow_on_windows as u32,
            self.behind_windows as u32,
            self.show_in_screenshots as u32,
            self.halloween as u32
        );
        let _ = std::fs::create_dir_all(&folder);
        let _ = std::fs::write(folder.join("settings"), text);
    }
}

pub fn opens_at_login() -> bool {
    autostart_entry().is_some_and(|path| path.exists())
}

pub fn set_opens_at_login(on: bool) -> std::io::Result<()> {
    let path = autostart_entry().ok_or_else(|| std::io::Error::other("no config folder"))?;
    if !on {
        return std::fs::remove_file(path);
    }
    // Inside an AppImage the app runs from a mount that is gone after a restart; the AppImage says where it is.
    let exe = match std::env::var_os("APPIMAGE") {
        Some(appimage) => PathBuf::from(appimage),
        None => std::env::current_exe()?,
    };
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(
        path,
        format!(
            "[Desktop Entry]\nType=Application\nName=Snowhush\nComment=Snow on your desktop\nExec=\"{}\"\nIcon=snowhush\nX-KDE-autostart-phase=2\n",
            exe.display()
        ),
    )
}

/// KDE's animation speed: zero, "Instant", means the person prefers less motion.
pub fn animations_enabled() -> bool {
    animation_factor() > 0.0
}

/// How much longer than usual KDE's animations take: zero for "Instant", larger when slower.
pub fn animation_factor() -> f32 {
    kde_setting("kdeglobals", "KDE", "AnimationDurationFactor").and_then(|value| value.parse::<f32>().ok()).map_or(1.0, |factor| factor.max(0.0))
}

/// Whether KWin slides the desktops past on a switch, as it does unless that effect is turned off.
pub fn slide_enabled() -> bool {
    kde_setting("kwinrc", "Plugins", "slideEnabled").is_none_or(|value| value != "false")
}

/// A setting from one of KDE's config files.
fn kde_setting(file: &str, group: &str, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_home()?.join(file)).ok()?;
    let header = format!("[{group}]");
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == header;
        } else if in_group
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn config_dir() -> Option<PathBuf> {
    config_home().map(|folder| folder.join("snowhush"))
}

fn autostart_entry() -> Option<PathBuf> {
    config_home().map(|folder| folder.join("autostart").join("snowhush.desktop"))
}
