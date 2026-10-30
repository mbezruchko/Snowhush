//! Draws the snow with OpenGL ES 3 through EGL, the same passes as the Windows renderer: Halloween's pumpkins, the
//! piles, read from a height texture, the ghosts' signs, then every flake, grain, puff and ghost as an instanced quad.

use std::ffi::c_void;
use std::mem::size_of;

use glow::HasContext;
use khronos_egl as egl;
use wayland_client::Proxy;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_egl::WlEglSurface;

use snowhush_core::geometry::V2;
use snowhush_core::halloween::{self, PUMPKIN_ASPECT, PUMPKIN_CANDLE, SIGN_CAPTIONS};
use snowhush_core::simulation::{COLUMN_WIDTH, PumpkinSprite, SnowSimulation, Sprite};

const SPRITE_VERTEX: &str = include_str!("shaders/sprite.vert");
const SPRITE_FRAGMENT: &str = include_str!("shaders/sprite.frag");
const PILE_VERTEX: &str = include_str!("shaders/pile.vert");
const PILE_FRAGMENT: &str = include_str!("shaders/pile.frag");
const SIGN_VERTEX: &str = include_str!("shaders/sign.vert");
const SIGN_FRAGMENT: &str = include_str!("shaders/sign.frag");
const PUMPKIN_VERTEX: &str = include_str!("shaders/pumpkin.vert");
const PUMPKIN_FRAGMENT: &str = include_str!("shaders/pumpkin.frag");
/// The ghosts' signs, one picture per caption at two pixels per point, the stick's foot at the middle of the bottom edge.
const SIGNS: [&[u8]; SIGN_CAPTIONS.len()] = [
    include_bytes!("../../windows/assets/signs/sign-00.png"),
    include_bytes!("../../windows/assets/signs/sign-01.png"),
    include_bytes!("../../windows/assets/signs/sign-02.png"),
    include_bytes!("../../windows/assets/signs/sign-03.png"),
    include_bytes!("../../windows/assets/signs/sign-04.png"),
    include_bytes!("../../windows/assets/signs/sign-05.png"),
    include_bytes!("../../windows/assets/signs/sign-06.png"),
    include_bytes!("../../windows/assets/signs/sign-07.png"),
    include_bytes!("../../windows/assets/signs/sign-08.png"),
    include_bytes!("../../windows/assets/signs/sign-09.png"),
    include_bytes!("../../windows/assets/signs/sign-10.png"),
    include_bytes!("../../windows/assets/signs/sign-11.png"),
];
/// The jack-o'-lantern's shell and its carved face, apart so the candle flickers in the face alone, at two pixels per
/// unit of a drawing 100 units wide.
const PUMPKIN: [&[u8]; 2] = [include_bytes!("../../windows/assets/lanterns/shell.png"), include_bytes!("../../windows/assets/lanterns/face.png")];
/// How far the cursor's candle reaches, in points.
pub const LANTERN_REACH: f32 = 130.0;
/// The height texture's width; the pile shader wraps its index at the same width.
const ROW: usize = 4096;
const OCCLUDERS: usize = 16;

/// The light the snow is drawn in.
#[derive(Clone, Copy)]
pub struct Ambient {
    pub light: [f32; 3],
    pub shadow_tint: [f32; 4],
    /// How far into Halloween night it is: moonlit snow, a candle's glow along the drifts' edges and one at the cursor.
    pub night: f32,
    /// Where the cursor's candle is, in the simulation's points.
    pub lantern: V2,
}

struct Program {
    program: glow::Program,
    uniforms: Vec<(String, Option<glow::UniformLocation>)>,
}

impl Program {
    fn location(&self, name: &str) -> Option<&glow::UniformLocation> {
        self.uniforms.iter().find(|(n, _)| n == name).and_then(|(_, location)| location.as_ref())
    }
}

pub struct Gpu {
    egl: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    gl: glow::Context,
    sprite: Program,
    pile: Program,
    sign: Program,
    pumpkin: Program,
    sprite_array: glow::VertexArray,
    sprite_buffer: glow::Buffer,
    /// Holds no attributes: the corners of the piles, the signs and the pumpkins come from the vertex index alone.
    empty_array: glow::VertexArray,
    heights: glow::Texture,
    /// Rows the height texture has room for; it is only made again when that changes.
    height_rows: usize,
    sprites: Vec<Sprite>,
    columns: Vec<f32>,
    /// Each caption's sign, made the first time a ghost holds it up, and its size in points.
    signs: Vec<Option<(glow::Texture, [f32; 2])>>,
    /// The pumpkin's shell and face, made the first time one is drawn.
    pumpkin_pictures: [Option<glow::Texture>; 2],
    pub adapter: String,
}

