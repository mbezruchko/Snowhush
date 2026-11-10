//! GNOME owns the trusted Wayland client; the existing Rust loop still simulates and draws every frame.

use std::sync::Arc;

use calloop::channel::Sender;
use snowhush_core::simulation::Mode;
use zbus::blocking::Connection;

use crate::{
    prefs,
    snow::Command,
    source::{Report, Shared},
};

pub fn is_engine() -> bool {
    std::env::args().any(|arg| arg == "--gnome-engine")
}

pub fn should_launch() -> bool {
    !is_engine()
        && std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').any(|part| part.eq_ignore_ascii_case("gnome"))
        && ["HYPRLAND_INSTANCE_SIGNATURE", "SWAYSOCK", "NIRI_SOCKET"].iter().all(|name| std::env::var_os(name).is_none())
}

pub fn launch() -> Result<(), String> {
    Connection::session()
        .and_then(|bus| {
            bus.call_method(
                Some("org.gnome.Shell"),
                "/io/github/mbezruchko/Snowhush/Gnome",
                Some("io.github.mbezruchko.Snowhush.Shell"),
                "Start",
                &(),
            )
        })
        .map(|_| ())
        .map_err(|error| format!("GNOME Shell: install and enable the Snowhush extension first (linux/gnome/install.sh). {error}"))
}

struct Bridge {
    shared: Arc<Shared>,
    commands: Sender<Command>,
}

#[zbus::interface(name = "io.github.mbezruchko.Snowhush.Gnome")]
impl Bridge {
    fn report(&self, state: &str, x: f64, y: f64) -> zbus::fdo::Result<()> {
        let report: Report = serde_json::from_str(state).map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        if !x.is_finite()
            || !y.is_finite()
            || report.screens.iter().any(|d| ![d.x, d.y, d.w, d.h, d.ax, d.ay, d.aw, d.ah].iter().all(|v| v.is_finite()) || d.w <= 0.0 || d.h <= 0.0)
        {
            return Err(zbus::fdo::Error::InvalidArgs("invalid desktop coordinates".into()));
        }
        self.shared.set_cursor(x as f32, y as f32);
        let (_, previous) = self.shared.report();
        if previous.as_deref() != Some(&report) {
            self.shared.publish(report);
        }
        Ok(())
    }

    fn control(&self, key: &str, value: i32) -> zbus::fdo::Result<()> {
        let command = control(key, value).map_err(zbus::fdo::Error::InvalidArgs)?;
        self.commands.send(command).map_err(|_| zbus::fdo::Error::Failed("Snowhush is stopping".into()))
    }

    fn settings(&self) -> String {
        let s = prefs::Settings::load();
        serde_json::json!({
            "intensity": s.intensity, "opacity": s.opacity, "ground": s.ground,
            "melt": s.melt, "windows": s.snow_on_windows, "behind": s.behind_windows,
            "fullscreen": s.over_full_screen, "performance": s.max_performance,
            "mode": match s.mode { Mode::Snow => 0, Mode::Autumn => 1, Mode::Rain => 2, Mode::Blossom => 3 },
            "halloween": s.halloween.wanted(crate::light::today()),
        })
        .to_string()
    }
}

fn control(key: &str, value: i32) -> Result<Command, String> {
    let on = value != 0;
    Ok(match key {
        "intensity" if (0..=100).contains(&value) => Command::Intensity(value as f32 / 100.0),
        "opacity" if (10..=100).contains(&value) => Command::FallingOpacity(value as f32 / 100.0),
        "ground" => Command::Ground(on),
        "melt" => Command::Melt(on),
        "windows" => Command::SnowOnWindows(on),
        "behind" => Command::BehindWindows(on),
        "fullscreen" => Command::OverFullScreen(on),
        "performance" => Command::Performance(on),
        "clear" => Command::ClearSnow(!prefs::animations_enabled()),
        "quit" => Command::Quit,
        "halloween" => {
            let mut choice = prefs::Settings::load().halloween;
            if choice.wanted(crate::light::today()) != on {
                choice = choice.toggled(crate::light::today());
            }
            Command::Halloween(choice)
        }
        "mode" if (0..4).contains(&value) => {
            let mode = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom][value as usize];
            Command::Mode { mode, mixed: false, mix: [mode; 2], calm: !prefs::animations_enabled() }
        }
        _ => return Err(format!("unknown setting or invalid value: {key}={value}")),
    })
}

pub fn bridge(shared: Arc<Shared>, commands: Sender<Command>) -> impl zbus::object_server::Interface {
    Bridge { shared, commands }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_validate_ranges_and_keep_percent_units() {
        assert!(matches!(control("intensity", 75), Ok(Command::Intensity(v)) if v == 0.75));
        assert!(matches!(control("opacity", 40), Ok(Command::FallingOpacity(v)) if v == 0.4));
        for (key, value) in [("intensity", -1), ("intensity", 101), ("opacity", 0), ("mode", 4), ("missing", 1)] {
            assert!(control(key, value).is_err(), "{key}={value}");
        }
        assert!(matches!(control("mode", 2), Ok(Command::Mode { mode: Mode::Rain, mixed: false, .. })));
    }
}
