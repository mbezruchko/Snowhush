//! What the snow loop must not wait on: the power profile, asked of the system bus, and the desktop pictures,
//! asked of Plasma and decoded for the shadows' tint. Either can stall for as long as the service behind it
//! does, so they run on a thread of their own and send what they find to the loop.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use zbus::blocking::Connection as Bus;

use crate::light::{self, Backdrop, Shade};
use crate::screen::PowerBudget;
use crate::snow::Command;

const POWER_EVERY: Duration = Duration::from_secs(5);
const BACKDROPS_EVERY: Duration = Duration::from_secs(60);

/// Starts the chores; a message on the returned sender asks for the desktop pictures again right away,
/// as when the displays changed.
pub fn start(bus: Bus, commands: Sender<Command>) -> mpsc::Sender<()> {
    let (wanted, requests) = mpsc::channel();
    let _ = std::thread::Builder::new().name("chores".into()).spawn(move || {
        let mut system = None;
        let mut budget = None;
        let mut tints: Vec<(Backdrop, Option<[f32; 4]>)> = Vec::new();
        let mut power_at = Instant::now();
        let mut backdrops_at = Instant::now();
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
            if now >= backdrops_at {
                backdrops_at = now + BACKDROPS_EVERY;
                if commands.send(Command::Backdrops(shades(&bus, &mut tints))).is_err() {
                    return;
                }
            }
            match requests.recv_timeout(power_at.min(backdrops_at).saturating_duration_since(Instant::now())) {
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
