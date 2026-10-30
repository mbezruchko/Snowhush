use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::mem::{offset_of, size_of};

use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
    D3D_SRV_DIMENSION_BUFFER, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::{
    DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR, DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R32_FLOAT,
    DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter,
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
};
use windows::core::{Error, Interface, PCSTR, Result, s};
use windows_numerics::Matrix3x2;

use snowhush_core::geometry::V2;
use snowhush_core::halloween::{PUMPKIN_ASPECT, PUMPKIN_CANDLE};
use snowhush_core::simulation::{COLUMN_WIDTH, PumpkinSprite, SnowSimulation, Sprite};

use crate::halloween::{self, SIGN_SCALE};
use crate::log;

const SHADERS: &str = include_str!("shaders.hlsl");
/// How far the cursor's candlelight reaches, in points.
const LANTERN_REACH: f32 = 130.0;

// The sprites' input layout reads a ghost's parts at these offsets.
const _: () =
    assert!(offset_of!(Sprite, ghost) == 48 && offset_of!(Sprite, pose) == 64 && offset_of!(Sprite, mood) == 80 && size_of::<Sprite>() == 88);

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    view_size: [f32; 2],
    scale: f32,
    time: f32,
    column_width: f32,
    column_count: i32,
    pile_top: f32,
    occluder_count: i32,
    origin: [f32; 2],
    pile_bottom: f32,
    height_offset: i32,
    light: [f32; 4],
    shadow_tint: [f32; 4],
    rim: [f32; 4],
    /// The cursor's glow: where it is, how far it reaches and how bright it is.
    lantern: [f32; 4],
    /// How far everything is jolted: x and y.
    shake: [f32; 4],
    /// The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
    pumpkins: [[f32; 4]; 3],
    occluders: [[f32; 4]; 16],
}

/// Where a sign stands: its stick's foot, its size in points, how far it is turned and the drift's surface it comes up out of.
#[repr(C)]
#[derive(Clone, Copy)]
struct Placement {
    anchor: [f32; 2],
    size: [f32; 2],
    angle: f32,
    ground: f32,
    padding: [f32; 2],
}

/// Where a pumpkin sits: the middle of its bottom, its size in points, where its candle burns and what sets its
/// flicker apart from the others'.
#[repr(C)]
#[derive(Clone, Copy)]
struct PumpkinPlacement {
    anchor: [f32; 2],
    size: [f32; 2],
    candle: [f32; 2],
    seed: f32,
    padding: f32,
}

impl PumpkinPlacement {
    fn new(pumpkin: &PumpkinSprite) -> PumpkinPlacement {
        let size = [pumpkin.width, pumpkin.width * PUMPKIN_ASPECT];
        let candle = [pumpkin.anchor.x + (PUMPKIN_CANDLE[0] - 0.5) * size[0], pumpkin.anchor.y + PUMPKIN_CANDLE[1] * size[1]];
        PumpkinPlacement { anchor: [pumpkin.anchor.x, pumpkin.anchor.y], size, candle, seed: pumpkin.seed, padding: 0.0 }
    }

    /// Its candle's light on the snow: where it burns, how far it reaches and how bright it is, brighter at `night`.
    fn light(&self, night: f32) -> [f32; 4] {
        [self.candle[0], self.candle[1], self.size[0] * 1.6, 0.4 + 0.6 * night]
    }
}

/// The light the snow is drawn in.
#[derive(Clone, Copy)]
pub struct Ambient {
    pub light: [f32; 3],
    pub shadow_tint: [f32; 4],
    /// The colour the snow's surface glows, and how much.
    pub rim: [f32; 4],
    /// The cursor, in points, glowing like a candle in a jack-o'-lantern, and how brightly.
    pub lantern: (V2, f32),
}

/// A sign's picture on the graphics card, and its size in points.
type Picture = (ID3D11ShaderResourceView, [f32; 2]);

