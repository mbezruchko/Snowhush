//! The wallpaper the snow lies in: which picture each display shows and how it is laid out, where Plasma doesn't say
//! so itself, and the reading of it. A picture is read once, by another run of the app that downloads the depth
//! network the first time, reads it, keeps what it found on disk and quits: running the network leaves a few hundred
//! megabytes behind in a process, and the snow's own stays small.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use snowhush_core::wallpaper::{Scene, Shown, cache_name};

use crate::log;

/// The argument that makes the app read a wallpaper and quit instead of starting up.
pub const READ: &str = "--read-wallpaper";
/// How long a reading may take, the download of the network the first time included.
const READING: Duration = Duration::from_secs(180);
/// The most readings kept, for a wallpaper that changes on its own every so often.
const KEPT: usize = 16;

/// How a picture fills its display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Scaled to cover the display, cropped where it reaches past.
    Fill,
    /// Scaled to fit inside the display, the fill colour round it.
    Fit,
    Stretch,
    /// As big as it is, in the middle.
    Center,
    Tile,
}

impl Placement {
    fn name(self) -> &'static str {
        match self {
            Placement::Fill => "fill",
            Placement::Fit => "fit",
            Placement::Stretch => "stretch",
            Placement::Center => "center",
            Placement::Tile => "tile",
        }
    }

    fn from_name(name: &str) -> Option<Placement> {
        Some(match name {
            "fill" | "crop" | "cover" => Placement::Fill,
            "fit" | "contain" => Placement::Fit,
            "stretch" => Placement::Stretch,
            "center" | "no" => Placement::Center,
            "tile" => Placement::Tile,
            _ => return None,
        })
    }

    /// Plasma's FillMode: 0 stretched, 1 scaled to fit, 2 scaled and cropped, 3 to 5 tiled, 6 centred.
    pub fn plasma(mode: &str) -> Placement {
        match mode.trim() {
            "0" => Placement::Stretch,
            "1" => Placement::Fit,
            "3" | "4" | "5" => Placement::Tile,
            "6" => Placement::Center,
            _ => Placement::Fill,
        }
    }
}

/// A display's wallpaper as it is laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Wallpaper {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub bytes: u64,
    pub placement: Placement,
    /// Red, green and blue round a picture that doesn't fill the display, 0 to 1.
    pub fill: [f32; 3],
}

impl Wallpaper {
    pub fn new(path: PathBuf, placement: Placement, fill: [f32; 3]) -> Option<Wallpaper> {
        let metadata = std::fs::metadata(&path).ok().filter(|metadata| metadata.is_file())?;
        Some(Wallpaper { modified: metadata.modified().ok(), bytes: metadata.len(), path, placement, fill })
    }

    /// The file what is found in it is kept in, laid out on a display `size` across, for ridges `column_width` apart.
    fn kept(&self, size: (u32, u32), column_width: f32) -> Option<PathBuf> {
        let modified = self.modified.and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |since| since.as_nanos());
        let name = cache_name(&[
            &self.path.to_string_lossy(),
            &format!("{}x{}", size.0, size.1),
            self.placement.name(),
            &format!("{:?}", self.fill),
            &modified.to_string(),
            &self.bytes.to_string(),
            &column_width.to_string(),
        ]);
        Some(folder()?.join("wallpaper").join(name))
    }

    /// What was found in it when it was read before, if it was.
    pub fn remembered(&self, size: (u32, u32), column_width: f32) -> Option<Scene> {
        Scene::from_bytes(&std::fs::read(self.kept(size, column_width)?).ok()?)
    }

    /// What is found in it, read by another run of the app the first time. Slow then, seconds to minutes: never on the
    /// snow's own thread.
    pub fn read_apart(&self, size: (u32, u32), column_width: f32) -> Option<Scene> {
        if let Some(kept) = self.remembered(size, column_width) {
            return Some(kept);
        }
        let out = self.kept(size, column_width)?;
        let executable = std::env::current_exe().ok()?;
        let fill = format!("{},{},{}", self.fill[0], self.fill[1], self.fill[2]);
        let mut reader = Command::new(executable)
            .arg(READ)
            .arg(&self.path)
            .args([self.placement.name(), &fill, &size.0.to_string(), &size.1.to_string(), &column_width.to_string()])
            .arg(&out)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let started = Instant::now();
        loop {
            match reader.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < READING => std::thread::sleep(Duration::from_millis(200)),
                _ => {
                    let _ = reader.kill();
                    let _ = reader.wait();
                    log!("wallpaper: reading {} took too long", self.path.display());
                    break;
                }
            }
        }
        self.remembered(size, column_width)
    }
}

/// Where the network and what is found in wallpapers are kept.
fn folder() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|cache| cache.join("snowhush"))
}

