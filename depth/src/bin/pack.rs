//! Packs Depth Anything V2 Small's published weights for download.
//!
//! usage: pack depth_anything_v2_vits.pth depth-anything-v2-small.snowdepth
//! The weights come from https://huggingface.co/depth-anything/Depth-Anything-V2-Small (Apache-2.0).

use sha2::{Digest, Sha256};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (from, to) = match args.as_slice() {
        [_, from, to] => (from, to),
        _ => {
            eprintln!("usage: pack <weights.pth> <out.snowdepth>");
            std::process::exit(2);
        }
    };
    let mut tensors: Vec<(String, Vec<usize>, Vec<f32>)> = candle_core::pickle::read_all(from)
        .expect("readable weights")
        .into_iter()
        .map(|(name, tensor)| {
            let tensor = tensor.to_dtype(candle_core::DType::F32).expect("floats");
            let dims = tensor.dims().to_vec();
            (name, dims, tensor.flatten_all().expect("flat").to_vec1().expect("values"))
        })
        .collect();
    tensors.sort_by(|a, b| a.0.cmp(&b.0));
    let packed = snowhush_depth::pack(&tensors);
    std::fs::write(to, &packed).expect("written");
    let digest: String = Sha256::digest(&packed).iter().map(|byte| format!("{byte:02x}")).collect();
    println!("{} tensors, {} bytes, sha256 {digest}", tensors.len(), packed.len());
}
