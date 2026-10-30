//! The tray icon and its menu, the same as on Windows. The tray runs on a thread of its own and sends
//! what was chosen to the snow loop.

use calloop::channel::Sender;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

use snowhush_core::simulation::Snowfall;

use crate::log;
use crate::prefs::{self, Settings};
use crate::snow::Command;

const ICON: &[u8] = include_bytes!("../../windows/assets/icon-256.png");
/// A jack-o'-lantern in place of the snowflake while Halloween is on.
const PUMPKIN: &[u8] = include_bytes!("../../windows/assets/pumpkin-256.png");

pub struct Tray {
    settings: Settings,
    commands: Sender<Command>,
    icon: Vec<ksni::Icon>,
    pumpkin: Vec<ksni::Icon>,
    /// Whether it is Halloween, wanted or not: the snow loop keeps it up to date.
    pub season: bool,
}

impl Tray {
    pub fn new(settings: Settings, commands: Sender<Command>) -> Tray {
        Tray { settings, commands, icon: icons(ICON), pumpkin: icons(PUMPKIN), season: false }
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

    /// Empty for the pumpkin, which has only a picture: a tray shows a named icon instead of the picture when it finds one.
    fn icon_name(&self) -> String {
        if self.halloween() { String::new() } else { "snowhush".into() }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        if self.halloween() { self.pumpkin.clone() } else { self.icon.clone() }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Snowhush".into(), ..Default::default() }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let settings = self.settings;
        let current = Snowfall::ALL.iter().position(|&s| s == settings.snowfall).unwrap_or(0);
        vec![
            SubMenu {
                label: "Snowfall".into(),
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
                label: "Melt Snow".into(),
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
                label: "Snow on Windows".into(),
                checked: settings.snow_on_windows,
                enabled: !settings.behind_windows,
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
                label: "Clear Snow".into(),
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
            MenuItem::Separator,
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
fn icons(png: &[u8]) -> Vec<ksni::Icon> {
    let Ok(picture) = image::load_from_memory(png) else { return Vec::new() };
    [22u32, 32, 48, 64]
        .iter()
        .map(|&size| {
            let scaled = picture.resize_exact(size, size, image::imageops::FilterType::Lanczos3).to_rgba8();
            let data = scaled.pixels().flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]]).collect();
            ksni::Icon { width: size as i32, height: size as i32, data }
        })
        .collect()
}
