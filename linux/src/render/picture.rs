//! Draws the snow lying in a wallpaper into a surface of the whole display, as the Mac's PictureRenderer does: its cover
//! on the surfaces that hold it and the caps along its ridges. It draws only when asked, when that snow has changed;
//! what it draws the compositor then shows as it is, frame after frame, for nothing.

use glow::HasContext;
use snowhush_core::wallpaper::{GROUND, Ridge, Scene};

use super::{Gpu, Program, Surface, bytes, program};

const COVER_VERTEX: &str = include_str!("../shaders/cover.vert");
const LIE_FRAGMENT: &str = include_str!("../shaders/lie.frag");
const SHOW_FRAGMENT: &str = include_str!("../shaders/show.frag");
const CAP_VERTEX: &str = include_str!("../shaders/cap.vert");
const CAP_FRAGMENT: &str = include_str!("../shaders/cap.frag");
const PICTURE: &str = include_str!("../shaders/picture.glsl");
const SHARED: [&str; 7] = ["viewSize", "scale", "level", "range", "thickest", "veil", "haze"];
/// A cap's full thickness on the nearest ridge once all the snow has fallen, in points.
const CAP_DEPTH: f32 = 9.0;
/// How thick the snow lies on the nearest level ground once all of it has fallen, in points.
const DEPTH: f32 = 5.0;

pub(super) struct Programs {
    lie: Program,
    show: Program,
    cap: Program,
    cap_array: glow::VertexArray,
    noise: glow::Texture,
}

/// One ridge's cap: where its columns start in the buffer of them all, as vertices.
struct Cap {
    first: usize,
    count: usize,
    left: f32,
    near: f32,
}

/// What was found in one wallpaper, as the graphics card has it.
pub struct Painting {
    relief: glow::Texture,
    light: glow::Texture,
    dabs: glow::Texture,
    specks: glow::Texture,
    dabs_version: Option<u64>,
    dabs_size: (usize, usize),
    /// Where the first pass leaves how the snow lies, half the surface's size, and the size it was made for.
    lie: Option<(glow::Texture, glow::Framebuffer, (i32, i32))>,
    columns: glow::Buffer,
    caps: Vec<Cap>,
    column_width: f32,
    range: [f32; 4],
    haze: [f32; 4],
}

/// How the snow lies in the picture now: `level` fallen everywhere, `veil` over the farthest things, the dabs and the
/// frost of the flakes seen landing over `dabs_area`, left, top, width and height in points from the display's top left.
pub struct Lying<'a> {
    pub level: f32,
    pub veil: f32,
    pub dabs: &'a [u8],
    pub specks: &'a [u8],
    pub dabs_size: (usize, usize),
    pub dabs_version: u64,
    pub dabs_area: [f32; 4],
}

impl Gpu {
    fn picture_programs(&mut self) -> Result<&Programs, String> {
        if self.picture.is_none() {
            let included = |source: &str| source.replace("#include \"picture.glsl\"", PICTURE);
            let gl = &self.gl;
            let programs = unsafe {
                let lie =
                    program(gl, COVER_VERTEX, &included(LIE_FRAGMENT), &[&SHARED[..], &["dabsArea", "dabs", "relief", "specks", "noise"]].concat())?;
                let show = program(gl, COVER_VERTEX, &included(SHOW_FRAGMENT), &[&SHARED[..], &["lie", "relief", "light"]].concat())?;
                let cap = program(
                    gl,
                    CAP_VERTEX,
                    &included(CAP_FRAGMENT),
                    &[&SHARED[..], &["left", "columnWidth", "thickness", "near", "light"]].concat(),
                )?;
                let cap_array = gl.create_vertex_array()?;
                let noise = texture(gl, glow::R8, glow::RED, 256, 256, &noise(), false, glow::REPEAT)?;
                Programs { lie, show, cap, cap_array, noise }
            };
            self.picture = Some(programs);
        }
        Ok(self.picture.as_ref().expect("made"))
    }

