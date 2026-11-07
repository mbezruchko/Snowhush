//! The app's own simulation for the site's Autumn Leaves, Rain and Cherry Blossom, built to WebAssembly with the Linux
//! app's shaders: the page hands it the windows and the pointer each frame and reads back the sprites and the piles to
//! draw. Coordinates are the simulation's, y up.

use std::collections::HashSet;

use snowhush_core::geometry::{Rect, V2};
use snowhush_core::simulation::{Mode, PileLayer, SnowSimulation, Sprite, WindowSurface};

/// Floats per pile in the pile table: offset, count, origin x and y, top, bottom, drift, occluder count, then up to
/// sixteen occluders of four.
const PILE: usize = 8 + 16 * 4;

const SHADERS: [&str; 7] = [
    include_str!("../../linux/src/shaders/sprite.vert"),
    include_str!("../../linux/src/shaders/sprite.frag"),
    include_str!("../../linux/src/shaders/pile.vert"),
    include_str!("../../linux/src/shaders/pile.frag"),
    include_str!("../../linux/src/shaders/field.frag"),
    include_str!("../../linux/src/shaders/glass.vert"),
    include_str!("../../linux/src/shaders/glass.frag"),
];
const DROP: &str = include_str!("../../linux/src/shaders/drop.glsl");

pub struct Engine {
    sim: SnowSimulation,
    sprites: Vec<Sprite>,
    heights: Vec<f32>,
    piles: Vec<f32>,
    input: Vec<f32>,
    shader: String,
}

fn engine<'a>(engine: *mut Engine) -> &'a mut Engine {
    unsafe { &mut *engine }
}

const MODES: [Mode; 4] = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom];

/// A simulation `width` by `height` points with this much falling and this mode: Snow, Autumn, Rain, Blossom.
#[unsafe(no_mangle)]
pub extern "C" fn engine_new(width: f32, height: f32, intensity: f32, mode: u32) -> *mut Engine {
    let mut sim = SnowSimulation::new(width, height, intensity);
    sim.set_mode(MODES[(mode as usize).min(3)]);
    let engine = Engine { sim, sprites: Vec::new(), heights: Vec::new(), piles: Vec::new(), input: Vec::new(), shader: String::new() };
    Box::into_raw(Box::new(engine))
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_free(e: *mut Engine) {
    drop(unsafe { Box::from_raw(e) });
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_resize(e: *mut Engine, width: f32, height: f32) {
    engine(e).sim.resize(width, height);
}

/// Clears the screen with a gust, then lets the other kind fall from the top.
#[unsafe(no_mangle)]
pub extern "C" fn engine_switch(e: *mut Engine, mode: u32, calm: bool) {
    let sim = &mut engine(e).sim;
    let direction = sim.breeze_direction();
    sim.switch_mode(MODES[(mode as usize).min(3)], direction, calm);
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_blow(e: *mut Engine, calm: bool) {
    let sim = &mut engine(e).sim;
    let direction = sim.breeze_direction();
    sim.blow_away(direction, calm);
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_intensity(e: *mut Engine, intensity: f32) {
    engine(e).sim.set_intensity(intensity);
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_opacity(e: *mut Engine, opacity: f32) {
    engine(e).sim.falling_opacity = opacity;
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_accumulates(e: *mut Engine, on: bool) {
    engine(e).sim.accumulates = on;
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_thaws(e: *mut Engine, on: bool) {
    engine(e).sim.thaws = on;
}

/// Room for `floats` of input, for the windows; returns where to write them.
#[unsafe(no_mangle)]
pub extern "C" fn engine_input(e: *mut Engine, floats: u32) -> *mut f32 {
    let engine = engine(e);
    engine.input.resize(floats as usize, 0.0);
    engine.input.as_mut_ptr()
}

/// The windows, from the input: per window its id, frame (x, bottom, width, height), corner radius and exponent.
#[unsafe(no_mangle)]
pub extern "C" fn engine_windows(e: *mut Engine, count: u32, interval: f32) {
    let engine = engine(e);
    let surfaces: Vec<WindowSurface> = engine
        .input
        .chunks_exact(7)
        .take(count as usize)
        .map(|w| WindowSurface {
            id: w[0] as isize,
            frame: Rect { x: w[1], y: w[2], width: w[3], height: w[4] },
            blockers: Vec::new(),
            cover: Vec::new(),
            corner_radius: w[5],
            corner_exponent: w[6],
        })
        .collect();
    engine.sim.update_windows(&surfaces, interval, &HashSet::new(), &HashSet::new(), false);
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_step(e: *mut Engine, dt: f32, x: f32, y: f32) {
    engine(e).sim.step(dt, V2::new(x, y));
}

/// Writes the sprites and returns how many; `engine_sprites_at` says where, 33 floats each.
#[unsafe(no_mangle)]
pub extern "C" fn engine_sprites(e: *mut Engine) -> u32 {
    let engine = engine(e);
    engine.sprites.resize(engine.sim.sprite_count(), Sprite::default());
    engine.sim.write_sprites(&mut engine.sprites) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_sprites_at(e: *mut Engine) -> *const Sprite {
    engine(e).sprites.as_ptr()
}

/// Writes the piles' heights and their table and returns how many piles; `engine_heights_at` and `engine_piles_at`
/// say where, and `engine_columns` how many heights there are.
#[unsafe(no_mangle)]
pub extern "C" fn engine_layers(e: *mut Engine) -> u32 {
    let engine = engine(e);
    engine.heights.resize(engine.sim.column_count(), 0.0);
    let piles: Vec<PileLayer> = engine.sim.write_layers(&mut engine.heights);
    engine.piles.clear();
    for pile in &piles {
        let mut row = [0.0; PILE];
        row[..8].copy_from_slice(&[
            pile.offset as f32,
            pile.count as f32,
            pile.origin.x,
            pile.origin.y,
            pile.top,
            pile.bottom,
            pile.drift,
            pile.occluders.len().min(16) as f32,
        ]);
        for (k, occluder) in pile.occluders.iter().take(16).enumerate() {
            row[8 + 4 * k..12 + 4 * k].copy_from_slice(occluder);
        }
        engine.piles.extend_from_slice(&row);
    }
    piles.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_heights_at(e: *mut Engine) -> *const f32 {
    engine(e).heights.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_columns(e: *mut Engine) -> u32 {
    engine(e).heights.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_piles_at(e: *mut Engine) -> *const f32 {
    engine(e).piles.as_ptr()
}

/// Whether rain runs down the glass, drawn through its field.
#[unsafe(no_mangle)]
pub extern "C" fn engine_glass(e: *mut Engine) -> bool {
    engine(e).sim.has_glass_drops()
}

/// Leaves or petals fall (one), petals (two), and whether fallen ones wither (four), added up.
#[unsafe(no_mangle)]
pub extern "C" fn engine_look(e: *mut Engine) -> u32 {
    let sim = &engine(e).sim;
    sim.leafy() as u32 | (sim.blossom() as u32) << 1 | (sim.wilts() as u32) << 2
}

/// A shader's source, with the drop it includes: sprite, pile, field, glass, each vertex then fragment, field's
/// vertex being the sprite's. Returns its length; `engine_text_at` says where.
#[unsafe(no_mangle)]
pub extern "C" fn engine_shader(e: *mut Engine, index: u32) -> u32 {
    let engine = engine(e);
    engine.shader = SHADERS[(index as usize).min(SHADERS.len() - 1)].replace("#include \"drop.glsl\"", DROP);
    engine.shader.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn engine_text_at(e: *mut Engine) -> *const u8 {
    engine(e).shader.as_ptr()
}
