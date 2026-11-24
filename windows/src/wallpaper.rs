//! The wallpaper the snow lies in, and the reading of it. A picture is read once, by another run of the app that reads
//! it, keeps what it found on disk and quits: running the network leaves a few hundred megabytes behind in a process,
//! and the snow's own stays small.

use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use snowhush_core::wallpaper::{Scene, Shown, cache_name};
use windows::Win32::Foundation::{GENERIC_READ, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HGDIOBJ,
    PaintDesktop, ReleaseDC, SelectObject,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeHighQualityCubic,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::{PCWSTR, w};

use crate::log;

/// The argument that makes the app read a wallpaper and quit instead of starting up.
pub const READ: &str = "--read-wallpaper";
/// How long a reading may take.
const READING: Duration = Duration::from_secs(180);
/// The most readings kept, for a wallpaper that changes on its own every so often.
const KEPT: usize = 16;
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x4000;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How a picture fills its display, as Personalization sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Fill,
    Fit,
    Stretch,
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
            "fill" => Placement::Fill,
            "fit" => Placement::Fit,
            "stretch" => Placement::Stretch,
            "center" => Placement::Center,
            "tile" => Placement::Tile,
            _ => return None,
        })
    }

    /// From IDesktopWallpaper's position: centre, tile, stretch, fit, fill, and span, which is taken as fill.
    pub fn from_position(position: i32) -> Placement {
        match position {
            0 => Placement::Center,
            1 => Placement::Tile,
            2 => Placement::Stretch,
            3 => Placement::Fit,
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
            "as Windows paints it",
        ]);
        Some(folder()?.join("wallpaper").join(name))
    }

    pub fn remembered(&self, size: (u32, u32), column_width: f32) -> Option<Scene> {
        Scene::from_bytes(&std::fs::read(self.kept(size, column_width)?).ok()?)
    }

    /// What is found in it on the display `bounds`, in physical pixels, read by another run of the app the first time.
    /// Slow then, seconds to minutes: never on the snow's own thread. Read as Windows paints it there, when it does:
    /// how it crops a picture that fills the display is its own, and isn't centred.
    pub fn read_apart(&self, size: (u32, u32), column_width: f32, bounds: RECT) -> Option<Scene> {
        if let Some(kept) = self.remembered(size, column_width) {
            return Some(kept);
        }
        let out = self.kept(size, column_width)?;
        let painted = out.parent().and_then(|folder| std::fs::create_dir_all(folder).ok()).and_then(|()| {
            let painted = out.with_extension("bmp");
            paint(bounds, &self.path, self.placement, &painted).then_some(painted)
        });
        let (picture, placement) = painted.as_deref().map_or((self.path.as_path(), self.placement), |painted| (painted, Placement::Stretch));
        let fill = format!("{},{},{}", self.fill[0], self.fill[1], self.fill[2]);
        let mut reader = Command::new(std::env::current_exe().ok()?)
            .arg(READ)
            .arg(picture)
            .args([placement.name(), &fill, &size.0.to_string(), &size.1.to_string(), &column_width.to_string()])
            .arg(&out)
            .creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
            .spawn()
            .ok()?;
        let started = Instant::now();
        loop {
            match reader.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        log!("wallpaper: reading {} ended with {status}", self.path.display());
                    }
                    break;
                }
                Ok(None) if started.elapsed() < READING => std::thread::sleep(Duration::from_millis(200)),
                _ => {
                    let _ = reader.kill();
                    let _ = reader.wait();
                    log!("wallpaper: reading {} took too long", self.path.display());
                    break;
                }
            }
        }
        if let Some(painted) = &painted {
            let _ = std::fs::remove_file(painted);
        }
        self.remembered(size, column_width)
    }
}

/// Where the network and what is found in wallpapers are kept.
fn folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("Snowhush"))
}

/// The other run's side of `read_apart`: reads the wallpaper the arguments name and keeps what it found. Returns the
/// exit code: 1 when the picture couldn't be opened, 4 when nothing was found.
pub fn read_for_another_run(arguments: &[String]) -> i32 {
    let [path, placement, fill, width, height, column_width, out] = arguments else { return 2 };
    let Some(placement) = Placement::from_name(placement) else { return 2 };
    let fill: Vec<f32> = fill.split(',').filter_map(|part| part.parse().ok()).collect();
    let (Ok(width), Ok(height), Ok(column_width)) = (width.parse::<u32>(), height.parse::<u32>(), column_width.parse::<f32>()) else { return 2 };
    let fill = [fill.first().copied().unwrap_or(0.0), fill.get(1).copied().unwrap_or(0.0), fill.get(2).copied().unwrap_or(0.0)];
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let Some(shown) = shown(Path::new(path), placement, fill, width, height) else { return 1 };
    let Ok(scene) = snowhush_depth::read(&shown, column_width) else { return 4 };
    let out = PathBuf::from(out);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
        forget_old(parent);
    }
    let partial = out.with_extension("partial");
    if std::fs::write(&partial, scene.to_bytes()).and_then(|()| std::fs::rename(&partial, &out)).is_err() {
        return 1;
    }
    0
}

