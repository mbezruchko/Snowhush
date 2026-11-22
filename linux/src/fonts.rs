//! Sets the snowball fight's words in the desktop's own sans-serif, as fontconfig picks it, and each character that
//! font lacks, a friend's name in another script, in the one fontconfig says has it. Without fontconfig the words are
//! left out and only the hearts show.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont, VariableFont, point};
use snowhush_core::snowman::writing::{Run, Set, Typesetter, Weight};

use crate::log;

#[derive(Default)]
pub struct Fonts {
    /// Where fontconfig found the sans-serif in each weight, once asked; none when there is none to be had.
    found: HashMap<Weight, Option<(PathBuf, u32)>>,
    /// The sans-serif in each weight, read while there are words to set.
    faces: HashMap<Weight, Option<FontVec>>,
    /// The font fontconfig gives for a character the sans-serif lacks, looked up once each. Such a font, maybe a large
    /// collection of Chinese, is read only while a run is set, and let go after.
    fallbacks: HashMap<(Weight, char), Option<(PathBuf, u32)>>,
    warned: bool,
}

fn weight_name(weight: Weight) -> &'static str {
    match weight {
        Weight::Semibold => "semibold",
        Weight::Bold => "bold",
    }
}

fn weight_value(weight: Weight) -> f32 {
    match weight {
        Weight::Semibold => 600.0,
        Weight::Bold => 700.0,
    }
}

/// The file and face fontconfig matches `pattern` to.
fn matched(pattern: &str) -> Option<(PathBuf, u32)> {
    let output = Command::new("fc-match")
        .args(["--format", "%{file}\n%{index}", pattern])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8(output.stdout).ok()?;
    let mut lines = text.lines();
    let file = PathBuf::from(lines.next().filter(|file| !file.is_empty())?);
    let index = lines.next().and_then(|index| index.trim().parse().ok()).unwrap_or(0);
    Some((file, index))
}

/// The face fontconfig matched, in `weight` when it is a variable font: fontconfig numbers a variable font's named
/// instances above its faces, the face in the low 16 bits.
fn load((file, index): &(PathBuf, u32), weight: Weight) -> Option<FontVec> {
    let data = std::fs::read(file).ok()?;
    let mut font = FontVec::try_from_vec_and_index(data, index & 0xFFFF).ok()?;
    font.set_variation(b"wght", weight_value(weight));
    Some(font)
}

impl Fonts {
    /// Lets go of the fonts read, while there are no words to set: a sans-serif may be a large collection.
    pub fn let_go(&mut self) {
        self.faces.clear();
    }

    fn face(&mut self, weight: Weight) -> Option<&FontVec> {
        if !self.faces.contains_key(&weight) {
            let asked = !self.found.contains_key(&weight);
            let found = self.found.entry(weight).or_insert_with(|| matched(&format!("sans-serif:weight={}", weight_name(weight)))).clone();
            let face = found.as_ref().and_then(|found| load(found, weight));
            match (&found, &face) {
                (Some((file, _)), Some(_)) if asked => log!("fight: words in {}", file.display()),
                (_, None) if !self.warned => {
                    self.warned = true;
                    log!("fight: no font from fc-match, so names and the round go unwritten");
                }
                _ => {}
            }
            self.faces.insert(weight, face);
        }
        self.faces.get(&weight).and_then(Option::as_ref)
    }

    fn fallback(&mut self, weight: Weight, c: char) -> Option<(PathBuf, u32)> {
        self.fallbacks
            .entry((weight, c))
            .or_insert_with(|| matched(&format!("sans-serif:weight={}:charset={:x}", weight_name(weight), c as u32)))
            .clone()
    }
}

/// Pixels to each of a font's own units, `em` pixels to its size.
fn px_scale(font: &FontVec, em: f32) -> PxScale {
    PxScale::from(em * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0))
}

impl Typesetter for Fonts {
    fn set(&mut self, run: &Run, scale: f32) -> Option<Set> {
        let em = run.size * scale;
        self.face(run.weight)?;
        let mut others: Vec<((PathBuf, u32), FontVec)> = Vec::new();
        let mut chosen: Vec<(char, Option<usize>)> = Vec::new();
        for c in run.text.chars() {
            let face = self.face(run.weight).expect("found");
            if face.glyph_id(c).0 != 0 || c.is_whitespace() {
                chosen.push((c, None));
                continue;
            }
            let font = self.fallback(run.weight, c).and_then(|found| {
                if let Some(k) = others.iter().position(|(known, _)| *known == found) {
                    return Some(k);
                }
                let font = load(&found, run.weight).filter(|font| font.glyph_id(c).0 != 0)?;
                others.push((found, font));
                Some(others.len() - 1)
            });
            chosen.push((c, font));
        }
        let face = self.face(run.weight).expect("found");
        let main = face.as_scaled(px_scale(face, em));
        let (ascent, descent) = (main.ascent(), main.descent());
        let height = (ascent - descent).ceil().max(1.0) as usize;
        let baseline = ascent.ceil() as usize;
        let font_of = |which: Option<usize>| which.map_or(face, |k| &others[k].1);
        let mut caret = 0.0f32;
        let mut placed: Vec<(Option<usize>, GlyphId, f32)> = Vec::new();
        let mut last: Option<(Option<usize>, GlyphId)> = None;
        for (c, which) in &chosen {
            let font = font_of(*which);
            let scaled = font.as_scaled(px_scale(font, em));
            let id = font.glyph_id(*c);
            if let Some((before, previous)) = last
                && before == *which
            {
                caret += scaled.kern(previous, id);
            }
            placed.push((*which, id, caret));
            caret += scaled.h_advance(id);
            last = Some((*which, id));
        }
        let width = caret.ceil().max(1.0) as usize;
        let mut coverage = vec![0u8; width * height];
        for (which, id, x) in placed {
            let font = font_of(which);
            let glyph = id.with_scale_and_position(px_scale(font, em), point(x, baseline as f32));
            let Some(outlined) = font.outline_glyph(glyph) else { continue };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, c| {
                let (x, y) = (bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32);
                if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
                    let at = y as usize * width + x as usize;
                    coverage[at] = coverage[at].saturating_add((c * 255.0).round() as u8);
                }
            });
        }
        Some(Set { width, height, baseline, coverage })
    }
}
