//! Settings in `~/.config/snowhush/settings`, Open at Login as an autostart entry, and KDE's animation speed.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use snowhush_core::halloween::Choice;
use snowhush_core::simulation::Mode;

const MODES: [Mode; 4] = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom];

#[derive(Clone, Copy)]
pub struct Settings {
    /// How much falls, in percent.
    pub intensity: u32,
    /// Snow and leaves settle along the bottom of the screen.
    pub ground: bool,
    /// Snow and leaves settle on a floating panel.
    pub panel: bool,
    pub melt: bool,
    pub snow_on_windows: bool,
    pub behind_windows: bool,
    /// The snow stays over full-screen apps instead of fading out of their way.
    pub over_full_screen: bool,
    pub show_in_screenshots: bool,
    /// Unix seconds of the first run, or 0 for an install older than the stamp.
    pub installed_at: u32,
    pub halloween: Choice,
    /// What falls: snow, leaves or rain; the kind picked last.
    pub mode: Mode,
    /// Two kinds fall together: `mix`, the one picked last after the one picked before it.
    pub mixed: bool,
    pub mix: [Mode; 2],
    /// Drawn at the display's highest rate all the time, instead of only while something is pushed around.
    pub max_performance: bool,
    /// How opaque what falls is drawn, in percent.
    pub opacity: u32,
}

impl Settings {
    /// Whether snow settles anywhere.
    pub fn accumulates(&self) -> bool {
        self.ground || self.panel || self.snow_on_windows
    }

    pub fn load() -> Settings {
        let text = config_dir().and_then(|folder| std::fs::read_to_string(folder.join("settings")).ok()).unwrap_or_default();
        let read = |key: &str| {
            text.lines().find_map(|line| line.split_once('=').filter(|(k, _)| k.trim() == key).and_then(|(_, v)| v.trim().parse::<u32>().ok()))
        };
        let calm = !animations_enabled();
        // Before the slider, the intensity was one of Off, Light, Steady, Heavy and Variable.
        let stop = read("snowfall").map(|i| i.min(4) as usize);
        let mode = read("mode").and_then(|i| MODES.get(i as usize).copied()).unwrap_or(if read("autumn").is_some_and(|v| v != 0) {
            Mode::Autumn
        } else {
            Mode::Snow
        });
        let stamped = read("installedAt");
        // Before Accumulate picked where snow settles, it turned settling off everywhere at once.
        let settled = read("accumulate").is_none_or(|v| v != 0) || read("settleGround").is_some();
        let settings = Settings {
            intensity: read("intensity").map(|v| v.clamp(0, 100)).or(stop.map(|i| [0, 25, 50, 75, 50][i])).unwrap_or(if calm { 25 } else { 50 }),
            ground: read("settleGround").map_or(settled, |v| v != 0),
            panel: read("settlePanel").map_or(settled, |v| v != 0),
            melt: read("meltSnow").is_none_or(|v| v != 0),
            snow_on_windows: settled && read("snowOnWindows").is_none_or(|v| v != 0),
            behind_windows: read("behindWindows").is_some_and(|v| v != 0),
            over_full_screen: read("showOverFullScreen").is_some_and(|v| v != 0),
            show_in_screenshots: read("showInScreenshots").is_some_and(|v| v != 0),
            installed_at: stamped.unwrap_or_else(|| if text.is_empty() { now() } else { 0 }),
            halloween: Choice { on: read("halloweenOn").is_some_and(|v| v != 0), skipped: read("halloweenSkipped").unwrap_or(0) },
            // Before rain there was only a switch for leaves.
            max_performance: read("performance").is_some_and(|v| v == 1),
            opacity: read("fallingOpacity").map_or(100, |v| v.clamp(10, 100)),
            mode,
            mixed: read("mixed").is_some_and(|v| v != 0),
            mix: mix_of(read("mix0"), read("mix1"), mode),
        };
        if stamped.is_none() {
            settings.save();
        }
        settings
    }

    pub fn save(&self) {
        let Some(folder) = config_dir() else { return };
        let text = format!(
            "intensity={}\nsettleGround={}\nsettlePanel={}\nmeltSnow={}\nsnowOnWindows={}\nbehindWindows={}\nshowOverFullScreen={}\nshowInScreenshots={}\ninstalledAt={}\nhalloweenOn={}\nhalloweenSkipped={}\nmode={}\nmixed={}\nmix0={}\nmix1={}\nautumn={}\nperformance={}\nfallingOpacity={}\n",
            self.intensity,
            self.ground as u32,
            self.panel as u32,
            self.melt as u32,
            self.snow_on_windows as u32,
            self.behind_windows as u32,
            self.over_full_screen as u32,
            self.show_in_screenshots as u32,
            self.installed_at,
            self.halloween.on as u32,
            self.halloween.skipped,
            MODES.iter().position(|&m| m == self.mode).unwrap_or(0),
            self.mixed as u32,
            MODES.iter().position(|&m| m == self.mix[0]).unwrap_or(0),
            MODES.iter().position(|&m| m == self.mix[1]).unwrap_or(0),
            (self.mode == Mode::Autumn) as u32,
            self.max_performance as u32,
            self.opacity
        );
        let _ = std::fs::create_dir_all(&folder);
        let _ = std::fs::write(folder.join("settings"), text);
    }

    /// The kinds falling: the two mixed, or the one picked.
    pub fn modes(&self) -> Vec<Mode> {
        if self.mixed { self.mix.to_vec() } else { vec![self.mode] }
    }

    /// Picks a kind; mixed, a third kind picked takes the place of the one picked before the other. Whether anything
    /// changed.
    pub fn pick(&mut self, mode: Mode) -> bool {
        if self.mixed {
            if self.mix.contains(&mode) {
                return false;
            }
            self.mix = [self.mix[1], mode];
        } else if mode == self.mode {
            return false;
        }
        self.mode = mode;
        true
    }

    /// Mixed, the kind picked last falls along with the one picked before it; unmixed, it falls alone.
    pub fn toggle_mixed(&mut self) {
        self.mixed = !self.mixed;
        if self.mixed {
            let other = if self.mix.contains(&self.mode) { self.mix.iter().copied().find(|&m| m != self.mode) } else { Some(self.mix[1]) };
            self.mix = [other.unwrap_or(other_than(self.mode)), self.mode];
        }
    }
}

/// Two different kinds read from the settings, or `mode` and another when they don't make a pair.
fn mix_of(first: Option<u32>, second: Option<u32>, mode: Mode) -> [Mode; 2] {
    let kind = |i: Option<u32>| i.and_then(|i| MODES.get(i as usize).copied());
    match (kind(first), kind(second)) {
        (Some(a), Some(b)) if a != b => [a, b],
        _ => [other_than(mode), mode],
    }
}

/// The kind to mix with `mode` until another is picked.
fn other_than(mode: Mode) -> Mode {
    if mode == Mode::Snow { Mode::Autumn } else { Mode::Snow }
}

fn now() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
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

pub fn config_home() -> Option<PathBuf> {
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
