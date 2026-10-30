//! Halloween's jack-o'-lanterns: a pair sits in the snow in the bottom right corner of the screen and one in the
//! left, sunk into the drift, their candles lighting the snow around them. They belong to the desktop: one a window
//! covers sinks below the screen's edge until the window moves off it. Where a panel's shelf runs along the bottom,
//! those over it sit in its snow instead, in front of the panel. The same as the Mac app's.

use super::{PumpkinSprite, SnowSimulation};
use crate::geometry::V2;
use crate::halloween::PUMPKIN_ASPECT;

/// How far each sits from the screen's right edge, or from the left one when negative, and its width, in points.
const SPOTS: [(f32, f32); 3] = [(120.0, 52.0), (190.0, 37.0), (-150.0, 42.0)];

#[derive(Default)]
pub(super) struct Pumpkins {
    /// Where each one's bottom rests, easing after the drift under it; empty until they are first placed.
    bottoms: Vec<f32>,
    pub(super) moved: bool,
}

impl Pumpkins {
    pub(super) fn reset(&mut self) {
        self.bottoms.clear();
        self.moved = true;
    }
}

impl SnowSimulation {
    /// The jack-o'-lanterns to draw on Halloween, before the snow so the drift covers their bottoms.
    pub fn pumpkins(&self) -> Vec<PumpkinSprite> {
        if !self.haunting.on {
            return Vec::new();
        }
        SPOTS
            .iter()
            .zip(&self.lanterns.bottoms)
            .enumerate()
            .map(|(k, (&(inset, width), &bottom))| PumpkinSprite { anchor: V2::new(self.pumpkin_x(inset), bottom), width, seed: k as f32 })
            .collect()
    }

    fn pumpkin_x(&self, inset: f32) -> f32 {
        if inset < 0.0 { -inset } else { self.width - inset }
    }

    /// Each pumpkin sits on the drift under it, sunk into it a little under half its height, or on the drift's bed
    /// while the snow there is shallower than that; it eases up and down as the drift grows and is swept.
    pub(super) fn settle_pumpkins(&mut self, dt: f32) {
        let xs = SPOTS.map(|(inset, _)| self.pumpkin_x(inset));
        let shelf = self.shelf.and_then(|id| self.ids.iter().position(|&other| other == id));
        let placed = !self.lanterns.bottoms.is_empty();
        if !placed {
            self.lanterns.bottoms = vec![0.0; SPOTS.len()];
        }
        for (k, &(_, width)) in SPOTS.iter().enumerate() {
            let x = xs[k];
            let height = width * PUMPKIN_ASPECT;
            let on_shelf = shelf.filter(|&li| {
                let layer = &self.layers[li];
                layer.base().x <= x - width / 2.0 && layer.base().x + layer.width() >= x + width / 2.0
            });
            let layer = &self.layers[on_shelf.unwrap_or(0)];
            let floor = layer.floor(layer.clamped_index(x));
            let (from, to) = (layer.clamped_index(x - width * 0.3), layer.clamped_index(x + width * 0.3));
            let surface = (from..=to).map(|i| layer.surface(i)).sum::<f32>() / (to - from + 1) as f32;
            let covered = self.covers.iter().any(|c| c[0] < x + width / 2.0 && c[2] > x - width / 2.0 && c[1] < floor + height && c[3] > floor);
            let target = if covered { floor - height - 6.0 } else { (surface - height * 0.42).max(floor) };
            let bottom = &mut self.lanterns.bottoms[k];
            let next = if placed { *bottom + (target - *bottom) * (dt * 5.0).min(1.0) } else { target };
            if (next - *bottom).abs() > 0.01 {
                self.lanterns.moved = true;
            }
            *bottom = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::geometry::{Rect, V2};
    use crate::halloween::PUMPKIN_ASPECT;
    use crate::simulation::{SnowSimulation, Snowfall, WindowSurface};

    const DT: f32 = 1.0 / 60.0;
    const AWAY: V2 = V2::new(-10_000.0, -10_000.0);

    fn run(simulation: &mut SnowSimulation, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            simulation.step(DT, AWAY);
        }
    }

    #[test]
    fn pumpkins_sit_in_the_drift_and_hide_from_a_window_over_them() {
        let mut simulation = SnowSimulation::new(1200.0, 800.0, Snowfall::Off);
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, 30.0);
        }
        assert!(simulation.pumpkins().is_empty());
        simulation.set_halloween(true);
        run(&mut simulation, 0.5);
        let pumpkins = simulation.pumpkins();
        assert_eq!(pumpkins.len(), 3);
        let big = pumpkins[0];
        assert_eq!(big.anchor.x, 1080.0);
        assert_eq!(pumpkins[2].anchor.x, 150.0);
        let sunk = 30.0 - big.anchor.y;
        assert!((sunk - big.width * PUMPKIN_ASPECT * 0.42).abs() < 1.0, "sunk {sunk}");

        simulation.covers = vec![[600.0, 0.0, 1200.0, 700.0]];
        run(&mut simulation, 2.0);
        let pumpkins = simulation.pumpkins();
        assert!(pumpkins[..2].iter().all(|p| p.anchor.y + p.width * PUMPKIN_ASPECT < 0.0));
        assert!(pumpkins[2].anchor.y > 0.0, "the left one stays out");

        simulation.covers.clear();
        run(&mut simulation, 2.0);
        assert!(simulation.pumpkins().iter().all(|p| p.anchor.y > 0.0));
        simulation.set_halloween(false);
        assert!(simulation.pumpkins().is_empty());
    }

    #[test]
    fn pumpkins_over_a_panels_shelf_sit_in_its_snow() {
        let mut simulation = SnowSimulation::new(1200.0, 800.0, Snowfall::Off);
        let shelf = WindowSurface {
            id: 7,
            frame: Rect { x: 8.0, y: 0.0, width: 1184.0, height: 44.0 },
            blockers: Vec::new(),
            corner_radius: 8.0,
            corner_exponent: 2.0,
        };
        simulation.update_windows(&[shelf], 0.1, &HashSet::new(), &HashSet::new(), false);
        simulation.shelf = Some(7);
        simulation.set_halloween(true);
        run(&mut simulation, 0.5);
        assert!(simulation.pumpkins().iter().all(|p| (p.anchor.y - 44.0).abs() < 1.0), "{:?}", simulation.pumpkins());

        simulation.covers = vec![[600.0, 44.0, 1200.0, 700.0]];
        run(&mut simulation, 2.0);
        let pumpkins = simulation.pumpkins();
        assert!(pumpkins[..2].iter().all(|p| p.anchor.y + p.width * PUMPKIN_ASPECT < 44.0));
        assert!((pumpkins[2].anchor.y - 44.0).abs() < 1.0);
    }
}
