use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use snowhush::card::{self, Card, Content, Hits, MARGIN};
use snowhush_core::geometry::V2;

use crate::log;
use crate::snow::Piece;

const CLASS: PCWSTR = w!("SnowhushWelcome");
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 430.0;
const TITLE: &str = "IT'S SNOWING\nON YOUR DESKTOP.";
const BODY: &str = "Snowhush lives in the system tray. Click its icon to change the snowfall, let the drift melt, or keep the snow off your windows. Sweep it aside with your cursor.";
/// Spacing of the pieces that fly off.
const CELL: f32 = 6.0;
/// Spacing of the crumbling front: fine enough that its edge reads as a line, not as squares.
const GRAIN: f32 = 2.0;
/// Seconds over which a spot fades out once the front reaches it.
const FEATHER: f64 = 0.05;
const TIMER: usize = 1;

pub struct Handlers {
    /// Receives the pieces of the window as it crumbles, in physical screen pixels.
    pub scatter: Box<dyn Fn(Vec<Piece>)>,
    pub finished: Box<dyn Fn()>,
    /// Called instead of `scatter` when the window can't be drawn and just closes.
    pub skipped: Box<dyn Fn()>,
}

thread_local! {
    static WELCOME: RefCell<Option<Welcome>> = const { RefCell::new(None) };
    static HANDLERS: RefCell<Option<Rc<Handlers>>> = const { RefCell::new(None) };
}

struct Welcome {
    window: HWND,
    card: Card,
    hits: Hits,
    pressed: bool,
    /// The mouse went down on the button and hasn't come up yet.
    holding: bool,
    crumble: Option<Crumble>,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Down,
    Move,
    Up,
}

struct Cell {
    /// DIPs from the card's top left.
    x: f32,
    y: f32,
    color: [f32; 3],
    delay: f64,
}

struct Crumble {
    started: Instant,
    base: Vec<u8>,
    /// When the front reaches each pixel of the bitmap.
    front: Vec<f32>,
    cells: Vec<Cell>,
    next: usize,
    duration: f64,
    origin: POINT,
    seed: u64,
}

/// Shown on first launch. Getting started crumbles the window into snow that falls onto the desktop.
/// If the window can't be drawn, the snowfall just starts.
pub fn show(handlers: Handlers) {
    let handlers = Rc::new(handlers);
    HANDLERS.set(Some(handlers.clone()));
    if let Err(error) = open() {
        log!("welcome failed: {error}");
        HANDLERS.set(None);
        (handlers.skipped)();
        (handlers.finished)();
    }
}

fn open() -> Result<()> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let _ = card::register(CLASS, Some(procedure));
    });
    let (scale, origin) = card::placement(WIDTH, HEIGHT);
    let card = Card::new(WIDTH, HEIGHT, scale)?;
    let (pixels, hits) = card.paint(&content(false))?;
    unsafe {
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("Snowhush"),
            WS_POPUP,
            origin.x,
            origin.y,
            card.size.0,
            card.size.1,
            None,
            None,
            None,
            None,
        )?;
        card::present(window, card.size, &pixels, Some(origin));
        WELCOME.set(Some(Welcome { window, card, hits, pressed: false, holding: false, crumble: None }));
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    Ok(())
}