/// The other run's side of `read_apart`: reads the wallpaper the arguments name and keeps what it found. Returns the
/// exit code.
pub fn read_for_another_run(arguments: &[String]) -> i32 {
    let [path, placement, fill, width, height, column_width, out] = arguments else { return 2 };
    let Some(placement) = Placement::from_name(placement) else { return 2 };
    let fill: Vec<f32> = fill.split(',').filter_map(|part| part.parse().ok()).collect();
    let (Ok(width), Ok(height), Ok(column_width)) = (width.parse::<u32>(), height.parse::<u32>(), column_width.parse::<f32>()) else { return 2 };
    lower_priority();
    let started = Instant::now();
    let fill = [fill.first().copied().unwrap_or(0.0), fill.get(1).copied().unwrap_or(0.0), fill.get(2).copied().unwrap_or(0.0)];
    let Some(shown) = shown(Path::new(path), placement, fill, width, height) else {
        log!("wallpaper: couldn't open {path}");
        return 1;
    };
    let model = match network() {
        Ok(model) => model,
        Err(error) => {
            log!("wallpaper: no depth network: {error}");
            return 1;
        }
    };
    let scene = match snowhush_depth::read(&shown, &model, column_width) {
        Ok(scene) => scene,
        Err(error) => {
            log!("wallpaper: reading {path} failed: {error}");
            return 1;
        }
    };
    let out = PathBuf::from(out);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
        forget_old(parent);
    }
    let partial = out.with_extension("partial");
    if std::fs::write(&partial, scene.to_bytes()).and_then(|()| std::fs::rename(&partial, &out)).is_err() {
        return 1;
    }
    log!("wallpaper: read {path} in {:.1} s, {} ridges", started.elapsed().as_secs_f32(), scene.ridges.len());
    0
}

/// The reading runs on every core: it gives way to whatever the user is doing.
fn lower_priority() {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 15);
    }
}

fn forget_old(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<(PathBuf, SystemTime)> =
        entries.filter_map(|entry| entry.ok()).filter_map(|entry| Some((entry.path(), entry.metadata().ok()?.modified().ok()?))).collect();
    files.sort_by(|a, b| b.1.cmp(&a.1));
    for (file, _) in files.into_iter().skip(KEPT) {
        let _ = std::fs::remove_file(file);
    }
}

/// The depth network's weights, downloaded the first time and checked against what was published.
fn network() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("SNOWHUSH_DEPTH_MODEL") {
        return Ok(PathBuf::from(path));
    }
    let path = folder().ok_or("no cache folder")?.join(snowhush_depth::FILE);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(path.parent().ok_or("no cache folder")?).map_err(|error| error.to_string())?;
    let partial = path.with_extension("partial");
    let url = std::env::var("SNOWHUSH_DEPTH_URL").unwrap_or_else(|_| snowhush_depth::URL.to_string());
    log!("wallpaper: downloading the depth network");
    let status = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "--retry", "2", "--max-time", "150", "--output"])
        .arg(&partial)
        .arg(&url)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| format!("curl: {error}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("download failed: {status}"));
    }
    let bytes = std::fs::read(&partial).map_err(|error| error.to_string())?;
    if !snowhush_depth::genuine(&bytes) {
        let _ = std::fs::remove_file(&partial);
        return Err("downloaded weights don't match".into());
    }
    std::fs::rename(&partial, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

/// The display as the wallpaper fills it, a pixel to a logical pixel.
pub fn shown(path: &Path, placement: Placement, fill: [f32; 3], width: u32, height: u32) -> Option<Shown> {
    let picture = image::open(path).ok()?.to_rgb8();
    let (pw, ph) = (picture.width() as f32, picture.height() as f32);
    if pw < 1.0 || ph < 1.0 || width == 0 || height == 0 {
        return None;
    }
    let (w, h) = (width as f32, height as f32);
    // Where the picture is drawn, left, top, width and height, in the display's logical pixels.
    let (left, top, drawn_w, drawn_h) = match placement {
        Placement::Stretch => (0.0, 0.0, w, h),
        Placement::Center | Placement::Tile => ((w - pw) / 2.0, (h - ph) / 2.0, pw, ph),
        Placement::Fill | Placement::Fit => {
            let scale = if placement == Placement::Fill { (w / pw).max(h / ph) } else { (w / pw).min(h / ph) };
            let (dw, dh) = (pw * scale, ph * scale);
            ((w - dw) / 2.0, (h - dh) / 2.0, dw, dh)
        }
    };
    let resized =
        image::imageops::resize(&picture, drawn_w.round().max(1.0) as u32, drawn_h.round().max(1.0) as u32, image::imageops::FilterType::Triangle);
    drop(picture);
    let background = fill.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    let (rw, rh) = (resized.width() as i64, resized.height() as i64);
    let (ox, oy) = (left.round() as i64, top.round() as i64);
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let (sx, sy) = (x - ox, y - oy);
            let colour = if placement == Placement::Tile {
                resized.get_pixel(sx.rem_euclid(rw) as u32, sy.rem_euclid(rh) as u32).0
            } else if sx >= 0 && sy >= 0 && sx < rw && sy < rh {
                resized.get_pixel(sx as u32, sy as u32).0
            } else {
                background
            };
            let i = ((y * width as i64 + x) * 4) as usize;
            pixels[i..i + 3].copy_from_slice(&colour);
        }
    }
    Some(Shown { width: width as usize, height: height as usize, pixels })
}