    /// Takes in what was found in a wallpaper, with the simulation's ridges `column_width` apart.
    pub fn paint(&mut self, scene: &Scene, column_width: f32) -> Result<Painting, String> {
        self.picture_programs()?;
        let gl = &self.gl;
        let relief = &scene.relief;
        let light = &scene.light;
        unsafe {
            let relief_texture = texture(gl, glow::RGBA8, glow::RGBA, relief.width, relief.height, &relief.packed(), true, glow::CLAMP_TO_EDGE)?;
            let light_texture = texture(gl, glow::RGBA8, glow::RGBA, light.width, light.height, &light.pixels, false, glow::CLAMP_TO_EDGE)?;
            let dabs = texture(gl, glow::R8, glow::RED, 1, 1, &[0], false, glow::CLAMP_TO_EDGE)?;
            let specks = texture(gl, glow::R8, glow::RED, 1, 1, &[0], false, glow::CLAMP_TO_EDGE)?;
            let (values, caps) = columns(&scene.ridges, column_width);
            let columns = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(columns));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&values), glow::STATIC_DRAW);
            let haze = scene.light.haze;
            Ok(Painting {
                relief: relief_texture,
                light: light_texture,
                dabs,
                specks,
                dabs_version: None,
                dabs_size: (1, 1),
                lie: None,
                columns,
                caps,
                column_width,
                range: [scene.terrain.far, scene.terrain.close, GROUND, 0.0],
                haze: [haze[0], haze[1], haze[2], 1.0],
            })
        }
    }

    pub fn unpaint(&self, painting: Painting) {
        let gl = &self.gl;
        unsafe {
            for texture in [painting.relief, painting.light, painting.dabs, painting.specks] {
                gl.delete_texture(texture);
            }
            if let Some((texture, framebuffer, _)) = painting.lie {
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(texture);
            }
            gl.delete_buffer(painting.columns);
        }
    }

    /// Draws the snow lying in the picture on `surface`, the whole display `view` points across, and shows it.
    pub fn draw_picture(&mut self, surface: &Surface, painting: &mut Painting, lying: &Lying, view: [f32; 2]) -> Result<(), String> {
        self.egl.make_current(self.display, Some(surface.surface), Some(surface.surface), Some(self.context)).map_err(|error| error.to_string())?;
        self.picture_programs()?;
        let programs = self.picture.as_ref().expect("made");
        let gl = &self.gl;
        let half = ((surface.width / 2).max(1), (surface.height / 2).max(1));
        unsafe {
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            if painting.dabs_version != Some(lying.dabs_version) && lying.dabs.len() >= lying.dabs_size.0 * lying.dabs_size.1 && lying.dabs_size.0 > 0
            {
                let (width, height) = (lying.dabs_size.0 as i32, lying.dabs_size.1 as i32);
                for (texture, values) in [(painting.dabs, lying.dabs), (painting.specks, lying.specks)] {
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    let data = glow::PixelUnpackData::Slice(Some(values));
                    if painting.dabs_size == lying.dabs_size {
                        gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, width, height, glow::RED, glow::UNSIGNED_BYTE, data);
                    } else {
                        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R8 as i32, width, height, 0, glow::RED, glow::UNSIGNED_BYTE, data);
                    }
                }
                painting.dabs_size = lying.dabs_size;
                painting.dabs_version = Some(lying.dabs_version);
            }
            if painting.lie.is_none_or(|(_, _, size)| size != half) {
                if let Some((texture, framebuffer, _)) = painting.lie.take() {
                    gl.delete_framebuffer(framebuffer);
                    gl.delete_texture(texture);
                }
                let texture = texture(gl, glow::RGBA8, glow::RGBA, half.0 as usize, half.1 as usize, &[], false, glow::CLAMP_TO_EDGE)?;
                let framebuffer = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
                painting.lie = Some((texture, framebuffer, half));
            }
            let (lie, framebuffer, _) = painting.lie.expect("made");
            let level = lying.level;
            // The first snow shows soon; past covering what holds it, it goes on getting deeper at half the pace.
            let amount = if level <= 1.0 { level.max(0.0).powf(0.6) } else { 1.0 + (level - 1.0) * 0.5 };
            let thickest = (DEPTH * level.max(0.0).powf(0.7)).max(0.01);
            let scale = surface.width as f32 / view[0].max(1.0);
            let shared = |program: &Program| {
                gl.use_program(Some(program.program));
                gl.uniform_2_f32_slice(program.location("viewSize"), &view);
                gl.uniform_1_f32(program.location("scale"), scale);
                gl.uniform_1_f32(program.location("level"), amount);
                gl.uniform_4_f32_slice(program.location("range"), &painting.range);
                gl.uniform_1_f32(program.location("thickest"), thickest);
                gl.uniform_1_f32(program.location("veil"), lying.veil);
                gl.uniform_4_f32_slice(program.location("haze"), &painting.haze);
            };
            let bind = |unit: u32, texture: glow::Texture| {
                gl.active_texture(glow::TEXTURE0 + unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            };
            gl.bind_vertex_array(Some(self.empty_array));
            gl.disable(glow::BLEND);
            // How the snow lies, at half the size: nothing it is worked out from is finer.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.viewport(0, 0, half.0, half.1);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            let lie_program = &programs.lie;
            shared(lie_program);
            gl.uniform_4_f32_slice(lie_program.location("dabsArea"), &lying.dabs_area);
            for (unit, name, texture) in
                [(0, "dabs", painting.dabs), (1, "relief", painting.relief), (2, "specks", painting.specks), (3, "noise", programs.noise)]
            {
                bind(unit, texture);
                gl.uniform_1_i32(lie_program.location(name), unit as i32);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            // The snow as it is seen, and the caps along the ridges.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, surface.width, surface.height);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            let show = &programs.show;
            shared(show);
            for (unit, name, texture) in [(0, "lie", lie), (1, "relief", painting.relief), (2, "light", painting.light)] {
                bind(unit, texture);
                gl.uniform_1_i32(show.location(name), unit as i32);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            if !painting.caps.is_empty() {
                let cap = &programs.cap;
                shared(cap);
                bind(2, painting.light);
                gl.uniform_1_i32(cap.location("light"), 2);
                gl.uniform_1_f32(cap.location("columnWidth"), painting.column_width);
                gl.bind_vertex_array(Some(programs.cap_array));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(painting.columns));
                gl.enable_vertex_attrib_array(0);
                for one in &painting.caps {
                    // The farther off the ridge, the thinner the same snow looks on it.
                    let thickness = CAP_DEPTH * level.max(0.0).powf(0.8) * (0.35 + 0.65 * one.near);
                    gl.uniform_1_f32(cap.location("left"), one.left);
                    gl.uniform_1_f32(cap.location("thickness"), thickness);
                    gl.uniform_1_f32(cap.location("near"), one.near);
                    gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 16, (one.first * 16) as i32);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, one.count as i32);
                }
            }
            gl.bind_vertex_array(None);
            gl.active_texture(glow::TEXTURE0);
        }
        self.egl.swap_buffers(self.display, surface.surface).map_err(|error| error.to_string())
    }
}

