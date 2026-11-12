//! The tray icon and its menu, the same as on Windows. The tray runs on a thread of its own and sends
//! what was chosen to the snow loop.

use calloop::channel::Sender;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

use snowhush_core::halloween::Today;
use snowhush_core::simulation::Mode;
use snowhush_core::text::{self, tr};

use crate::log;
use crate::prefs::{self, Settings};
use crate::snow::Command;

/// A glyph drawn like the Mac's menu bar ones, at the sizes a tray picks from.
macro_rules! glyph {
    ($name:literal) => {
        [
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-16.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-20.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-24.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-32.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-40.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-48.png")).as_slice(),
            include_bytes!(concat!("../../windows/assets/tray/", $name, "-64.png")).as_slice(),
        ]
    };
}

/// The snowflake, the leaf while leaves fall, the jack-o'-lantern while Halloween is on, the rain cloud and the cherry
/// blossom, each white then black.
const GLYPHS: [[[&[u8]; 7]; 2]; 5] = [
    [glyph!("snowflake-white"), glyph!("snowflake-black")],
    [glyph!("leaf-white"), glyph!("leaf-black")],
    [glyph!("pumpkin-white"), glyph!("pumpkin-black")],
    [glyph!("rain-white"), glyph!("rain-black")],
    [glyph!("blossom-white"), glyph!("blossom-black")],
];
const MODES: [Mode; 4] = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom];
/// The panel's menu can't hold a slider: Intensity and Opacity offer these, in percent.
const INTENSITY_STOPS: [u32; 5] = [0, 25, 50, 75, 100];
const OPACITY_STOPS: [u32; 5] = [100, 80, 60, 40, 20];
const SPONSORS: &str = "https://github.com/sponsors/mbezruchko";

pub struct Tray {
    settings: Settings,
    commands: Sender<Command>,
    glyphs: [[Vec<ksni::Icon>; 2]; 5],
    /// The year and whether it is Halloween: the snow loop keeps it up to date.
    pub today: Today,
    /// Whether the desktop is light, so the panel wants a black glyph: the snow loop keeps it up to date.
    pub light: bool,
}

impl Tray {
    pub fn new(settings: Settings, commands: Sender<Command>) -> Tray {
        let glyphs = GLYPHS.map(|colors| colors.map(|pictures| icons(&pictures)));
        Tray { settings, commands, glyphs, today: Today::default(), light: false }
    }

    fn halloween(&self) -> bool {
        self.settings.modes() == [Mode::Snow] && self.settings.halloween.wanted(self.today)
    }

    fn change(&mut self, change: impl FnOnce(&mut Settings) -> Command) {
        let command = change(&mut self.settings);
        self.settings.save();
        let _ = self.commands.send(command);
    }

    fn pick(&mut self, mode: Mode) {
        if self.settings.pick(mode) {
            self.switch_modes();
        }
    }

    fn switch_modes(&mut self) {
        self.settings.save();
        let settings = self.settings;
        let calm = !prefs::animations_enabled();
        let _ = self.commands.send(Command::Mode { mode: settings.mode, mixed: settings.mixed, mix: settings.mix, calm });
    }
}

