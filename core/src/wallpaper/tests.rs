use super::*;

/// A mountain under a pale sky, its ridge a gentle wave a little under halfway down.
fn mountain(width: usize, height: usize) -> Shown {
    let mut pixels = vec![255u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let ridge = height as f32 * 0.4 + 30.0 * (x as f32 / 90.0).sin();
            let i = (y * width + x) * 4;
            let colour = if (y as f32) < ridge {
                [170, 195, 230]
            } else {
                let grain = ((x * 7 + y * 13) % 17) as u8 * 3;
                [70 + grain, 62 + grain, 58 + grain]
            };
            pixels[i..i + 3].copy_from_slice(&colour);
        }
    }
    Shown { width, height, pixels }
}

/// Takes whatever is dark for near, nearer the lower it is: as the mountain against the sky is.
struct Darkness;

impl Network for Darkness {
    fn size(&self) -> (usize, usize) {
        (128, 96)
    }

    fn depth(&mut self, shown: &Shown, part: [usize; 4]) -> Option<Vec<f32>> {
        let (width, height) = self.size();
        let mut values = Vec::with_capacity(width * height);
        for v in 0..height {
            for u in 0..width {
                let x = part[0] + (u * part[2]) / width;
                let y = part[1] + (v * part[3]) / height;
                let level = luminance(shown.rgb((y * shown.width + x) * 4)) / 255.0;
                values.push(if level > 0.6 { 0.0 } else { 0.4 + 0.5 * y as f32 / shown.height as f32 });
            }
        }
        Some(values)
    }
}

#[test]
fn a_mountain_against_the_sky_has_a_ridge_and_is_kept_whole() {
    let (width, height) = (640, 400);
    let shown = mountain(width, height);
    let scene = scene(&shown, &mut Darkness, 2.0).expect("a scene");
    assert!(!scene.ridges.is_empty(), "no ridge found");
    let ridge = &scene.ridges[0];
    let k = ridge.tops.len() / 2;
    let x = ridge.left + (k as f32 + 0.5) * 2.0;
    let expected = height as f32 - (height as f32 * 0.4 + 30.0 * (x / 90.0).sin());
    assert!((ridge.tops[k] - expected).abs() < 10.0, "ridge at {} where the mountain's top is {expected}", ridge.tops[k]);
    assert!(ridge.nearness > 0.1);
    assert_eq!(scene.relief.width, width / 2);
    assert!(scene.terrain.near.iter().any(|near| *near > (GROUND * 255.0) as u8));
    let kept = Scene::from_bytes(&scene.to_bytes()).expect("read back");
    assert_eq!(kept, scene);
}
