//! The snow loop: one overlay per display, stepped and drawn at the pace the snow needs, the tray's choices,
//! and what pauses the snow. Everything runs on the main thread around one event loop; the tray and the
//! KWin reports come in from threads of their own.

use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop::channel::Event;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, QueueHandle};
use zbus::blocking::Connection as Bus;

use snowhush_core::geometry::V2;
use snowhush_core::simulation::{Borrowed, SnowSimulation, Snowfall, Stray, WindowId};

use crate::chores;
use crate::kwin;
use crate::light::{self, Shade, daylight};
use crate::log;
use crate::prefs::Settings;
use crate::render::{Ambient, Gpu};
use crate::screen::{PowerBudget, Screen};
use crate::source::{Report, Shared, Source};
use crate::tray::Tray;
use crate::windows::{self, Area};

pub enum Command {
    Snowfall(Snowfall),
    Accumulate(bool),
    Melt(bool),
    SnowOnWindows(bool),
    BehindWindows(bool),
    ShowInScreenshots(bool),
    /// Halloween wanted or not, while it is Halloween.
    Halloween(bool),
    /// The screen locked or unlocked.
    Locked(bool),
    /// Blows all the snow away; when calm, for less motion, melts it where it lies instead.
    ClearSnow(bool),
    Power(PowerBudget),
    /// Each display's desktop picture, with the tint it gives the shadows.
    Backdrops(Vec<Shade>),
    Quit,
}

pub struct App {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    qh: QueueHandle<App>,
    gpu: Gpu,
    bus: Bus,
    shared: Arc<Shared>,
    source: Source,
    settings: Settings,
    screens: Vec<Screen>,
    locked: bool,
    quit: bool,
    displays_changed_at: Option<Instant>,
    report: (u64, Option<Arc<Report>>),
    /// The display each window's snow belongs to, by name.
    owners: HashMap<WindowId, String>,
    budget: PowerBudget,
    light: [f32; 3],
    /// Whether it is Halloween, wanted or not.
    season: bool,
    /// Whether the snow has Halloween's ghosts: in season and wanted.
    halloween: bool,
    /// How far into Halloween night it is.
    night: f32,
    tray: Option<ksni::blocking::Handle<Tray>>,
    started: Instant,
    chores_at: Instant,
    light_at: Instant,
    /// Asks the chores for the desktop pictures again.
    backdrops_wanted: mpsc::Sender<()>,
    stats: Stats,
}

