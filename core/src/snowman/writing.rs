//! What the snowball fight writes over the wallpaper, as the Mac's SnowballFight shows it: a friend's name and hearts
//! over his snowman, and the round at the top of the main display. The words are chosen and put together here, the
//! hearts drawn and the shadow cast; each app only sets a run of text in its own fonts, through `Typesetter`.

use super::fight::Drawing;
use super::room::{HEARTS, HITS_PER_HEART, Room};
use crate::text::tr;

/// Text the size, weight and colour of the Mac's labels: a name over a snowman.
const LABEL_SIZE: f32 = 11.0;
const LABEL_SHADOW: f32 = 2.0;
const BOARD_SHADOW: f32 = 3.0;
/// The board's first line, down from the top of the display, in points.
const BOARD_TOP: f32 = 40.0;
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const PINK: [f32; 3] = [1.0, 0.45, 0.5];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Semibold,
    Bold,
}

/// A run of text in one size and weight, the size in points.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub size: f32,
    pub weight: Weight,
}

/// A run of text as an app set it: how much of each pixel it covers, rows top first, its line from the font's ascent
/// down to its descent, and how far down that the baseline lies.
pub struct Set {
    pub width: usize,
    pub height: usize,
    pub baseline: usize,
    pub coverage: Vec<u8>,
}

/// Sets runs of text in the app's fonts, `scale` pixels to the point.
pub trait Typesetter {
    fn set(&mut self, run: &Run, scale: f32) -> Option<Set>;
}

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Text {
        run: Run,
        color: [f32; 4],
    },
    /// The five hearts of a snowman with `lives` hits left in him.
    Hearts {
        lives: i32,
        size: f32,
        weight: Weight,
        color: [f32; 3],
    },
}

/// Lines of words and hearts, each centred under the one before, with a soft shadow round them.
#[derive(Clone, Debug, PartialEq)]
pub struct Writing {
    lines: Vec<Vec<Piece>>,
    shadow: f32,
    /// Tells this writing from any other: what it says, how it looks.
    pub key: String,
}

/// How a writing sits on its point: centred on it, or hanging from it, centred across.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hang {
    Centre,
    Below,
}

/// A writing to show, where on the display, in its points from the bottom left.
#[derive(Clone, Debug)]
pub struct Shown {
    pub writing: Writing,
    pub at: [f32; 2],
    pub hang: Hang,
}

/// A writing as pixels: premultiplied RGBA, rows top first, `scale` pixels to the point, with `margin` pixels all
/// round for its shadow.
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub scale: f32,
    pub margin: usize,
}

impl Image {
    /// What it covers shown `at` a point as `hang` says: left, bottom, right, top in the display's points from its
    /// bottom left.
    pub fn rect(&self, at: [f32; 2], hang: Hang) -> [f32; 4] {
        let (width, height) = (self.width as f32 / self.scale, self.height as f32 / self.scale);
        let left = (at[0] - width / 2.0).round();
        let top = match hang {
            Hang::Centre => (at[1] + height / 2.0).round(),
            Hang::Below => at[1] + self.margin as f32 / self.scale,
        };
        [left, top - height, left + width, top]
    }
}

/// A friend's name and what is left of him: his hearts beside it.
pub fn label(name: &str, lives: i32) -> Writing {
    let pieces = vec![
        Piece::Text { run: Run { text: format!("{name} "), size: LABEL_SIZE, weight: Weight::Semibold }, color: [1.0, 1.0, 1.0, 0.92] },
        Piece::Hearts { lives, size: LABEL_SIZE, weight: Weight::Semibold, color: WHITE },
    ];
    Writing { key: format!("label|{name}|{lives}"), lines: vec![pieces], shadow: LABEL_SHADOW }
}

