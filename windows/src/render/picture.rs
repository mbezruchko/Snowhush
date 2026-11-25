//! Draws the snow lying in a wallpaper into a surface of the whole display, as the Mac's PictureRenderer does: its cover
//! on the surfaces that hold it and the caps along its ridges. It draws only when asked, when that snow has changed;
//! what it draws the compositor then shows as it is, frame after frame, for nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::size_of;

use snowhush_core::snowman::writing::{Image, Shown};
use snowhush_core::snowman::{Drawing, Picture};
use snowhush_core::wallpaper::drawing::{self, Cap};
use snowhush_core::wallpaper::{GROUND, Scene};
use windows::Win32::Graphics::Direct3D::{D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::DXGI_PRESENT;
use windows::core::{Error, PCSTR, Result, s};

use super::{Gpu, Surface, compile_from, dynamic_buffer, pixel_shader, vertex_shader};
use crate::{halloween, log};

const SHADERS: &str = include_str!("../picture.hlsl");
/// The snowman's body, throwing arm and hat, drawn once on the Mac 220 points tall at two pixels a point.
const ART: [&[u8]; 3] =
    [include_bytes!("../../assets/snowman/body.png"), include_bytes!("../../assets/snowman/arm.png"), include_bytes!("../../assets/snowman/hat.png")];

#[repr(C)]
#[derive(Clone, Copy)]
struct SnowmanUniforms {
    rect: [f32; 4],
    matrix: [f32; 4],
    offset: [f32; 2],
    alpha: f32,
    near: f32,
    picture: [f32; 4],
    cut: [f32; 4],
    tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PictureUniforms {
    view_size: [f32; 2],
    scale: f32,
    level: f32,
    range: [f32; 4],
    thickest: f32,
    veil: f32,
    unused: [f32; 2],
    haze: [f32; 4],
    dabs_area: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CapUniforms {
    left: f32,
    column_width: f32,
    thickness: f32,
    near: f32,
}

pub(super) struct Programs {
    cover_vertex: ID3D11VertexShader,
    lie_fragment: ID3D11PixelShader,
    show_fragment: ID3D11PixelShader,
    cap_vertex: ID3D11VertexShader,
    cap_fragment: ID3D11PixelShader,
    cap_layout: ID3D11InputLayout,
    constants: ID3D11Buffer,
    cap_constants: ID3D11Buffer,
    tiled: ID3D11SamplerState,
    noise: ID3D11ShaderResourceView,
    copy_fragment: ID3D11PixelShader,
    snowman_vertex: ID3D11VertexShader,
    snowman_fragment: ID3D11PixelShader,
    snowman_constants: ID3D11Buffer,
    art: [ID3D11ShaderResourceView; 3],
    writing_vertex: ID3D11VertexShader,
    writing_fragment: ID3D11PixelShader,
    /// What sets the words, made the first time there are any.
    fonts: RefCell<Option<crate::fonts::Fonts>>,
}

/// What was found in one wallpaper, as the graphics card has it.
pub struct Painting {
    relief: ID3D11ShaderResourceView,
    light: ID3D11ShaderResourceView,
    dabs: (ID3D11Texture2D, ID3D11ShaderResourceView),
    specks: (ID3D11Texture2D, ID3D11ShaderResourceView),
    dabs_size: (usize, usize),
    dabs_version: Option<u64>,
    /// Where the first pass leaves how the snow lies, half the surface's size, and the size it was made for.
    lie: Option<(ID3D11RenderTargetView, ID3D11ShaderResourceView, (u32, u32))>,
    /// The snow as last drawn, the surface's size, for what moves over it to be drawn on; and whether it holds any.
    cache: Option<(ID3D11RenderTargetView, ID3D11ShaderResourceView, (u32, u32))>,
    cached: bool,
    /// The words shown last, as the graphics card has them, by what they say at what scale.
    written: HashMap<String, (ID3D11ShaderResourceView, Image)>,
    /// Each ridge's columns, each twice, one for each corner of its strip.
    columns: Option<ID3D11Buffer>,
    caps: Vec<Cap>,
    column_width: f32,
    range: [f32; 4],
    haze: [f32; 4],
}

/// How the snow lies in the picture now: `level` fallen everywhere, `veil` over the farthest things, the dabs and the
/// frost of the flakes seen landing over `dabs_area`, left, top, width and height in DIPs from the display's top left.
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
    fn picture_programs(&self) -> Option<&Programs> {
        self.picture.get_or_init(|| self.make_picture_programs().inspect_err(|error| log!("wallpaper shaders failed: {error}")).ok()).as_ref()
    }

    fn make_picture_programs(&self) -> Result<Programs> {
        let compile = |entry: PCSTR, target: PCSTR| compile_from(SHADERS, s!("picture.hlsl"), entry, target);
        let device = &self.device;
        let cap_code = compile(s!("capVertex"), s!("vs_4_0"))?;
        let element = D3D11_INPUT_ELEMENT_DESC {
            SemanticName: s!("COLUMN"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        };
        let mut cap_layout = None;
        let mut tiled = None;
        unsafe {
            device.CreateInputLayout(&[element], &cap_code, Some(&mut cap_layout))?;
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            device.CreateSamplerState(&desc, Some(&mut tiled))?;
        }
        Ok(Programs {
            cover_vertex: vertex_shader(device, &compile(s!("coverVertex"), s!("vs_4_0"))?)?,
            lie_fragment: pixel_shader(device, &compile(s!("lieFragment"), s!("ps_4_0"))?)?,
            show_fragment: pixel_shader(device, &compile(s!("showFragment"), s!("ps_4_0"))?)?,
            cap_vertex: vertex_shader(device, &cap_code)?,
            cap_fragment: pixel_shader(device, &compile(s!("capFragment"), s!("ps_4_0"))?)?,
            cap_layout: cap_layout.ok_or_else(Error::empty)?,
            constants: dynamic_buffer(device, size_of::<PictureUniforms>(), D3D11_BIND_CONSTANT_BUFFER)?,
            cap_constants: dynamic_buffer(device, size_of::<CapUniforms>(), D3D11_BIND_CONSTANT_BUFFER)?,
            tiled: tiled.ok_or_else(Error::empty)?,
            noise: self.texture(DXGI_FORMAT_R8_UNORM, 256, 256, 1, &drawing::noise(), false)?.1,
            copy_fragment: pixel_shader(device, &compile(s!("copyFragment"), s!("ps_4_0"))?)?,
            snowman_vertex: vertex_shader(device, &compile(s!("snowmanVertex"), s!("vs_4_0"))?)?,
            snowman_fragment: pixel_shader(device, &compile(s!("snowmanFragment"), s!("ps_4_0"))?)?,
            snowman_constants: dynamic_buffer(device, size_of::<SnowmanUniforms>(), D3D11_BIND_CONSTANT_BUFFER)?,
            art: [self.art(ART[0])?, self.art(ART[1])?, self.art(ART[2])?],
            writing_vertex: vertex_shader(device, &compile(s!("writingVertex"), s!("vs_4_0"))?)?,
            writing_fragment: pixel_shader(device, &compile(s!("writingFragment"), s!("ps_4_0"))?)?,
            fonts: RefCell::new(None),
        })
    }

    fn art(&self, png: &[u8]) -> Result<ID3D11ShaderResourceView> {
        let picture = halloween::decode(png)?;
        Ok(self.texture(DXGI_FORMAT_R8G8B8A8_UNORM, picture.width as usize, picture.height as usize, 4, &picture.pixels, true)?.1)
    }

    /// A texture both drawn into and read, `size` across, with its target and view.
    fn target(&self, size: (u32, u32)) -> Result<(ID3D11RenderTargetView, ID3D11ShaderResourceView, (u32, u32))> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: size.0,
            Height: size.1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            ..Default::default()
        };
        let (mut texture, mut target, mut view) = (None, None, None);
        unsafe {
            self.device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.ok_or_else(Error::empty)?;
            self.device.CreateRenderTargetView(&texture, None, Some(&mut target))?;
            self.device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
        }
        Ok((target.ok_or_else(Error::empty)?, view.ok_or_else(Error::empty)?, size))
    }

    /// A texture of `pixels`, top row first, `bytes` each, and a view of it; with its smaller copies, `mipmapped`.
    fn texture(
        &self,
        format: DXGI_FORMAT,
        width: usize,
        height: usize,
        bytes: usize,
        pixels: &[u8],
        mipmapped: bool,
    ) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
        let (width, height) = (width.max(1) as u32, height.max(1) as u32);
        let levels = if mipmapped { 32 - width.max(height).leading_zeros() } else { 1 };
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: levels,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | if mipmapped { D3D11_BIND_RENDER_TARGET.0 } else { 0 }) as u32,
            CPUAccessFlags: 0,
            MiscFlags: if mipmapped { D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32 } else { 0 },
        };
        let mut texture = None;
        let mut view = None;
        unsafe {
            self.device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.ok_or_else(Error::empty)?;
            self.device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
            let view = view.ok_or_else(Error::empty)?;
            if pixels.len() >= (width * height) as usize * bytes {
                self.context.UpdateSubresource(&texture, 0, None, pixels.as_ptr().cast(), width * bytes as u32, 0);
            }
            if mipmapped {
                self.context.GenerateMips(&view);
            }
            Ok((texture, view))
        }
    }

    /// Takes in what was found in a wallpaper, with the simulation's ridges `column_width` apart.
    pub fn paint(&self, scene: &Scene, column_width: f32) -> Result<Painting> {
        self.picture_programs().ok_or_else(Error::empty)?;
        let relief = &scene.relief;
        let light = &scene.light;
        let (columns, caps) = drawing::caps(&scene.ridges, column_width);
        let doubled: Vec<[f32; 4]> = columns.iter().flat_map(|column| [*column, *column]).collect();
        let columns = if doubled.is_empty() {
            None
        } else {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: (doubled.len() * 16) as u32,
                Usage: D3D11_USAGE_IMMUTABLE,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                ..Default::default()
            };
            let data = D3D11_SUBRESOURCE_DATA { pSysMem: doubled.as_ptr().cast(), ..Default::default() };
            let mut buffer = None;
            unsafe { self.device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))? };
            buffer
        };
        let haze = light.haze;
        Ok(Painting {
            relief: self.texture(DXGI_FORMAT_R8G8B8A8_UNORM, relief.width, relief.height, 4, &relief.packed(), true)?.1,
            light: self.texture(DXGI_FORMAT_R8G8B8A8_UNORM, light.width, light.height, 4, &light.pixels, false)?.1,
            dabs: self.texture(DXGI_FORMAT_R8_UNORM, 1, 1, 1, &[0], false)?,
            specks: self.texture(DXGI_FORMAT_R8_UNORM, 1, 1, 1, &[0], false)?,
            dabs_size: (1, 1),
            dabs_version: None,
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

impl Surface {
    /// Draws the snow lying in the picture, the whole display `view` DIPs across, and shows it with the snowmen standing
    /// in it and the words over them.
    pub fn draw_picture(
        &mut self,
        gpu: &Gpu,
        painting: &mut Painting,
        lying: &Lying,
        view: [f32; 2],
        snowmen: &[Drawing],
        words: &[Shown],
    ) -> Result<()> {
        let programs = gpu.picture_programs().ok_or_else(Error::empty)?;
        let context = &gpu.context;
        if painting.dabs_version != Some(lying.dabs_version) && lying.dabs_size.0 > 0 && lying.dabs.len() >= lying.dabs_size.0 * lying.dabs_size.1 {
            let (width, height) = lying.dabs_size;
            if painting.dabs_size != lying.dabs_size {
                painting.dabs = gpu.texture(DXGI_FORMAT_R8_UNORM, width, height, 1, lying.dabs, false)?;
                painting.specks = gpu.texture(DXGI_FORMAT_R8_UNORM, width, height, 1, lying.specks, false)?;
                painting.dabs_size = lying.dabs_size;
            } else {
                unsafe {
                    context.UpdateSubresource(&painting.dabs.0, 0, None, lying.dabs.as_ptr().cast(), width as u32, 0);
                    context.UpdateSubresource(&painting.specks.0, 0, None, lying.specks.as_ptr().cast(), width as u32, 0);
                }
            }
            painting.dabs_version = Some(lying.dabs_version);
        }
        let half = ((self.width / 2).max(1), (self.height / 2).max(1));
        if painting.lie.as_ref().is_none_or(|(_, _, size)| *size != half) {
            painting.lie = Some(gpu.target(half)?);
        }
        let whole = (self.width.max(1), self.height.max(1));
        if painting.cache.as_ref().is_none_or(|(_, _, size)| *size != whole) {
            painting.cache = Some(gpu.target(whole)?);
        }
        let (lie_target, lie_view, _) = painting.lie.clone().ok_or_else(Error::empty)?;
        let (cache_target, _, _) = painting.cache.clone().ok_or_else(Error::empty)?;
        let level = lying.level;
        let uniforms = PictureUniforms {
            view_size: view,
            scale: self.width as f32 / view[0].max(1.0),
            level: drawing::amount(level),
            range: painting.range,
            thickest: drawing::thickest(level),
            veil: lying.veil,
            unused: [0.0; 2],
            haze: painting.haze,
            dabs_area: lying.dabs_area,
        };
        let viewport = |width: u32, height: u32| D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        unsafe {
            gpu.write(&programs.constants, &uniforms)?;
            let constants = [Some(programs.constants.clone()), Some(programs.cap_constants.clone())];
            context.VSSetConstantBuffers(0, Some(&constants));
            context.PSSetConstantBuffers(0, Some(&constants));
            context.PSSetSamplers(0, Some(&[Some(gpu.sampler.clone()), Some(programs.tiled.clone())]));
            context.RSSetState(&gpu.raster);
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            // How the snow lies, at half the size: nothing it is worked out from is finer.
            context.OMSetRenderTargets(Some(&[Some(lie_target.clone())]), None);
            context.ClearRenderTargetView(&lie_target, &[0.0; 4]);
            context.RSSetViewports(Some(&[viewport(half.0, half.1)]));
            context.OMSetBlendState(None, None, u32::MAX);
            context.VSSetShader(&programs.cover_vertex, None);
            context.PSSetShader(&programs.lie_fragment, None);
            context.PSSetShaderResources(
                0,
                Some(&[Some(painting.dabs.1.clone()), Some(painting.relief.clone()), Some(painting.specks.1.clone()), Some(programs.noise.clone())]),
            );
            context.Draw(3, 0);
            // The snow as it is seen, and the caps along the ridges, kept for the snowmen to be drawn over.
            context.OMSetRenderTargets(Some(&[Some(cache_target.clone())]), None);
            context.ClearRenderTargetView(&cache_target, &[0.0; 4]);
            context.RSSetViewports(Some(&[viewport(self.width, self.height)]));
            context.OMSetBlendState(&gpu.blend, None, u32::MAX);
            context.PSSetShader(&programs.show_fragment, None);
            context.PSSetShaderResources(0, Some(&[Some(lie_view), Some(painting.relief.clone()), Some(painting.light.clone()), None]));
            context.Draw(3, 0);
            if let Some(columns) = &painting.columns {
                context.IASetInputLayout(&programs.cap_layout);
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
                context.VSSetShader(&programs.cap_vertex, None);
                context.PSSetShader(&programs.cap_fragment, None);
                let stride = 16u32;
                for cap in &painting.caps {
                    let constants = CapUniforms {
                        left: cap.left,
                        column_width: painting.column_width,
                        thickness: drawing::cap_thickness(level, cap.near),
                        near: cap.near,
                    };
                    gpu.write(&programs.cap_constants, &constants)?;
                    let offset = (cap.first * 2 * 16) as u32;
                    context.IASetVertexBuffers(0, 1, Some(&Some(columns.clone())), Some(&stride), Some(&offset));
                    context.Draw((cap.count * 2) as u32, 0);
                }
            }
            context.PSSetShaderResources(0, Some(&[None, None, None, None]));
        }
        painting.cached = true;
        self.present_picture(gpu, painting, view, snowmen, words)
    }

    /// Shows the snow as last drawn with the snowmen over it, as they are now, each hidden behind what in the picture is
    /// nearer than he is, and the words over everything.
    pub fn present_picture(&mut self, gpu: &Gpu, painting: &mut Painting, view: [f32; 2], snowmen: &[Drawing], words: &[Shown]) -> Result<()> {
        let programs = gpu.picture_programs().ok_or_else(Error::empty)?;
        let scale = self.width as f32 / view[0].max(1.0);
        gpu.write_words(programs, painting, scale, words);
        let context = &gpu.context;
        let uniforms = PictureUniforms {
            view_size: view,
            scale: self.width as f32 / view[0].max(1.0),
            level: 0.0,
            range: painting.range,
            thickest: 0.0,
            veil: 0.0,
            unused: [0.0; 2],
            haze: painting.haze,
            dabs_area: [0.0; 4],
        };
        unsafe {
            gpu.write(&programs.constants, &uniforms)?;
            let constants = [Some(programs.constants.clone()), Some(programs.cap_constants.clone()), Some(programs.snowman_constants.clone())];
            context.VSSetConstantBuffers(0, Some(&constants));
            context.PSSetConstantBuffers(0, Some(&constants));
            context.PSSetSamplers(0, Some(&[Some(gpu.sampler.clone())]));
            context.RSSetState(&gpu.raster);
            context.IASetInputLayout(None);
            context.OMSetRenderTargets(Some(&[Some(self.target_view.clone())]), None);
            context.ClearRenderTargetView(&self.target_view, &[0.0; 4]);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            if painting.cached
                && let Some((_, drawn, _)) = &painting.cache
            {
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.OMSetBlendState(None, None, u32::MAX);
                context.VSSetShader(&programs.cover_vertex, None);
                context.PSSetShader(&programs.copy_fragment, None);
                context.PSSetShaderResources(0, Some(&[Some(drawn.clone())]));
                context.Draw(3, 0);
            }
            if !snowmen.is_empty() {
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
                context.OMSetBlendState(&gpu.blend, None, u32::MAX);
                context.VSSetShader(&programs.snowman_vertex, None);
                context.PSSetShader(&programs.snowman_fragment, None);
                context.PSSetShaderResources(1, Some(&[Some(painting.relief.clone())]));
                for figure in snowmen {
                    for part in &figure.parts {
                        let art = match part.picture {
                            Picture::Body => &programs.art[0],
                            Picture::Arm => &programs.art[1],
                            Picture::Hat => &programs.art[2],
                        };
                        let constants = SnowmanUniforms {
                            rect: part.rect,
                            matrix: part.matrix,
                            offset: part.offset,
                            alpha: part.alpha,
                            near: figure.near,
                            picture: part.whole,
                            cut: part.cut,
                            tint: [figure.tint[0], figure.tint[1], figure.tint[2], figure.nearness],
                        };
                        gpu.write(&programs.snowman_constants, &constants)?;
                        context.PSSetShaderResources(0, Some(&[Some(art.clone())]));
                        context.Draw(4, 0);
                    }
                }
            }
            if !words.is_empty() {
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
                context.OMSetBlendState(&gpu.blend, None, u32::MAX);
                context.VSSetShader(&programs.writing_vertex, None);
                context.PSSetShader(&programs.writing_fragment, None);
                for shown in words {
                    let Some((texture, image)) = painting.written.get(&written_key(&shown.writing.key, scale)) else { continue };
                    let constants = SnowmanUniforms {
                        rect: image.rect(shown.at, shown.hang),
                        matrix: [0.0; 4],
                        offset: [0.0; 2],
                        alpha: 1.0,
                        near: 0.0,
                        picture: [0.0; 4],
                        cut: [0.0; 4],
                        tint: [0.0; 4],
                    };
                    gpu.write(&programs.snowman_constants, &constants)?;
                    context.PSSetShaderResources(0, Some(&[Some(texture.clone())]));
                    context.Draw(4, 0);
                }
            }
            context.PSSetShaderResources(0, Some(&[None, None]));
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
        }
    }
}

impl Gpu {
    /// Makes sure the graphics card has each of `words` as it looks at `scale`, and lets go of what isn't shown now.
    fn write_words(&self, programs: &Programs, painting: &mut Painting, scale: f32, words: &[Shown]) {
        let wanted: Vec<String> = words.iter().map(|shown| written_key(&shown.writing.key, scale)).collect();
        painting.written.retain(|key, _| wanted.contains(key));
        let mut fonts = programs.fonts.borrow_mut();
        for (shown, key) in words.iter().zip(wanted) {
            if painting.written.contains_key(&key) {
                continue;
            }
            if fonts.is_none() {
                *fonts = crate::fonts::Fonts::new().inspect_err(|error| log!("fight: no DirectWrite: {error}")).ok();
            }
            let Some(fonts) = fonts.as_mut() else { return };
            let mut image = shown.writing.image(fonts, scale);
            match self.texture(DXGI_FORMAT_R8G8B8A8_UNORM, image.width, image.height, 4, &image.pixels, false) {
                Ok((_, view)) => {
                    image.pixels = Vec::new();
                    painting.written.insert(key, (view, image));
                }
                Err(error) => log!("fight: words not taken in: {error}"),
            }
        }
    }
}

fn written_key(key: &str, scale: f32) -> String {
    format!("{key}@{scale}")
}

impl Painting {
    /// Forgets the snow drawn last, as when it is gone: only the snowmen show after.
    pub fn uncache(&mut self) {
        self.cached = false;
    }
}