pub fn run() -> Result<(), String> {
    let settings = Settings::load();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().map_err(|error| error.to_string())?;
    // First of all: the signals are blocked on this thread so the loop can take them, and threads started
    // later (D-Bus, the tray, the graphics driver's) inherit that, or a quit would kill the app on the spot
    // and leave its script in KWin.
    let signals =
        calloop::signals::Signals::new(&[calloop::signals::Signal::SIGINT, calloop::signals::Signal::SIGTERM]).map_err(|error| error.to_string())?;
    event_loop.handle().insert_source(signals, |_, _, app| app.quit = true).map_err(|error| error.to_string())?;
    let (ping, ping_source) = calloop::ping::make_ping().map_err(|error| error.to_string())?;
    event_loop.handle().insert_source(ping_source, |_, _, _| {}).map_err(|error| error.to_string())?;
    let shared = Arc::new(Shared::new(ping));
    let Some(bus) = kwin::serve(shared.clone()).map_err(|error| format!("D-Bus: {error}"))? else {
        eprintln!("Snowhush is already running.");
        return Ok(());
    };
    // Only now: a second copy that stops here must not wipe the running one's log.
    crate::log::start();
    log!("Snowhush {}", env!("CARGO_PKG_VERSION"));

    let connection = Connection::connect_to_env().map_err(|error| format!("Wayland: {error}"))?;
    let (globals, mut queue) = registry_queue_init::<App>(&connection).map_err(|error| error.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|error| error.to_string())?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|_| "the compositor has no layer shell".to_string())?;
    let gpu = Gpu::new(connection.backend().display_ptr().cast())?;
    log!("adapter: {}", gpu.adapter);

    let (commands, channel) = calloop::channel::channel::<Command>();
    event_loop
        .handle()
        .insert_source(channel, |event, _, app| {
            if let Event::Msg(command) = event {
                app.command(command);
            }
        })
        .map_err(|error| error.to_string())?;

    let now = Instant::now();
    let mut app = App {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        qh: qh.clone(),
        gpu,
        bus: bus.clone(),
        source: Source::start(&bus, shared.clone(), settings.show_in_screenshots),
        shared,
        settings,
        screens: Vec::new(),
        locked: false,
        quit: false,
        displays_changed_at: None,
        report: (0, None),
        owners: HashMap::new(),
        budget: PowerBudget::Full,
        light: daylight(),
        season: false,
        halloween: false,
        night: 0.0,
        tray: None,
        started: now,
        chores_at: now,
        light_at: now,
        backdrops_wanted: chores::start(bus.clone(), commands.clone()),
        stats: Stats::new(now),
    };
    // The displays are known after a round trip; the snow starts on them right away.
    queue.roundtrip(&mut app).map_err(|error| error.to_string())?;
    app.sync_screens();
    WaylandSource::new(connection, queue).insert(event_loop.handle()).map_err(|error| error.to_string())?;

    use ksni::blocking::TrayMethods;
    app.tray = Tray::new(settings, commands.clone()).spawn().inspect_err(|error| log!("tray: {error}")).ok();
    watch_lock(&bus, commands);

    while !app.quit {
        let timeout = app.timeout();
        event_loop.dispatch(timeout, &mut app).map_err(|error| error.to_string())?;
        app.tick()?;
    }

    app.source.stop(&app.bus);
    if let Some(tray) = app.tray.take() {
        tray.shutdown().wait();
    }
    for screen in app.screens.drain(..) {
        screen.release(&app.gpu);
    }
    Ok(())
}

/// Pauses the snow while the screen is locked.
fn watch_lock(bus: &Bus, commands: calloop::channel::Sender<Command>) {
    let bus = bus.clone();
    let _ = std::thread::Builder::new().name("lock".into()).spawn(move || {
        let Ok(rule) = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.ScreenSaver")
            .and_then(|rule| rule.member("ActiveChanged"))
            .map(|rule| rule.build())
        else {
            return;
        };
        let Ok(messages) = zbus::blocking::MessageIterator::for_match_rule(rule, &bus, None) else { return };
        // Asked only once listening, so a lock in between isn't missed: Snowhush may start behind the lock
        // screen, as with automatic login.
        if let Ok(reply) = bus.call_method(
            Some("org.freedesktop.ScreenSaver"),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "GetActive",
            &(),
        ) && let Ok(locked) = reply.body().deserialize::<bool>()
            && commands.send(Command::Locked(locked)).is_err()
        {
            return;
        }
        for message in messages.flatten() {
            if let Ok(locked) = message.body().deserialize::<bool>()
                && commands.send(Command::Locked(locked)).is_err()
            {
                return;
            }
        }
    });
}