/// The round as this player sees it, at the top of the display: who won it, or his hearts, or that he is out, and
/// everyone's wins; and `hint`, how to throw, while it isn't empty. None out of a room with no hint to give.
pub fn board(room: &Room, hint: &str) -> Option<Writing> {
    let mut lines = Vec::new();
    let mut key = String::from("board");
    if let Some(me) = room.me() {
        let name = |id: &str| if id == me { tr("fight.you").to_string() } else { room.name(id).unwrap_or_default().to_string() };
        let big = room.winner.is_some();
        let (size, weight) = (if big { 26.0 } else { 18.0 }, Weight::Bold);
        let headline = match &room.winner {
            Some(winner) if winner == me => Some(tr("fight.youwon").to_string()),
            Some(winner) => Some(tr("fight.won").replace("%@", &name(winner))),
            None if room.is_out(me) => Some(tr("fight.out").to_string()),
            None => None,
        };
        key.push_str(&format!("|{}|{}", headline.as_deref().unwrap_or_default(), room.lives(me)));
        lines.push(vec![match headline {
            Some(text) => {
                let color = if big { WHITE } else { PINK };
                Piece::Text { run: Run { text, size, weight }, color: [color[0], color[1], color[2], 1.0] }
            }
            None => Piece::Hearts { lives: room.lives(me), size, weight, color: PINK },
        }]);
        let everyone: Vec<&str> = std::iter::once(me).chain(room.players.iter().map(|player| player.id.as_str())).collect();
        if everyone.iter().any(|id| room.wins(id) > 0) {
            let wins = everyone.iter().map(|id| format!("{} {}", name(id), room.wins(id))).collect::<Vec<_>>().join(" · ");
            let text = format!("{}: {wins}", tr("fight.wins"));
            key.push_str(&format!("|{text}"));
            lines.push(vec![Piece::Text { run: Run { text, size: 12.0, weight: Weight::Semibold }, color: [1.0, 1.0, 1.0, 0.9] }]);
        }
    }
    if !hint.is_empty() {
        key.push_str(&format!("|{hint}"));
        lines.push(vec![Piece::Text { run: Run { text: hint.to_string(), size: 13.0, weight: Weight::Semibold }, color: [1.0, 1.0, 1.0, 0.9] }]);
    }
    (!lines.is_empty()).then_some(Writing { lines, shadow: BOARD_SHADOW, key })
}

/// All the fight writes over a display `view` points across: each friend's label over his snowman, and on the `main`
/// display the board.
pub fn shown(snowmen: &[Drawing], room: &Room, hint: &str, main: bool, view: [f32; 2]) -> Vec<Shown> {
    let mut shown: Vec<Shown> = snowmen
        .iter()
        .filter_map(|figure| {
            let (name, lives) = figure.label.as_ref()?;
            Some(Shown { writing: label(name, *lives), at: figure.head, hang: Hang::Centre })
        })
        .collect();
    if main && let Some(writing) = board(room, hint) {
        shown.push(Shown { writing, at: [view[0] / 2.0, view[1] - BOARD_TOP], hang: Hang::Below });
    }
    shown
}

/// A run set, or the hearts drawn, ready to be put in a line.
struct Block {
    width: usize,
    height: usize,
    baseline: usize,
    /// Premultiplied RGBA.
    pixels: Vec<u8>,
}

impl Writing {
    /// Draws the writing, its runs set by `typesetter`, `scale` pixels to the point. Runs it can't set are left out.
    pub fn image(&self, typesetter: &mut dyn Typesetter, scale: f32) -> Image {
        let lines: Vec<Vec<Block>> = self
            .lines
            .iter()
            .map(|line| line.iter().filter_map(|piece| block(piece, typesetter, scale)).collect::<Vec<_>>())
            .filter(|line| !line.is_empty())
            .collect();
        let sigma = self.shadow * scale / 2.0;
        let margin = (sigma * 3.0).ceil() as usize + 1;
        let measures: Vec<(usize, usize, usize)> = lines
            .iter()
            .map(|line| {
                let width = line.iter().map(|block| block.width).sum();
                let ascent = line.iter().map(|block| block.baseline).max().unwrap_or(0);
                let descent = line.iter().map(|block| block.height - block.baseline).max().unwrap_or(0);
                (width, ascent, descent)
            })
            .collect();
        let inner = measures.iter().map(|m| m.0).max().unwrap_or(0);
        let tall: usize = measures.iter().map(|m| m.1 + m.2).sum();
        let (width, height) = (inner + 2 * margin, tall + 2 * margin);
        let mut pixels = vec![0u8; width * height * 4];
        let mut top = margin;
        for (line, (line_width, ascent, descent)) in lines.iter().zip(&measures) {
            let mut left = margin + (inner - line_width) / 2;
            for block in line {
                let y0 = top + ascent - block.baseline;
                for row in 0..block.height {
                    let from = row * block.width * 4;
                    let to = ((y0 + row) * width + left) * 4;
                    for k in 0..block.width * 4 {
                        let value = pixels[to + k] as u16 + block.pixels[from + k] as u16;
                        pixels[to + k] = value.min(255) as u8;
                    }
                }
                left += block.width;
            }
            top += ascent + descent;
        }
        shade(&mut pixels, width, height, sigma, 0.6);
        Image { width, height, pixels, scale, margin }
    }
}