/// Paints the wallpaper on the display `bounds` as Windows does into `file`, a bitmap; whether it painted a picture.
/// Logs how far from centred Windows lays a `picture` that fills the display.
fn paint(bounds: RECT, picture: &Path, placement: Placement, file: &Path) -> bool {
    if bounds.left < 0 || bounds.top < 0 || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return false;
    }
    let (width, height) = ((bounds.right - bounds.left) as usize, (bounds.bottom - bounds.top) as usize);
    let mut pixels = vec![0u8; width * height * 4];
    let painted = unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bounds.right,
                biHeight: -bounds.bottom,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let painted = match CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) => {
                let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
                let painted = PaintDesktop(memory).as_bool();
                let _ = GdiFlush();
                if painted {
                    let all = std::slice::from_raw_parts(bits.cast::<u8>(), bounds.right as usize * bounds.bottom as usize * 4);
                    for (y, row) in pixels.chunks_exact_mut(width * 4).enumerate() {
                        let from = ((bounds.top as usize + y) * bounds.right as usize + bounds.left as usize) * 4;
                        row.copy_from_slice(&all[from..from + width * 4]);
                    }
                }
                SelectObject(memory, previous);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                painted
            }
            Err(_) => false,
        };
        let _ = DeleteDC(memory);
        painted
    };
    let lightness: Vec<f32> = pixels.chunks_exact(4).step_by(997).map(|p| luma(p[2], p[1], p[0])).collect();
    let mean = lightness.iter().sum::<f32>() / lightness.len().max(1) as f32;
    let spread = (lightness.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / lightness.len().max(1) as f32).sqrt();
    if !painted || spread < 2.0 {
        log!("wallpaper: Windows painted {}", if painted { "one colour" } else { "nothing" });
        return false;
    }
    if placement == Placement::Fill
        && let Some((right, down)) = off_centre(picture, &pixels, width, height)
    {
        log!("wallpaper: Windows lays {} {right} px right and {down} px down of centred", picture.display());
    }
    pixels.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    write_bitmap(file, width, height, &pixels).is_ok()
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722
}

/// How far right and down of centred `picture` lies where Windows filled the display with it, `painted` BGRA, in
/// physical pixels: found by sliding it, small, over what was painted.
fn off_centre(picture: &Path, painted: &[u8], width: usize, height: usize) -> Option<(i32, i32)> {
    let (w, h) = (256usize, (height * 256 / width).max(1));
    let mut seen = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (x0, y0) = (x * width / w, y * height / h);
            let (x1, y1) = (((x + 1) * width / w).max(x0 + 1), ((y + 1) * height / h).max(y0 + 1));
            let (mut sum, mut count) = (0.0, 0.0);
            for py in (y0..y1).step_by(2) {
                for px in (x0..x1).step_by(2) {
                    let i = (py * width + px) * 4;
                    sum += luma(painted[i + 2], painted[i + 1], painted[i]);
                    count += 1.0;
                }
            }
            seen[y * w + x] = sum / count;
        }
    }
    let name: Vec<u16> = picture.as_os_str().encode_wide().chain([0]).collect();
    let (dw, dh, drawn) = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = imaging.CreateDecoderFromFilename(PCWSTR(name.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut pw, mut ph) = (0u32, 0u32);
        frame.GetSize(&mut pw, &mut ph).ok()?;
        let scale = (w as f32 / pw.max(1) as f32).max(h as f32 / ph.max(1) as f32);
        let (dw, dh) = (((pw as f32 * scale).round() as usize).max(w), ((ph as f32 * scale).round() as usize).max(h));
        let scaler = imaging.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, dw as u32, dh as u32, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let converter = imaging.CreateFormatConverter().ok()?;
        converter.Initialize(&scaler, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let mut drawn = vec![0u8; dw * dh * 4];
        converter.CopyPixels(std::ptr::null(), (dw * 4) as u32, &mut drawn).ok()?;
        (dw, dh, drawn)
    };
    let (ex, ey) = (dw - w, dh - h);
    let mut best = (f32::MAX, 0, 0);
    for oy in 0..=ey {
        for ox in 0..=ex {
            let mut differs = 0.0;
            for y in 0..h {
                for x in 0..w {
                    let i = ((oy + y) * dw + ox + x) * 4;
                    differs += (luma(drawn[i], drawn[i + 1], drawn[i + 2]) - seen[y * w + x]).abs();
                }
            }
            if differs < best.0 {
                best = (differs, ox, oy);
            }
        }
    }
    let factor = width as f32 / w as f32;
    Some((((ex as f32 / 2.0 - best.1 as f32) * factor).round() as i32, ((ey as f32 / 2.0 - best.2 as f32) * factor).round() as i32))
}

