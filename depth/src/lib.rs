//! The depth network that reads a wallpaper on Windows and Linux: Depth Anything V2 Small (Apache-2.0), run on the CPU
//! by candle, looking at 518 by 392 pixels as the Mac's Core ML network does. Its weights are built into the app, as the
//! Mac's are, packed to a quarter of their size: see the build script.

mod network;
mod packing;

use std::collections::HashMap;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use half::f16;
use packing::{MAGIC, VERSION};
use snowhush_core::wallpaper::{Network, Scene, Shown, scene};

/// The packed weights, about 25 MB: only the process reading a wallpaper touches them.
static WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/weights.snowdepth"));

/// What the network looks at and gives, as the Mac's does, 37 of its 14-pixel patches across and 28 down: the parts the
/// depth is fitted from are laid out for this.
const SIZE: (usize, usize) = (518, 392);

/// Everything found in a wallpaper. Slow: seconds of every core, and a few hundred megabytes, for a process of its own.
pub fn read(shown: &Shown, column_width: f32) -> Result<Scene, String> {
    let mut model = Model::load()?;
    scene(shown, &mut model, column_width).ok_or_else(|| "nothing found".to_string())
}

pub struct Model {
    network: network::Network,
    device: Device,
}

impl Model {
    pub fn load() -> Result<Model, String> {
        let device = Device::Cpu;
        let tensors = unpack(WEIGHTS, &device)?;
        let weights = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let network = network::Network::new(weights).map_err(|error| error.to_string())?;
        Ok(Model { network, device })
    }

    fn look(&self, shown: &Shown, part: [usize; 4]) -> candle_core::Result<Vec<f32>> {
        let (width, height) = SIZE;
        let planes = normalized(&resample(shown, part, width, height), width * height);
        let input = Tensor::from_vec(planes, (1, 3, height, width), &self.device)?;
        self.network.forward(&input)?.flatten_all()?.to_vec1()
    }
}

impl Network for Model {
    fn size(&self) -> (usize, usize) {
        SIZE
    }

    fn depth(&mut self, shown: &Shown, part: [usize; 4]) -> Option<Vec<f32>> {
        if part[2] == 0 || part[3] == 0 || part[0] + part[2] > shown.width || part[1] + part[3] > shown.height {
            return None;
        }
        self.look(shown, part).ok()
    }
}

/// A part of the picture stretched to `width` by `height`, RGB 0 to 1: each pixel the mean of what it covers, shrinking,
/// and blended between its neighbours, growing.
fn resample(shown: &Shown, part: [usize; 4], width: usize, height: usize) -> Vec<[f32; 3]> {
    let [left, top, across, down] = part;
    let mut source = Vec::with_capacity(across * down);
    for y in top..top + down {
        for x in left..left + across {
            let i = (y * shown.width + x) * 4;
            source.push([shown.pixels[i] as f32 / 255.0, shown.pixels[i + 1] as f32 / 255.0, shown.pixels[i + 2] as f32 / 255.0]);
        }
    }
    let rows = resize_axis(&source, across, down, width, true);
    resize_axis(&rows, width, down, height, false)
}

/// Resizes along one axis: across each row when `rows`, else down each column.
fn resize_axis(source: &[[f32; 3]], width: usize, height: usize, to: usize, rows: bool) -> Vec<[f32; 3]> {
    let (from, lines) = if rows { (width, height) } else { (height, width) };
    let (out_width, out_height) = if rows { (to, height) } else { (width, to) };
    let mut out = vec![[0f32; 3]; out_width * out_height];
    let at = |line: usize, k: usize| {
        if rows { line * width + k } else { k * width + line }
    };
    let put = |line: usize, k: usize| {
        if rows { line * out_width + k } else { k * out_width + line }
    };
    let ratio = from as f32 / to as f32;
    for line in 0..lines {
        for k in 0..to {
            let mut sum = [0f32; 3];
            if ratio > 1.0 {
                let (start, end) = (k as f32 * ratio, (k + 1) as f32 * ratio);
                let mut total = 0.0;
                let mut s = start.floor() as usize;
                while (s as f32) < end && s < from {
                    let weight = ((s + 1) as f32).min(end) - (s as f32).max(start);
                    let c = source[at(line, s)];
                    for channel in 0..3 {
                        sum[channel] += c[channel] * weight;
                    }
                    total += weight;
                    s += 1;
                }
                sum = sum.map(|value| value / total.max(1e-6));
            } else {
                let position = ((k as f32 + 0.5) * ratio - 0.5).clamp(0.0, (from - 1) as f32);
                let (s, t) = (position as usize, position.fract());
                let (a, b) = (source[at(line, s)], source[at(line, (s + 1).min(from - 1))]);
                for channel in 0..3 {
                    sum[channel] = a[channel] * (1.0 - t) + b[channel] * t;
                }
            }
            out[put(line, k)] = sum;
        }
    }
    out
}

/// The network's input: red, green and blue planes, each as ImageNet's photographs are spread.
fn normalized(pixels: &[[f32; 3]], count: usize) -> Vec<f32> {
    let mean = [0.485, 0.456, 0.406];
    let deviation = [0.229, 0.224, 0.225];
    let mut planes = vec![0f32; 3 * count];
    for (i, pixel) in pixels.iter().enumerate() {
        for c in 0..3 {
            planes[c * count + i] = (pixel[c] - mean[c]) / deviation[c];
        }
    }
    planes
}

fn unpack(bytes: &[u8], device: &Device) -> Result<HashMap<String, Tensor>, String> {
    let broken = || "broken weights".to_string();
    let mut input = bytes.strip_prefix(MAGIC.as_slice()).ok_or_else(broken)?;
    let mut take = |count: usize| -> Result<&[u8], String> {
        if input.len() < count {
            return Err(broken());
        }
        let (taken, rest) = input.split_at(count);
        input = rest;
        Ok(taken)
    };
    let word = |bytes: &[u8]| u32::from_le_bytes(bytes.try_into().unwrap_or_default()) as usize;
    if word(take(4)?) != VERSION as usize {
        return Err("weights of another version".into());
    }
    let count = word(take(4)?);
    let mut tensors = HashMap::with_capacity(count);
    for _ in 0..count {
        let length = u16::from_le_bytes(take(2)?.try_into().map_err(|_| broken())?) as usize;
        let name = String::from_utf8(take(length)?.to_vec()).map_err(|_| broken())?;
        let rank = take(1)?[0] as usize;
        let dims: Vec<usize> = (0..rank).map(|_| take(4).map(word)).collect::<Result<_, _>>()?;
        let total: usize = dims.iter().product();
        let values: Vec<f32> = match take(1)?[0] {
            0 => take(total * 2)?.chunks_exact(2).map(|pair| f16::from_le_bytes([pair[0], pair[1]]).to_f32()).collect(),
            1 => {
                let rows = *dims.first().ok_or_else(broken)?;
                let scales: Vec<f32> = take(rows * 2)?.chunks_exact(2).map(|pair| f16::from_le_bytes([pair[0], pair[1]]).to_f32()).collect();
                let per = total / rows.max(1);
                take(total)?.iter().enumerate().map(|(i, code)| *code as i8 as f32 * scales[i / per]).collect()
            }
            _ => return Err(broken()),
        };
        let tensor = Tensor::from_vec(values, dims, device).map_err(|error| error.to_string())?;
        tensors.insert(name, tensor);
    }
    Ok(tensors)
}