pub struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    factory: IDXGIFactory2,
    composition: IDCompositionDevice,
    sprite_vertex: ID3D11VertexShader,
    sprite_fragment: ID3D11PixelShader,
    sprite_layout: ID3D11InputLayout,
    pile_vertex: ID3D11VertexShader,
    pile_fragment: ID3D11PixelShader,
    sign_vertex: ID3D11VertexShader,
    sign_fragment: ID3D11PixelShader,
    pumpkin_vertex: ID3D11VertexShader,
    pumpkin_fragment: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    /// Clear past the picture's edges, for the pumpkins' glow around them.
    bordered: ID3D11SamplerState,
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    uniforms: ID3D11Buffer,
    placement: ID3D11Buffer,
    pumpkin: ID3D11Buffer,
    /// Each caption's sign, made the first time a ghost holds it up; none if it couldn't be.
    signs: RefCell<HashMap<usize, Option<Picture>>>,
    /// The pumpkin's shell and its face, made the first time one is drawn; none if they couldn't be.
    pumpkin_pictures: OnceCell<Option<[ID3D11ShaderResourceView; 2]>>,
    pub adapter: String,
}

impl Gpu {
    pub fn new() -> Result<Gpu> {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device: ID3D11Device = device.ok_or_else(Error::empty)?;
            let context = context.ok_or_else(Error::empty)?;
            let dxgi: IDXGIDevice = device.cast()?;
            let adapter: IDXGIAdapter = dxgi.GetAdapter()?;
            let description = adapter.GetDesc()?;
            let name_length = description.Description.iter().position(|&c| c == 0).unwrap_or(description.Description.len());
            let factory: IDXGIFactory2 = adapter.GetParent()?;
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;

            let sprite_code = compile(s!("spriteVertex"), s!("vs_4_0"))?;
            let pile_code = compile(s!("pileVertex"), s!("vs_4_0"))?;
            let sprite_vertex = vertex_shader(&device, &sprite_code)?;
            let pile_vertex = vertex_shader(&device, &pile_code)?;
            let sprite_fragment = pixel_shader(&device, &compile(s!("spriteFragment"), s!("ps_4_0"))?)?;
            let pile_fragment = pixel_shader(&device, &compile(s!("pileFragment"), s!("ps_4_0"))?)?;
            let sign_vertex = vertex_shader(&device, &compile(s!("signVertex"), s!("vs_4_0"))?)?;
            let sign_fragment = pixel_shader(&device, &compile(s!("signFragment"), s!("ps_4_0"))?)?;
            let pumpkin_vertex = vertex_shader(&device, &compile(s!("pumpkinVertex"), s!("vs_4_0"))?)?;
            let pumpkin_fragment = pixel_shader(&device, &compile(s!("pumpkinFragment"), s!("ps_4_0"))?)?;

            let element = |name: PCSTR, format: DXGI_FORMAT, offset: u32| D3D11_INPUT_ELEMENT_DESC {
                SemanticName: name,
                SemanticIndex: 0,
                Format: format,
                InputSlot: 0,
                AlignedByteOffset: offset,
                InputSlotClass: D3D11_INPUT_PER_INSTANCE_DATA,
                InstanceDataStepRate: 1,
            };
            let elements = [
                element(s!("POSITION"), DXGI_FORMAT_R32G32_FLOAT, 0),
                element(s!("VELOCITY"), DXGI_FORMAT_R32G32_FLOAT, 8),
                element(s!("SIZE"), DXGI_FORMAT_R32_FLOAT, 16),
                element(s!("ALPHA"), DXGI_FORMAT_R32_FLOAT, 20),
                element(s!("SOFT"), DXGI_FORMAT_R32_FLOAT, 24),
                element(s!("TINTAMOUNT"), DXGI_FORMAT_R32_FLOAT, 28),
                element(s!("TINTCOLOR"), DXGI_FORMAT_R32G32B32_FLOAT, 32),
                element(s!("LUMP"), DXGI_FORMAT_R32_FLOAT, 44),
                element(s!("GHOST"), DXGI_FORMAT_R32G32B32A32_FLOAT, 48),
                element(s!("POSE"), DXGI_FORMAT_R32G32B32A32_FLOAT, 64),
                element(s!("MOOD"), DXGI_FORMAT_R32G32_FLOAT, 80),
            ];
            let mut sprite_layout = None;
            device.CreateInputLayout(&elements, blob_bytes(&sprite_code), Some(&mut sprite_layout))?;

            let target = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let blend_desc = D3D11_BLEND_DESC { RenderTarget: [target; 8], ..Default::default() };
            let mut blend = None;
            device.CreateBlendState(&blend_desc, Some(&mut blend))?;

            let raster_desc =
                D3D11_RASTERIZER_DESC { FillMode: D3D11_FILL_SOLID, CullMode: D3D11_CULL_NONE, DepthClipEnable: true.into(), ..Default::default() };
            let mut raster = None;
            device.CreateRasterizerState(&raster_desc, Some(&mut raster))?;

            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let mut sampler = None;
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
            let bordered_desc = D3D11_SAMPLER_DESC {
                AddressU: D3D11_TEXTURE_ADDRESS_BORDER,
                AddressV: D3D11_TEXTURE_ADDRESS_BORDER,
                AddressW: D3D11_TEXTURE_ADDRESS_BORDER,
                BorderColor: [0.0; 4],
                ..sampler_desc
            };
            let mut bordered = None;
            device.CreateSamplerState(&bordered_desc, Some(&mut bordered))?;

            let uniforms = dynamic_buffer(&device, size_of::<Uniforms>(), D3D11_BIND_CONSTANT_BUFFER)?;
            let placement = dynamic_buffer(&device, size_of::<Placement>(), D3D11_BIND_CONSTANT_BUFFER)?;
            let pumpkin = dynamic_buffer(&device, size_of::<PumpkinPlacement>(), D3D11_BIND_CONSTANT_BUFFER)?;

            Ok(Gpu {
                device,
                context,
                factory,
                composition,
                sprite_vertex,
                sprite_fragment,
                sprite_layout: sprite_layout.ok_or_else(Error::empty)?,
                pile_vertex,
                pile_fragment,
                sign_vertex,
                sign_fragment,
                pumpkin_vertex,
                pumpkin_fragment,
                sampler: sampler.ok_or_else(Error::empty)?,
                bordered: bordered.ok_or_else(Error::empty)?,
                blend: blend.ok_or_else(Error::empty)?,
                raster: raster.ok_or_else(Error::empty)?,
                uniforms,
                placement,
                pumpkin,
                signs: RefCell::new(HashMap::new()),
                pumpkin_pictures: OnceCell::new(),
                adapter: String::from_utf16_lossy(&description.Description[..name_length]),
            })
        }
    }

    fn write<T: Copy>(&self, buffer: &ID3D11Buffer, value: &T) -> Result<()> {
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            mapped.pData.cast::<T>().write(*value);
            self.context.Unmap(buffer, 0);
        }
        Ok(())
    }

    fn sign(&self, caption: usize) -> Option<Picture> {
        let mut signs = self.signs.borrow_mut();
        let picture =
            signs.entry(caption).or_insert_with(|| self.make_sign(caption).inspect_err(|error| log!("sign {caption} failed: {error}")).ok());
        picture.clone()
    }

    fn make_sign(&self, caption: usize) -> Result<Picture> {
        let picture = halloween::sign_picture(caption)?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: picture.width,
            Height: picture.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let data = D3D11_SUBRESOURCE_DATA { pSysMem: picture.pixels.as_ptr().cast(), SysMemPitch: picture.width * 4, SysMemSlicePitch: 0 };
        let mut texture = None;
        let mut view = None;
        unsafe {
            self.device.CreateTexture2D(&desc, Some(&data), Some(&mut texture))?;
            self.device.CreateShaderResourceView(&texture.ok_or_else(Error::empty)?, None, Some(&mut view))?;
        }
        Ok((view.ok_or_else(Error::empty)?, [picture.width as f32 / SIGN_SCALE, picture.height as f32 / SIGN_SCALE]))
    }

    fn pumpkin_pictures(&self) -> Option<&[ID3D11ShaderResourceView; 2]> {
        self.pumpkin_pictures.get_or_init(|| self.make_pumpkin_pictures().inspect_err(|error| log!("pumpkins failed: {error}")).ok()).as_ref()
    }

    fn make_pumpkin_pictures(&self) -> Result<[ID3D11ShaderResourceView; 2]> {
        let [shell, face] = halloween::pumpkin_pictures()?;
        Ok([self.mipmapped(&shell)?, self.mipmapped(&face)?])
    }

    /// A picture with its smaller copies, made on the graphics card: the pumpkins are shown at a fraction of their
    /// pictures' size, and would shimmer without them.
    fn mipmapped(&self, picture: &halloween::Picture) -> Result<ID3D11ShaderResourceView> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: picture.width,
            Height: picture.height,
            MipLevels: 0,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let mut texture = None;
        let mut view = None;
        unsafe {
            self.device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.ok_or_else(Error::empty)?;
            self.device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
            let view = view.ok_or_else(Error::empty)?;
            self.context.UpdateSubresource(&texture, 0, None, picture.pixels.as_ptr().cast(), picture.width * 4, 0);
            self.context.GenerateMips(&view);
            Ok(view)
        }
    }
}

