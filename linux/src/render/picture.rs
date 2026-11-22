//! Draws the snow lying in a wallpaper into a surface of the whole display, as the Mac's PictureRenderer does: its cover
//! on the surfaces that hold it and the caps along its ridges. It draws only when asked, when that snow has changed;
//! what it draws the compositor then shows as it is, frame after frame, for nothing.

use std::collections::HashMap;

use glow::HasContext;
use snowhush_core::snowman::writing::{Image, Shown};
use snowhush_core::snowman::{Drawing, Picture};
use snowhush_core::wallpaper::drawing::{self, Cap};
use snowhush_core::wallpaper::{GROUND, Scene};

use super::{Gpu, Program, Surface, bytes, program};

const COVER_VERTEX: &str = include_str!("../shaders/cover.vert");
const LIE_FRAGMENT: &str = include_str!("../shaders/lie.frag");
const SHOW_FRAGMENT: &str = include_str!("../shaders/show.frag");
const CAP_VERTEX: &str = include_str!("../shaders/cap.vert");
const CAP_FRAGMENT: &str = include_str!("../shaders/cap.frag");
const PICTURE: &str = include_str!("../shaders/picture.glsl");
const COPY_FRAGMENT: &str = include_str!("../shaders/copy.frag");
const SNOWMAN_VERTEX: &str = include_str!("../shaders/snowman.vert");
const SNOWMAN_FRAGMENT: &str = include_str!("../shaders/snowman.frag");
const WRITING_VERTEX: &str = include_str!("../shaders/writing.vert");
const WRITING_FRAGMENT: &str = include_str!("../shaders/writing.frag");
/// The snowman's body, throwing arm and hat, drawn once on the Mac 220 points tall at two pixels a point.
const ART: [&[u8]; 3] = [
    include_bytes!("../../../windows/assets/snowman/body.png"),
    include_bytes!("../../../windows/assets/snowman/arm.png"),
    include_bytes!("../../../windows/assets/snowman/hat.png"),
];
const SHARED: [&str; 7] = ["viewSize", "scale", "level", "range", "thickest", "veil", "haze"];

