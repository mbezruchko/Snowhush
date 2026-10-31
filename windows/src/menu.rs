//! The tray menu: a panel of frosted glass drawn with Direct2D into a layered window, as the welcome card is.
//! It is shown and run like `TrackPopupMenuEx`: `track` returns what was chosen once the menu has closed.

use std::cell::RefCell;
use std::mem::size_of;
use std::sync::Once;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Error, PCWSTR, Result, w};
use windows_numerics::Vector2;

use snowhush::card;

use crate::log;

const CLASS: PCWSTR = w!("SnowhushMenu");
/// Sizes in DIPs.
const WIDTH: f32 = 272.0;
/// Room around the panel for its glow.
const SHADOW: f32 = 28.0;
const PAD: f32 = 8.0;
const ROW: f32 = 34.0;
const SEPARATOR: f32 = 13.0;
const RADIUS: f32 = 24.0;
const ROW_RADIUS: f32 = 13.0;
/// Where the labels start, from a row's left edge: the check marks sit in front of them.
const LABEL: f32 = 38.0;
/// The glass tints the desktop behind it with this.
const TINT: [f32; 3] = [0.09, 0.12, 0.21];

/// What the menu lists.
#[derive(Clone)]
pub enum Entry {
    Item {
        id: u32,
        label: String,
        checked: bool,
        enabled: bool,
    },
    /// Opens a page of its own, with a way back.
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
    Separator,
}

impl Entry {
    pub fn item(id: u32, label: &str, checked: bool, enabled: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked, enabled }
    }
}

struct Page {
    title: Option<String>,
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// The way back from a page.
    Back,
    /// An entry of the page, by index.
    Entry(usize),
    Separator,
}

/// Where a row of the page sits, in DIPs from the panel's top.
struct Row {
    kind: Kind,
    top: f32,
    height: f32,
}