/// One display's drawing surface: a swap chain shown through DirectComposition, so it composes with per-pixel alpha.
pub struct Surface {
    swap_chain: IDXGISwapChain1,
    target_view: ID3D11RenderTargetView,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    width: u32,
    height: u32,
    sprites: Option<ID3D11Buffer>,
    sprite_capacity: usize,
    heights: Option<(ID3D11Buffer, ID3D11ShaderResourceView)>,
    height_capacity: usize,
}

impl Surface {
    /// Snow is soft enough that one pixel per DIP is indistinguishable from the panel's full resolution,
    /// so the swap chain is DIP-sized and composition scales it up.
    pub fn new(gpu: &Gpu, window: HWND, width: u32, height: u32, scale: (f32, f32)) -> Result<Surface> {
        unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swap_chain = gpu.factory.CreateSwapChainForComposition(&gpu.device, &desc, None)?;
            let back: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
            let mut target_view = None;
            gpu.device.CreateRenderTargetView(&back, None, Some(&mut target_view))?;

            let target = gpu.composition.CreateTargetForHwnd(window, true)?;
            let visual = gpu.composition.CreateVisual()?;
            visual.SetContent(&swap_chain)?;
            visual.SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)?;
            visual.SetTransform2(&Matrix3x2 { M11: scale.0, M12: 0.0, M21: 0.0, M22: scale.1, M31: 0.0, M32: 0.0 })?;
            target.SetRoot(&visual)?;
            gpu.composition.Commit()?;

