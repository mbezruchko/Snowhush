//! Builds the depth network's weights into the crate, as the Mac's build.sh does its Core ML model: Depth Anything V2
//! Small's published weights (Apache-2.0), fetched once at a fixed revision and checked, packed to a quarter of their
//! size. SNOWHUSH_DEPTH_WEIGHTS names a copy of the published file to build from without the network.

use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

#[path = "src/packing.rs"]
mod packing;

const REVISION: &str = "03876f8651c73a60fe4c2c48294e09fcb6838fcf";
const PUBLISHED: &str = "715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378";
/// What they pack to: the same every time, from the same weights.
const PACKED: &str = "5529647891ac4b3eff98ef376531f0e661f96f90c6e2944cf77fe48c6eee9227";

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/packing.rs");
    println!("cargo:rerun-if-env-changed=SNOWHUSH_DEPTH_WEIGHTS");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let packed = out.join("weights.snowdepth");
    if std::fs::read(&packed).is_ok_and(|bytes| hex(&bytes) == PACKED) {
        return;
    }
    let published = match std::env::var_os("SNOWHUSH_DEPTH_WEIGHTS") {
        Some(path) => PathBuf::from(path),
        None => {
            let path = out.join("depth_anything_v2_vits.pth");
            if !std::fs::read(&path).is_ok_and(|bytes| hex(&bytes) == PUBLISHED) {
                let url = format!("https://huggingface.co/depth-anything/Depth-Anything-V2-Small/resolve/{REVISION}/depth_anything_v2_vits.pth");
                let status = Command::new("curl")
                    .args(["--fail", "--silent", "--show-error", "--location", "--retry", "3", "--output"])
                    .arg(&path)
                    .arg(&url)
                    .status();
                assert!(status.is_ok_and(|status| status.success()), "couldn't download the depth network's weights from {url}");
            }
            path
        }
    };
    let bytes = std::fs::read(&published).expect("the depth network's weights");
    assert_eq!(hex(&bytes), PUBLISHED, "{} aren't Depth Anything V2 Small's published weights", published.display());
    let mut tensors: Vec<(String, Vec<usize>, Vec<f32>)> = candle_core::pickle::read_all(&published)
        .expect("readable weights")
        .into_iter()
        .map(|(name, tensor)| {
            let tensor = tensor.to_dtype(candle_core::DType::F32).expect("floats");
            let dims = tensor.dims().to_vec();
            (name, dims, tensor.flatten_all().expect("flat").to_vec1().expect("values"))
        })
        .collect();
    tensors.sort_by(|a, b| a.0.cmp(&b.0));
    let bytes = packing::pack(&tensors);
    assert_eq!(hex(&bytes), PACKED, "the weights packed to something else");
    std::fs::write(&packed, bytes).expect("packed weights written");
    if std::env::var_os("SNOWHUSH_DEPTH_WEIGHTS").is_none() {
        let _ = std::fs::remove_file(&published);
    }
}
