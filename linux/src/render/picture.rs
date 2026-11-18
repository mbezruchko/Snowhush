//! Draws the snow lying in a wallpaper into a surface of the whole display, as the Mac's PictureRenderer does: its cover
//! on the surfaces that hold it and the caps along its ridges. It draws only when asked, when that snow has changed;
//! what it draws the compositor then shows as it is, frame after frame, for nothing.

use glow::HasContext;
use snowhush_core::wallpaper::drawing::{self, Cap};
use snowhush_core::wallpaper::{GROUND, Scene};

use super::{Gpu, Program, Surface, bytes, program};

const COVER_VERTEX: &str = include_str!("../shaders/cover.vert");
const LIE_FRAGMENT: &str = include_str!("../shaders/lie.frag");
const SHOW_FRAGMENT: &str = include_str!("../shaders/show.frag");
const CAP_VERTEX: &str = include_str!("../shaders/cap.vert");
const CAP_FRAGMENT: &str = include_str!("../shaders/cap.frag");
const PICTURE: &str = include_str!("../shaders/picture.glsl");
const SHARED: [&str; 7] = ["viewSize", "scale", "level", "range", "thickest", "veil", "haze"];

pub(super) struct Programs {
    lie: Program,
    show: Program,
    cap: Program,
    cap_array: glow::VertexArray,
    noise: glow::Texture,
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
                let noise = texture(gl, glow::R8, glow::RED, 256, 256, &drawing::noise(), false, glow::REPEAT)?;
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
                    let thickness = drawing::cap_thickness(level, one.near);
                    gl.uniform_1_f32(cap.location("left"), one.left);
                    gl.uniform_1_f32(cap.location("thickness"), thickness);
                    gl.uniform_1_f32(cap.location("near"), one.near);
                    gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 16, (one.first * 2 * 16) as i32);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, (one.count * 2) as i32);
                }
            }
            gl.bind_vertex_array(None);
            gl.active_texture(glow::TEXTURE0);
        }
        self.egl.swap_buffers(self.display, surface.surface).map_err(|error| error.to_string())
    }
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
