use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, CreateWaitableTimerW, INFINITE, SetWaitableTimer, TIMER_ALL_ACCESS,
    WaitForMultipleObjects,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MB_ICONERROR, MB_OK, MSG, MWMO_INPUTAVAILABLE, MessageBoxW, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostMessageW,
    QS_ALLINPUT, TranslateMessage, WM_CLOSE,
};
use windows::core::{HSTRING, PCWSTR, Result, w};

use snowhush_core::geometry::V2;
use snowhush_core::halloween::Choice;
use snowhush_core::simulation::{Borrowed, Mapping, Mode, ScatterPiece, SnowSimulation, Stray};

use crate::desktop::{self, DesktopLayer};
use crate::halloween;
use crate::light::{Sky, Wallpapers};
use crate::log;
use crate::prefs::Settings;
use crate::render::Gpu;
use crate::screen::{BACKDROP_CHANGED, Clock, DISPLAYS_CHANGED, Monitor, PowerBudget, Screen, dip_size, listen, monitors, walls, watch_taskbar};
use crate::tracker::{self, Tracker};

/// A piece of the welcome window crumbling into snow, in physical screen pixels.
pub struct Piece {
    pub x: f32,
    pub y: f32,
    pub velocity: V2,
    pub color: [f32; 3],
}

pub enum Command {
    /// How much falls, 0 to 1.
    Intensity(f32),
    /// How opaque what falls is drawn, 0.1 to 1.
    FallingOpacity(f32),
    Ground(bool),
    Melt(bool),
    SnowOnWindows(bool),
    BehindWindows(bool),
    OverFullScreen(bool),
    /// Every frame at the display's highest rate, or as fast as what moves needs.
    Performance(bool),
    BehindTaskbar(bool),
    /// Explorer started again, taking the desktop's wallpaper layer with it.
    ShellRestarted,
    ShowInScreenshots(bool),
    /// The snow settles on a selection dragged across the desktop too.
    DeepInteraction(bool),
    /// Off while the screen is locked, asleep or the session is away.
    Active(bool),
    Scatter(Vec<Piece>),
    /// Starts the snowfall held back for the welcome window.
    ReleaseSnowfall,
    /// Blows all the snow away; when calm, for less motion, melts it where it lies instead.
    ClearSnow(bool),
    /// Halloween turned on or off from the menu.
    Halloween(Choice),
    /// Other kinds fall, once a gust has blown what is there away; when calm, for less motion, it melts where it
    /// lies instead. A kind only joining what falls starts at once.
    SwitchMode {
        mode: Mode,
        mixed: bool,
        mix: [Mode; 2],
        calm: bool,
    },
    Quit,
}

/// Raw handles cross to the snow thread as numbers: the thread only waits on the event and posts to the window.
pub struct Links {
    pub wake: isize,
    pub controller: isize,
}