impl App {
    fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.quit = true,
            Command::Snowfall(snowfall) => {
                self.settings.snowfall = snowfall;
                self.screens.iter_mut().for_each(|screen| screen.simulation.set_snowfall(snowfall));
            }
            Command::Accumulate(on) => {
                self.settings.accumulate = on;
                self.screens.iter_mut().for_each(|screen| screen.simulation.accumulates = on);
            }
            Command::Melt(on) => {
                self.settings.melt = on;
                self.screens.iter_mut().for_each(|screen| screen.simulation.thaws = on);
            }
            Command::SnowOnWindows(on) => {
                self.settings.snow_on_windows = on;
                if on {
                    self.screens.iter_mut().for_each(Screen::rewatch);
                } else {
                    self.screens.iter_mut().for_each(Screen::drop_window_snow);
                }
            }
            Command::BehindWindows(on) => {
                self.settings.behind_windows = on;
                if on {
                    self.screens.iter_mut().for_each(Screen::drop_window_snow);
                } else {
                    self.screens.iter_mut().for_each(Screen::rewatch);
                }
                self.screens.iter_mut().for_each(|screen| screen.set_behind(on));
            }
            Command::ShowInScreenshots(on) => {
                self.settings.show_in_screenshots = on;
                self.source.show_in_screenshots(&self.bus, on);
            }
            Command::Halloween(on) => {
                self.settings.halloween = on;
                self.update_light();
            }
            Command::ClearSnow(calm) => {
                let direction = self.screens.first().map_or(-1.0, |screen| screen.simulation.breeze_direction());
                self.screens.iter_mut().for_each(|screen| screen.simulation.blow_away(direction, calm));
            }
            Command::Locked(locked) => {
                if locked == self.locked {
                    return;
                }
                self.locked = locked;
                if !locked {
                    self.screens.iter_mut().for_each(Screen::resume);
                }
                log!("locked: {locked}");
            }
            Command::Power(budget) => {
                if budget == self.budget {
                    return;
                }
                self.budget = budget;
                log!("power: {budget:?}");
                self.screens.iter_mut().for_each(|screen| screen.simulation.set_density(density(budget)));
            }
            Command::Backdrops(shades) => self.screens.iter_mut().for_each(|screen| screen.update_backdrop(&shades)),
        }
    }

    /// How long the loop may sleep: until the next frame is due, or for good while the snow is paused.
    fn timeout(&self) -> Option<Duration> {
        let now = Instant::now();
        let chores = self.chores_at.saturating_duration_since(now);
        if self.locked {
            return Some(chores);
        }
        let frame = self.screens.iter().map(|screen| screen.next_due.saturating_duration_since(now)).min();
        Some(frame.map_or(chores, |frame| frame.min(chores)))
    }

    fn tick(&mut self) -> Result<(), String> {
        let now = Instant::now();
        let report = self.shared.report();
        if report.0 != self.report.0 {
            if let Some(next) = &report.1
                && self.report.1.as_deref().map(window_order) != Some(window_order(next))
            {
                log!("windows front to back: {}", window_list(next));
            }
            self.report = report;
            self.place_screens();
            self.hand_over(now);
        }
        // Display changes come in bursts; the layout is read once they settle.
        if let Some(changed_at) = self.displays_changed_at
            && now - changed_at > Duration::from_millis(400)
        {
            self.displays_changed_at = None;
            self.sync_screens();
            let _ = self.backdrops_wanted.send(());
        }

        if now >= self.chores_at {
            self.chores_at = now + Duration::from_secs(1);
            if now >= self.light_at {
                self.light_at = now + Duration::from_secs(60);
                self.update_light();
            }
        }

        if self.locked {
            return Ok(());
        }
        let snow_on_windows = self.settings.snow_on_windows && !self.settings.behind_windows;
        // A drag starting is drawn right away instead of at the next calm frame.
        if self.shared.take_started_moving() && snow_on_windows {
            self.screens.iter_mut().for_each(|screen| screen.wake(now));
        }
        let cursor = self.shared.cursor();
        let time = (now - self.started).as_secs_f32() % 1000.0;
        let ambient = Ambient { light: self.light, shadow_tint: [0.0; 4], night: self.night, lantern: V2::ZERO };
        let report = self.report.1.clone();
        let windows = report.as_deref().map(|report| (report, self.report.0, report.moving));
        if let Some(report) = report.as_deref() {
            self.lend(report, snow_on_windows, now);
        }
        let mut released = false;
        for screen in self.screens.iter_mut().filter(|screen| screen.next_due <= now) {
            let covering = report.as_deref().and_then(|report| windows::covering(report, &screen.bounds));
            if covering.is_some() != screen.covered() {
                log!("{} {}", screen.name, covering.map_or("uncovered".to_string(), |class| format!("covered by {class}")));
                screen.set_covered(&self.gpu, covering.is_some());
            }
            if screen.covered() {
                screen.next_due = now + Duration::from_millis(250);
                continue;
            }
            let began = Instant::now();
            match screen.frame(&mut self.gpu, &self.qh, now, self.budget, ambient, time, cursor, windows, snow_on_windows) {
                Ok(drew) => {
                    self.stats.frame(drew, began.elapsed(), screen.simulation.sprite_count());
                    released |= screen.simulation.take_snowfall_released();
                }
                Err(error) => log!("{}: frame failed: {error}", screen.name),
            }
        }
        if released {
            self.screens.iter_mut().for_each(|screen| screen.simulation.set_snowfall_held(false));
        }
        self.pass_snow();
        self.stats.report(Instant::now());
        Ok(())
    }

    /// The light of the hour, and Halloween with it: its ghosts, its night and the tray's pumpkin.
    fn update_light(&mut self) {
        let season = light::is_halloween();
        if season != self.season {
            self.season = season;
            if let Some(tray) = &self.tray {
                tray.update(|tray| tray.season = season);
            }
        }
        let halloween = season && self.settings.halloween;
        if halloween != self.halloween {
            self.halloween = halloween;
            log!("halloween: {halloween}");
            self.screens.iter_mut().for_each(|screen| screen.simulation.set_halloween(halloween));
        }
        self.light = daylight();
        self.night = light::night(halloween);
    }

    /// One overlay per display: new displays get snow, removed ones lose theirs.
    fn sync_screens(&mut self) {
        let mut current = std::mem::take(&mut self.screens);
        for output in self.outputs.outputs() {
            let Some(info) = self.outputs.info(&output) else { continue };
            let (Some(name), Some(position), Some(size)) = (info.name.clone(), info.logical_position, info.logical_size) else { continue };
            let bounds = Area { x: position.0 as f64, y: position.1 as f64, w: size.0 as f64, h: size.1 as f64 };
            if let Some(index) = current.iter().position(|screen| screen.name == name && screen.output == output) {
                let mut screen = current.swap_remove(index);
                screen.bounds = bounds;
                self.screens.push(screen);
                continue;
            }
            let refresh = info.modes.iter().find(|mode| mode.current).map_or(60, |mode| (mode.refresh_rate as f32 / 1000.0).round() as u32);
            log!("display {name}: {bounds:?}, scale {}, {refresh} Hz", info.scale_factor);
            let mut simulation = new_simulation(&bounds, &self.settings, self.budget);
            simulation.set_halloween(self.halloween);
            let screen = Screen::new(
                (&self.compositor, &self.layer_shell, &self.qh),
                output.clone(),
                name,
                bounds,
                refresh,
                simulation,
                self.settings.behind_windows,
            );
            self.screens.push(screen);
        }
        for screen in current {
            log!("display {} gone", screen.name);
            screen.release(&self.gpu);
        }
        self.place_screens();
    }

    /// Moves a window's snow to the display it now belongs to, as when it is dragged across: each display has
    /// a simulation of its own, and the window's snow would otherwise fall off the old one's edge and be lost.
    fn hand_over(&mut self, now: Instant) {
        let Some(report) = self.report.1.clone() else { return };
        let mut owners = HashMap::new();
        for window in &report.windows {
            let Some(owner) = windows::owner(&report, window) else { continue };
            let id = windows::window_id(&window.id);
            if let Some(previous) = self.owners.get(&id).cloned()
                && previous != owner
            {
                self.move_snow(id, &previous, owner, now);
            }
            owners.insert(id, owner.to_string());
        }
        self.owners = owners;
    }

    fn move_snow(&mut self, id: WindowId, from: &str, to: &str, now: Instant) {
        let (Some(a), Some(b)) = (self.screens.iter().position(|s| s.name == from), self.screens.iter().position(|s| s.name == to)) else {
            return;
        };
        let Some(snow) = self.screens[a].simulation.take_window(id) else { return };
        let (old, new) = (self.screens[a].area(), self.screens[b].area());
        // Overlay coordinates run right and up from each overlay's bottom left.
        let shift = V2::new((old.x - new.x) as f32, (new.bottom() - old.bottom()) as f32);
        self.screens[b].simulation.adopt_window(id, snow, shift);
        self.screens[a].rewatch();
        self.screens[b].rewatch();
        self.screens[b].wake(now);
        log!("window snow from {from} to {to}");
    }

    /// Gives each display about to draw the snow of windows reaching over it that another display keeps.
    fn lend(&mut self, report: &Report, snow_on_windows: bool, now: Instant) {
        for index in 0..self.screens.len() {
            if self.screens[index].next_due > now {
                continue;
            }
            let borrowed = if snow_on_windows { self.borrowed(index, report) } else { Vec::new() };
            self.screens[index].simulation.set_borrowed(borrowed);
        }
    }

    /// The snow of windows kept by other displays, where it reaches over display `index`, in its coordinates: a
    /// window lying across two displays keeps its drift in one piece instead of ending at the edge.
    fn borrowed(&self, index: usize, report: &Report) -> Vec<Borrowed> {
        let screen = &self.screens[index];
        let here = screen.area();
        let mut borrowed = Vec::new();
        for window in &report.windows {
            let id = windows::window_id(&window.id);
            let Some(owner) = self.owners.get(&id).filter(|owner| **owner != screen.name) else { continue };
            // The drift stands above the window's top edge, never as high as this.
            let near = window.x < here.right() && window.x + window.w > here.x && window.y - 300.0 < here.bottom() && window.y + 20.0 > here.y;
            let Some(home) = self.screens.iter().find(|other| other.name == *owner).filter(|_| near) else { continue };
            let Some(lent) = home.simulation.lend(id) else { continue };
            let there = home.area();
            let lent = lent.shifted(V2::new((there.x - here.x) as f32, (here.bottom() - there.bottom()) as f32));
            let [left, bottom, right, top] = lent.bounds();
            if right > 0.0 && left < here.w as f32 && top > 0.0 && bottom < here.h as f32 {
                borrowed.push(lent);
            }
        }
        borrowed
    }

    /// Passes on the snow that went from one display to another this frame: snow that fell on a window's drift
    /// lent by another display goes to that one, and snow thrown past an edge two displays share goes on falling
    /// on the other side.
    fn pass_snow(&mut self) {
        for index in 0..self.screens.len() {
            let here = self.screens[index].area();
            for caught in self.screens[index].simulation.take_caught() {
                let Some(owner) = self.owners.get(&caught.id) else { continue };
                let Some(home) = self.screens.iter_mut().find(|other| other.name == *owner) else { continue };
                let shift = (here.x - home.area().x) as f32;
                home.simulation.catch(&caught, shift);
            }
            let mut passed: Vec<(usize, Vec<Stray>)> = Vec::new();
            for stray in self.screens[index].simulation.take_strays() {
                let point = stray.position();
                let (x, y) = (here.x + point.x as f64, here.bottom() - point.y as f64);
                let Some(next) = self.screens.iter().enumerate().position(|(other, screen)| {
                    let there = screen.area();
                    other != index && !screen.covered() && x >= there.x && x < there.right() && y < there.bottom()
                }) else {
                    continue;
                };
                match passed.iter_mut().find(|(other, _)| *other == next) {
                    Some((_, strays)) => strays.push(stray),
                    None => passed.push((next, vec![stray])),
                }
            }
            for (next, strays) in passed {
                let there = self.screens[next].area();
                let shift = V2::new((here.x - there.x) as f32, (there.bottom() - here.bottom()) as f32);
                self.screens[next].simulation.adopt_strays(strays, shift);
            }
        }
    }

    /// Fits each overlay to its display's work area, and tells each simulation where its walls are.
    fn place_screens(&mut self) {
        let Some(report) = self.report.1.clone() else { return };
        for screen in &mut self.screens {
            let Some(display) = report.screens.iter().find(|display| display.name == screen.name) else { continue };
            screen.place(Area::bounds(display), Area::work(display), windows::shelf(&report, display));
            let walls = windows::walls(display, &report.screens);
            screen.simulation.walls = walls;
            screen.simulation.neighbours = (!walls.0, !walls.1);
        }
    }
}

