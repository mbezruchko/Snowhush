//! Halloween: the snow takes on a spooky mood from October 30 until dawn on November 1, and any other day it is
//! turned on from the menu.

use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::core::{Error, Result};

use snowhush_core::halloween::{Choice, SIGN_CAPTIONS, Today};
use snowhush_core::simulation::Mode;

use crate::prefs::Settings;

/// The signs the ghosts hold up, one picture for each of `SIGN_CAPTIONS`, drawn on the Mac.
const SIGNS: [&[u8]; SIGN_CAPTIONS.len()] = [
    include_bytes!("../assets/signs/sign-00.png"),
    include_bytes!("../assets/signs/sign-01.png"),
    include_bytes!("../assets/signs/sign-02.png"),
    include_bytes!("../assets/signs/sign-03.png"),
    include_bytes!("../assets/signs/sign-04.png"),
    include_bytes!("../assets/signs/sign-05.png"),
    include_bytes!("../assets/signs/sign-06.png"),
    include_bytes!("../assets/signs/sign-07.png"),
    include_bytes!("../assets/signs/sign-08.png"),
    include_bytes!("../assets/signs/sign-09.png"),
    include_bytes!("../assets/signs/sign-10.png"),
    include_bytes!("../assets/signs/sign-11.png"),
];

/// Pixels to a point in the signs' pictures.
pub const SIGN_SCALE: f32 = 2.0;

/// The jack-o'-lantern sitting in the snow, drawn on the Mac: its shell and its carved face, apart so the candle
/// flickers in the face alone.
const LANTERN: [&[u8]; 2] = [include_bytes!("../assets/lanterns/shell.png"), include_bytes!("../assets/lanterns/face.png")];

pub fn today() -> Today {
    let now = unsafe { GetLocalTime() };
    Today::at(now.wYear as u32, now.wMonth as u32, now.wDay as u32, now.wHour as u32)
}

/// Whether Halloween is wanted today, over snow or leaves.
pub fn wanted(choice: Choice) -> bool {
    choice.wanted(today())
}

/// Whether the snow is spooky: Halloween wanted, over snow alone, not leaves or rain.
pub fn shows(settings: &Settings) -> bool {
    settings.modes() == [Mode::Snow] && wanted(settings.halloween)
}

/// A picture drawn on the Mac: rows of premultiplied RGBA from the top.
pub struct Picture {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A sign's picture, `SIGN_SCALE` pixels to a point, the stick's foot at the middle of the bottom edge.
pub fn sign_picture(caption: usize) -> Result<Picture> {
    decode(SIGNS.get(caption).ok_or_else(Error::empty)?)
}

/// The jack-o'-lantern's shell and its face, the same size and laid over each other.
pub fn pumpkin_pictures() -> Result<[Picture; 2]> {
    Ok([decode(LANTERN[0])?, decode(LANTERN[1])?])
}

fn decode(data: &[u8]) -> Result<Picture> {
    unsafe {
        let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = SHCreateMemStream(Some(data)).ok_or_else(Error::empty)?;
        let decoder = imaging.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
        let converter = imaging.CreateFormatConverter()?;
        converter.Initialize(&decoder.GetFrame(0)?, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        let (mut width, mut height) = (0, 0);
        converter.GetSize(&mut width, &mut height)?;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha = pixel[3] as u32;
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
            }
        }
        Ok(Picture { pixels, width, height })
    }
}
