//! Sets the snowball fight's words with DirectWrite in Segoe UI, and a friend's name in another script in whatever
//! font Windows falls back to for it, drawn by Direct2D into a bitmap of their coverage.

use snowhush_core::snowman::writing::{Run, Set, Typesetter, Weight};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::core::{HSTRING, Result, w};
use windows_numerics::Vector2;

use crate::log;

pub struct Fonts {
    d2d: ID2D1Factory,
    text: IDWriteFactory,
    imaging: IWICImagingFactory,
}

impl Fonts {
    pub fn new() -> Result<Fonts> {
        unsafe {
            Ok(Fonts {
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                text: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
                imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
            })
        }
    }

    fn try_set(&self, run: &Run, scale: f32) -> Result<Set> {
        let units: Vec<u16> = run.text.encode_utf16().collect();
        let weight = match run.weight {
            Weight::Semibold => DWRITE_FONT_WEIGHT_SEMI_BOLD,
            Weight::Bold => DWRITE_FONT_WEIGHT_BOLD,
        };
        unsafe {
            // The locale picks the fallback font, so Japanese and Chinese get their own forms of the shared characters.
            let locale = HSTRING::from(snowhush_core::text::code());
            let format =
                self.text.CreateTextFormat(w!("Segoe UI"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, run.size, &locale)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let layout = self.text.CreateTextLayout(&units, &format, 4096.0, 512.0)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            let mut lines = [DWRITE_LINE_METRICS::default()];
            let mut count = 0;
            layout.GetLineMetrics(Some(&mut lines), &mut count)?;
            let width = (metrics.widthIncludingTrailingWhitespace * scale).ceil().max(1.0) as usize;
            let height = (metrics.height * scale).ceil().max(1.0) as usize;
            let baseline = (lines[0].baseline * scale).round() as usize;
            let bitmap = self.imaging.CreateBitmap(width as u32, height as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties)?;
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let white = target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, None)?;
            target.DrawTextLayout(Vector2 { X: 0.0, Y: 0.0 }, &layout, &white, D2D1_DRAW_TEXT_OPTIONS_NONE);
            target.EndDraw(None, None)?;
            drop(target);
            let mut pixels = vec![0u8; width * height * 4];
            bitmap.CopyPixels(std::ptr::null(), (width * 4) as u32, &mut pixels)?;
            Ok(Set { width, height, baseline, coverage: pixels.chunks_exact(4).map(|pixel| pixel[3]).collect() })
        }
    }
}

impl Typesetter for Fonts {
    fn set(&mut self, run: &Run, scale: f32) -> Option<Set> {
        self.try_set(run, scale).inspect_err(|error| log!("fight: {:?} not set: {error}", run.text)).ok()
    }
}