/// The reported windows by app, front to back, for the log.
fn window_order(report: &Report) -> String {
    report.windows.iter().map(|window| window.class.as_str()).collect::<Vec<_>>().join(", ")
}

/// The reported windows with where they are, for telling which one the snow lies on.
fn window_list(report: &Report) -> String {
    let window = |window: &crate::source::Window| {
        let solid = if window.solid { "" } else { ", not solid" };
        format!("{} ({}x{} at {},{}{solid})", window.class, window.w, window.h, window.x, window.y)
    };
    report.windows.iter().map(window).collect::<Vec<_>>().join(", ")
}

fn new_simulation(bounds: &Area, settings: &Settings, budget: PowerBudget) -> SnowSimulation {
    let mut simulation = SnowSimulation::new(bounds.w as f32, bounds.h as f32, settings.snowfall);
    simulation.thaws = settings.melt;
    simulation.accumulates = settings.accumulate;
    simulation.set_density(density(budget));
    simulation
}

/// Power saver makes do with a little less snow.
fn density(budget: PowerBudget) -> f32 {
    if budget == PowerBudget::Saver { 0.75 } else { 1.0 }
}

/// What a frame costs, written to the log twice a minute.
struct Stats {
    since: Instant,
    steps: u32,
    draws: u32,
    busy: Duration,
    sprites: usize,
}

