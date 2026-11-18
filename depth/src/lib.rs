//! The depth network that reads a wallpaper on Windows and Linux: Depth Anything V2 Small (Apache-2.0), run on the CPU
//! by candle. The Mac runs the same network through Core ML. Its weights are downloaded on first use, packed to a
//! quarter of their size: each large tensor's rows at eight bits with a scale of their own, which leaves the depth as it
//! was to within a twentieth of a percent.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use candle_core::{DType, Device, Module, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::depth_anything_v2::{DepthAnythingV2, DepthAnythingV2Config};
use candle_transformers::models::dinov2;
use half::f16;
use sha2::{Digest, Sha256};
use snowhush_core::wallpaper::{Network, Scene, Shown, scene};

/// The packed weights' file, where it is downloaded from, and what it hashes to.
pub const FILE: &str = "depth-anything-v2-small.snowdepth";
pub const URL: &str = "https://snowhush.app/models/depth-anything-v2-small.snowdepth";
pub const SHA256: &str = "5529647891ac4b3eff98ef376531f0e661f96f90c6e2944cf77fe48c6eee9227";

const MAGIC: &[u8; 8] = b"SNOWDEPT";
const VERSION: u32 = 1;
/// The side of the square the network looks at: 37 of its 14-pixel patches.
const SIDE: usize = 518;
/// What one look gives, as the Mac's network does: the parts it is fitted from are laid out for this.
const SIZE: (usize, usize) = (518, 392);
/// Tensors smaller than this are kept at half precision whole.
const SMALL: usize = 4096;

/// Whether `bytes` are the weights as published.
pub fn genuine(bytes: &[u8]) -> bool {
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    hex == SHA256
}

/// Everything found in a wallpaper, its depth read with the weights at `weights`. Slow: seconds of every core, and a few
/// hundred megabytes, for a process of its own.
pub fn read(shown: &Shown, weights: &Path, column_width: f32) -> Result<Scene, String> {
    let mut model = Model::load(weights)?;
    scene(shown, &mut model, column_width).ok_or_else(|| "nothing found".to_string())
}

pub struct Model {
    network: DepthAnythingV2,
    device: Device,
}

impl Model {
    pub fn load(path: &Path) -> Result<Model, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let device = Device::Cpu;
        let mut tensors = unpack(&bytes, &device)?;
        // The depth network has no use for DINOv2's classification head, which candle's encoder is made with all the same.
        let zeros = |shape: &[usize]| Tensor::zeros(shape, DType::F32, &device).map_err(|error| error.to_string());
        tensors.insert("pretrained.head.weight".into(), zeros(&[1000, 768])?);
        tensors.insert("pretrained.head.bias".into(), zeros(&[1000])?);
        let weights = VarBuilder::from_tensors(tensors, DType::F32, &device);
        let encoder = dinov2::vit_small(weights.pp("pretrained")).map_err(|error| error.to_string())?;
        let network = DepthAnythingV2::new(Arc::new(encoder), DepthAnythingV2Config::vit_small(), weights).map_err(|error| error.to_string())?;
        Ok(Model { network, device })
    }

    fn look(&self, shown: &Shown, part: [usize; 4]) -> candle_core::Result<Vec<f32>> {
        let planes = normalized(&resample(shown, part, SIDE, SIDE), SIDE * SIDE);
        let input = Tensor::from_vec(planes, (1, 3, SIDE, SIDE), &self.device)?;
        let output = self.network.forward(&input)?;
        let dims = output.dims().to_vec();
        let (height, width) = (dims[dims.len() - 2], dims[dims.len() - 1]);
        let values: Vec<f32> = output.flatten_all()?.to_vec1()?;
        Ok(stretched(&values, width, height, SIZE.0, SIZE.1))
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

/// A grid of values stretched to another size, bilinear.
fn stretched(values: &[f32], width: usize, height: usize, to_width: usize, to_height: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(to_width * to_height);
    for y in 0..to_height {
        let fy = ((y as f32 + 0.5) / to_height as f32 * height as f32 - 0.5).clamp(0.0, (height - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(height - 1);
        for x in 0..to_width {
            let fx = ((x as f32 + 0.5) / to_width as f32 * width as f32 - 0.5).clamp(0.0, (width - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(width - 1);
            let top = values[y0 * width + x0] * (1.0 - tx) + values[y0 * width + x1] * tx;
            let bottom = values[y1 * width + x0] * (1.0 - tx) + values[y1 * width + x1] * tx;
            out.push(top * (1.0 - ty) + bottom * ty);
        }
    }
    out
}

/// Packs weights for download: each tensor by name, the large ones' rows at eight bits with a half-precision scale each.
pub fn pack(tensors: &[(String, Vec<usize>, Vec<f32>)]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend(VERSION.to_le_bytes());
    out.extend((tensors.len() as u32).to_le_bytes());
    for (name, dims, values) in tensors {
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(name.as_bytes());
        out.push(dims.len() as u8);
        for dim in dims {
            out.extend((*dim as u32).to_le_bytes());
        }
        if values.len() < SMALL || dims.is_empty() {
            out.push(0);
            for value in values {
                out.extend(f16::from_f32(*value).to_le_bytes());
            }
            continue;
        }
        out.push(1);
        let rows = dims[0];
        let per = values.len() / rows;
        let mut scales = Vec::with_capacity(rows);
        let mut codes = Vec::with_capacity(values.len());
        for row in values.chunks(per) {
            let most = row.iter().fold(0f32, |most, value| most.max(value.abs())).max(1e-12);
            let scale = f16::from_f32(most / 127.0);
            let step = scale.to_f32().max(1e-12);
            scales.push(scale);
            codes.extend(row.iter().map(|value| (value / step).round().clamp(-127.0, 127.0) as i8 as u8));
        }
        for scale in scales {
            out.extend(scale.to_le_bytes());
        }
        out.extend(codes);
    }
    out
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
