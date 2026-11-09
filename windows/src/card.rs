//! The dark card of the welcome window and the installer: drawn with Direct2D into a bitmap
//! that a layered window shows with per-pixel alpha, shadow included.

use std::mem::size_of;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH, DWRITE_FONT_STRETCH_CONDENSED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_LIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextLayout, IDWriteTextLayout1,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetMonitorInfoW, HGDIOBJ, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapCacheOnLoad, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Error, HSTRING, Interface, PCWSTR, Result, w};
use windows_numerics::Vector2;

/// Room around the card for its shadow.
pub const MARGIN: f32 = 36.0;
const BACKGROUND: [f32; 3] = [0.105, 0.088, 0.072];
const INK: [f32; 3] = [0.93, 0.9, 0.85];
const MUTED: [f32; 3] = [0.78, 0.75, 0.7];
const LABEL: [f32; 3] = [0.09, 0.075, 0.06];
const BUTTON: (f32, f32) = (190.0, 44.0);
/// The least room above and below the content when a card grows to fit it.
const PADDING: f32 = 24.0;
const ICON: &[u8] = include_bytes!("../assets/icon-256.png");

/// What the card shows, top to bottom.
pub struct Content<'a> {
    pub icon: f32,
    pub title: &'a str,
    pub title_size: f32,
    pub body: &'a str,
    pub button: &'a str,
    pub pressed: bool,
    /// Draws a close mark in the top right corner.
    pub closable: bool,
}

/// Where the card's controls ended up, in DIPs from the bitmap's top left.
#[derive(Clone, Copy, Default)]
pub struct Hits {
    pub button: D2D_RECT_F,
    pub close: Option<D2D_RECT_F>,
}

pub struct Card {
    /// The card itself in DIPs, without the shadow's margin.
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    /// The bitmap in physical pixels, shadow included.
    pub size: (i32, i32),
    d2d: ID2D1Factory,
    text: IDWriteFactory,
    imaging: IWICImagingFactory,
    icon: IWICBitmapSource,
}