impl Stats {
    fn new(now: Instant) -> Stats {
        Stats { since: now, steps: 0, draws: 0, busy: Duration::ZERO, sprites: 0 }
    }

    fn frame(&mut self, drew: bool, busy: Duration, sprites: usize) {
        self.steps += 1;
        self.draws += drew as u32;
        self.busy += busy;
        self.sprites = self.sprites.max(sprites);
    }

    fn report(&mut self, now: Instant) {
        let elapsed = (now - self.since).as_secs_f64();
        if elapsed < 30.0 {
            return;
        }
        let busy = self.busy.as_secs_f64();
        log!(
            "frames: {:.1} steps/s, {:.1} draws/s, {:.0} µs per step, {:.2}% of a core, up to {} sprites",
            self.steps as f64 / elapsed,
            self.draws as f64 / elapsed,
            busy * 1e6 / self.steps.max(1) as f64,
            busy / elapsed * 100.0,
            self.sprites
        );
        *self = Stats::new(now);
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(screen) = self.screens.iter_mut().find(|screen| screen.owns(surface)) {
            screen.shown();
        }
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.displays_changed_at = Some(Instant::now());
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.displays_changed_at = Some(Instant::now());
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.displays_changed_at = Some(Instant::now());
    }
}

impl LayerShellHandler for App {
    /// The compositor took the surface away, as when its display goes: the display sync makes a new one if needed.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(index) = self.screens.iter().position(|screen| screen.is(layer)) {
            let screen = self.screens.swap_remove(index);
            log!("{}: overlay closed", screen.name);
            screen.release(&self.gpu);
            self.displays_changed_at = Some(Instant::now());
        }
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        if let Some(screen) = self.screens.iter_mut().find(|screen| screen.is(layer)) {
            let (width, height) = configure.new_size;
            screen.configure(&self.gpu, width, height);
        }
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_registry!(App);
delegate_dispatch2!(App);