fn block(piece: &Piece, typesetter: &mut dyn Typesetter, scale: f32) -> Option<Block> {
    match piece {
        Piece::Text { run, color } => {
            let set = typesetter.set(run, scale)?;
            let pixels = set
                .coverage
                .iter()
                .flat_map(|&c| {
                    let a = c as f32 / 255.0 * color[3];
                    [color[0] * a, color[1] * a, color[2] * a, a].map(|v| (v * 255.0).round() as u8)
                })
                .collect();
            Some(Block { width: set.width, height: set.height, baseline: set.baseline, pixels })
        }
        Piece::Hearts { lives, size, weight, color } => {
            // The hearts stand in a line as tall as the font's, set from a space when the app can set one.
            let line = typesetter.set(&Run { text: " ".into(), size: *size, weight: *weight }, scale);
            let (height, baseline) =
                line.map_or(((size * 1.2 * scale).ceil() as usize, (size * 0.95 * scale).round() as usize), |set| (set.height, set.baseline));
            Some(hearts(*lives, *size * scale, height, baseline, *color))
        }
    }
}

/// Five hearts, `em` pixels to the font's size: full for each whole one left, fainter for one that has taken a hit
/// or two, an outline for each one gone.
fn hearts(lives: i32, em: f32, height: usize, baseline: usize, color: [f32; 3]) -> Block {
    let advance = (em * 0.82).round().max(1.0) as usize;
    let width = advance * HEARTS as usize;
    let mut pixels = vec![0u8; width * height * 4];
    let per = HITS_PER_HEART as i32;
    let (heart_w, heart_h) = (em * 0.7, em * 0.66);
    let bottom = baseline as f32 + em * 0.02;
    for k in 0..HEARTS as i32 {
        let left = (lives - k * per).clamp(0, per);
        let alpha = if left == per || left == 0 { 1.0 } else { 0.3 + 0.4 * left as f32 / per as f32 };
        let middle = k as f32 * advance as f32 + advance as f32 / 2.0;
        let stroke = if left > 0 { None } else { Some((em * 0.075).max(1.0)) };
        const STEPS: usize = 4;
        for y in 0..height {
            for x in (k as usize * advance)..((k as usize + 1) * advance) {
                let mut inside = 0;
                for sy in 0..STEPS {
                    for sx in 0..STEPS {
                        let px = x as f32 + (sx as f32 + 0.5) / STEPS as f32;
                        let py = y as f32 + (sy as f32 + 0.5) / STEPS as f32;
                        let u = (px - middle) / (heart_w / 2.0) * HEART_X;
                        let v = HEART_BOTTOM + (bottom - py) / heart_h * (HEART_TOP - HEART_BOTTOM);
                        let hit = match stroke {
                            None => in_heart(u, v),
                            Some(stroke) => {
                                let k = 1.0 - stroke / (heart_h / 2.0);
                                let centre = (HEART_TOP + HEART_BOTTOM) / 2.0;
                                in_heart(u, v) && !in_heart(u / k, centre + (v - centre) / k)
                            }
                        };
                        inside += hit as usize;
                    }
                }
                if inside > 0 {
                    let a = inside as f32 / (STEPS * STEPS) as f32 * alpha;
                    let at = (y * width + x) * 4;
                    let value = [color[0] * a, color[1] * a, color[2] * a, a];
                    for c in 0..4 {
                        pixels[at + c] = (value[c] * 255.0).round() as u8;
                    }
                }
            }
        }
    }
    Block { width, height, baseline, pixels }
}

/// The heart (x² + y² − 1)³ = x²y³ reaches this far out to either side, and from this low to this high.
const HEART_X: f32 = 1.139;
const HEART_BOTTOM: f32 = -1.0;
const HEART_TOP: f32 = 1.236;

fn in_heart(x: f32, y: f32) -> bool {
    let a = x * x + y * y - 1.0;
    a * a * a - x * x * y * y * y <= 0.0
}