impl ksni::Tray for Tray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "snowhush".into()
    }

    fn title(&self) -> String {
        "Snowhush".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let glyph = if self.halloween() {
            2
        } else {
            match self.settings.mode {
                Mode::Snow => 0,
                Mode::Autumn => 1,
                Mode::Rain => 3,
                Mode::Blossom => 4,
            }
        };
        self.glyphs[glyph][usize::from(self.light)].clone()
    }

    fn watcher_online(&self) {
        log!("tray: the panel's tray is back");
    }

    /// Waits for a panel's tray to come, as when Snowhush starts before the panel.
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        log!("tray: no panel's tray yet ({reason:?})");
        true
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Snowhush".into(), ..Default::default() }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let settings = self.settings;
        let picked = settings.modes();
        let falling: Vec<Mode> = MODES.iter().copied().filter(|mode| picked.contains(mode)).collect();
        // Rain doesn't lie anywhere: nothing to keep, melt or clear.
        let heaps: Vec<Mode> = falling.iter().copied().filter(|&mode| mode != Mode::Rain).collect();
        let lies = !heaps.is_empty();
        let melt = tr(if heaps.contains(&Mode::Snow) {
            "melt.snow"
        } else if heaps == [Mode::Blossom] {
            "melt.blossom"
        } else {
            "melt.autumn"
        });
        let mut kinds: Vec<MenuItem<Self>> = vec![
            CheckmarkItem {
                label: tr("mixed").into(),
                checked: settings.mixed,
                activate: Box::new(|tray: &mut Tray| {
                    tray.settings.toggle_mixed();
                    tray.switch_modes();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
        ];
        let labels = [tr("snow"), tr("autumn"), tr("rain"), tr("blossom")];
        if settings.mixed {
            // Mixed, the kinds are checkboxes: a third picked takes the place of the one picked before the other.
            for (&mode, &label) in MODES.iter().zip(&labels) {
                kinds.push(
                    CheckmarkItem {
                        label: label.into(),
                        checked: picked.contains(&mode),
                        activate: Box::new(move |tray: &mut Tray| tray.pick(mode)),
                        ..Default::default()
                    }
                    .into(),
                );
            }
        } else {
            kinds.push(
                RadioGroup {
                    selected: MODES.iter().position(|&m| m == settings.mode).unwrap_or(0),
                    select: Box::new(|tray: &mut Tray, index| {
                        if let Some(&mode) = MODES.get(index) {
                            tray.pick(mode);
                        }
                    }),
                    options: labels.iter().map(|&label| RadioItem { label: label.into(), ..Default::default() }).collect(),
                }
                .into(),
            );
        }
        kinds.extend([
            MenuItem::Separator,
            CheckmarkItem {
                label: tr("halloween").into(),
                checked: settings.halloween.wanted(self.today),
                enabled: falling == [Mode::Snow],
                activate: Box::new(|tray: &mut Tray| {
                    let today = tray.today;
                    tray.change(|settings| {
                        settings.halloween = settings.halloween.toggled(today);
                        Command::Halloween(settings.halloween)
                    })
                }),
                ..Default::default()
            }
            .into(),
        ]);
        // Top to bottom, as they lie on the screen. Rain splashes on the ground whatever Ground says, so with only rain
        // falling it isn't offered.
        let surfaces = vec![
            CheckmarkItem {
                label: tr("surface.windows").into(),
                checked: settings.snow_on_windows,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.snow_on_windows = !settings.snow_on_windows;
                        Command::SnowOnWindows(settings.snow_on_windows)
                    })
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: tr("surface.panel").into(),
                checked: settings.panel,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.panel = !settings.panel;
                        Command::Panel(settings.panel)
                    })
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: tr("surface.ground").into(),
                checked: settings.ground,
                visible: lies,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.ground = !settings.ground;
                        Command::Ground(settings.ground)
                    })
                }),
                ..Default::default()
            }
            .into(),
        ];
        vec![
            SubMenu {
                label: tr("intensity").into(),
                submenu: vec![
                    RadioGroup {
                        selected: INTENSITY_STOPS.iter().position(|&stop| stop == settings.intensity).unwrap_or(usize::MAX),
                        select: Box::new(|tray: &mut Tray, index| {
                            let Some(&intensity) = INTENSITY_STOPS.get(index) else { return };
                            tray.change(|settings| {
                                settings.intensity = intensity;
                                Command::Intensity(intensity as f32 / 100.0)
                            });
                        }),
                        options: INTENSITY_STOPS
                            .iter()
                            .map(|&stop| RadioItem { label: if stop == 0 { tr("off").into() } else { format!("{stop}%") }, ..Default::default() })
                            .collect(),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: tr("opacity").into(),
                submenu: vec![
                    RadioGroup {
                        selected: OPACITY_STOPS.iter().position(|&stop| stop == settings.opacity).unwrap_or(usize::MAX),
                        select: Box::new(|tray: &mut Tray, index| {
                            let Some(&opacity) = OPACITY_STOPS.get(index) else { return };
                            tray.change(|settings| {
                                settings.opacity = opacity;
                                Command::FallingOpacity(opacity as f32 / 100.0)
                            });
                        }),
                        options: OPACITY_STOPS.iter().map(|stop| RadioItem { label: format!("{stop}%"), ..Default::default() }).collect(),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            SubMenu { label: tr("accumulate").into(), submenu: surfaces, ..Default::default() }.into(),
            SubMenu { label: tr("mode").into(), submenu: kinds, ..Default::default() }.into(),
            StandardItem {
                label: text::kinds("clear", &heaps).into(),
                visible: lies,
                activate: Box::new(|tray: &mut Tray| {
                    let _ = tray.commands.send(Command::ClearSnow(!prefs::animations_enabled()));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: tr("settings").into(),
                submenu: vec![
                    StandardItem { label: tr("behavior").into(), enabled: false, ..Default::default() }.into(),
                    CheckmarkItem {
                        label: melt.into(),
                        checked: settings.melt,
                        enabled: settings.accumulates(),
                        visible: lies,
                        activate: Box::new(|tray: &mut Tray| {
                            tray.change(|settings| {
                                settings.melt = !settings.melt;
                                Command::Melt(settings.melt)
                            })
                        }),
                        ..Default::default()
                    }
                    .into(),
                    MenuItem::Separator,
                    StandardItem { label: tr("display").into(), enabled: false, ..Default::default() }.into(),
                    CheckmarkItem {
                        label: tr("behind").into(),
                        checked: settings.behind_windows,
                        activate: Box::new(|tray: &mut Tray| {
                            tray.change(|settings| {
                                settings.behind_windows = !settings.behind_windows;
                                Command::BehindWindows(settings.behind_windows)
                            })
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: tr("full_screen").into(),
                        checked: settings.over_full_screen,
                        enabled: !settings.behind_windows,
                        activate: Box::new(|tray: &mut Tray| {
                            tray.change(|settings| {
                                settings.over_full_screen = !settings.over_full_screen;
                                Command::OverFullScreen(settings.over_full_screen)
                            })
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: tr("screenshots").into(),
                        checked: settings.show_in_screenshots,
                        activate: Box::new(|tray: &mut Tray| {
                            tray.change(|settings| {
                                settings.show_in_screenshots = !settings.show_in_screenshots;
                                Command::ShowInScreenshots(settings.show_in_screenshots)
                            })
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: tr("performance").into(),
                        checked: settings.max_performance,
                        activate: Box::new(|tray: &mut Tray| {
                            tray.change(|settings| {
                                settings.max_performance = !settings.max_performance;
                                Command::Performance(settings.max_performance)
                            })
                        }),
                        ..Default::default()
                    }
                    .into(),
                    MenuItem::Separator,
                    StandardItem { label: tr("general").into(), enabled: false, ..Default::default() }.into(),
                    CheckmarkItem {
                        label: tr("login").into(),
                        checked: prefs::opens_at_login(),
                        activate: Box::new(|_: &mut Tray| {
                            let on = !prefs::opens_at_login();
                            let status = prefs::set_opens_at_login(on);
                            log!("open at login {on}: {status:?}");
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            StandardItem { label: tr("support").into(), activate: Box::new(|_: &mut Tray| open(SPONSORS)), ..Default::default() }.into(),
            StandardItem {
                label: tr("quit").into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Tray| {
                    let _ = tray.commands.send(Command::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn open(url: &str) {
    match std::process::Command::new("xdg-open").arg(url).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(error) => log!("xdg-open {url}: {error}"),
    }
}

/// An icon in the sizes a tray picks from, as ARGB in network byte order.
fn icons(pictures: &[&[u8]]) -> Vec<ksni::Icon> {
    pictures
        .iter()
        .filter_map(|png| {
            let picture = image::load_from_memory(png).ok()?.to_rgba8();
            let data = picture.pixels().flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]]).collect();
            Some(ksni::Icon { width: picture.width() as i32, height: picture.height() as i32, data })
        })
        .collect()
}