/// Each ridge's cap, worked out once: thinner where the ridge climbs steeply, uneven along it as drifted snow is, and
/// tapering away to its ends. Each column twice, once for each corner of the strip.
fn columns(ridges: &[Ridge], column_width: f32) -> (Vec<f32>, Vec<Cap>) {
    let smoothstep = snowhush_core::geometry::smoothstep;
    let fraction = |x: f32| x - x.floor();
    let mut values = Vec::new();
    let mut caps = Vec::new();
    for ridge in ridges.iter().filter(|ridge| ridge.tops.len() > 4) {
        let tops = &ridge.tops;
        let count = tops.len();
        // The same ridge always gets the same cap, however often its wallpaper is read again.
        let seed = ridge.left * 12.9898 + count as f32 * 78.233;
        let phases =
            (2.0 * std::f32::consts::PI * fraction(seed.sin() * 43758.547), 2.0 * std::f32::consts::PI * fraction((seed + 1.7).sin() * 43758.547));
        let near = ridge.nearness.clamp(0.0, 1.0);
        caps.push(Cap { first: values.len() / 4, count: count * 2, left: ridge.left, near });
        for k in 0..count {
            let (a, b) = (k.saturating_sub(3), (k + 3).min(count - 1));
            let slope = (tops[b] - tops[a]).abs() / ((b - a) as f32 * column_width);
            let lie = (1.0 - slope / 2.2).clamp(0.3, 1.0);
            let x = k as f32 * column_width;
            let uneven = 0.85 + 0.2 * (x * 0.045 + phases.0).sin() + 0.1 * (x * 0.13 + phases.1).sin();
            let end = ((k.min(count - 1 - k)) as f32 / 10.0).min(1.0);
            let taper = end * end * (3.0 - 2.0 * end);
            let (low, high) = (k.saturating_sub(5), (k + 5).min(count - 1));
            let smooth = tops[low..=high].iter().sum::<f32>() / (high - low + 1) as f32;
            let room = ridge.room.get(k).copied().unwrap_or(1.0);
            // A cap sinks its foot three points into a mountain's top, and hardly at all into a branch's.
            let foot = 1.0 + 2.0 * smoothstep(0.25, 0.9, room);
            // On a branch the cap goes by the branch's own thickness, however near it is.
            let slim = (room / (0.35 + 0.65 * near)).min(1.0);
            let carried = slim + (1.0 - slim) * smoothstep(0.6, 0.95, room);
            // A mountain far off is small on the screen, and a cap a few points thick on it would be a drift as deep
            // as a house: there the snow shows on its faces, and its ridge is left without a line drawn round it.
            let mountain = if ridge.mass.get(k).is_some_and(|mass| *mass >= 0.5) { 1.0 } else { 0.0 };
            let far = 1.0 - mountain * (1.0 - smoothstep(0.3, 0.45, near));
            let column = [tops[k], smooth, lie * uneven * taper * carried * far, foot + 8.0 * mountain];
            values.extend(column);
            values.extend(column);
        }
    }
    (values, caps)
}