/// Puts a soft black shadow, `opacity` at its darkest, under premultiplied pixels: their alpha blurred, `sigma` pixels.
fn shade(pixels: &mut [u8], width: usize, height: usize, sigma: f32, opacity: f32) {
    if width == 0 || height == 0 {
        return;
    }
    let mut alpha: Vec<f32> = pixels.chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
    // Three box blurs come close to a Gaussian.
    let radius = ((sigma * sigma + 1.0).sqrt() * 0.87).round().max(1.0) as usize;
    let mut scratch = vec![0.0; alpha.len()];
    for _ in 0..3 {
        box_blur(&alpha, &mut scratch, width, height, radius, true);
        box_blur(&scratch, &mut alpha, width, height, radius, false);
    }
    for (pixel, shadow) in pixels.chunks_exact_mut(4).zip(alpha) {
        let a = pixel[3] as f32 / 255.0;
        let under = shadow.min(1.0) * opacity * (1.0 - a);
        pixel[3] = ((a + under) * 255.0).round().min(255.0) as u8;
    }
}

fn box_blur(from: &[f32], to: &mut [f32], width: usize, height: usize, radius: usize, across: bool) {
    let (count, length) = if across { (height, width) } else { (width, height) };
    let at = |line: usize, k: usize| if across { line * width + k } else { k * width + line };
    let span = (2 * radius + 1) as f32;
    for line in 0..count {
        let mut sum = 0.0;
        for k in 0..=radius.min(length - 1) {
            sum += from[at(line, k)];
        }
        for k in 0..length {
            to[at(line, k)] = sum / span;
            if k + radius + 1 < length {
                sum += from[at(line, k + radius + 1)];
            }
            if k >= radius {
                sum -= from[at(line, k - radius)];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets each character as a box a size wide, its line 1.2 sizes tall.
    struct Boxes;

    impl Typesetter for Boxes {
        fn set(&mut self, run: &Run, scale: f32) -> Option<Set> {
            let em = (run.size * scale).round() as usize;
            let width = em * run.text.chars().count();
            let height = (em as f32 * 1.2) as usize;
            Some(Set { width, height, baseline: em, coverage: vec![255; width * height] })
        }
    }

    #[test]
    fn a_label_has_its_name_and_five_hearts() {
        let image = label("Ann", 15).image(&mut Boxes, 2.0);
        let em = 22;
        assert_eq!(image.width, 4 * em + 5 * (em as f32 * 0.82).round() as usize + 2 * image.margin);
        let rect = image.rect([100.0, 50.0], Hang::Centre);
        assert!((rect[0] + rect[2]) / 2.0 - 100.0 < 1.0 && (rect[1] + rect[3]) / 2.0 - 50.0 < 1.0);
    }

    #[test]
    fn hearts_fill_with_lives() {
        let full = hearts(15, 22.0, 26, 22, WHITE);
        let none = hearts(0, 22.0, 26, 22, WHITE);
        let ink = |block: &Block| block.pixels.chunks_exact(4).map(|p| p[3] as u32).sum::<u32>();
        assert!(ink(&full) > 2 * ink(&none));
        assert!(ink(&none) > 0);
    }

    #[test]
    fn the_board_shows_only_with_something_to_say() {
        let room = Room::default();
        assert!(board(&room, "").is_none());
        let hint = board(&room, "Click a snowman").expect("hint");
        assert_eq!(hint.lines.len(), 1);
        let image = hint.image(&mut Boxes, 1.0);
        let rect = image.rect([500.0, 960.0], Hang::Below);
        assert_eq!(rect[3] - image.margin as f32, 960.0);
    }

    #[test]
    fn the_shadow_stays_under_the_text() {
        let mut pixels = vec![0u8; 9 * 9 * 4];
        pixels[(4 * 9 + 4) * 4..(4 * 9 + 4) * 4 + 4].copy_from_slice(&[255; 4]);
        shade(&mut pixels, 9, 9, 1.0, 0.6);
        assert_eq!(&pixels[(4 * 9 + 4) * 4..(4 * 9 + 4) * 4 + 4], &[255; 4]);
        assert!(pixels[(4 * 9 + 5) * 4 + 3] > 0 && pixels[(4 * 9 + 5) * 4] == 0);
    }
}
