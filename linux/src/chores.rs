//! What the snow loop must not wait on: the power profile, asked of the system bus, the desktop pictures,
//! asked of Plasma and decoded for the shadows' tint, and whether the desktop is light, asked of the portal. Any
//! can stall for as long as the service behind it does, so they run on a thread of their own and send what they
//! find to the loop.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use zbus::blocking::Connection as Bus;

use crate::light::{self, Backdrop, Shade};
use crate::prefs;
use crate::screen::PowerBudget;
use crate::snow::Command;

const POWER_EVERY: Duration = Duration::from_secs(5);
const BACKDROPS_EVERY: Duration = Duration::from_secs(60);
const LIGHT_EVERY: Duration = Duration::from_secs(5);
/// A desktop with no portal to ask may keep each ask waiting for long; it is asked again only now and then.
const LIGHT_UNKNOWN_EVERY: Duration = Duration::from_secs(60);

/// Starts the chores; a message on the returned sender asks for the desktop pictures again right away,
/// as when the displays changed.
pub fn start(bus: Bus, commands: Sender<Command>) -> mpsc::Sender<()> {
    let (wanted, requests) = mpsc::channel();
    let _ = std::thread::Builder::new().name("chores".into()).spawn(move || {
        let mut system = None;
        let mut budget = None;
        let mut light = false;
        let mut tints: Vec<(Backdrop, Option<[f32; 4]>)> = Vec::new();
        let mut power_at = Instant::now();
        let mut backdrops_at = Instant::now();
        let mut light_at = Instant::now();
        loop {
            let now = Instant::now();
            if now >= power_at {
                power_at = now + POWER_EVERY;
                let next = power_budget(&mut system);
                if budget != Some(next) {
                    budget = Some(next);
                    if commands.send(Command::Power(next)).is_err() {
                        return;
                    }
                }
            }
            if now >= light_at {
                let next = if plasma() { plasma_panel_light() } else { prefers_light(&bus) };
                light_at = Instant::now() + if next.is_some() { LIGHT_EVERY } else { LIGHT_UNKNOWN_EVERY };
                let next = next.unwrap_or(false);
                if next != light {
                    light = next;
                    if commands.send(Command::Light(next)).is_err() {
                        return;
                    }
                }
            }
            if now >= backdrops_at {
                backdrops_at = now + BACKDROPS_EVERY;
                if commands.send(Command::Backdrops(shades(&bus, &mut tints))).is_err() {
                    return;
                }
            }
            match requests.recv_timeout(power_at.min(backdrops_at).min(light_at).saturating_duration_since(Instant::now())) {
                Ok(()) => backdrops_at = Instant::now(),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    wanted
}

/// Each display's backdrop with its tint, decoding only pictures not seen before.
fn shades(bus: &Bus, tints: &mut Vec<(Backdrop, Option<[f32; 4]>)>) -> Vec<Shade> {
    let shades: Vec<Shade> = light::backdrops(bus)
        .into_iter()
        .map(|(x, y, backdrop)| {
            let tint = match tints.iter().find(|(seen, _)| *seen == backdrop) {
                Some(&(_, tint)) => tint,
                None => light::tint(&backdrop),
            };
            Shade { x, y, backdrop, tint }
        })
        .collect();
    *tints = shades.iter().map(|shade| (shade.backdrop.clone(), shade.tint)).collect();
    shades
}

fn plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.to_ascii_uppercase().split(':').any(|part| part == "KDE"))
}

/// Whether Plasma's panel is light. It takes its colours from the Plasma theme, which needn't match the apps'
/// (Breeze Twilight sets a dark panel over light windows), or, for a theme without colours of its own, from the
/// colour scheme.
fn plasma_panel_light() -> Option<bool> {
    let config = prefs::config_home()?;
    let theme = ini(&config.join("plasmarc"), "Theme", "name").unwrap_or_else(|| "default".into());
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let colors = data_home
        .into_iter()
        .chain(data_dirs.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from))
        .map(|dir| dir.join("plasma").join("desktoptheme").join(&theme).join("colors"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| config.join("kdeglobals"));
    // Breeze, the default colour scheme, is light.
    let Some(background) = ini(&colors, "Colors:Window", "BackgroundNormal") else { return Some(true) };
    let rgb: Vec<f32> = background.split(',').filter_map(|part| part.trim().parse().ok()).collect();
    let [r, g, b] = rgb[..] else { return None };
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b > 128.0)
}

/// A value from a KDE-style ini file.
fn ini(path: &Path, section: &str, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            inside = name == section;
        } else if inside
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Whether the desktop asks for light colours, as the portal tells apps; none when there is no portal to ask, and the
/// desktop is taken for dark, as most panels are.
fn prefers_light(bus: &Bus) -> Option<bool> {
    let reply = bus.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.Settings"),
        "ReadOne",
        &("org.freedesktop.appearance", "color-scheme"),
    );
    let value = reply.and_then(|reply| reply.body().deserialize::<zbus::zvariant::OwnedValue>()).ok()?;
    let scheme = match &*value {
        zbus::zvariant::Value::Value(inner) => u32::try_from(&**inner),
        value => u32::try_from(value),
    };
    Some(scheme.is_ok_and(|scheme| scheme == 2))
}

/// On battery from the power supplies the kernel lists; power saver from power-profiles-daemon.
fn power_budget(system: &mut Option<Bus>) -> PowerBudget {
    if power_saver(system) {
        return PowerBudget::Saver;
    }
    let mut battery = false;
    let mut mains_online = false;
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let read = |file: &str| std::fs::read_to_string(entry.path().join(file)).unwrap_or_default().trim().to_string();
            match read("type").as_str() {
                "Battery" if read("scope") != "Device" => battery = true,
                "Mains" | "USB" if read("online") == "1" => mains_online = true,
                _ => {}
            }
        }
    }
    if battery && !mains_online { PowerBudget::Battery } else { PowerBudget::Full }
}

/// One connection to the system bus, kept for every ask.
fn power_saver(system: &mut Option<Bus>) -> bool {
    if system.is_none() {
        *system = Bus::system().ok();
    }
    let Some(system) = system else { return false };
    let reply = system.call_method(
        Some("org.freedesktop.UPower.PowerProfiles"),
        "/org/freedesktop/UPower/PowerProfiles",
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &("org.freedesktop.UPower.PowerProfiles", "ActiveProfile"),
    );
    reply
        .ok()
        .and_then(|reply| reply.body().deserialize::<zbus::zvariant::OwnedValue>().ok())
        .and_then(|value| String::try_from(value).ok())
        .is_some_and(|profile| profile == "power-saver")
}