impl Card {
    pub fn new(width: f32, height: f32, scale: f32) -> Result<Card> {
        unsafe {
            let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let stream = SHCreateMemStream(Some(ICON)).ok_or_else(Error::empty)?;
            let decoder = imaging.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
            let converter = imaging.CreateFormatConverter()?;
            converter.Initialize(
                &decoder.GetFrame(0)?,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
            Ok(Card {
                width,
                height,
                scale,
                size: (((width + 2.0 * MARGIN) * scale).ceil() as i32, ((height + 2.0 * MARGIN) * scale).ceil() as i32),
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                text: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
                imaging,
                icon: converter.cast()?,
            })
        }
    }

    /// Premultiplied BGRA rows, top down, and where the controls are.
    pub fn paint(&self, content: &Content) -> Result<(Vec<u8>, Hits)> {
        let (width, height) = (self.size.0, self.size.1);
        unsafe {
            let bitmap = self.imaging.CreateBitmap(width as u32, height as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0 * self.scale,
                dpiY: 96.0 * self.scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties)?;
            target.BeginDraw();
            target.Clear(Some(&color([0.0; 3], 0.0)));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let hits = self.draw(&target, content);
            target.EndDraw(None, None)?;
            let hits = hits?;
            drop(target);

            let mut pixels = vec![0u8; (width * height * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (width * 4) as u32, &mut pixels)?;
            Ok((pixels, hits))
        }
    }

    fn draw(&self, target: &ID2D1RenderTarget, content: &Content) -> Result<Hits> {
        let card = D2D_RECT_F { left: MARGIN, top: MARGIN, right: MARGIN + self.width, bottom: MARGIN + self.height };
        let brush = |rgb: [f32; 3], alpha: f32| unsafe { target.CreateSolidColorBrush(&color(rgb, alpha), None) };
        unsafe {
            // A soft shadow from stacked translucent outlines, lighter as they spread.
            let shade = brush([0.0; 3], 0.022)?;
            for step in 0..16 {
                let spread = step as f32 * 1.6;
                let rect = D2D_RECT_F {
                    left: card.left - spread,
                    top: card.top - spread + 10.0,
                    right: card.right + spread,
                    bottom: card.bottom + spread + 10.0,
                };
                target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: 18.0 + spread, radiusY: 18.0 + spread }, &shade);
            }
            let inset = D2D_RECT_F { left: card.left + 0.5, top: card.top + 0.5, right: card.right - 0.5, bottom: card.bottom - 0.5 };
            let shape = D2D1_ROUNDED_RECT { rect: inset, radiusX: 18.0, radiusY: 18.0 };
            target.FillRoundedRectangle(&shape, &brush(BACKGROUND, 1.0)?);
            target.DrawRoundedRectangle(&shape, &brush([1.0; 3], 0.08)?, 1.0, None);

            let body_width = (self.width - 64.0).min(330.0);
            let (title, body) = self.texts(content)?;
            let label =
                self.layout(content.button, w!("Segoe UI"), DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STRETCH_NORMAL, 12.5, BUTTON.0, 0.0, 1.8)?;
            label.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            label.SetMaxHeight(BUTTON.1)?;
            let (title_height, body_height) = (height(&title)?, height(&body)?);

            let total = content.icon + 10.0 + title_height + 18.0 + body_height + 28.0 + BUTTON.1;
            let mut y = card.top + (self.height - total) / 2.0;
            let icon = target.CreateBitmapFromWicBitmap(&self.icon, None)?;
            let left = card.left + (self.width - content.icon) / 2.0;
            let rect = D2D_RECT_F { left, top: y, right: left + content.icon, bottom: y + content.icon };
            target.DrawBitmap(&icon, Some(&rect), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
            y += content.icon + 10.0;
            target.DrawTextLayout(Vector2 { X: card.left + 32.0, Y: y }, &title, &brush(INK, 1.0)?, D2D1_DRAW_TEXT_OPTIONS_NONE);
            y += title_height + 18.0;
            let muted = brush(MUTED, 1.0)?;
            target.DrawTextLayout(Vector2 { X: card.left + (self.width - body_width) / 2.0, Y: y }, &body, &muted, D2D1_DRAW_TEXT_OPTIONS_NONE);
            y += body_height + 28.0;

            let left = card.left + (self.width - BUTTON.0) / 2.0;
            let button = D2D_RECT_F { left, top: y, right: left + BUTTON.0, bottom: y + BUTTON.1 };
            let fill = if content.pressed { INK.map(|c| c * 0.88) } else { INK };
            target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: button, radiusX: 2.0, radiusY: 2.0 }, &brush(fill, 1.0)?);
            target.DrawTextLayout(Vector2 { X: left, Y: y }, &label, &brush(LABEL, 1.0)?, D2D1_DRAW_TEXT_OPTIONS_NONE);

            let close = content.closable.then(|| {
                let (x, y, arm) = (card.right - 28.0, card.top + 28.0, 5.5);
                for (dx, dy) in [(arm, arm), (arm, -arm)] {
                    target.DrawLine(Vector2 { X: x - dx, Y: y - dy }, Vector2 { X: x + dx, Y: y + dy }, &muted, 1.4, None);
                }
                D2D_RECT_F { left: x - 16.0, top: y - 16.0, right: x + 16.0, bottom: y + 16.0 }
            });
            Ok(Hits { button, close })
        }
    }

    /// How tall the card has to be for `contents`, at least `least`: a translation can run a line or two longer.
    pub fn fitting_height(&self, contents: &[Content], least: f32) -> Result<f32> {
        let mut tallest = least;
        for content in contents {
            let (title, body) = self.texts(content)?;
            let total = content.icon + 10.0 + height(&title)? + 18.0 + height(&body)? + 28.0 + BUTTON.1;
            tallest = tallest.max((total + 2.0 * PADDING).ceil());
        }
        Ok(tallest)
    }

    fn texts(&self, content: &Content) -> Result<(IDWriteTextLayout, IDWriteTextLayout)> {
        let text_width = self.width - 64.0;
        let title = self.layout(
            content.title,
            w!("Bahnschrift"),
            DWRITE_FONT_WEIGHT_LIGHT,
            DWRITE_FONT_STRETCH_CONDENSED,
            content.title_size,
            text_width,
            content.title_size * 1.13,
            0.6,
        )?;
        let body =
            self.layout(content.body, w!("Segoe UI"), DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 13.5, text_width.min(330.0), 23.0, 0.0)?;
        Ok((title, body))
    }

    fn layout(
        &self,
        text: &str,
        family: PCWSTR,
        weight: DWRITE_FONT_WEIGHT,
        stretch: DWRITE_FONT_STRETCH,
        size: f32,
        width: f32,
        line: f32,
        tracking: f32,
    ) -> Result<IDWriteTextLayout> {
        let units: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            // The locale picks the fallback font, so Japanese and Chinese get their own forms of the shared characters.
            let locale = HSTRING::from(snowhush_core::text::code());
            let format = self.text.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, stretch, size, &locale)?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            if line > 0.0 {
                format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line, line * 0.8)?;
            }
            let layout = self.text.CreateTextLayout(&units, &format, width, 1000.0)?;
            if tracking > 0.0
                && let Ok(spaced) = layout.cast::<IDWriteTextLayout1>()
            {
                spaced.SetCharacterSpacing(0.0, tracking, 0.0, DWRITE_TEXT_RANGE { startPosition: 0, length: units.len() as u32 })?;
            }
            Ok(layout)
        }
    }

    /// A point in the window's own pixels, in the DIPs `Hits` uses.
    pub fn dips(&self, point: POINT) -> (f32, f32) {
        (point.x as f32 / self.scale, point.y as f32 / self.scale)
    }
}