/// One display's drawing surface.
pub struct Surface {
    window: WlEglSurface,
    surface: egl::Surface,
    pub width: i32,
    pub height: i32,
}

impl Gpu {
    /// `display` is the Wayland connection's `wl_display`.
    pub fn new(display: *mut c_void) -> Result<Gpu, String> {
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }.map_err(|error| format!("loading EGL: {error}"))?;
        let display = unsafe { egl.get_display(display) }.ok_or("no EGL display")?;
        egl.initialize(display).map_err(|error| format!("EGL initialize: {error}"))?;
        egl.bind_api(egl::OPENGL_ES_API).map_err(|error| format!("EGL bind: {error}"))?;
        let attributes = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = egl.choose_first_config(display, &attributes).map_err(|error| error.to_string())?.ok_or("no EGL config with alpha")?;
        let context = egl
            .create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE])
            .map_err(|error| format!("EGL context: {error}"))?;
        egl.make_current(display, None, None, Some(context)).map_err(|error| format!("EGL make current: {error}"))?;
        let gl = unsafe {
            glow::Context::from_loader_function(|name| egl.get_proc_address(name).map_or(std::ptr::null(), |function| function as *const c_void))
        };
        unsafe {
            let adapter = gl.get_parameter_string(glow::RENDERER);
            let sprite = program(
                &gl,
                SPRITE_VERTEX,
                SPRITE_FRAGMENT,
                &["viewSize", "scale", "time", "light", "shadowTint", "rim", "lantern", "pumpkins", "shake", "panel"],
            )?;
            let pile = program(
                &gl,
                PILE_VERTEX,
                PILE_FRAGMENT,
                &[
                    "viewSize",
                    "scale",
                    "time",
                    "columnWidth",
                    "columnCount",
                    "pileTop",
                    "occluderCount",
                    "origin",
                    "pileBottom",
                    "heightOffset",
                    "light",
                    "shadowTint",
                    "rim",
                    "lantern",
                    "pumpkins",
                    "shake",
                    "occluders",
                    "heights",
                    "panel",
                ],
            )?;
            let sign = program(
                &gl,
                SIGN_VERTEX,
                SIGN_FRAGMENT,
                &["viewSize", "shake", "anchor", "size", "angle", "time", "ground", "light", "lantern", "pumpkins", "panel", "picture"],
            )?;
            let pumpkin = program(
                &gl,
                PUMPKIN_VERTEX,
                PUMPKIN_FRAGMENT,
                &["viewSize", "shake", "anchor", "size", "wick", "seed", "time", "light", "night", "panel", "shell", "face"],
            )?;

            let sprite_array = gl.create_vertex_array()?;
            let sprite_buffer = gl.create_buffer()?;
            gl.bind_vertex_array(Some(sprite_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(sprite_buffer));
            let stride = size_of::<Sprite>() as i32;
            // Position, velocity, size, alpha, soft, tint amount, tint, lump, ghost, pose, mood: see `Sprite`.
            for (location, components, offset) in
                [(0, 2, 0), (1, 2, 8), (2, 1, 16), (3, 1, 20), (4, 1, 24), (5, 1, 28), (6, 3, 32), (7, 1, 44), (8, 4, 48), (9, 4, 64), (10, 2, 80)]
            {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(location, components, glow::FLOAT, false, stride, offset);
                gl.vertex_attrib_divisor(location, 1);
            }
            let empty_array = gl.create_vertex_array()?;
            gl.bind_vertex_array(None);

            let heights = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(heights));
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
            }

            Ok(Gpu {
                egl,
                display,
                config,
                context,
                gl,
                sprite,
                pile,
                sign,
                pumpkin,
                sprite_array,
                sprite_buffer,
                empty_array,
                heights,
                height_rows: 0,
                sprites: Vec::new(),
                columns: Vec::new(),
                signs: vec![None; SIGNS.len()],
                pumpkin_pictures: [None; 2],
                adapter,
            })
        }
    }

    pub fn surface(&self, surface: &WlSurface, width: i32, height: i32) -> Result<Surface, String> {
        let window = WlEglSurface::new(surface.id(), width.max(1), height.max(1)).map_err(|error| error.to_string())?;
        let surface = unsafe { self.egl.create_window_surface(self.display, self.config, window.ptr() as egl::NativeWindowType, None) }
            .map_err(|error| format!("EGL surface: {error}"))?;
        self.egl.make_current(self.display, Some(surface), Some(surface), Some(self.context)).map_err(|error| error.to_string())?;
        // Frames are paced by the snow loop; a swap must never wait on a display that is off.
        let _ = self.egl.swap_interval(self.display, 0);
        Ok(Surface { window, surface, width, height })
    }

    pub fn resize(&self, surface: &mut Surface, width: i32, height: i32) {
        surface.window.resize(width.max(1), height.max(1), 0, 0);
        surface.width = width;
        surface.height = height;
    }

    pub fn release(&self, surface: Surface) {
        let _ = self.egl.make_current(self.display, None, None, Some(self.context));
        let _ = self.egl.destroy_surface(self.display, surface.surface);
    }

    /// Shows nothing on the surface, as when its display is covered.
    pub fn clear(&self, surface: &Surface) -> Result<(), String> {
        self.egl.make_current(self.display, Some(surface.surface), Some(surface.surface), Some(self.context)).map_err(|error| error.to_string())?;
        unsafe {
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.egl.swap_buffers(self.display, surface.surface).map_err(|error| error.to_string())
    }

    /// `panel` is a rectangle the panel stands in front of, left, bottom, right, top in the simulation's
    /// coordinates: nothing of the snow shows inside it.
    pub fn draw(
        &mut self,
        surface: &Surface,
        simulation: &SnowSimulation,
        ambient: Ambient,
        panel: Option<[f32; 4]>,
        time: f32,
    ) -> Result<(), String> {
        self.egl.make_current(self.display, Some(surface.surface), Some(surface.surface), Some(self.context)).map_err(|error| error.to_string())?;
        let sprite_count = simulation.sprite_count();
        self.sprites.resize(sprite_count, Sprite::default());
        let sprites = simulation.write_sprites(&mut self.sprites);
        let columns = simulation.column_count();
        let rows = columns.div_ceil(ROW).max(1);
        self.columns.resize(rows * ROW, 0.0);
        let piles = simulation.write_layers(&mut self.columns[..columns]);
        let signs = simulation.signs();
        for sign in &signs {
            self.load_sign(sign.caption);
        }
        let pumpkins = simulation.pumpkins();
        if !pumpkins.is_empty() {
            self.load_pumpkin();
        }

        let gl = &self.gl;
        let view = [simulation.width(), simulation.height()];
        let (light, shadow_tint) = halloween::moonlit(ambient.light, ambient.shadow_tint, ambient.night);
        let light = [light[0], light[1], light[2], 1.0];
        let rim = [1.0, 0.56, 0.2, 0.3 * ambient.night];
        let lantern = [ambient.lantern.x, ambient.lantern.y, LANTERN_REACH, ambient.night];
        // The pumpkins' candles light the snow as the cursor's does, a little even by day; a missing one is dark.
        let mut candles = [[0.0; 4]; 3];
        for (candle, pumpkin) in candles.iter_mut().zip(&pumpkins) {
            let wick = wick(pumpkin);
            *candle = [wick.x, wick.y, pumpkin.width * 1.6, 0.4 + 0.6 * ambient.night];
        }
        let candles = candles.as_flattened();
        let shake = simulation.shake();
        let shake = [shake.x, shake.y];
        // An empty rectangle hides nothing.
        let panel = panel.unwrap_or([0.0; 4]);
        unsafe {
            gl.viewport(0, 0, surface.width, surface.height);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);

            // The pumpkins go down before the snow, so the drift they sit in covers their bottoms.
            if !pumpkins.is_empty()
                && let [Some(shell), Some(face)] = self.pumpkin_pictures
            {
                let pumpkin = &self.pumpkin;
                gl.use_program(Some(pumpkin.program));
                gl.bind_vertex_array(Some(self.empty_array));
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(face));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(shell));
                gl.uniform_2_f32_slice(pumpkin.location("viewSize"), &view);
                gl.uniform_2_f32_slice(pumpkin.location("shake"), &shake);
                gl.uniform_1_f32(pumpkin.location("time"), time);
                gl.uniform_4_f32_slice(pumpkin.location("light"), &light);
                gl.uniform_1_f32(pumpkin.location("night"), ambient.night);
                gl.uniform_4_f32_slice(pumpkin.location("panel"), &panel);
                gl.uniform_1_i32(pumpkin.location("shell"), 0);
                gl.uniform_1_i32(pumpkin.location("face"), 1);
                for sitting in &pumpkins {
                    let wick = wick(sitting);
                    gl.uniform_2_f32(pumpkin.location("anchor"), sitting.anchor.x, sitting.anchor.y);
                    gl.uniform_2_f32(pumpkin.location("size"), sitting.width, sitting.width * PUMPKIN_ASPECT);
                    gl.uniform_2_f32(pumpkin.location("wick"), wick.x, wick.y);
                    gl.uniform_1_f32(pumpkin.location("seed"), sitting.seed);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            if !piles.is_empty() {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.heights));
                let data = glow::PixelUnpackData::Slice(Some(bytes(&self.columns)));
                if rows == self.height_rows {
                    gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, ROW as i32, rows as i32, glow::RED, glow::FLOAT, data);
                } else {
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::R32F as i32, ROW as i32, rows as i32, 0, glow::RED, glow::FLOAT, data);
                    self.height_rows = rows;
                }
                let pile = &self.pile;
                gl.use_program(Some(pile.program));
                gl.bind_vertex_array(Some(self.empty_array));
                gl.uniform_2_f32_slice(pile.location("viewSize"), &view);
                gl.uniform_1_f32(pile.location("scale"), 1.0);
                gl.uniform_1_f32(pile.location("time"), time);
                gl.uniform_1_f32(pile.location("columnWidth"), COLUMN_WIDTH);
                gl.uniform_4_f32_slice(pile.location("light"), &light);
                gl.uniform_4_f32_slice(pile.location("shadowTint"), &shadow_tint);
                gl.uniform_4_f32_slice(pile.location("rim"), &rim);
                gl.uniform_4_f32_slice(pile.location("lantern"), &lantern);
                gl.uniform_4_f32_slice(pile.location("pumpkins"), candles);
                gl.uniform_2_f32_slice(pile.location("shake"), &shake);
                gl.uniform_1_i32(pile.location("heights"), 0);
                gl.uniform_4_f32_slice(pile.location("panel"), &panel);
                for layer in &piles {
                    let count = layer.occluders.len().min(OCCLUDERS);
                    let occluders: Vec<f32> = layer.occluders[..count].iter().flatten().copied().collect();
                    gl.uniform_1_i32(pile.location("columnCount"), layer.count as i32);
                    gl.uniform_1_f32(pile.location("pileTop"), layer.top + 2.0);
                    gl.uniform_1_f32(pile.location("pileBottom"), layer.bottom);
                    gl.uniform_2_f32(pile.location("origin"), layer.origin.x, layer.origin.y);
                    gl.uniform_1_i32(pile.location("heightOffset"), layer.offset as i32);
                    gl.uniform_1_i32(pile.location("occluderCount"), count as i32);
                    if count > 0 {
                        gl.uniform_4_f32_slice(pile.location("occluders"), &occluders);
                    }
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            if !signs.is_empty() {
                let sign = &self.sign;
                gl.use_program(Some(sign.program));
                gl.bind_vertex_array(Some(self.empty_array));
                gl.active_texture(glow::TEXTURE0);
                gl.uniform_2_f32_slice(sign.location("viewSize"), &view);
                gl.uniform_2_f32_slice(sign.location("shake"), &shake);
                gl.uniform_1_f32(sign.location("time"), time);
                gl.uniform_4_f32_slice(sign.location("light"), &light);
                gl.uniform_4_f32_slice(sign.location("lantern"), &lantern);
                gl.uniform_4_f32_slice(sign.location("pumpkins"), candles);
                gl.uniform_4_f32_slice(sign.location("panel"), &panel);
                gl.uniform_1_i32(sign.location("picture"), 0);
                for held in &signs {
                    let Some(&Some((texture, size))) = self.signs.get(held.caption) else { continue };
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    gl.uniform_2_f32(sign.location("anchor"), held.anchor.x, held.anchor.y);
                    gl.uniform_2_f32_slice(sign.location("size"), &size);
                    gl.uniform_1_f32(sign.location("angle"), held.angle);
                    gl.uniform_1_f32(sign.location("ground"), held.ground);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            if sprites > 0 {
                let sprite = &self.sprite;
                gl.use_program(Some(sprite.program));
                gl.bind_vertex_array(Some(self.sprite_array));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.sprite_buffer));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&self.sprites[..sprites]), glow::STREAM_DRAW);
                gl.uniform_2_f32_slice(sprite.location("viewSize"), &view);
                gl.uniform_1_f32(sprite.location("scale"), 1.0);
                gl.uniform_1_f32(sprite.location("time"), time);
                gl.uniform_4_f32_slice(sprite.location("light"), &light);
                gl.uniform_4_f32_slice(sprite.location("shadowTint"), &shadow_tint);
                gl.uniform_4_f32_slice(sprite.location("rim"), &rim);
                gl.uniform_4_f32_slice(sprite.location("lantern"), &lantern);
                gl.uniform_4_f32_slice(sprite.location("pumpkins"), candles);
                gl.uniform_2_f32_slice(sprite.location("shake"), &shake);
                gl.uniform_4_f32_slice(sprite.location("panel"), &panel);
                gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, sprites as i32);
            }
            gl.bind_vertex_array(None);
        }
        self.egl.swap_buffers(self.display, surface.surface).map_err(|error| error.to_string())
    }

    /// Makes a caption's sign the first time a ghost holds it up.
    fn load_sign(&mut self, caption: usize) {
        if self.signs.get(caption).is_none_or(Option::is_some) {
            return;
        }
        let Some((texture, [width, height])) = self.picture(SIGNS[caption], false) else { return };
        self.signs[caption] = Some((texture, [width as f32 / 2.0, height as f32 / 2.0]));
    }

    /// Makes the pumpkin's pictures the first time one is drawn. They are mipmapped: the pumpkins are shown at a
    /// fraction of their size, and would shimmer otherwise.
    fn load_pumpkin(&mut self) {
        for (part, png) in PUMPKIN.iter().enumerate() {
            if self.pumpkin_pictures[part].is_none() {
                self.pumpkin_pictures[part] = self.picture(png, true).map(|(texture, _)| texture);
            }
        }
    }

    /// A PNG as a texture, with its colours premultiplied as the blending wants, and its size in pixels.
    fn picture(&self, png: &[u8], mipmapped: bool) -> Option<(glow::Texture, [u32; 2])> {
        let picture = image::load_from_memory(png).ok()?.to_rgba8();
        let (width, height) = picture.dimensions();
        let mut pixels = picture.into_raw();
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha = pixel[3] as u32;
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
            }
        }
        let gl = &self.gl;
        unsafe {
            let texture = gl.create_texture().ok()?;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, if mipmapped { glow::LINEAR_MIPMAP_LINEAR } else { glow::LINEAR }),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
            }
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&pixels)),
            );
            if mipmapped {
                gl.generate_mipmap(glow::TEXTURE_2D);
            }
            Some((texture, [width, height]))
        }
    }
}

/// Where a pumpkin's candle burns, in the simulation's points.
fn wick(pumpkin: &PumpkinSprite) -> V2 {
    let [across, up] = PUMPKIN_CANDLE;
    pumpkin.anchor + V2::new((across - 0.5) * pumpkin.width, up * pumpkin.width * PUMPKIN_ASPECT)
}

/// Plain numbers as the bytes OpenGL takes.
fn bytes<T: Copy>(values: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values)) }
}

unsafe fn program(gl: &glow::Context, vertex: &str, fragment: &str, uniforms: &[&str]) -> Result<Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, source) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(format!("shader: {}", gl.get_shader_info_log(shader)));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(format!("program: {}", gl.get_program_info_log(program)));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let uniforms = uniforms.iter().map(|&name| (name.to_string(), gl.get_uniform_location(program, name))).collect();
        Ok(Program { program, uniforms })
    }
}
