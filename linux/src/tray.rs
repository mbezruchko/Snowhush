//! The tray icon and its menu, the same as on Windows. The tray runs on a thread of its own and sends
//! what was chosen to the snow loop.

use calloop::channel::Sender;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

use snowhush_core::simulation::Snowfall;

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

/// The snowflake, the leaf while leaves fall and the jack-o'-lantern while Halloween is on, each white then black.
const GLYPHS: [[[&[u8]; 7]; 2]; 3] = [
    [glyph!("snowflake-white"), glyph!("snowflake-black")],
    [glyph!("leaf-white"), glyph!("leaf-black")],
    [glyph!("pumpkin-white"), glyph!("pumpkin-black")],
];

pub struct Tray {
    settings: Settings,
    commands: Sender<Command>,
    glyphs: [[Vec<ksni::Icon>; 2]; 3],
    /// Whether it is Halloween, wanted or not: the snow loop keeps it up to date.
    pub season: bool,
    /// Whether the desktop is light, so the panel wants a black glyph: the snow loop keeps it up to date.
    pub light: bool,
}

impl Tray {
    pub fn new(settings: Settings, commands: Sender<Command>) -> Tray {
        let glyphs = GLYPHS.map(|colors| colors.map(|pictures| icons(&pictures)));
        Tray { settings, commands, glyphs, season: false, light: false }
    }

    fn halloween(&self) -> bool {
        self.season && self.settings.halloween
    }

    fn change(&mut self, change: impl FnOnce(&mut Settings) -> Command) {
        let command = change(&mut self.settings);
        self.settings.save();
        let _ = self.commands.send(command);
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
        } else if self.settings.autumn {
            1
        } else {
            0
        };
        self.glyphs[glyph][usize::from(self.light)].clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Snowhush".into(), ..Default::default() }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let settings = self.settings;
        let current = Snowfall::ALL.iter().position(|&s| s == settings.snowfall).unwrap_or(0);
        let autumn = settings.autumn;
        vec![
            SubMenu {
                label: "Intensity".into(),
                submenu: vec![
                    RadioGroup {
                        selected: current,
                        select: Box::new(|tray: &mut Tray, index| {
                            let Some(&snowfall) = Snowfall::ALL.get(index) else { return };
                            tray.change(|settings| {
                                settings.snowfall = snowfall;
                                Command::Snowfall(snowfall)
                            });
                        }),
                        options: Snowfall::ALL.iter().map(|option| RadioItem { label: option.title().into(), ..Default::default() }).collect(),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Mode".into(),
                submenu: vec![
                    RadioGroup {
                        selected: autumn as usize,
                        select: Box::new(|tray: &mut Tray, index| {
                            let autumn = index == 1;
                            if autumn == tray.settings.autumn {
                                return;
                            }
                            tray.change(|settings| {
                                settings.autumn = autumn;
                                Command::Mode { autumn, calm: !prefs::animations_enabled() }
                            });
                        }),
                        options: ["Snow", "Autumn Leaves"].iter().map(|&label| RadioItem { label: label.into(), ..Default::default() }).collect(),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Accumulate".into(),
                checked: settings.accumulate,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.accumulate = !settings.accumulate;
                        Command::Accumulate(settings.accumulate)
                    })
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: if autumn { "Wither Leaves" } else { "Melt Snow" }.into(),
                checked: settings.melt,
                enabled: settings.accumulate,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.melt = !settings.melt;
                        Command::Melt(settings.melt)
                    })
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: if autumn { "Leaves on Windows" } else { "Snow on Windows" }.into(),
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
            StandardItem {
                label: if autumn { "Clear Leaves" } else { "Clear Snow" }.into(),
                activate: Box::new(|tray: &mut Tray| {
                    let _ = tray.commands.send(Command::ClearSnow(!prefs::animations_enabled()));
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Halloween".into(),
                checked: settings.halloween,
                visible: self.season,
                activate: Box::new(|tray: &mut Tray| {
                    tray.change(|settings| {
                        settings.halloween = !settings.halloween;
                        Command::Halloween(settings.halloween)
                    })
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Behind Windows".into(),
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
                label: "Show in Screenshots".into(),
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
            MenuItem::Separator,
            CheckmarkItem {
                label: "Open at Login".into(),
                checked: prefs::opens_at_login(),
                activate: Box::new(|_: &mut Tray| {
                    let on = !prefs::opens_at_login();
                    let status = prefs::set_opens_at_login(on);
                    log!("open at login {on}: {status:?}");
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
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