pub fn contains(rect: &D2D_RECT_F, (x, y): (f32, f32)) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// The point packed into a mouse message's `lparam`.
pub fn point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 }
}

/// The scale of the display under the pointer, and where a card of `width` by `height` DIPs sits centred on it.
pub fn placement(width: f32, height: f32) -> (f32, POINT) {
    unsafe {
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut dpi, mut _y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
        let scale = dpi.max(96) as f32 / 96.0;
        let size = (((width + 2.0 * MARGIN) * scale).ceil() as i32, ((height + 2.0 * MARGIN) * scale).ceil() as i32);
        let work = info.rcWork;
        (scale, POINT { x: work.left + (work.right - work.left - size.0) / 2, y: work.top + (work.bottom - work.top - size.1) / 2 })
    }
}

/// Registers a window class for a card, with the app's icon for the taskbar.
pub fn register(class: PCWSTR, procedure: WNDPROC) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let icon = LoadImageW(Some(instance.into()), PCWSTR(1 as *const u16), IMAGE_ICON, 0, 0, LR_DEFAULTSIZE).ok();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: procedure,
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: icon.map(|icon| HICON(icon.0)).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&class);
    }
    Ok(())
}

/// Shows premultiplied pixels in a layered window, moving it to `position` if given.
pub fn present(window: HWND, size: (i32, i32), pixels: &[u8], position: Option<POINT>) {
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0,
                biHeight: -size.1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        if let Ok(bitmap) = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let size = SIZE { cx: size.0, cy: size.1 };
            let _ = UpdateLayeredWindow(
                window,
                Some(screen),
                position.as_ref().map(|p| p as *const POINT),
                Some(&size),
                Some(memory),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(memory, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
    }
}

fn height(layout: &IDWriteTextLayout) -> Result<f32> {
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics)? };
    Ok(metrics.height)
}

fn color(rgb: [f32; 3], alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb[0], g: rgb[1], b: rgb[2], a: alpha }
}
