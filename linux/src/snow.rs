//! The snow loop: one overlay per display, stepped and drawn at the pace the snow needs, the tray's choices,
//! and what pauses the snow. Everything runs on the main thread around one event loop; the tray and the
//! KWin reports come in from threads of their own.

use std::sync::Arc;
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

use snowhush::simulation::{SnowSimulation, Snowfall};

use crate::kwin::{self, Report, Shared};
use crate::light::{self, daylight};
use crate::log;
use crate::prefs::Settings;
use crate::render::{Ambient, Gpu};
use crate::screen::{PowerBudget, Screen};
use crate::tray::Tray;
use crate::windows::{self, Area};

pub enum Command {
    Snowfall(Snowfall),
    Accumulate(bool),
    Melt(bool),
    SnowOnWindows(bool),
    BehindWindows(bool),
    ShowInScreenshots(bool),
    /// The screen locked or unlocked.
    Locked(bool),
    /// Blows all the snow away; when calm, for less motion, melts it where it lies instead.
    ClearSnow(bool),
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
    settings: Settings,
    screens: Vec<Screen>,
    locked: bool,
    quit: bool,
    displays_changed_at: Option<Instant>,
    report: (u64, Option<Arc<Report>>),
    budget: PowerBudget,
    light: [f32; 3],
    started: Instant,
    chores_at: Instant,
    light_at: Instant,
    power_at: Instant,
    stats: Stats,
}

pub fn run() -> Result<(), String> {
    let settings = Settings::load();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().map_err(|error| error.to_string())?;
    // First of all: the signals are blocked on this thread so the loop can take them, and threads started
    // later (D-Bus, the tray, the graphics driver's) inherit that, or a quit would kill the app on the spot
    // and leave its script in KWin.
    let signals = calloop::signals::Signals::new(&[calloop::signals::Signal::SIGINT, calloop::signals::Signal::SIGTERM])
        .map_err(|error| error.to_string())?;
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
        shared,
        settings,
        screens: Vec::new(),
        locked: false,
        quit: false,
        displays_changed_at: None,
        report: (0, None),
        budget: PowerBudget::Full,
        light: daylight(),
        started: now,
        chores_at: now,
        light_at: now,
        power_at: now,
        stats: Stats::new(now),
    };
    // The displays are known after a round trip; the snow starts on them right away.
    queue.roundtrip(&mut app).map_err(|error| error.to_string())?;
    app.sync_screens();
    WaylandSource::new(connection, queue).insert(event_loop.handle()).map_err(|error| error.to_string())?;

    if let Err(error) = kwin::load_script(&bus, settings.show_in_screenshots) {
        log!("KWin script failed: {error}; no snow on windows");
    }
    use ksni::blocking::TrayMethods;
    let tray = Tray::new(settings, commands.clone()).spawn().inspect_err(|error| log!("tray: {error}")).ok();
    watch_lock(&bus, commands);

    while !app.quit {
        let timeout = app.timeout();
        event_loop.dispatch(timeout, &mut app).map_err(|error| error.to_string())?;
        app.tick()?;
    }

    kwin::unload_script(&app.bus);
    if let Some(tray) = tray {
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
                if let Err(error) = kwin::load_script(&self.bus, on) {
                    log!("KWin script failed: {error}");
                }
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
            if let Some(next) = &report.1 {
                let order = window_order(next);
                if self.report.1.as_deref().map(window_order) != Some(order.clone()) {
                    log!("windows front to back: {order}");
                }
            }
            self.report = report;
            self.place_screens();
        }
        // Display changes come in bursts; the layout is read once they settle.
        if let Some(changed_at) = self.displays_changed_at
            && now - changed_at > Duration::from_millis(400)
        {
            self.displays_changed_at = None;
            self.sync_screens();
            self.light_at = now;
        }

        if now >= self.chores_at {
            self.chores_at = now + Duration::from_secs(1);
            if now >= self.power_at {
                self.power_at = now + Duration::from_secs(5);
                let next = power_budget();
                if next != self.budget {
                    self.budget = next;
                    log!("power: {next:?}");
                    let density = if next == PowerBudget::Saver { 0.75 } else { 1.0 };
                    self.screens.iter_mut().for_each(|screen| screen.simulation.set_density(density));
                }
            }
            if now >= self.light_at {
                self.light_at = now + Duration::from_secs(60);
                self.light = daylight();
                let backdrops = light::backdrops(&self.bus);
                self.screens.iter_mut().for_each(|screen| screen.update_backdrop(&backdrops));
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
        let ambient = Ambient { light: self.light, shadow_tint: [0.0; 4] };
        let report = self.report.1.clone();
        let windows = report.as_deref().map(|report| (report, self.report.0, report.moving));
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
        self.stats.report(Instant::now());
        Ok(())
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
            let simulation = new_simulation(&bounds, &self.settings);
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

    /// Fits each overlay to its display's work area, and tells each simulation where its walls are.
    fn place_screens(&mut self) {
        let Some(report) = self.report.1.clone() else { return };
        for screen in &mut self.screens {
            let Some(display) = report.screens.iter().find(|display| display.name == screen.name) else { continue };
            screen.place(Area::bounds(display), Area::work(display), windows::shelf(&report, display));
            screen.simulation.walls = windows::walls(display, &report.screens);
        }
    }
}

/// The reported windows by app, front to back, for the log.
fn window_order(report: &Report) -> String {
    report.windows.iter().map(|window| window.class.as_str()).collect::<Vec<_>>().join(", ")
}

fn new_simulation(bounds: &Area, settings: &Settings) -> SnowSimulation {
    let mut simulation = SnowSimulation::new(bounds.w as f32, bounds.h as f32, settings.snowfall);
    simulation.thaws = settings.melt;
    simulation.accumulates = settings.accumulate;
    simulation
}

/// On battery from the power supplies the kernel lists; power saver from power-profiles-daemon.
fn power_budget() -> PowerBudget {
    if power_saver() {
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

fn power_saver() -> bool {
    let Ok(system) = zbus::blocking::Connection::system() else { return false };
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