pub fn run(commands: Receiver<Command>, links: Links, settings: Settings) {
    if let Err(error) = run_loop(&commands, &links, settings) {
        log!("stopped: {error}");
        let text = HSTRING::from(format!("Snowhush couldn't start drawing snow.\n\n{error}"));
        unsafe {
            MessageBoxW(None, &text, w!("Snowhush"), MB_OK | MB_ICONERROR);
            let _ = PostMessageW(Some(HWND(links.controller as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn run_loop(commands: &Receiver<Command>, links: &Links, mut settings: Settings) -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let mut gpu = Gpu::new()?;
    log!("adapter: {}", gpu.adapter);
    let _listener = listen()?;
    let mut layer = settings.behind_windows.then(find_layer).flatten();
    // The displays whose taskbar a full-screen app the snow shows over has put away, and since when each display has
    // been seen that way or not.
    let mut full_screen: HashSet<String> = HashSet::new();
    let mut full_screen_seen: HashMap<String, (bool, i64)> = HashMap::new();
    let mut screens = sync_screens(&gpu, &settings, &full_screen, Vec::new(), layer, false);
    let timer = unsafe {
        CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
            .or_else(|_| CreateWaitableTimerW(None, false, PCWSTR::null()))?
    };
    let wake = HANDLE(links.wake as _);
    let mut clock = Clock::new();
    let started = Clock::now();
    let mut active = true;
    let mut budget = PowerBudget::Full;
    let mut spooky = halloween::shows(&settings);
    let mut sky = Sky::now(spooky);
    let wallpapers = Wallpapers::new().inspect_err(|error| log!("wallpapers unavailable: {error}")).ok();
    let mut chores_at = 0;
    let mut light_at = 0;
    let mut displays_changed_at = None;
    let mut desktop_logged = String::new();
    let mut stats = Stats::new(started);
    let mut tracker = Tracker::new();
    // The display each window's snow lies on, as of the last window list.
    let mut owners = HashMap::new();
    watch_taskbar(links.wake);

    loop {
        screens.iter().filter(|screen| screen.under_taskbar()).for_each(Screen::raise);
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Quit => return Ok(()),
                Command::Intensity(intensity) => {
                    settings.intensity = (intensity * 100.0).round() as u32;
                    let now = Clock::now();
                    screens.iter_mut().for_each(|screen| {
                        screen.simulation.set_intensity(intensity);
                        screen.wake(now);
                    });
                }
                Command::FallingOpacity(opacity) => {
                    settings.opacity = (opacity * 100.0).round() as u32;
                    screens.iter_mut().for_each(|screen| screen.simulation.falling_opacity = opacity);
                }
                Command::Ground(on) => {
                    settings.ground = on;
                    let accumulates = settings.accumulates();
                    screens.iter_mut().for_each(|screen| {
                        screen.simulation.ground_settles = on;
                        screen.simulation.accumulates = accumulates;
                    });
                }
                Command::Melt(on) => {
                    settings.melt = on;
                    screens.iter_mut().for_each(|screen| screen.simulation.thaws = on);
                }
                Command::SnowOnWindows(on) => {
                    settings.snow_on_windows = on;
                    let accumulates = settings.accumulates();
                    screens.iter_mut().for_each(|screen| screen.simulation.accumulates = accumulates);
                    if !on {
                        screens.iter_mut().for_each(Screen::drop_window_snow);
                    }
                }
                Command::BehindWindows(on) => {
                    settings.behind_windows = on;
                    layer = on.then(find_layer).flatten();
                    screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, true);
                }
                Command::OverFullScreen(on) => {
                    settings.over_full_screen = on;
                    if !on && !full_screen.is_empty() {
                        full_screen.clear();
                        screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, false);
                    }
                }
                Command::DeepInteraction(on) => {
                    settings.deep_interaction = on;
                    if !on {
                        tracker.drop_marquee();
                    }
                }
                Command::Performance(max) => {
                    settings.max_performance = max;
                    screens.iter_mut().for_each(|screen| screen.max_rate = max);
                }
                Command::BehindTaskbar(on) => {
                    settings.behind_taskbar = on;
                    screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, true);
                }
                Command::ShellRestarted => {
                    if settings.behind_windows {
                        layer = find_layer();
                        screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, true);
                    }
                }
                Command::ShowInScreenshots(on) => {
                    settings.show_in_screenshots = on;
                    screens.iter().for_each(|screen| screen.show_in_screenshots(on));
                }
                Command::Scatter(pieces) => {
                    let Some(first) = pieces.first() else { continue };
                    let index = screens.iter().position(|screen| screen.contains(first.x, first.y)).unwrap_or(0);
                    let Some(screen) = screens.get_mut(index) else { continue };
                    let local: Vec<ScatterPiece> = pieces
                        .iter()
                        .map(|piece| ScatterPiece { position: screen.local(piece.x, piece.y), velocity: piece.velocity, color: piece.color })
                        .collect();
                    screen.simulation.scatter(&local);
                    screen.wake(Clock::now());
                }
                Command::ReleaseSnowfall => {
                    settings.welcomed = true;
                    screens.iter_mut().for_each(|screen| screen.simulation.set_snowfall_held(false));
                }
                Command::ClearSnow(calm) => {
                    let direction = screens.first().map_or(-1.0, |screen| screen.simulation.breeze_direction());
                    screens.iter_mut().for_each(|screen| screen.simulation.blow_away(direction, calm));
                }
                Command::Halloween(choice) => {
                    settings.halloween = choice;
                    chores_at = 0;
                    light_at = 0;
                }
                Command::SwitchMode { mode, mixed, mix, calm } => {
                    settings.mode = mode;
                    settings.mixed = mixed;
                    settings.mix = mix;
                    light_at = 0;
                    let modes = settings.modes();
                    let direction = screens.first().map_or(-1.0, |screen| screen.simulation.breeze_direction());
                    let now = Clock::now();
                    screens.iter_mut().for_each(|screen| {
                        screen.simulation.switch_modes(&modes, direction, calm);
                        screen.wake(now);
                    });
                }
                Command::Active(on) => {
                    if on && !active {
                        screens.iter_mut().for_each(Screen::resume);
                    }
                    tracker::watch_shell_state(on);
                    active = on;
                    log!("active: {on}");
                }
            }
        }

        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        let mut now = Clock::now();
        if DISPLAYS_CHANGED.take() {
            displays_changed_at = Some(now);
        }
        // Display changes come in bursts; the layout is read once they settle.
        if let Some(changed_at) = displays_changed_at
            && clock.seconds(now - changed_at) > 0.4
        {
            displays_changed_at = None;
            if settings.behind_windows {
                layer = find_layer();
            }
            screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, false);
            clock.refresh();
            light_at = 0;
        }
        if BACKDROP_CHANGED.take() {
            light_at = 0;
        }

        if now >= chores_at {
            chores_at = now + clock.frequency;
            clock.refresh();
            // In case a display change went unannounced.
            let current = monitors(over_taskbar(&settings, &screens), &full_screen);
            if current.len() != screens.len() || screens.iter().any(|screen| !current.contains(&screen.monitor)) {
                DISPLAYS_CHANGED.set(true);
            }
            let next = power_budget();
            if next != budget {
                budget = next;
                log!("power: {budget:?}");
                let density = if budget == PowerBudget::Saver { 0.75 } else { 1.0 };
                screens.iter_mut().for_each(|screen| screen.simulation.set_density(density));
            }
            // Behind windows, a SetWindowPos every second on every monitor's child overlay restacks them under the
            // icons and the desktop layer blinks — often on one display, then another. Raise only when the
            // wallpaper layer's children change (a live wallpaper landing on top, Explorer rebuilding, …).
            if settings.behind_windows {
                let tree = desktop::tree();
                if tree != desktop_logged {
                    log!("desktop: {tree}");
                    desktop_logged = tree;
                    screens.iter().for_each(Screen::raise);
                }
            } else {
                screens.iter().for_each(Screen::raise);
            }
            // Behind windows, the overlays live inside Explorer's windows and vanish when it restarts; until the
            // wallpaper layer is found they wait on top of everything instead.
            if settings.behind_windows && (layer.is_none() || screens.iter().any(|screen| !screen.alive())) {
                let found = find_layer();
                if found.is_some() || screens.iter().any(|screen| !screen.alive()) {
                    layer = found;
                    screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, true);
                }
            }
            if now >= light_at {
                light_at = now + clock.frequency * 60;
                let on = halloween::shows(&settings);
                if on != spooky {
                    spooky = on;
                    log!("halloween: {on}");
                    screens.iter_mut().for_each(|screen| {
                        screen.simulation.set_halloween(on);
                        screen.wake(now);
                    });
                }
                sky = Sky::now(spooky);
                if let Some(wallpapers) = &wallpapers {
                    screens.iter_mut().for_each(|screen| screen.update_backdrop(wallpapers));
                }
            }
        }

        if active {
            // A drag starting is drawn right away instead of at the next calm frame.
            if tracker.take_started_moving() && settings.snow_on_windows {
                screens.iter_mut().for_each(|screen| screen.wake(now));
            }
            // The desktop's rubber band moves with the pointer, so it is drawn right away too.
            if settings.snow_on_windows && settings.deep_interaction && tracker.follow_marquee() {
                screens.iter_mut().for_each(|screen| screen.wake(now));
            }
            tracker.set_displays(screens.iter().map(|screen| (screen.monitor.name.as_str(), screen.monitor.rect, screen.monitor.bounds)));
            if screens.iter().any(|screen| screen.next_due <= now + clock.tolerance()) && tracker.refresh(&clock, now) {
                if tracker.animating() {
                    screens.iter_mut().for_each(|screen| screen.wake(now));
                }
                if settings.snow_on_windows {
                    hand_over(&mut screens, &mut owners, &tracker, now);
                }
            }
            // Behind the windows the snow still settles on them: what lies on a window's top edge is above it, in plain view.
            let windows = settings.snow_on_windows.then_some(&tracker);
            lend(&mut screens, windows, now + clock.tolerance());
            let time = (clock.seconds(now - started) % 1000.0) as f32;
            let mut lost = false;
            let mut released = false;
            let mut reshaped = false;
            for screen in screens.iter_mut().filter(|screen| screen.next_due <= now + clock.tolerance()) {
                let in_front = tracker.covering(&screen.monitor.bounds);
                // Shown over a full-screen app, the snow reaches down to the bottom of the display while the app has put
                // the taskbar away, so the drift lies along the bottom instead of over a taskbar no longer there; a
                // change counts once it has lasted, as the app goes full screen or comes out of it in a few steps.
                if settings.over_full_screen && !settings.behind_windows {
                    let wanted = in_front.is_some();
                    match full_screen_seen.get(&screen.monitor.name) {
                        Some(&(state, _)) if state == wanted => {}
                        _ => {
                            full_screen_seen.insert(screen.monitor.name.clone(), (wanted, now));
                        }
                    }
                    if let Some(&(state, since)) = full_screen_seen.get(&screen.monitor.name)
                        && state != full_screen.contains(&screen.monitor.name)
                        && clock.seconds(now - since) > 0.3
                    {
                        log!("{} {}", screen.monitor.name, if state { "taskbar put away by a full-screen app" } else { "taskbar back" });
                        if state {
                            full_screen.insert(screen.monitor.name.clone());
                        } else {
                            full_screen.remove(&screen.monitor.name);
                        }
                        reshaped = true;
                    }
                }
                let covering = if settings.over_full_screen { None } else { in_front };
                if covering.is_some() != screen.covered() {
                    log!("{} {}", screen.monitor.name, covering.map_or("uncovered".to_string(), |class| format!("covered by {class}")));
                    screen.set_covered(&gpu, covering.is_some());
                }
                if screen.hidden() {
                    screen.next_due = now + clock.frequency / 4;
                    continue;
                }
                let began = Clock::now();
                match screen.frame(&gpu, &clock, now, budget, sky, time, windows) {
                    Ok(drew) => {
                        stats.frame(drew, Clock::now() - began, screen.simulation.sprite_count());
                        released |= screen.simulation.take_snowfall_released();
                    }
                    Err(error) => {
                        log!("frame failed: {error}");
                        lost = true;
                    }
                }
            }
            pass_snow(&mut screens, &tracker);
            // The first display the welcome window's snow reaches starts the snowfall on all of them.
            if released {
                settings.welcomed = true;
                screens.iter_mut().for_each(|screen| screen.simulation.set_snowfall_held(false));
            }
            if lost {
                screens.clear();
                gpu = Gpu::new()?;
                screens = sync_screens(&gpu, &settings, &full_screen, Vec::new(), layer, false);
            } else if reshaped {
                screens = sync_screens(&gpu, &settings, &full_screen, screens, layer, false);
            }
            now = Clock::now();
            stats.report(&clock, now, &mut tracker);
        }

        let due = screens.iter().map(|screen| screen.next_due).min().filter(|_| active);
        unsafe {
            if let Some(due) = due {
                let wait = -(((due - now).max(0) as i128 * 10_000_000) / clock.frequency as i128) as i64;
                let _ = SetWaitableTimer(timer, &wait, 0, None, None, false);
                // Messages wait for the next frame: window reports arrive with every mouse move,
                // and waking for each would cost more than the snow.
                WaitForMultipleObjects(&[wake, timer], false, INFINITE);
            } else {
                MsgWaitForMultipleObjectsEx(Some(&[wake]), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
    }
}

/// One overlay per display: new displays get snow, removed ones lose theirs, and a display that changed
/// size keeps its snow in a fresh overlay. `rebuild` gives every display a fresh overlay, as when the snow
/// moves behind windows or back.
/// Whether the overlays reach down over the taskbar: behind it, or while it rains, which runs down the taskbar as down a
/// window. Snow and leaves lie on its top edge. What falls is what the screens show, the switch to another kind
/// coming only once the gust has cleared the screen.
fn over_taskbar(settings: &Settings, screens: &[Screen]) -> bool {
    settings.behind_taskbar || screens.first().map_or(settings.modes().contains(&Mode::Rain), |screen| screen.simulation.raining())
}

fn sync_screens(
    gpu: &Gpu,
    settings: &Settings,
    full_screen: &HashSet<String>,
    mut current: Vec<Screen>,
    layer: Option<DesktopLayer>,
    rebuild: bool,
) -> Vec<Screen> {
    let mut result = Vec::new();
    let displays = monitors(over_taskbar(settings, &current), full_screen);
    for monitor in displays.iter().cloned() {
        let kept = current.iter().position(|screen| screen.monitor.name == monitor.name);
        let simulation = match kept.map(|i| current.swap_remove(i)) {
            Some(screen) if screen.monitor == monitor && !rebuild && screen.behind_windows() == layer.is_some() => {
                result.push(screen);
                continue;
            }
            Some(screen) => {
                let mut simulation = screen.simulation;
                let (width, height) = dip_size(&monitor);
                // Reaching further down, as over a taskbar a full-screen app has put away, the drift falls from
                // where it lay onto the new bottom; stopping short again, what lay along the bottom is under the
                // taskbar. Everything else stays where it is on the screen.
                let drop = monitor.rect.bottom - screen.monitor.rect.bottom;
                let scale = monitor.dpi.max(96) as f32 / 96.0;
                if drop > 0 && monitor.rect.top == screen.monitor.rect.top {
                    simulation.lower_ground(width as f32, height as f32, drop as f32 / scale);
                } else if drop < 0 && monitor.rect.top == screen.monitor.rect.top {
                    simulation.raise_ground(width as f32, height as f32, -drop as f32 / scale);
                } else {
                    simulation.resize(width as f32, height as f32);
                }
                simulation
            }
            None => new_simulation(&monitor, settings),
        };
        log!("display {}: {:?}, {} dpi, {} Hz", monitor.name, monitor.rect, monitor.dpi, monitor.refresh);
        let screen = Screen::new(gpu, monitor.clone(), simulation, settings, layer).or_else(|(error, simulation)| {
            // Snow that can't go behind windows stays on top of them rather than vanishing.
            if layer.is_none() {
                return Err((error, simulation));
            }
            log!("overlay behind windows failed: {error}");
            Screen::new(gpu, monitor, simulation, settings, None)
        });
        match screen {
            Ok(screen) => result.push(screen),
            Err((error, _)) => log!("overlay failed: {error}"),
        }
    }
    for screen in &mut result {
        let walls = walls(&screen.monitor, &displays);
        screen.simulation.walls = walls;
        screen.simulation.neighbours = (!walls.0, !walls.1);
    }
    result
}

/// Takes overlay coordinates on `from`'s display to `to`'s, through the physical pixels both lie on: displays can
/// differ in how many pixels make a DIP.
fn mapping(from: &Screen, to: &Screen) -> Mapping {
    let (a, b) = (from.pixels_per_point(), to.pixels_per_point());
    let (here, there) = (&from.monitor.rect, &to.monitor.rect);
    Mapping { scale: a / b, shift: V2::new((here.left - there.left) as f32 / b, (there.bottom - here.bottom) as f32 / b) }
}

/// Moves windows' snow to the display it now lies on, as when a window is dragged across: each display has a
/// simulation of its own, and the snow would otherwise fall off the old one's edge and be lost.
fn hand_over(screens: &mut [Screen], owners: &mut HashMap<isize, String>, tracker: &Tracker, now: i64) {
    for (&id, owner) in &tracker.owners {
        let Some(previous) = owners.get(&id).filter(|previous| *previous != owner) else { continue };
        let (Some(a), Some(b)) =
            (screens.iter().position(|screen| screen.monitor.name == *previous), screens.iter().position(|screen| screen.monitor.name == *owner))
        else {
            continue;
        };
        let Some(snow) = screens[a].simulation.take_window(id) else { continue };
        let mapping = mapping(&screens[a], &screens[b]);
        screens[b].simulation.adopt_window(id, snow, mapping);
        screens[a].rewatch();
        screens[b].rewatch();
        screens[b].wake(now);
        log!("window snow from {previous} to {owner}");
    }
    owners.clone_from(&tracker.owners);
}

/// Gives each display about to draw the snow of windows reaching over it that another display keeps.
fn lend(screens: &mut [Screen], tracker: Option<&Tracker>, due: i64) {
    for index in 0..screens.len() {
        if screens[index].next_due > due {
            continue;
        }
        let borrowed = tracker.map_or_else(Vec::new, |tracker| borrowed(screens, index, tracker));
        screens[index].simulation.set_borrowed(borrowed);
    }
}

/// The snow of windows kept by other displays, where it reaches over display `index`, in its coordinates: a window
/// lying across two displays keeps its drift in one piece instead of ending at the edge.
fn borrowed(screens: &[Screen], index: usize, tracker: &Tracker) -> Vec<Borrowed> {
    let screen = &screens[index];
    let here = &screen.monitor.rect;
    let (width, height) = screen.size();
    // The drift stands above the window's top edge, never as high as this.
    let reach = (300.0 * screen.pixels_per_point()) as i32;
    let mut borrowed = Vec::new();
    for (&id, owner) in &tracker.owners {
        if *owner == screen.monitor.name {
            continue;
        }
        let Some(r) = tracker.rect(id) else { continue };
        let near = r.left < here.right && r.right > here.left && r.top - reach < here.bottom && r.top + 20 > here.top;
        let Some(home) = screens.iter().find(|other| other.monitor.name == *owner).filter(|_| near) else { continue };
        let Some(lent) = home.simulation.lend(id) else { continue };
        let lent = lent.mapped(mapping(home, screen));
        let [left, bottom, right, top] = lent.bounds();
        if right > 0.0 && left < width as f32 && top > 0.0 && bottom < height as f32 {
            borrowed.push(lent);
        }
    }
    borrowed
}

/// Passes on the snow that went from one display to another this frame: snow that fell on a window's drift lent by
/// another display goes to that one, and snow thrown past an edge two displays share goes on falling on the other side.
fn pass_snow(screens: &mut [Screen], tracker: &Tracker) {
    for index in 0..screens.len() {
        for caught in screens[index].simulation.take_caught() {
            let Some(home) = tracker.owners.get(&caught.id).and_then(|owner| screens.iter().position(|screen| screen.monitor.name == *owner)) else {
                continue;
            };
            let mapping = mapping(&screens[index], &screens[home]);
            screens[home].simulation.catch(&caught, mapping);
        }
        let strays = screens[index].simulation.take_strays();
        if strays.is_empty() {
            continue;
        }
        let (here, scale) = (screens[index].monitor.rect, screens[index].pixels_per_point());
        let mut passed: Vec<(usize, Vec<Stray>)> = Vec::new();
        for stray in strays {
            let point = stray.position();
            let (x, y) = (here.left as f32 + point.x * scale, here.bottom as f32 - point.y * scale);
            let Some(next) = screens.iter().enumerate().position(|(other, screen)| {
                let r = &screen.monitor.rect;
                other != index && !screen.covered() && x >= r.left as f32 && x < r.right as f32 && y < r.bottom as f32
            }) else {
                continue;
            };
            match passed.iter_mut().find(|(other, _)| *other == next) {
                Some((_, strays)) => strays.push(stray),
                None => passed.push((next, vec![stray])),
            }
        }
        for (next, strays) in passed {
            let mapping = mapping(&screens[index], &screens[next]);
            screens[next].simulation.adopt_strays(strays, mapping);
        }
    }
}

fn find_layer() -> Option<DesktopLayer> {
    let layer = desktop::find();
    log!("desktop layer: {layer:?}");
    layer
}

fn new_simulation(monitor: &Monitor, settings: &Settings) -> SnowSimulation {
    let (width, height) = dip_size(monitor);
    let mut simulation = SnowSimulation::new(width as f32, height as f32, settings.intensity as f32 / 100.0);
    simulation.thaws = settings.melt;
    simulation.accumulates = settings.accumulates();
    simulation.ground_settles = settings.ground;
    simulation.falling_opacity = settings.opacity as f32 / 100.0;
    simulation.set_snowfall_held(!settings.welcomed);
    simulation.set_halloween(halloween::shows(settings));
    simulation.set_modes(&settings.modes());
    simulation
}

fn power_budget() -> PowerBudget {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return PowerBudget::Full;
    }
    if status.SystemStatusFlag == 1 {
        PowerBudget::Saver
    } else if status.ACLineStatus == 0 {
        PowerBudget::Battery
    } else {
        PowerBudget::Full
    }
}

