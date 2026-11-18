//! How the weights are packed to a quarter of their size: each tensor by name, the large ones' rows at eight bits with a
//! half-precision scale each, which leaves the depth as it was to within a twentieth of a percent. Shared with the build
//! script, which packs them.

#![allow(dead_code)]

use half::f16;

pub const MAGIC: &[u8; 8] = b"SNOWDEPT";
pub const VERSION: u32 = 1;
/// Tensors smaller than this are kept at half precision whole.
const SMALL: usize = 4096;

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