fn content(pressed: bool) -> Content<'static> {
    Content { icon: 112.0, title: TITLE, title_size: 34.0, body: BODY, button: "GET STARTED", pressed, closable: false }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => {
            let over = WELCOME.with_borrow(|welcome| welcome.as_ref().map(|welcome| welcome.over_button(card::point(lparam), true)));
            // The card drags the window anywhere but on the button.
            LRESULT(if over == Some(false) { HTCAPTION as isize } else { HTCLIENT as isize })
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            let phase = match message {
                WM_LBUTTONDOWN => Phase::Down,
                WM_MOUSEMOVE => Phase::Move,
                _ => Phase::Up,
            };
            match phase {
                Phase::Down => unsafe {
                    SetCapture(window);
                },
                Phase::Up => unsafe {
                    let _ = ReleaseCapture();
                },
                Phase::Move => {}
            }
            let outcome =
                WELCOME.with_borrow_mut(|welcome| welcome.as_mut().map(|welcome| (welcome.press(lparam, phase), welcome.window, welcome.card.size)));
            if let Some(((clicked, repaint), window, size)) = outcome {
                if let Some(pixels) = repaint {
                    card::present(window, size, &pixels, None);
                }
                if clicked {
                    get_started();
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_RETURN.0 as usize || wparam.0 == VK_ESCAPE.0 as usize => {
            get_started();
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER => {
            tick();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn get_started() {
    let started = WELCOME.with_borrow_mut(|welcome| {
        let welcome = welcome.as_mut()?;
        if welcome.crumble.is_some() {
            return None;
        }
        welcome.pressed = false;
        let (base, _) = welcome.card.paint(&content(false)).ok()?;
        let mut rect = RECT::default();
        unsafe { GetWindowRect(welcome.window, &mut rect).ok()? };
        welcome.crumble = Some(Crumble::new(base, welcome.card.size, welcome.card.scale, POINT { x: rect.left, y: rect.top }));
        unsafe { SetTimer(Some(welcome.window), TIMER, 15, None) };
        Some(())
    });
    if started.is_none() {
        finish(true);
    }
}

fn tick() {
    let step = WELCOME.with_borrow_mut(|welcome| {
        let welcome = welcome.as_mut()?;
        let scale = welcome.card.scale;
        let crumble = welcome.crumble.as_mut()?;
        let elapsed = crumble.started.elapsed().as_secs_f64();
        let pieces = crumble.pieces(elapsed, scale);
        let frame = crumble.frame(elapsed);
        let done = elapsed >= crumble.duration;
        Some((pieces, done, frame, welcome.window, welcome.card.size))
    });
    let Some((pieces, done, frame, window, size)) = step else { return };
    card::present(window, size, &frame, None);
    if !pieces.is_empty()
        && let Some(handlers) = HANDLERS.with_borrow(Clone::clone)
    {
        (handlers.scatter)(pieces);
    }
    if done {
        finish(false);
    }
}

fn finish(skipped: bool) {
    if let Some(welcome) = WELCOME.take() {
        unsafe {
            let _ = KillTimer(Some(welcome.window), TIMER);
            let _ = DestroyWindow(welcome.window);
        }
    }
    if let Some(handlers) = HANDLERS.take() {
        if skipped {
            (handlers.skipped)();
        }
        (handlers.finished)();
    }
}

impl Welcome {
    /// Whether a point, in screen pixels or the window's own, is on the button; `false` elsewhere on the card.
    fn over_button(&self, point: POINT, screen: bool) -> bool {
        let mut origin = RECT::default();
        if screen {
            unsafe {
                let _ = GetWindowRect(self.window, &mut origin);
            }
        }
        card::contains(&self.hits.button, self.card.dips(POINT { x: point.x - origin.left, y: point.y - origin.top }))
    }

    /// Tracks the button being held: returns whether it was released over the button, a click,
    /// and the window redrawn if the button's look changed.
    fn press(&mut self, lparam: LPARAM, phase: Phase) -> (bool, Option<Vec<u8>>) {
        let over = self.over_button(card::point(lparam), false);
        match phase {
            Phase::Down => self.holding = over,
            Phase::Move if !self.holding => return (false, None),
            _ => {}
        }
        let clicked = phase == Phase::Up && self.holding && over;
        if phase == Phase::Up {
            self.holding = false;
        }
        let pressed = self.holding && over;
        if pressed == self.pressed || self.crumble.is_some() {
            return (clicked, None);
        }
        self.pressed = pressed;
        (clicked, self.card.paint(&content(pressed)).ok().map(|(pixels, _)| pixels))
    }
}

impl Crumble {
    fn new(base: Vec<u8>, size: (i32, i32), scale: f32, origin: POINT) -> Crumble {
        let (width, height) = (size.0 as usize, size.1 as usize);
        let columns = (WIDTH / GRAIN).ceil() as usize;
        let rows = (HEIGHT / GRAIN).ceil() as usize;
        let mut grid = vec![0.0f32; columns * rows];
        for row in 0..rows {
            for column in 0..columns {
                let x = (column as f32 + 0.5) * GRAIN;
                let y = (row as f32 + 0.5) * GRAIN;
                grid[row * columns + column] = arrival(x, y) as f32;
            }
        }
        // Each pixel takes the front of the nearest point on the card, so the shadow goes with it.
        let mut front = vec![0.0f32; width * height];
        for py in 0..height {
            let y = ((py as f32 / scale - MARGIN) / GRAIN - 0.5).clamp(0.0, (rows - 1) as f32);
            let (row, fy) = (y.floor() as usize, y.fract());
            let below = (row + 1).min(rows - 1);
            for px in 0..width {
                let x = ((px as f32 / scale - MARGIN) / GRAIN - 0.5).clamp(0.0, (columns - 1) as f32);
                let (column, fx) = (x.floor() as usize, x.fract());
                let right = (column + 1).min(columns - 1);
                let top = grid[row * columns + column] * (1.0 - fx) + grid[row * columns + right] * fx;
                let bottom = grid[below * columns + column] * (1.0 - fx) + grid[below * columns + right] * fx;
                front[py * width + px] = top * (1.0 - fy) + bottom * fy;
            }
        }

        let mut cells = Vec::new();
        let mut y = CELL / 2.0;
        while y < HEIGHT {
            let mut x = CELL / 2.0;
            while x < WIDTH {
                let px = (((MARGIN + x) * scale) as usize).min(width - 1);
                let py = (((MARGIN + y) * scale) as usize).min(height - 1);
                let i = (py * width + px) * 4;
                let alpha = base[i + 3] as f32 / 255.0;
                if alpha > 0.1 {
                    let channel = |c: u8| (c as f32 / 255.0 / alpha).min(1.0);
                    cells.push(Cell { x, y, color: [channel(base[i + 2]), channel(base[i + 1]), channel(base[i])], delay: arrival(x, y) });
                }
                x += CELL;
            }
            y += CELL;
        }
        cells.sort_by(|a, b| a.delay.total_cmp(&b.delay));
        let duration = grid.iter().copied().fold(0.0, f32::max) as f64 + FEATHER;
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |time| time.as_nanos() as u64) | 1;
        Crumble { started: Instant::now(), base, front, cells, next: 0, duration, origin, seed }
    }

    /// The pieces the front passed since the last call.
    fn pieces(&mut self, elapsed: f64, scale: f32) -> Vec<Piece> {
        let mut pieces = Vec::new();
        while self.next < self.cells.len() && self.cells[self.next].delay <= elapsed {
            let cell = &self.cells[self.next];
            let outward = (cell.x - WIDTH / 2.0) / WIDTH;
            let spread = self.random() * 140.0 - 70.0;
            let lift = 30.0 + self.random() * 120.0;
            let cell = &self.cells[self.next];
            pieces.push(Piece {
                x: self.origin.x as f32 + (MARGIN + cell.x) * scale,
                y: self.origin.y as f32 + (MARGIN + cell.y) * scale,
                velocity: V2::new(outward * 140.0 + spread, lift),
                color: cell.color,
            });
            self.next += 1;
        }
        pieces
    }

    /// The window with everything the front has passed faded out.
    fn frame(&self, elapsed: f64) -> Vec<u8> {
        let mut pixels = self.base.clone();
        for (pixel, &front) in pixels.chunks_exact_mut(4).zip(&self.front) {
            let alpha = ((front as f64 - elapsed) / FEATHER).clamp(0.0, 1.0) as f32;
            if alpha < 1.0 {
                for channel in pixel {
                    *channel = (*channel as f32 * alpha) as u8;
                }
            }
        }
        pixels
    }

    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u32 << 24) as f32
    }
}

/// The front sweeps down in about half a second, held back here and there by smooth noise
/// and roughened by a fine grain so its edge looks like crumbling snow. `x` and `y` are DIPs from the card's top left.
fn arrival(x: f32, y: f32) -> f64 {
    let from_top = y as f64 / HEIGHT as f64;
    // The noise runs in the Mac's coordinates, up from the bottom, so the front looks the same.
    let (x, y) = (x as f64, (HEIGHT - y) as f64);
    let broad = value_noise(x / 46.0, y / 46.0) * 0.65 + value_noise(x / 15.0 + 7.3, y / 15.0 + 2.1) * 0.35;
    from_top * 0.5 + broad * 0.2 + hash((x * 1.7) as i64, (y * 1.7) as i64) * 0.035
}

fn value_noise(x: f64, y: f64) -> f64 {
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = hash(x0, y0) + (hash(x0 + 1, y0) - hash(x0, y0)) * u;
    let bottom = hash(x0, y0 + 1) + (hash(x0 + 1, y0 + 1) - hash(x0, y0 + 1)) * u;
    top + (bottom - top) * v
}

fn hash(x: i64, y: i64) -> f64 {
    let mut n = x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263)) as u32;
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n ^= n >> 16;
    (n & 0xffff) as f64 / 65535.0
}