/// Each display's wallpaper by its output's name, where a wallpaper daemon other than Plasma's draws it: swaybg, as
/// Sway and niri start it, swww or hyprpaper.
pub fn by_output() -> Vec<(String, Wallpaper)> {
    let mut found = swaybg();
    for (output, wallpaper) in swww().into_iter().chain(hyprpaper()) {
        if !found.iter().any(|(name, _)| *name == output) {
            found.push((output, wallpaper));
        }
    }
    found
}

/// The wallpapers the running swaybg processes were started with: `-o` names an output, or `*` for every one, and the
/// `-i`, `-m` and `-c` after it are its.
fn swaybg() -> Vec<(String, Wallpaper)> {
    let Ok(processes) = std::fs::read_dir("/proc") else { return Vec::new() };
    let mut found = Vec::new();
    for process in processes.filter_map(|entry| entry.ok()) {
        let Ok(command) = std::fs::read(process.path().join("cmdline")) else { continue };
        let words: Vec<String> = command.split(|byte| *byte == 0).map(|word| String::from_utf8_lossy(word).into_owned()).collect();
        if !words.first().is_some_and(|program| program.ends_with("swaybg")) {
            continue;
        }
        let mut output = "*".to_string();
        let mut config: (Option<PathBuf>, Placement, [f32; 3]) = (None, Placement::Fill, [0.0; 3]);
        let mut words = words.into_iter().skip(1).peekable();
        let mut flush = |output: &str, config: &mut (Option<PathBuf>, Placement, [f32; 3])| {
            if let Some(path) = config.0.take()
                && let Some(wallpaper) = Wallpaper::new(path, config.1, config.2)
            {
                found.push((output.to_string(), wallpaper));
            }
        };
        while let Some(word) = words.next() {
            match word.as_str() {
                "-o" | "--output" => {
                    flush(&output, &mut config);
                    output = words.next().unwrap_or_else(|| "*".into());
                    config = (None, Placement::Fill, [0.0; 3]);
                }
                "-i" | "--image" => config.0 = words.next().map(PathBuf::from),
                "-m" | "--mode" => config.1 = words.next().as_deref().and_then(Placement::from_name).unwrap_or(Placement::Fill),
                "-c" | "--color" => config.2 = words.next().as_deref().and_then(hex_colour).unwrap_or([0.0; 3]),
                _ => {}
            }
        }
        flush(&output, &mut config);
    }
    found
}

fn hex_colour(text: &str) -> Option<[f32; 3]> {
    let hex = text.trim_start_matches('#');
    let value = u32::from_str_radix(hex.get(..6)?, 16).ok()?;
    Some([(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff].map(|channel| channel as f32 / 255.0))
}

/// What `swww query` says each output shows: "DP-1: 1920x1080, scale: 1, currently displaying: image: /path".
fn swww() -> Vec<(String, Wallpaper)> {
    let Some(text) = run("swww", &["query"]) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches(':').trim();
            let (output, rest) = line.split_once(':')?;
            let path = rest.split_once("image:")?.1.trim();
            Some((output.trim().to_string(), Wallpaper::new(PathBuf::from(path), Placement::Fill, [0.0; 3])?))
        })
        .collect()
}

/// What hyprpaper says each output shows: "DP-1 = /path".
fn hyprpaper() -> Vec<(String, Wallpaper)> {
    let Some(text) = run("hyprctl", &["hyprpaper", "listactive"]) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let (output, path) = line.split_once(" = ")?;
            let (placement, path) = match path.trim().split_once(':') {
                Some(("contain", path)) => (Placement::Fit, path),
                Some(("tile", path)) => (Placement::Tile, path),
                _ => (Placement::Fill, path.trim()),
            };
            Some((output.trim().to_string(), Wallpaper::new(PathBuf::from(path.trim()), placement, [0.0; 3])?))
        })
        .collect()
}

/// A command's output, or nothing if it isn't there or fails; never waiting more than a second for it.
fn run(program: &str, arguments: &[&str]) -> Option<String> {
    let mut child = Command::new(program).args(arguments).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if started.elapsed() < Duration::from_secs(1) => std::thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut text).ok()?;
    Some(text)
}