struct Menu {
    window: HWND,
    scale: f32,
    /// The work area of the display the menu is on, in physical pixels.
    work: RECT,
    /// The panel's left edge, and the edge that stays where it is as the page changes height:
    /// the bottom one if `up`, as in a menu above the taskbar, else the top one. Physical pixels.
    left: i32,
    edge: i32,
    up: bool,
    pages: Vec<Page>,
    /// The highlighted row of the page.
    hover: Option<usize>,
    chosen: u32,
    done: bool,
    d2d: ID2D1Factory,
    text: IDWriteFactory,
    imaging: IWICImagingFactory,
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

/// Shows the menu with its corner near `x`, `y` in screen pixels, runs it until it closes,
/// and returns the id of the item chosen, or 0. Fails if the menu can't be drawn, so that a plainer one can take over.
pub fn track(x: i32, y: i32, entries: Vec<Entry>) -> Result<u32> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let _ = card::register(CLASS, Some(procedure));
    });
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut dpi, mut _y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
        let scale = dpi.max(96) as f32 / 96.0;
        let work = info.rcWork;
        let gap = (8.0 * scale).round() as i32;
        let panel = (WIDTH * scale).round() as i32;
        let left = (x - panel / 2).clamp(work.left + gap, (work.right - gap - panel).max(work.left + gap));
        // Above the taskbar when the tray is in the lower half of the display, else below it.
        let up = y > work.top + (work.bottom - work.top) / 2;
        let edge = if up { y.min(work.bottom) - gap } else { y.max(work.top) + gap };

        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let text: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let window =
            CreateWindowExW(WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW, CLASS, w!("Snowhush"), WS_POPUP, 0, 0, 1, 1, None, None, None, None)?;
        let menu = Menu {
            window,
            scale,
            work,
            left,
            edge,
            up,
            pages: vec![Page { title: None, entries }],
            hover: None,
            chosen: 0,
            done: false,
            d2d,
            text,
            imaging,
        };
        let Some((pixels, size, origin)) = menu.render() else {
            let _ = DestroyWindow(window);
            return Err(Error::empty());
        };
        MENU.set(Some(menu));
        card::present(window, size, &pixels, Some(origin));
        let _ = ShowWindow(window, SW_SHOW);
        // The menu closes when it loses the foreground, and also on a click anywhere outside while it holds the mouse.
        let _ = SetForegroundWindow(window);
        SetCapture(window);

        loop {
            if MENU.with_borrow(|menu| menu.as_ref().is_none_or(|menu| menu.done)) {
                break;
            }
            let mut message = MSG::default();
            let status = GetMessageW(&mut message, None, 0, 0);
            if status.0 <= 0 {
                if status.0 == 0 {
                    PostQuitMessage(message.wParam.0 as i32);
                }
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let chosen = MENU.take().map_or(0, |menu| menu.chosen);
        let _ = ReleaseCapture();
        let _ = DestroyWindow(window);
        Ok(chosen)
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_MOUSEMOVE => {
            let point = card::point(lparam);
            act(|menu| {
                let over = menu.row_at(point);
                if over == menu.hover {
                    return false;
                }
                menu.hover = over;
                true
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let point = card::point(lparam);
            act(|menu| {
                if !menu.inside(point) {
                    menu.done = true;
                }
                false
            });
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let point = card::point(lparam);
            act(|menu| match menu.row_at(point) {
                Some(row) => menu.activate(row, false),
                None => false,
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            act(|menu| menu.press(wparam.0));
            LRESULT(0)
        }
        WM_ACTIVATE if wparam.0 & 0xffff == 0 => {
            act(|menu| {
                menu.done = true;
                false
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Changes the menu, and draws it again if that says its look changed.
fn act(change: impl FnOnce(&mut Menu) -> bool) {
    let frame = MENU.with_borrow_mut(|menu| {
        let menu = menu.as_mut()?;
        if !change(menu) {
            return None;
        }
        menu.render().map(|(pixels, size, origin)| (menu.window, pixels, size, origin))
    });
    if let Some((window, pixels, size, origin)) = frame {
        card::present(window, size, &pixels, Some(origin));
    }
}

impl Menu {
    fn page(&self) -> &Page {
        &self.pages[self.pages.len() - 1]
    }

    /// The rows of the shown page, and the panel's height.
    fn layout(&self) -> (Vec<Row>, f32) {
        let page = self.page();
        let mut rows = Vec::new();
        let mut y = PAD;
        let mut add = |kind: Kind, height: f32| {
            rows.push(Row { kind, top: y, height });
            y += height;
        };
        if page.title.is_some() {
            add(Kind::Back, ROW);
            add(Kind::Separator, SEPARATOR);
        }
        for (i, entry) in page.entries.iter().enumerate() {
            match entry {
                Entry::Separator => add(Kind::Separator, SEPARATOR),
                _ => add(Kind::Entry(i), ROW),
            }
        }
        (rows, y + PAD)
    }

    /// Whether the row can be highlighted and chosen.
    fn selectable(&self, row: &Row) -> bool {
        match row.kind {
            Kind::Back => true,
            Kind::Entry(i) => !matches!(self.page().entries[i], Entry::Item { enabled: false, .. }),
            Kind::Separator => false,
        }
    }

    /// A point in the window's own pixels, as DIPs from the panel's top left.
    fn panel_point(&self, point: POINT) -> (f32, f32) {
        (point.x as f32 / self.scale - SHADOW, point.y as f32 / self.scale - SHADOW)
    }

    fn inside(&self, point: POINT) -> bool {
        let (x, y) = self.panel_point(point);
        let (_, height) = self.layout();
        x >= 0.0 && x < WIDTH && y >= 0.0 && y < height
    }

    fn row_at(&self, point: POINT) -> Option<usize> {
        let (x, y) = self.panel_point(point);
        if x < PAD || x >= WIDTH - PAD {
            return None;
        }
        let (rows, _) = self.layout();
        rows.iter().position(|row| y >= row.top && y < row.top + row.height && self.selectable(row))
    }

    /// Moves the highlight to the next row that can be chosen, wrapping around.
    fn step(&mut self, forward: bool) {
        let (rows, _) = self.layout();
        let list: Vec<usize> = (0..rows.len()).filter(|&i| self.selectable(&rows[i])).collect();
        if list.is_empty() {
            return;
        }
        let current = self.hover.and_then(|row| list.iter().position(|&i| i == row));
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => list.len() - 1,
            (Some(at), true) => (at + 1) % list.len(),
            (Some(at), false) => (at + list.len() - 1) % list.len(),
        };
        self.hover = Some(list[next]);
    }

    fn pop(&mut self) {
        if self.pages.len() > 1 {
            self.pages.pop();
            self.hover = None;
        }
    }

    /// Chooses an item, opens a submenu or goes back. Returns whether the menu needs drawing again.
    fn activate(&mut self, row: usize, keyboard: bool) -> bool {
        let (rows, _) = self.layout();
        let Some(row) = rows.get(row) else { return false };
        match row.kind {
            Kind::Back => {
                self.pop();
                true
            }
            Kind::Entry(i) => match self.page().entries[i].clone() {
                Entry::Item { id, enabled: true, .. } => {
                    self.chosen = id;
                    self.done = true;
                    false
                }
                Entry::Submenu { label, entries } => {
                    self.pages.push(Page { title: Some(label), entries });
                    // The first entry after the way back and its separator.
                    self.hover = keyboard.then_some(2);
                    true
                }
                _ => false,
            },
            Kind::Separator => false,
        }
    }

    /// Handles a key; returns whether the menu needs drawing again.
    fn press(&mut self, key: usize) -> bool {
        let is = |code: u16| key == code as usize;
        if is(VK_ESCAPE.0) {
            if self.pages.len() > 1 {
                self.pop();
                return true;
            }
            self.done = true;
            false
        } else if is(VK_DOWN.0) || is(VK_UP.0) {
            self.step(is(VK_DOWN.0));
            true
        } else if is(VK_LEFT.0) {
            let nested = self.pages.len() > 1;
            self.pop();
            nested
        } else if is(VK_RIGHT.0) {
            match self.hover {
                Some(row) if self.is_submenu(row) => self.activate(row, true),
                _ => false,
            }
        } else if is(VK_RETURN.0) || is(VK_SPACE.0) {
            match self.hover {
                Some(row) => self.activate(row, true),
                None => false,
            }
        } else {
            false
        }
    }

    fn is_submenu(&self, row: usize) -> bool {
        let (rows, _) = self.layout();
        matches!(rows.get(row).map(|row| row.kind), Some(Kind::Entry(i)) if matches!(self.page().entries[i], Entry::Submenu { .. }))
    }

    /// The window's size in physical pixels for a panel `height` DIPs tall.
    fn size(&self, height: f32) -> (i32, i32) {
        (((WIDTH + 2.0 * SHADOW) * self.scale).ceil() as i32, ((height + 2.0 * SHADOW) * self.scale).ceil() as i32)
    }

    /// Where the window's top left goes, keeping the panel on the display.
    fn origin(&self, height: f32) -> POINT {
        let shadow = (SHADOW * self.scale).round() as i32;
        let panel = (height * self.scale).round() as i32;
        let top = if self.up { self.edge - panel } else { self.edge };
        let top = top.min(self.work.bottom - panel).max(self.work.top);
        POINT { x: self.left - shadow, y: top - shadow }
    }

    /// Premultiplied pixels of the menu, the window's size and where it goes.
    fn render(&self) -> Option<(Vec<u8>, (i32, i32), POINT)> {
        let (rows, height) = self.layout();
        let size = self.size(height);
        match self.paint(&rows, height, size) {
            Ok(pixels) => Some((pixels, size, self.origin(height))),
            Err(error) => {
                log!("menu paint failed: {error}");
                None
            }
        }
    }

    fn paint(&self, rows: &[Row], height: f32, size: (i32, i32)) -> Result<Vec<u8>> {
        unsafe {
            let bitmap = self.imaging.CreateBitmap(size.0 as u32, size.1 as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
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
            let drawn = self.draw(&target, rows, height);
            target.EndDraw(None, None)?;
            drawn?;
            drop(target);

            let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (size.0 * 4) as u32, &mut pixels)?;
            Ok(pixels)
        }
    }

    fn draw(&self, target: &ID2D1RenderTarget, rows: &[Row], height: f32) -> Result<()> {
        let panel = D2D_RECT_F { left: SHADOW, top: SHADOW, right: SHADOW + WIDTH, bottom: SHADOW + height };
        let brush = |rgb: [f32; 3], alpha: f32| unsafe { target.CreateSolidColorBrush(&color(rgb, alpha), None) };
        unsafe {
            // A soft glow outside the panel, from outlines that fade as they spread. Nothing is drawn under the glass.
            for step in 0..14 {
                let spread = 1.0 + step as f32 * 1.4;
                let fade = 1.0 - step as f32 / 14.0;
                let rect =
                    D2D_RECT_F { left: panel.left - spread, top: panel.top - spread, right: panel.right + spread, bottom: panel.bottom + spread };
                let outline = D2D1_ROUNDED_RECT { rect, radiusX: RADIUS + spread, radiusY: RADIUS + spread };
                target.DrawRoundedRectangle(&outline, &brush([0.0; 3], 0.09 * fade * fade)?, 1.5, None);
            }

            // The glass: a tint, a sheen that is strongest at the top, and a bright rim with a faint one inside it.
            let inset = D2D_RECT_F { left: panel.left + 0.5, top: panel.top + 0.5, right: panel.right - 0.5, bottom: panel.bottom - 0.5 };
            let shape = D2D1_ROUNDED_RECT { rect: inset, radiusX: RADIUS, radiusY: RADIUS };
            target.FillRoundedRectangle(&shape, &brush(TINT, 0.62)?);
            let sheen = brush([1.0; 3], 0.02)?;
            for band in 0..5 {
                let clip = D2D_RECT_F { bottom: panel.top + height * (0.12 + 0.1 * band as f32), ..panel };
                target.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                target.FillRoundedRectangle(&shape, &sheen);
                target.PopAxisAlignedClip();
            }
            target.DrawRoundedRectangle(&shape, &brush([1.0; 3], 0.4)?, 1.0, None);
            let inner = D2D_RECT_F { left: inset.left + 1.5, top: inset.top + 1.5, right: inset.right - 1.5, bottom: inset.bottom - 1.5 };
            let ring = D2D1_ROUNDED_RECT { rect: inner, radiusX: RADIUS - 1.5, radiusY: RADIUS - 1.5 };
            target.DrawRoundedRectangle(&ring, &brush([1.0; 3], 0.1)?, 1.0, None);

            let ink = brush([1.0; 3], 0.96)?;
            let dim = brush([1.0; 3], 0.42)?;
            let line = brush([1.0; 3], 0.2)?;
            let page = self.page();
            for (index, row) in rows.iter().enumerate() {
                let top = panel.top + row.top;
                let bounds = D2D_RECT_F { left: panel.left + PAD, top, right: panel.right - PAD, bottom: top + row.height };
                let (label, checked, enabled, forward, back) = match row.kind {
                    Kind::Separator => {
                        let y = top + row.height / 2.0;
                        target.DrawLine(Vector2 { X: bounds.left + 6.0, Y: y }, Vector2 { X: bounds.right - 6.0, Y: y }, &line, 1.0, None);
                        continue;
                    }
                    Kind::Back => (page.title.as_deref().unwrap_or(""), false, true, false, true),
                    Kind::Entry(i) => match &page.entries[i] {
                        Entry::Item { label, checked, enabled, .. } => (label.as_str(), *checked, *enabled, false, false),
                        Entry::Submenu { label, .. } => (label.as_str(), false, true, true, false),
                        Entry::Separator => continue,
                    },
                };
                if enabled && self.hover == Some(index) {
                    let pill = D2D1_ROUNDED_RECT { rect: bounds, radiusX: ROW_RADIUS, radiusY: ROW_RADIUS };
                    target.FillRoundedRectangle(&pill, &brush([1.0; 3], 0.2)?);
                    target.DrawRoundedRectangle(&pill, &brush([1.0; 3], 0.16)?, 1.0, None);
                }
                let ink = if enabled { &ink } else { &dim };
                let layout = self.label(label, WIDTH - 2.0 * PAD - LABEL - 30.0)?;
                target.DrawTextLayout(Vector2 { X: bounds.left + LABEL, Y: top }, &layout, ink, D2D1_DRAW_TEXT_OPTIONS_NONE);

                let y = top + row.height / 2.0;
                let stroke = |from: (f32, f32), to: (f32, f32), x: f32| {
                    target.DrawLine(Vector2 { X: x + from.0, Y: y + from.1 }, Vector2 { X: x + to.0, Y: y + to.1 }, ink, 1.8, None);
                };
                if checked {
                    let x = bounds.left + 19.0;
                    stroke((-4.5, 0.5), (-1.5, 3.5), x);
                    stroke((-1.5, 3.5), (5.0, -3.5), x);
                }
                if forward {
                    let x = bounds.right - 16.0;
                    stroke((-2.5, -4.5), (2.0, 0.0), x);
                    stroke((2.0, 0.0), (-2.5, 4.5), x);
                }
                if back {
                    let x = bounds.left + 19.0;
                    stroke((2.5, -4.5), (-2.0, 0.0), x);
                    stroke((-2.0, 0.0), (2.5, 4.5), x);
                }
            }
        }
        Ok(())
    }

    /// A line of text, centred vertically in a row.
    fn label(&self, text: &str, width: f32) -> Result<IDWriteTextLayout> {
        let units: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let format = self.text.CreateTextFormat(
                w!("Segoe UI Variable Text"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                14.0,
                w!("en-us"),
            )?;
            let layout = self.text.CreateTextLayout(&units, &format, width, ROW)?;
            layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            Ok(layout)
        }
    }
}

fn color(rgb: [f32; 3], alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb[0], g: rgb[1], b: rgb[2], a: alpha }
}
