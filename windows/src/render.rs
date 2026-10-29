use std::mem::size_of;

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
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
    DXGI_FORMAT_R32G32B32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter,
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
};
use windows::core::{Error, Interface, PCSTR, Result, s};
use windows_numerics::Matrix3x2;

use snowhush_core::simulation::{COLUMN_WIDTH, SnowSimulation, Sprite};

const SHADERS: &str = include_str!("shaders.hlsl");

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
    occluders: [[f32; 4]; 16],
}

/// The light the snow is drawn in.
#[derive(Clone, Copy)]
pub struct Ambient {
    pub light: [f32; 3],
    pub shadow_tint: [f32; 4],
}

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
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    uniforms: ID3D11Buffer,
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

            let uniforms = dynamic_buffer(&device, size_of::<Uniforms>(), D3D11_BIND_CONSTANT_BUFFER)?;

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
                blend: blend.ok_or_else(Error::empty)?,
                raster: raster.ok_or_else(Error::empty)?,
                uniforms,
                adapter: String::from_utf16_lossy(&description.Description[..name_length]),
            })
        }
    }

    fn write_uniforms(&self, uniforms: &Uniforms) -> Result<()> {
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(&self.uniforms, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            mapped.pData.cast::<Uniforms>().write(*uniforms);
            self.context.Unmap(&self.uniforms, 0);
        }
        Ok(())
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
            let constants = Some(gpu.uniforms.clone());
            context.VSSetConstantBuffers(0, Some(&[constants.clone()]));
            context.PSSetConstantBuffers(0, Some(&[constants]));

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
                occluders: [[0.0; 4]; 16],
            };

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
                    gpu.write_uniforms(&uniforms)?;
                    context.Draw(4, 0);
                }
            }
            if sprites > 0 {
                uniforms.occluder_count = 0;
                gpu.write_uniforms(&uniforms)?;
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
