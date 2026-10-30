//! The light the snow is drawn in: warmer at dawn and dusk, cooler at night, with shadows taking on the colour
//! of the desktop picture along the bottom of each display, as on Windows.

use std::path::PathBuf;
use std::time::SystemTime;

use zbus::blocking::Connection;

use snowhush_core::geometry::smoothstep;
use snowhush_core::halloween;

use crate::windows::Area;

/// Clock-based rather than solar: close enough for mood, and needs no location.
pub fn daylight() -> [f32; 3] {
    let hour = hour();
    let day = smoothstep(6.0, 8.5, hour) * (1.0 - smoothstep(17.5, 20.5, hour));
    let golden = ((-((hour - 7.6) / 1.1).powi(2)).exp() + (-((hour - 18.6) / 1.2).powi(2)).exp()).min(1.0);
    let night = [0.86, 0.89, 0.97];
    let warm = [1.0, 0.965, 0.915];
    std::array::from_fn(|i| {
        let base = night[i] + (1.0 - night[i]) * day;
        base + (warm[i] - base) * golden * 0.55
    })
}

/// Whether it is Halloween, wanted or not.
pub fn is_halloween() -> bool {
    let (month, day, hour) = local_time().unwrap_or((1, 1, 12.0));
    halloween::is_halloween(month, day, hour as u32)
}

/// How far into Halloween night it is while Halloween is on.
pub fn night(on: bool) -> f32 {
    halloween::night(on)
}

/// The hour the light follows: the clock's, or one being previewed.
fn hour() -> f32 {
    halloween::previewed_hour().unwrap_or_else(|| local_time().map_or(12.0, |(_, _, hour)| hour))
}

/// The local month, counted from 1, the day of the month and the hour.
fn local_time() -> Option<(u32, u32, f32)> {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut local).is_null() {
            return None;
        }
        Some((local.tm_mon as u32 + 1, local.tm_mday as u32, local.tm_hour as f32 + local.tm_min as f32 / 60.0))
    }
}

/// What a display shows behind its windows: a picture, and when it was last written, or a solid colour.
#[derive(Clone, PartialEq, Debug)]
pub enum Backdrop {
    Picture(PathBuf, Option<SystemTime>),
    Color([f32; 3]),
}

/// Asks Plasma for each desktop's wallpaper, by where its display sits: x, y, and the backdrop.
pub fn backdrops(connection: &Connection) -> Vec<(i32, i32, Backdrop)> {
    const SCRIPT: &str = r#"var lines = [];
for (const d of desktops()) {
    var g = screenGeometry(d.screen);
    d.currentConfigGroup = ["Wallpaper", d.wallpaperPlugin, "General"];
    lines.push([g.x, g.y, d.wallpaperPlugin, d.readConfig("Image"), d.readConfig("Color")].join("\t"));
}
print(lines.join("\n"));"#;
    let Ok(reply) = connection.call_method(Some("org.kde.plasmashell"), "/PlasmaShell", Some("org.kde.PlasmaShell"), "evaluateScript", &(SCRIPT,))
    else {
        return Vec::new();
    };
    let Ok(text) = reply.body().deserialize::<String>() else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let x = parts.next()?.parse().ok()?;
            let y = parts.next()?.parse().ok()?;
            let plugin = parts.next()?;
            let image = parts.next().unwrap_or_default();
            let color = parts.next().unwrap_or_default();
            let backdrop = match plugin {
                "org.kde.color" => Backdrop::Color(parse_color(color)?),
                _ => {
                    let path = PathBuf::from(image.strip_prefix("file://").unwrap_or(image));
                    if !path.is_file() {
                        return None;
                    }
                    let modified = std::fs::metadata(&path).and_then(|metadata| metadata.modified()).ok();
                    Backdrop::Picture(path, modified)
                }
            };
            Some((x, y, backdrop))
        })
        .collect()
}

/// A display's backdrop, by where the display sits, with the tint it gives the shadows.
#[derive(Clone, PartialEq, Debug)]
pub struct Shade {
    pub x: i32,
    pub y: i32,
    pub backdrop: Backdrop,
    pub tint: Option<[f32; 4]>,
}

/// The shade of the display with these bounds.
pub fn shade_of<'a>(shades: &'a [Shade], bounds: &Area) -> Option<&'a Shade> {
    shades.iter().find(|shade| shade.x as f64 == bounds.x && shade.y as f64 == bounds.y)
}

/// The backdrop's average colour along the bottom quarter, brought to the brightness of snow in shade.
pub fn tint(backdrop: &Backdrop) -> Option<[f32; 4]> {
    let average = match backdrop {
        Backdrop::Picture(path, _) => average(path)?,
        Backdrop::Color(color) => *color,
    };
    let luminance = average[0] * 0.2126 + average[1] * 0.7152 + average[2] * 0.0722;
    let shade = 0.775 / luminance.max(0.04);
    Some([(average[0] * shade).min(1.0), (average[1] * shade).min(1.0), (average[2] * shade).min(1.0), 0.35])
}

fn average(path: &PathBuf) -> Option<[f32; 3]> {
    let picture = image::open(path).ok()?.thumbnail(64, 64).to_rgb8();
    let (width, height) = picture.dimensions();
    let rows = (height * 3 / 4)..height;
    let mut sum = [0.0f32; 3];
    let mut count = 0.0f32;
    for y in rows {
        for x in 0..width {
            let pixel = picture.get_pixel(x, y);
            for i in 0..3 {
                sum[i] += pixel[i] as f32 / 255.0;
            }
            count += 1.0;
        }
    }
    (count > 0.0).then(|| sum.map(|channel| channel / count))
}

/// Plasma writes a colour as "r,g,b" or "#rrggbb".
fn parse_color(text: &str) -> Option<[f32; 3]> {
    if let Some(hex) = text.strip_prefix('#') {
        let value = u32::from_str_radix(hex.get(hex.len().checked_sub(6)?..)?, 16).ok()?;
        return Some([(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff].map(|c| c as f32 / 255.0));
    }
    let channels: Vec<f32> = text.split(',').filter_map(|part| part.trim().parse::<f32>().ok()).collect();
    (channels.len() >= 3).then(|| [channels[0] / 255.0, channels[1] / 255.0, channels[2] / 255.0])
}
