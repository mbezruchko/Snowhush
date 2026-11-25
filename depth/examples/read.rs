//! Reads a wallpaper as Windows and Linux will, and sets what it finds beside what the Mac found in the same picture.
//!
//! usage: read <picture> <width> <height> [mac-scene.json]

use std::time::Instant;

use base64::Engine;
use snowhush_core::wallpaper::{Scene, Shown};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let picture = &args[1];
    let (width, height): (u32, u32) = (args[2].parse().unwrap(), args[3].parse().unwrap());
    let image = image::open(picture).expect("picture").to_rgb8();
    let scale = (width as f32 / image.width() as f32).max(height as f32 / image.height() as f32);
    let (drawn_w, drawn_h) = ((image.width() as f32 * scale).round() as u32, (image.height() as f32 * scale).round() as u32);
    let drawn = image::imageops::resize(&image, drawn_w, drawn_h, image::imageops::FilterType::CatmullRom);
    let (left, top) = ((drawn_w - width) / 2, (drawn_h - height) / 2);
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let p = drawn.get_pixel(x + left, y + top);
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 3].copy_from_slice(&p.0);
        }
    }
    let shown = Shown { width: width as usize, height: height as usize, pixels };
    let started = Instant::now();
    let scene = if let Ok(tool) = std::env::var("COREML") {
        snowhush_core::wallpaper::scene(&shown, &mut CoreMl(tool), 2.0).expect("scene")
    } else {
        snowhush_depth::read(&shown, 2.0).expect("scene")
    };
    println!("read in {:.1} s", started.elapsed().as_secs_f32());
    summary("rust", &scene.ridges.iter().map(|r| (r.left, r.tops.clone(), r.nearness)).collect::<Vec<_>>());
    println!(
        "terrain far {:.3} close {:.3}, haze {:?}",
        scene.terrain.far,
        scene.terrain.close,
        scene.light.haze.map(|v| (v * 1000.0).round() / 1000.0)
    );
    println!("bytes kept {}", scene.to_bytes().len());
    if let Some(mac) = args.get(4) {
        compare(&scene, &std::fs::read_to_string(mac).expect("mac scene"));
    }
}

fn summary(who: &str, ridges: &[(f32, Vec<f32>, f32)]) {
    let columns: usize = ridges.iter().map(|r| r.1.len()).sum();
    println!("{who}: {} ridges, {columns} columns", ridges.len());
    let mut sorted: Vec<_> = ridges.iter().collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (left, tops, near) in sorted.iter().take(6) {
        println!("  left {:5.0} n {:4} near {:.3} top mid {:6.1}", left, tops.len(), near, tops[tops.len() / 2]);
    }
}

fn plane(value: &serde_json::Value) -> Vec<u8> {
    let png = base64::engine::general_purpose::STANDARD.decode(value.as_str().unwrap()).unwrap();
    image::load_from_memory(&png).unwrap().to_luma8().into_raw()
}

fn correlation(a: &[u8], b: &[u8]) -> f32 {
    let n = a.len().min(b.len()) as f64;
    let (ma, mb) = (a.iter().map(|v| *v as f64).sum::<f64>() / n, b.iter().map(|v| *v as f64).sum::<f64>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (dx, dy) = (*x as f64 - ma, *y as f64 - mb);
        sab += dx * dy;
        saa += dx * dx;
        sbb += dy * dy;
    }
    (sab / (saa * sbb).sqrt()) as f32
}

fn compare(scene: &Scene, mac: &str) {
    let mac: serde_json::Value = serde_json::from_str(mac).unwrap();
    let ridges: Vec<(f32, Vec<f32>, f32)> = mac["ridges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let tops = r["tops"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
            (r["left"].as_f64().unwrap() as f32, tops, r["nearness"].as_f64().unwrap() as f32)
        })
        .collect();
    summary("mac", &ridges);
    let relief = &mac["relief"];
    for (name, ours) in
        [("lightness", &scene.relief.lightness), ("tops", &scene.relief.tops), ("hold", &scene.relief.hold), ("near", &scene.relief.near)]
    {
        let theirs = plane(&relief[name]);
        let covered = |p: &[u8]| p.iter().filter(|v| **v > 64).count() as f32 / p.len() as f32;
        println!("{name:9}: correlation {:.3}, over a quarter: rust {:.3} mac {:.3}", correlation(ours, &theirs), covered(ours), covered(&theirs));
    }
    // How many of the Mac's ridge columns have one of ours within six points.
    let (mut matched, mut all) = (0, 0);
    for (left, tops, _) in &ridges {
        for (k, top) in tops.iter().enumerate() {
            let x = left + k as f32 * 2.0;
            all += 1;
            if scene.ridges.iter().any(|r| {
                let j = ((x - r.left) / 2.0).round() as i32;
                j >= 0 && (j as usize) < r.tops.len() && (r.tops[j as usize] - top).abs() < 6.0
            }) {
                matched += 1;
            }
        }
    }
    println!("mac ridge columns matched within 6 pt: {matched}/{all}");
}

/// The Mac's own network, run by a tool of its own on each part: what the Mac's reading of the picture starts from.
struct CoreMl(String);

impl snowhush_core::wallpaper::Network for CoreMl {
    fn size(&self) -> (usize, usize) {
        (518, 392)
    }

    fn depth(&mut self, shown: &Shown, part: [usize; 4]) -> Option<Vec<f32>> {
        let [left, top, width, height] = part;
        let mut crop = image::RgbImage::new(width as u32, height as u32);
        for y in 0..height {
            for x in 0..width {
                let i = ((top + y) * shown.width + left + x) * 4;
                crop.put_pixel(x as u32, y as u32, image::Rgb([shown.pixels[i], shown.pixels[i + 1], shown.pixels[i + 2]]));
            }
        }
        let dir = std::env::temp_dir();
        let (png, out) = (dir.join("part.png"), dir.join("part.f32"));
        crop.save(&png).ok()?;
        let model = std::env::var("COREML_MODEL").ok()?;
        let status =
            std::process::Command::new(&self.0).args([&model, png.to_str()?, out.to_str()?]).stdout(std::process::Stdio::null()).status().ok()?;
        if !status.success() {
            return None;
        }
        Some(std::fs::read(out).ok()?.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
    }
}