            Ok(Surface {
                swap_chain,
                target_view: target_view.ok_or_else(Error::empty)?,
                _target: target,
                _visual: visual,
                width,
                height,
                sprites: None,
                sprite_capacity: 0,
                heights: None,
                height_capacity: 0,
            })
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, simulation: &SnowSimulation, ambient: Ambient, time: f32) -> Result<()> {
        self.reserve(gpu, simulation.sprite_count(), simulation.column_count())?;
        let (Some(sprite_buffer), Some((height_buffer, height_view))) = (&self.sprites, &self.heights) else {
            return Ok(());
        };
        let context = &gpu.context;
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(sprite_buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            let sprites = simulation.write_sprites(std::slice::from_raw_parts_mut(mapped.pData.cast::<Sprite>(), self.sprite_capacity));
            context.Unmap(sprite_buffer, 0);

            context.Map(height_buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            let piles = simulation.write_layers(std::slice::from_raw_parts_mut(mapped.pData.cast::<f32>(), self.height_capacity));
            context.Unmap(height_buffer, 0);
            let pumpkins: Vec<PumpkinPlacement> = simulation.pumpkins().iter().map(PumpkinPlacement::new).collect();

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
            context.RSSetState(&gpu.raster);
            context.OMSetBlendState(&gpu.blend, None, u32::MAX);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            let constants = [Some(gpu.uniforms.clone()), Some(gpu.placement.clone()), Some(gpu.pumpkin.clone())];
            context.VSSetConstantBuffers(0, Some(&constants));
            context.PSSetConstantBuffers(0, Some(&constants));

            let (cursor, glow) = ambient.lantern;
            let shake = simulation.shake();
            let mut uniforms = Uniforms {
                view_size: [simulation.width(), simulation.height()],
                scale: 1.0,
                time,
                column_width: COLUMN_WIDTH,
                column_count: 0,
                pile_top: 0.0,
                occluder_count: 0,
                origin: [0.0; 2],
                pile_bottom: 0.0,
                height_offset: 0,
                light: [ambient.light[0], ambient.light[1], ambient.light[2], 1.0],
                shadow_tint: ambient.shadow_tint,
                rim: ambient.rim,
                lantern: [cursor.x, cursor.y, LANTERN_REACH, glow],
                shake: [shake.x, shake.y, 0.0, 0.0],
                pumpkins: std::array::from_fn(|k| pumpkins.get(k).map_or([0.0; 4], |pumpkin| pumpkin.light(glow))),
                occluders: [[0.0; 4]; 16],
            };

            // The pumpkins go down before the snow, so the drift they sit in covers their bottoms.
            if !pumpkins.is_empty()
                && let Some([shell, face]) = gpu.pumpkin_pictures()
            {
                gpu.write(&gpu.uniforms, &uniforms)?;
                context.IASetInputLayout(None);
                context.VSSetShader(&gpu.pumpkin_vertex, None);
                context.PSSetShader(&gpu.pumpkin_fragment, None);
                context.PSSetSamplers(1, Some(&[Some(gpu.bordered.clone())]));
                context.PSSetShaderResources(2, Some(&[Some(shell.clone()), Some(face.clone())]));
                for pumpkin in &pumpkins {
                    gpu.write(&gpu.pumpkin, pumpkin)?;
                    context.Draw(4, 0);
                }
            }
            if !piles.is_empty() {
                context.IASetInputLayout(None);
                context.VSSetShader(&gpu.pile_vertex, None);
                context.PSSetShader(&gpu.pile_fragment, None);
                context.PSSetShaderResources(0, Some(&[Some(height_view.clone())]));
                for pile in &piles {
                    uniforms.column_count = pile.count as i32;
                    uniforms.pile_top = pile.top + 2.0;
                    uniforms.pile_bottom = pile.bottom;
                    uniforms.origin = [pile.origin.x, pile.origin.y];
                    uniforms.height_offset = pile.offset as i32;
                    uniforms.occluder_count = pile.occluders.len().min(16) as i32;
                    uniforms.occluders[..pile.occluders.len().min(16)].copy_from_slice(&pile.occluders[..pile.occluders.len().min(16)]);
                    gpu.write(&gpu.uniforms, &uniforms)?;
                    context.Draw(4, 0);
                }
            }
            uniforms.occluder_count = 0;
            let signs = simulation.signs();
            if !signs.is_empty() {
                gpu.write(&gpu.uniforms, &uniforms)?;
                context.IASetInputLayout(None);
                context.VSSetShader(&gpu.sign_vertex, None);
                context.PSSetShader(&gpu.sign_fragment, None);
                context.PSSetSamplers(0, Some(&[Some(gpu.sampler.clone())]));
                for sign in &signs {
                    let Some((picture, size)) = gpu.sign(sign.caption) else { continue };
                    let placement =
                        Placement { anchor: [sign.anchor.x, sign.anchor.y], size, angle: sign.angle, ground: sign.ground, padding: [0.0; 2] };
                    gpu.write(&gpu.placement, &placement)?;
                    context.PSSetShaderResources(1, Some(&[Some(picture)]));
                    context.Draw(4, 0);
                }
            }
            if sprites > 0 {
                gpu.write(&gpu.uniforms, &uniforms)?;
                let stride = size_of::<Sprite>() as u32;
                let offset = 0;
                context.IASetInputLayout(&gpu.sprite_layout);
                context.IASetVertexBuffers(0, 1, Some(&Some(sprite_buffer.clone())), Some(&stride), Some(&offset));
                context.VSSetShader(&gpu.sprite_vertex, None);
                context.PSSetShader(&gpu.sprite_fragment, None);
                context.DrawInstanced(4, sprites as u32, 0, 0);
            }
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
        }
    }

    fn reserve(&mut self, gpu: &Gpu, sprites: usize, columns: usize) -> Result<()> {
        if sprites > self.sprite_capacity || self.sprites.is_none() {
            self.sprite_capacity = 1024.max(sprites * 3 / 2);
            self.sprites = Some(dynamic_buffer(&gpu.device, self.sprite_capacity * size_of::<Sprite>(), D3D11_BIND_VERTEX_BUFFER)?);
        }
        if columns > self.height_capacity || self.heights.is_none() {
            self.height_capacity = 256.max(columns);
            let buffer = dynamic_buffer(&gpu.device, self.height_capacity * size_of::<f32>(), D3D11_BIND_SHADER_RESOURCE)?;
            let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R32_FLOAT,
                ViewDimension: D3D_SRV_DIMENSION_BUFFER,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Buffer: D3D11_BUFFER_SRV {
                        Anonymous1: D3D11_BUFFER_SRV_0 { FirstElement: 0 },
                        Anonymous2: D3D11_BUFFER_SRV_1 { NumElements: self.height_capacity as u32 },
                    },
                },
            };
            let mut view = None;
            unsafe { gpu.device.CreateShaderResourceView(&buffer, Some(&desc), Some(&mut view))? };
            self.heights = Some((buffer, view.ok_or_else(Error::empty)?));
        }
        Ok(())
    }
}

fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob> {
    let mut code = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            SHADERS.as_ptr().cast(),
            SHADERS.len(),
            s!("shaders.hlsl"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(error) = result {
        let message = errors.as_ref().map(|blob| String::from_utf8_lossy(blob_bytes(blob)).into_owned()).unwrap_or_default();
        return Err(Error::new(error.code(), message));
    }
    code.ok_or_else(Error::empty)
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize()) }
}

fn vertex_shader(device: &ID3D11Device, code: &ID3DBlob) -> Result<ID3D11VertexShader> {
    let mut shader = None;
    unsafe { device.CreateVertexShader(blob_bytes(code), None, Some(&mut shader))? };
    shader.ok_or_else(Error::empty)
}

fn pixel_shader(device: &ID3D11Device, code: &ID3DBlob) -> Result<ID3D11PixelShader> {
    let mut shader = None;
    unsafe { device.CreatePixelShader(blob_bytes(code), None, Some(&mut shader))? };
    shader.ok_or_else(Error::empty)
}

fn dynamic_buffer(device: &ID3D11Device, length: usize, bind: D3D11_BIND_FLAG) -> Result<ID3D11Buffer> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: length as u32,
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: bind.0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let mut buffer = None;
    unsafe { device.CreateBuffer(&desc, None, Some(&mut buffer))? };
    buffer.ok_or_else(Error::empty)
}