/// What a frame costs on this thread, written to the log twice a minute.
struct Stats {
    since: i64,
    steps: u32,
    draws: u32,
    busy: i64,
    sprites: usize,
}

impl Stats {
    fn new(now: i64) -> Stats {
        Stats { since: now, steps: 0, draws: 0, busy: 0, sprites: 0 }
    }

    fn frame(&mut self, drew: bool, busy: i64, sprites: usize) {
        self.steps += 1;
        self.draws += drew as u32;
        self.busy += busy;
        self.sprites = self.sprites.max(sprites);
    }

    fn report(&mut self, clock: &Clock, now: i64, tracker: &mut Tracker) {
        let elapsed = clock.seconds(now - self.since);
        if elapsed < 30.0 {
            return;
        }
        let busy = clock.seconds(self.busy);
        let listing = clock.seconds(tracker.cost);
        log!(
            "frames: {:.1} steps/s, {:.1} draws/s, {:.0} µs per step, {:.2}% of a core, up to {} sprites; windows: {:.1} reads/s, {:.0} µs per read",
            self.steps as f64 / elapsed,
            self.draws as f64 / elapsed,
            busy * 1e6 / self.steps.max(1) as f64,
            (busy + listing) / elapsed * 100.0,
            self.sprites,
            tracker.listings as f64 / elapsed,
            listing * 1e6 / tracker.listings.max(1) as f64
        );
        *self = Stats::new(now);
        tracker.listings = 0;
        tracker.cost = 0;
    }
}