pub(super) struct Programs {
    lie: Program,
    show: Program,
    cap: Program,
    cap_array: glow::VertexArray,
    noise: glow::Texture,
    copy: Program,
    snowman: Program,
    /// The snowman's pictures, made the first time one is drawn.
    art: Option<[glow::Texture; 3]>,
    writing: Program,
    fonts: crate::fonts::Fonts,
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
    /// The snow as last drawn, the surface's size, for what moves over it to be drawn on; and whether it holds any.
    cache: Option<(glow::Texture, glow::Framebuffer, (i32, i32))>,
    cached: bool,
    /// The words shown last, as the graphics card has them, by what they say at what scale.
    written: HashMap<String, (glow::Texture, Image)>,
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
                let noise = texture(gl, glow::R8, glow::RED, 256, 256, &drawing::noise(), false, glow::REPEAT)?;
                let copy = program(gl, COVER_VERTEX, COPY_FRAGMENT, &["viewSize", "drawn"])?;
                let snowman = program(
                    gl,
                    SNOWMAN_VERTEX,
                    SNOWMAN_FRAGMENT,
                    &["viewSize", "rect", "matrix", "offset", "picture", "alpha", "cut", "near", "nearness", "tint", "art", "relief"],
                )?;
                let writing = program(gl, WRITING_VERTEX, WRITING_FRAGMENT, &["viewSize", "rect", "words"])?;
                Programs { lie, show, cap, cap_array, noise, copy, snowman, art: None, writing, fonts: Default::default() }
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
            let (columns, caps) = drawing::caps(&scene.ridges, column_width);
            // Each column twice, once for each corner of the strip.
            let values: Vec<f32> = columns.iter().flat_map(|column| column.iter().chain(column).copied()).collect();
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
                cache: None,
                cached: false,
                written: HashMap::new(),
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
            for (texture, _) in painting.written.into_values() {
                gl.delete_texture(texture);
            }
            for (texture, framebuffer, _) in painting.lie.into_iter().chain(painting.cache) {
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(texture);
            }
            gl.delete_buffer(painting.columns);
        }
    }

    /// Draws the snow lying in the picture for `surface`, the whole display `view` points across, and shows it with the
    /// snowmen standing in it and the words over them.
    pub fn draw_picture(
        &mut self,
        surface: &Surface,
        painting: &mut Painting,
        lying: &Lying,
        view: [f32; 2],
        snowmen: &[Drawing],
        words: &[Shown],
    ) -> Result<(), String> {
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
            let size = (surface.width.max(1), surface.height.max(1));
            if painting.cache.is_none_or(|(_, _, made)| made != size) {
                if let Some((texture, framebuffer, _)) = painting.cache.take() {
                    gl.delete_framebuffer(framebuffer);
                    gl.delete_texture(texture);
                }
                let texture = texture(gl, glow::RGBA8, glow::RGBA, size.0 as usize, size.1 as usize, &[], false, glow::CLAMP_TO_EDGE)?;
                let framebuffer = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
                painting.cache = Some((texture, framebuffer, size));
            }
            let (_, cache, _) = painting.cache.expect("made");
            let (lie, framebuffer, _) = painting.lie.expect("made");
            let level = lying.level;
            let (amount, thickest) = (drawing::amount(level), drawing::thickest(level));
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
            // The snow as it is seen, and the caps along the ridges, kept for the snowmen to be drawn over.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(cache));
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
                    let thickness = drawing::cap_thickness(level, one.near);
                    gl.uniform_1_f32(cap.location("left"), one.left);
                    gl.uniform_1_f32(cap.location("thickness"), thickness);
                    gl.uniform_1_f32(cap.location("near"), one.near);
                    gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 16, (one.first * 2 * 16) as i32);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, (one.count * 2) as i32);
                }
            }
            gl.bind_vertex_array(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.active_texture(glow::TEXTURE0);
        }
        painting.cached = true;
        self.present_picture(surface, painting, view, snowmen, words)
    }

    /// Forgets the snow drawn last, as when it is gone: only the snowmen show after.
    pub fn uncache(&self, painting: &mut Painting) {
        painting.cached = false;
    }

    /// Shows the snow as last drawn with the snowmen over it, as they are now, each hidden behind what in the picture is
    /// nearer than he is, and the words over everything.
    pub fn present_picture(
        &mut self,
        surface: &Surface,
        painting: &mut Painting,
        view: [f32; 2],
        snowmen: &[Drawing],
        words: &[Shown],
    ) -> Result<(), String> {
        self.egl.make_current(self.display, Some(surface.surface), Some(surface.surface), Some(self.context)).map_err(|error| error.to_string())?;
        self.picture_programs()?;
        self.write(painting, surface.width as f32 / view[0].max(1.0), words)?;
        if !snowmen.is_empty() && self.picture.as_ref().is_some_and(|programs| programs.art.is_none()) {
            let art = [self.picture(ART[0], true), self.picture(ART[1], true), self.picture(ART[2], true)];
            if let [Some(body), Some(arm), Some(hat)] = art {
                self.picture.as_mut().expect("made").art = Some([body.0, arm.0, hat.0]);
            }
        }
        let programs = self.picture.as_ref().expect("made");
        let gl = &self.gl;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, surface.width, surface.height);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_vertex_array(Some(self.empty_array));
            if painting.cached
                && let Some((drawn, _, _)) = painting.cache
            {
                gl.disable(glow::BLEND);
                let copy = &programs.copy;
                gl.use_program(Some(copy.program));
                gl.uniform_2_f32_slice(copy.location("viewSize"), &view);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(drawn));
                gl.uniform_1_i32(copy.location("drawn"), 0);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            if let Some(art) = programs.art {
                gl.enable(glow::BLEND);
                gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                let snowman = &programs.snowman;
                gl.use_program(Some(snowman.program));
                gl.uniform_2_f32_slice(snowman.location("viewSize"), &view);
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(painting.relief));
                gl.uniform_1_i32(snowman.location("relief"), 1);
                gl.uniform_1_i32(snowman.location("art"), 0);
                gl.active_texture(glow::TEXTURE0);
                for figure in snowmen {
                    gl.uniform_1_f32(snowman.location("near"), figure.near);
                    gl.uniform_1_f32(snowman.location("nearness"), figure.nearness);
                    gl.uniform_3_f32_slice(snowman.location("tint"), &figure.tint);
                    for part in &figure.parts {
                        let texture = match part.picture {
                            Picture::Body => art[0],
                            Picture::Arm => art[1],
                            Picture::Hat => art[2],
                        };
                        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                        gl.uniform_4_f32_slice(snowman.location("rect"), &part.rect);
                        gl.uniform_4_f32_slice(snowman.location("matrix"), &part.matrix);
                        gl.uniform_2_f32_slice(snowman.location("offset"), &part.offset);
                        gl.uniform_4_f32_slice(snowman.location("picture"), &part.whole);
                        gl.uniform_1_f32(snowman.location("alpha"), part.alpha);
                        gl.uniform_4_f32_slice(snowman.location("cut"), &part.cut);
                        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                    }
                }
            }
            if !words.is_empty() {
                gl.enable(glow::BLEND);
                gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                let writing = &programs.writing;
                gl.use_program(Some(writing.program));
                gl.uniform_2_f32_slice(writing.location("viewSize"), &view);
                gl.uniform_1_i32(writing.location("words"), 0);
                gl.active_texture(glow::TEXTURE0);
                let scale = surface.width as f32 / view[0].max(1.0);
                for shown in words {
                    let Some((texture, image)) = painting.written.get(&written_key(&shown.writing.key, scale)) else { continue };
                    gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                    gl.uniform_4_f32_slice(writing.location("rect"), &image.rect(shown.at, shown.hang));
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            gl.bind_vertex_array(None);
        }
        self.egl.swap_buffers(self.display, surface.surface).map_err(|error| error.to_string())
    }

    /// Makes sure the graphics card has each of `words` as it looks at `scale`, and lets go of what isn't shown now.
    fn write(&mut self, painting: &mut Painting, scale: f32, words: &[Shown]) -> Result<(), String> {
        let programs = self.picture.as_mut().expect("made");
        if words.is_empty() {
            programs.fonts.let_go();
        }
        let gl = &self.gl;
        let wanted: Vec<String> = words.iter().map(|shown| written_key(&shown.writing.key, scale)).collect();
        painting.written.retain(|key, (texture, _)| {
            let keep = wanted.contains(key);
            if !keep {
                unsafe { gl.delete_texture(*texture) };
            }
            keep
        });
        for (shown, key) in words.iter().zip(wanted) {
            if painting.written.contains_key(&key) {
                continue;
            }
            let mut image = shown.writing.image(&mut programs.fonts, scale);
            let made = unsafe { texture(gl, glow::RGBA8, glow::RGBA, image.width, image.height, &image.pixels, false, glow::CLAMP_TO_EDGE)? };
            image.pixels = Vec::new();
            painting.written.insert(key, (made, image));
        }
        Ok(())
    }
}

fn written_key(key: &str, scale: f32) -> String {
    format!("{key}@{scale}")
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