/// A bitmap file of `bgra`, rows top first, `width` by `height`.
fn write_bitmap(file: &Path, width: usize, height: usize, bgra: &[u8]) -> std::io::Result<()> {
    let size = width * height * 4;
    let mut bytes = Vec::with_capacity(54 + size);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&((54 + size) as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&(width as i32).to_le_bytes());
    bytes.extend_from_slice(&(height as i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 24]);
    for row in bgra.chunks_exact(width * 4).rev() {
        bytes.extend_from_slice(row);
    }
    std::fs::write(file, bytes)
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

struct Handle(*mut std::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// The whole of a file served over HTTPS.
pub fn download(host: &str, path: &str) -> Result<Vec<u8>, String> {
    let failed = |what: &str| format!("{what} failed: {}", windows::core::Error::from_thread().message());
    let host: Vec<u16> = host.encode_utf16().chain([0]).collect();
    let path: Vec<u16> = path.encode_utf16().chain([0]).collect();
    unsafe {
        let session = Handle(WinHttpOpen(w!("Snowhush"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(failed("Opening a connection"));
        }
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000);
        let connection = Handle(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0));
        if connection.0.is_null() {
            return Err(failed("Connecting"));
        }
        let request = Handle(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(failed("Preparing the request"));
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|error| error.message())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|error| error.message())?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|error| error.message())?;
        if status != 200 {
            return Err(format!("the server answered {status}"));
        }
        let mut body = Vec::new();
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).map_err(|error| error.message())?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(body)
    }
}

/// The display as the wallpaper fills it, a pixel to a DIP: the picture scaled by the Windows Imaging Component.
pub fn shown(path: &Path, placement: Placement, fill: [f32; 3], width: u32, height: u32) -> Option<Shown> {
    let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = imaging.CreateDecoderFromFilename(PCWSTR(name.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut pw, mut ph) = (0u32, 0u32);
        frame.GetSize(&mut pw, &mut ph).ok()?;
        if pw == 0 || ph == 0 || width == 0 || height == 0 {
            return None;
        }
        let (w, h, fw, fh) = (width as f32, height as f32, pw as f32, ph as f32);
        let (left, top, drawn_w, drawn_h) = match placement {
            Placement::Stretch => (0.0, 0.0, w, h),
            Placement::Center | Placement::Tile => ((w - fw) / 2.0, (h - fh) / 2.0, fw, fh),
            Placement::Fill | Placement::Fit => {
                let scale = if placement == Placement::Fill { (w / fw).max(h / fh) } else { (w / fw).min(h / fh) };
                ((w - fw * scale) / 2.0, (h - fh * scale) / 2.0, fw * scale, fh * scale)
            }
        };
        let (dw, dh) = (drawn_w.round().max(1.0) as u32, drawn_h.round().max(1.0) as u32);
        let scaler = imaging.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, dw, dh, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let converter = imaging.CreateFormatConverter().ok()?;
        converter.Initialize(&scaler, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let mut drawn = vec![0u8; (dw * dh * 4) as usize];
        converter.CopyPixels(std::ptr::null(), dw * 4, &mut drawn).ok()?;
        let background = fill.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        let mut pixels = vec![255u8; (width * height * 4) as usize];
        let (ox, oy) = (left.round() as i64, top.round() as i64);
        let (rw, rh) = (dw as i64, dh as i64);
        for y in 0..height as i64 {
            for x in 0..width as i64 {
                let (sx, sy) = (x - ox, y - oy);
                let at = if placement == Placement::Tile {
                    Some((sx.rem_euclid(rw), sy.rem_euclid(rh)))
                } else {
                    (sx >= 0 && sy >= 0 && sx < rw && sy < rh).then_some((sx, sy))
                };
                let i = ((y * width as i64 + x) * 4) as usize;
                match at {
                    Some((sx, sy)) => {
                        let k = ((sy * rw + sx) * 4) as usize;
                        pixels[i..i + 3].copy_from_slice(&drawn[k..k + 3]);
                    }
                    None => pixels[i..i + 3].copy_from_slice(&background),
                }
            }
        }
        Some(Shown { width: width as usize, height: height as usize, pixels })
    }
}