/// A gradient noise that tiles, 32 cells across 256 pixels: unlike noise of values at a grid's corners it has no square
/// blobs for a threshold to cut out. Always the same, so the snow's edge doesn't move between drawings.
fn noise() -> Vec<u8> {
    let (size, cells) = (256usize, 32usize);
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    let slopes: Vec<(f32, f32)> = (0..cells * cells)
        .map(|_| {
            let angle = next() * 2.0 * std::f32::consts::PI;
            (angle.cos(), angle.sin())
        })
        .collect();
    let slope = |x: i32, y: i32| slopes[((y + cells as i32) as usize % cells) * cells + (x + cells as i32) as usize % cells];
    let dot = |s: (f32, f32), x: f32, y: f32| s.0 * x + s.1 * y;
    let step = cells as f32 / size as f32;
    let mut pixels = vec![0u8; size * size];
    for py in 0..size {
        for px in 0..size {
            let (x, y) = ((px as f32 + 0.5) * step, (py as f32 + 0.5) * step);
            let (ix, iy) = (x.floor(), y.floor());
            let (fx, fy) = (x - ix, y - iy);
            let (cx, cy) = (ix as i32, iy as i32);
            let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
            let (sx, sy) = (fade(fx), fade(fy));
            let a = dot(slope(cx, cy), fx, fy);
            let b = dot(slope(cx + 1, cy), fx - 1.0, fy);
            let c = dot(slope(cx, cy + 1), fx, fy - 1.0);
            let d = dot(slope(cx + 1, cy + 1), fx - 1.0, fy - 1.0);
            let (top, bottom) = (a + (b - a) * sx, c + (d - c) * sx);
            pixels[py * size + px] = ((0.5 + 0.75 * (top + (bottom - top) * sy)).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    pixels
}

/// A texture of `pixels`, top row first, or with nothing in it yet when they are empty.
#[allow(clippy::too_many_arguments)]
unsafe fn texture(
    gl: &glow::Context,
    internal: u32,
    format: u32,
    width: usize,
    height: usize,
    pixels: &[u8],
    mipmapped: bool,
    wrap: u32,
) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        let data = glow::PixelUnpackData::Slice((!pixels.is_empty()).then_some(pixels));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, width.max(1) as i32, height.max(1) as i32, 0, format, glow::UNSIGNED_BYTE, data);
        let min = if mipmapped { glow::LINEAR_MIPMAP_LINEAR } else { glow::LINEAR };
        for (parameter, value) in
            [(glow::TEXTURE_MIN_FILTER, min), (glow::TEXTURE_MAG_FILTER, glow::LINEAR), (glow::TEXTURE_WRAP_S, wrap), (glow::TEXTURE_WRAP_T, wrap)]
        {
            gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
        }
        if mipmapped {
            gl.generate_mipmap(glow::TEXTURE_2D);
        }
        Ok(texture)
    }
}
