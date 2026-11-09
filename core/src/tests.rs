use std::collections::HashSet;

use crate::geometry::{Rect, V2};
use crate::layer::SnowLayer;
use crate::simulation::{Mapping, Mode, Pace, ScatterPiece, SnowSimulation, Sprite, WindowSurface};

const DT: f32 = 1.0 / 60.0;
const AWAY: V2 = V2::new(-10_000.0, -10_000.0);

fn run(simulation: &mut SnowSimulation, seconds: f32, cursor: V2) {
    for _ in 0..(seconds / DT) as usize {
        simulation.step(DT, cursor);
    }
}

fn surface(id: isize, x: f32, top: f32, width: f32) -> WindowSurface {
    WindowSurface {
        id,
        frame: Rect { x, y: top - 300.0, width, height: 300.0 },
        blockers: Vec::new(),
        cover: Vec::new(),
        corner_radius: 8.0,
        corner_exponent: 2.5,
    }
}

/// Snow area per layer, from the smoothed surfaces the renderer gets.
fn piles(simulation: &SnowSimulation) -> Vec<(V2, f32)> {
    let mut buffer = vec![0.0; simulation.column_count()];
    let layers = simulation.write_layers(&mut buffer);
    layers
        .iter()
        .map(|pile| {
            let (surface, bed) = buffer[pile.offset..pile.offset + 2 * pile.count].split_at(pile.count);
            (pile.origin, surface.iter().zip(bed).map(|(s, b)| s - b).sum::<f32>() * 2.0)
        })
        .collect()
}

fn assert_finite(simulation: &SnowSimulation) {
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    for sprite in &sprites[..count] {
        assert!(sprite.position.x.is_finite() && sprite.position.y.is_finite() && sprite.size.is_finite() && sprite.alpha.is_finite());
    }
    let mut buffer = vec![0.0; simulation.column_count()];
    simulation.write_layers(&mut buffer);
    assert!(buffer.iter().all(|h| h.is_finite()));
}

#[test]
fn snow_falls_and_settles_on_the_ground() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    run(&mut simulation, 1.0, AWAY);
    assert!(simulation.sprite_count() > 500, "flakes: {}", simulation.sprite_count());
    run(&mut simulation, 60.0, AWAY);
    assert!(simulation.has_snow());
    let piles = piles(&simulation);
    assert_eq!(piles.len(), 1);
    assert!(piles[0].1 > 1000.0, "ground area {}", piles[0].1);
    assert_finite(&simulation);
}

#[test]
fn off_stops_the_snowfall() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.0);
    run(&mut simulation, 30.0, AWAY);
    assert_eq!(simulation.sprite_count(), 0);
    assert!(!simulation.has_snow());
}

#[test]
fn cursor_sweeps_the_drift_aside() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    simulation.thaws = false;
    run(&mut simulation, 120.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 15.0, AWAY);
    let before = piles(&simulation)[0].1;

    let mut sprayed = 0;
    for i in 0..120 {
        let x = 200.0 + i as f32 * 8.0;
        simulation.step(DT, V2::new(x, 10.0));
        sprayed = sprayed.max(simulation.sprite_count());
    }
    assert!(sprayed > 20, "no snow thrown up: {sprayed}");
    run(&mut simulation, 10.0, AWAY);
    let after = piles(&simulation)[0].1;
    // Swept snow lands again; it moves rather than vanishes.
    assert!((after - before).abs() / before < 0.1, "before {before}, after {after}");
    assert_finite(&simulation);
}

#[test]
fn a_gust_blows_all_the_snow_away() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    let none = HashSet::new();
    let window = [surface(7, 300.0, 600.0, 500.0)];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    assert_eq!(piles(&simulation).len(), 2);

    simulation.blow_away(1.0, false);
    run(&mut simulation, 1.5, AWAY);
    assert!(!simulation.has_snow(), "drifts left: {:?}", piles(&simulation));
    run(&mut simulation, 3.0, AWAY);
    // The new snowfall starts above the screen and hasn't reached the ground yet.
    assert!(!simulation.has_snow());
    let mut sprites = vec![Sprite::default(); 20_000];
    let count = simulation.write_sprites(&mut sprites);
    assert!(sprites[..count].iter().all(|sprite| sprite.position.y > 400.0), "snow left in the lower half");
    run(&mut simulation, 60.0, AWAY);
    assert!(simulation.has_snow(), "the snowfall didn't come back");
    assert_finite(&simulation);
}

#[test]
fn a_screen_rests_only_once_its_snow_is_gone_and_none_is_coming() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.5);
    simulation.thaws = true;
    let none = HashSet::new();
    simulation.update_windows(&[surface(7, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 30.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 1.0, AWAY);
    assert!(!simulation.is_empty(), "snow still falls and lies");

    simulation.blow_away(1.0, false);
    run(&mut simulation, 30.0, AWAY);
    assert!(simulation.is_empty(), "sprites {}, snow {}", simulation.sprite_count(), simulation.has_snow());
    assert!(!simulation.step(DT, AWAY), "a screen at rest has nothing to redraw");

    simulation.set_halloween(true);
    assert!(!simulation.is_empty(), "Halloween's pumpkins are coming");
    simulation.set_halloween(false);
    simulation.set_intensity(0.25);
    assert!(!simulation.is_empty(), "the snowfall is coming");
}

#[test]
fn a_calm_gust_melts_the_drifts_where_they_lie() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    let before = piles(&simulation)[0].1;
    let falling = simulation.sprite_count();
    simulation.blow_away(-1.0, true);
    simulation.step(DT, AWAY);
    assert!(simulation.sprite_count() <= falling, "a calm gust shouldn't throw anything up");
    run(&mut simulation, 2.0, AWAY);
    // Only the last of the snowfall, still landing, is left.
    let after = piles(&simulation).first().map_or(0.0, |p| p.1);
    assert!(after < before * 0.05, "before {before}, after {after}");
}

#[test]
fn a_sliding_desktop_skims_the_drifts_without_blowing_them_away() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 15.0, AWAY);
    let before = piles(&simulation)[0].1;
    let flying = simulation.sprite_count();

    simulation.space_slid(-9000.0);
    run(&mut simulation, 0.2, AWAY);
    assert!(simulation.sprite_count() > flying + 20, "no powder off the crests");
    assert_eq!(simulation.pace(), Pace::Lively);
    run(&mut simulation, 4.0, AWAY);
    let after = piles(&simulation)[0].1;
    // The last of the snowfall is still landing meanwhile.
    assert!((after - before).abs() < before * 0.05, "before {before}, after {after}");
    assert_eq!(simulation.pace(), Pace::Calm);
    assert_finite(&simulation);
}

#[test]
fn snow_settles_on_a_window_and_drops_when_it_closes() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    let none = HashSet::new();
    let window = [surface(7, 300.0, 600.0, 500.0)];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    let layers = piles(&simulation);
    assert_eq!(layers.len(), 2, "no snow on the window");
    let ledge = layers.iter().find(|(origin, _)| origin.y > 500.0).expect("ledge");
    assert!(ledge.1 > 200.0, "ledge area {}", ledge.1);

    simulation.set_intensity(0.0);
    simulation.update_windows(&[], 0.2, &none, &none, false);
    simulation.step(DT, AWAY);
    assert!(simulation.sprite_count() > 20, "the window's snow didn't fall");
    run(&mut simulation, 5.0, AWAY);
    assert_eq!(piles(&simulation).len(), 1);
    assert_finite(&simulation);
}

#[test]
fn snow_falling_off_a_window_shows_at_the_falling_opacity() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    let none = HashSet::new();
    simulation.update_windows(&[surface(7, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 60.0, AWAY);
    simulation.set_intensity(0.0);
    simulation.falling_opacity = 0.4;
    simulation.update_windows(&[], 0.2, &none, &none, false);
    simulation.step(DT, AWAY);
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    assert!(count > 20, "the window's snow didn't fall");
    assert!(sprites[..count].iter().all(|sprite| sprite.alpha <= 0.4), "snow off the window ignores the opacity");
}

#[test]
fn snow_back_from_another_desktop_grows_into_shape() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    let none = HashSet::new();
    let window = [surface(7, 300.0, 600.0, 500.0)];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    run(&mut simulation, 60.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 2.0, AWAY);
    let ledge = |simulation: &SnowSimulation| piles(simulation).iter().find(|(origin, _)| origin.y > 500.0).map(|ledge| ledge.1);
    let full = ledge(&simulation).expect("snow on the window");

    simulation.update_windows(&[], 0.2, &none, &HashSet::from([7]), false);
    run(&mut simulation, 1.0, AWAY);
    assert_eq!(ledge(&simulation), None, "the snow stayed on screen while its window was away");
    simulation.update_windows(&window, 0.2, &none, &none, false);
    simulation.step(DT, AWAY);
    let first = ledge(&simulation).expect("the snow came back");
    assert!(first < full * 0.1, "came back whole at once: {first} of {full}");
    run(&mut simulation, 0.5, AWAY);
    let grown = ledge(&simulation).expect("the snow stayed");
    assert!((grown - full).abs() < full * 0.05, "grew to {grown} of {full}");
}

#[test]
fn dragged_window_sheds_its_snow() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 10.0, AWAY);
    let resting = piles(&simulation).iter().find(|(origin, _)| origin.y > 500.0).map(|p| p.1).unwrap();

    // Shaken hard from side to side, as when a window is dragged back and forth.
    shake(&mut simulation, 3);
    run(&mut simulation, 3.0, AWAY);
    let shaken = piles(&simulation).iter().find(|(origin, _)| origin.y > 500.0).map_or(0.0, |p| p.1);
    assert!(shaken < resting * 0.7, "resting {resting}, after shaking {shaken}");
    assert_finite(&simulation);
}

#[test]
fn scattered_window_starts_the_held_snowfall() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.5);
    simulation.set_snowfall_held(true);
    run(&mut simulation, 2.0, AWAY);
    assert_eq!(simulation.sprite_count(), 0, "snowfall should wait for the welcome window");

    let pieces: Vec<ScatterPiece> = (0..2000)
        .map(|i| ScatterPiece {
            position: V2::new(420.0 + (i % 70) as f32 * 6.0, 300.0 + (i / 70) as f32 * 6.0),
            velocity: V2::new(0.0, 60.0),
            color: [0.1, 0.09, 0.07],
        })
        .collect();
    simulation.scatter(&pieces);
    let mut released = false;
    for _ in 0..(10.0 / DT) as usize {
        simulation.step(DT, AWAY);
        released |= simulation.take_snowfall_released();
    }
    assert!(released, "the scattered snow never reached the ground");
    run(&mut simulation, 10.0, AWAY);
    assert!(simulation.sprite_count() > 200, "snowfall didn't start: {}", simulation.sprite_count());
    assert_finite(&simulation);
}

#[test]
fn a_window_slid_past_the_screen_edge_leaves_its_snow_heaped_against_it() {
    for (out, wall) in [(-300.0, 0.0), (1080.0, 1280.0)] {
        let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
        simulation.thaws = false;
        simulation.walls = (true, true);
        let none = HashSet::new();
        simulation.update_windows(&[surface(5, 400.0, 600.0, 500.0)], 0.2, &none, &none, false);
        run(&mut simulation, 90.0, AWAY);
        simulation.set_intensity(0.0);
        run(&mut simulation, 10.0, AWAY);
        let resting = ledge(&simulation);

        for step in 1..=180 {
            let x = 400.0 + (out - 400.0) * step as f32 / 180.0;
            simulation.update_windows(&[surface(5, x, 600.0, 500.0)], DT, &none, &none, false);
            simulation.step(DT, AWAY);
        }
        run(&mut simulation, 5.0, AWAY);
        let heaped = ledge(&simulation);
        // Three fifths of the window went past the wall, and their snow packs down once, to 0.6 of its bulk.
        assert!((heaped / resting - 0.76).abs() < 0.08, "resting {resting}, against the wall {heaped}");

        let mut buffer = vec![0.0; simulation.column_count()];
        let layers = simulation.write_layers(&mut buffer);
        let pile = layers.iter().find(|pile| pile.origin.y > 500.0).expect("ledge");
        let (surface, bed) = buffer[pile.offset..pile.offset + 2 * pile.count].split_at(pile.count);
        let depth = |i: usize| surface[i] - bed[i];
        let x = |i: usize| pile.origin.x + (i as f32 + 0.5) * 2.0;
        let past = (0..pile.count).filter(|&i| (x(i) - wall).abs() > 4.0 && (x(i) < 0.0 || x(i) > 1280.0)).map(depth).fold(0.0, f32::max);
        assert!(past < 0.01, "snow left past the edge at {wall}: {past}");
        let deepest = (0..pile.count).max_by(|&a, &b| depth(a).total_cmp(&depth(b))).unwrap();
        assert!((x(deepest) - wall).abs() < 30.0, "deepest snow at {}, the wall at {wall}", x(deepest));
        assert_finite(&simulation);
    }
}

#[test]
fn a_window_dragged_to_the_next_display_takes_its_snow_along() {
    // Two displays side by side, each with a simulation of its own; the window belongs to the one that
    // holds most of its top edge.
    let (width, window) = (1280.0, 500.0);
    let none = HashSet::new();
    let mut left = SnowSimulation::new(width, 800.0, 0.75);
    let mut right = SnowSimulation::new(width, 800.0, 0.0);
    left.thaws = false;
    right.thaws = false;
    left.walls = (true, false);
    right.walls = (false, true);
    left.update_windows(&[surface(4, 300.0, 600.0, window)], 0.2, &none, &none, false);
    run(&mut left, 90.0, AWAY);
    left.set_intensity(0.0);
    run(&mut left, 10.0, AWAY);
    let resting = ledge(&left);

    let mut on_right = false;
    for step in 0..=240 {
        // Eased in and out, as a hand drags, from x = 300 on the left display to x = 1500.
        let t = step as f32 / 240.0;
        let x = 300.0 + 1200.0 * t * t * (3.0 - 2.0 * t);
        if !on_right && x + window / 2.0 > width {
            on_right = true;
            let snow = left.take_window(4).expect("snow on the window");
            right.adopt_window(4, snow, Mapping::shift(V2::new(-width, 0.0)));
        }
        if on_right {
            right.update_windows(&[surface(4, x - width, 600.0, window)], DT, &none, &none, false);
            left.update_windows(&[], DT, &none, &none, false);
        } else {
            left.update_windows(&[surface(4, x, 600.0, window)], DT, &none, &none, false);
        }
        left.step(DT, AWAY);
        right.step(DT, AWAY);
    }
    run(&mut right, 3.0, AWAY);
    let arrived = ledge(&right);
    assert!(arrived > resting * 0.5, "resting {resting}, arrived {arrived}");
    assert_eq!(ledge(&left), 0.0, "snow left behind on the first display");
    assert_finite(&left);
    assert_finite(&right);
}

/// Two displays side by side, each with a simulation of its own, passing snow between them as the Linux app does:
/// the left one keeps the snow of window 4, which reaches over onto the right one.
struct Pair {
    left: SnowSimulation,
    right: SnowSimulation,
}

const SEAM: f32 = 1280.0;

impl Pair {
    fn new(intensity: f32) -> Pair {
        let mut left = SnowSimulation::new(SEAM, 800.0, intensity);
        let mut right = SnowSimulation::new(SEAM, 800.0, intensity);
        for (simulation, left_side) in [(&mut left, false), (&mut right, true)] {
            simulation.thaws = false;
            simulation.walls = (!left_side, left_side);
            simulation.neighbours = (left_side, !left_side);
        }
        Pair { left, right }
    }

    fn place(&mut self, x: f32) {
        let none = HashSet::new();
        self.left.update_windows(&[surface(4, x, 600.0, 600.0)], DT, &none, &none, false);
    }

    fn step(&mut self) {
        let over = V2::new(-SEAM, 0.0);
        self.left.step(DT, AWAY);
        self.right.set_borrowed(self.left.lend(4).map(|lent| lent.mapped(Mapping::shift(over))).into_iter().collect());
        self.right.step(DT, AWAY);
        for caught in self.right.take_caught() {
            self.left.catch(&caught, Mapping::shift(V2::new(SEAM, 0.0)));
        }
        let strays = self.left.take_strays();
        self.right.adopt_strays(strays, Mapping::shift(over));
        let strays = self.right.take_strays();
        self.left.adopt_strays(strays, Mapping::shift(V2::new(SEAM, 0.0)));
    }

    fn run(&mut self, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            self.step();
        }
    }

    /// How deep the window's snow lies on average, left of the seam and right of it.
    fn halves(&self) -> (f32, f32) {
        let mut buffer = vec![0.0; self.left.column_count()];
        let piles = self.left.write_layers(&mut buffer);
        let Some(pile) = piles.iter().find(|pile| pile.origin.y > 500.0) else { return (0.0, 0.0) };
        let (surface, bed) = buffer[pile.offset..pile.offset + 2 * pile.count].split_at(pile.count);
        let (mut sums, mut counts) = ([0.0; 2], [0.0; 2]);
        for (i, (s, b)) in surface.iter().zip(bed).enumerate() {
            let side = usize::from(pile.origin.x + (i as f32 + 0.5) * 2.0 > SEAM);
            sums[side] += s - b;
            counts[side] += 1.0;
        }
        (sums[0] / counts[0], sums[1] / counts[1])
    }
}

fn ground(simulation: &SnowSimulation) -> f32 {
    piles(simulation).iter().find(|(origin, _)| origin.y == 0.0).map_or(0.0, |p| p.1)
}

#[test]
fn snow_settles_on_both_sides_of_a_window_across_two_displays() {
    let mut pair = Pair::new(0.75);
    pair.place(980.0);
    pair.run(60.0);
    let (left, right) = pair.halves();
    assert!(left > 2.0, "left half {left}");
    assert!(right > left * 0.6, "left half {left}, right half {right}");
    assert_finite(&pair.left);
    assert_finite(&pair.right);
}

#[test]
fn snow_shaken_off_over_the_next_display_falls_there() {
    let mut pair = Pair::new(0.75);
    pair.place(980.0);
    pair.run(60.0);
    pair.left.set_intensity(0.0);
    pair.right.set_intensity(0.0);
    pair.run(30.0);
    let before = ground(&pair.right);
    let (_, right) = pair.halves();
    let mut t = 0.0;
    for _ in 0..120 {
        t += DT;
        pair.place(980.0 + 100.0 * (2.0 * std::f32::consts::PI * 4.0 * t).sin());
        pair.step();
    }
    pair.place(980.0);
    pair.run(5.0);
    let fallen = ground(&pair.right) - before;
    // The right half held about `right` deep over 300 points of the window.
    assert!(fallen > right * 300.0 * 0.15, "fell {fallen} of about {}", right * 300.0);
    assert_finite(&pair.left);
    assert_finite(&pair.right);
}

#[test]
fn a_window_handed_to_a_display_with_a_clock_of_its_own_keeps_its_snow_in_sight() {
    let none = HashSet::new();
    let mut left = SnowSimulation::new(SEAM, 800.0, 0.75);
    let mut right = SnowSimulation::new(SEAM, 800.0, 0.0);
    left.thaws = false;
    let window = [surface(4, 300.0, 600.0, 500.0)];
    left.update_windows(&window, 0.2, &none, &none, false);
    run(&mut left, 60.0, AWAY);
    left.set_intensity(0.0);
    run(&mut left, 10.0, AWAY);
    // Back from another desktop, the snow grows in again by the left display's clock, 70 seconds ahead.
    left.update_windows(&[], 0.2, &none, &HashSet::from([4]), false);
    left.step(DT, AWAY);
    left.update_windows(&window, 0.2, &none, &none, false);
    run(&mut left, 1.0, AWAY);
    let resting = ledge(&left);

    let snow = left.take_window(4).expect("snow on the window");
    right.adopt_window(4, snow, Mapping::shift(V2::new(-SEAM, 0.0)));
    right.update_windows(&[surface(4, 300.0 - SEAM, 600.0, 500.0)], 0.2, &none, &none, false);
    right.step(DT, AWAY);
    let arrived = ledge(&right);
    assert!(arrived > resting * 0.9, "resting {resting}, arrived {arrived}");
}

/// A display at 150% beside one at 100%, as Windows lays them out: the left one is 1280 points across on 1920 pixels,
/// the right one 1920 points, both 1200 pixels tall.
const LEFT_TO_RIGHT: Mapping = Mapping { scale: 1.5, shift: V2::new(-1920.0, 0.0) };
const RIGHT_TO_LEFT: Mapping = Mapping { scale: 1.0 / 1.5, shift: V2::new(1280.0, 0.0) };

fn mixed_pair() -> (SnowSimulation, SnowSimulation) {
    let mut left = SnowSimulation::new(1280.0, 800.0, 0.75);
    let mut right = SnowSimulation::new(1920.0, 1200.0, 0.75);
    for (simulation, left_side) in [(&mut left, false), (&mut right, true)] {
        simulation.thaws = false;
        simulation.walls = (!left_side, left_side);
        simulation.neighbours = (left_side, !left_side);
    }
    (left, right)
}

/// The smoothed surface of the window's drift at `x`, from the pile drawn at that height.
fn surface_at(simulation: &SnowSimulation, x: f32, above: f32) -> f32 {
    let mut buffer = vec![0.0; simulation.column_count()];
    let piles = simulation.write_layers(&mut buffer);
    let pile =
        piles.iter().find(|pile| pile.origin.y > above && x >= pile.origin.x && x < pile.origin.x + 2.0 * pile.count as f32).expect("a drift there");
    let i = ((x - pile.origin.x) / 2.0) as usize;
    pile.origin.y + buffer[pile.offset + i]
}

#[test]
fn a_drift_lent_to_a_display_of_other_pixels_meets_the_rest_at_the_edge() {
    let none = HashSet::new();
    let (mut left, mut right) = mixed_pair();
    left.update_windows(&[surface(4, 980.0, 600.0, 600.0)], 0.2, &none, &none, false);
    run(&mut left, 60.0, AWAY);
    right.set_borrowed(left.lend(4).map(|lent| lent.mapped(LEFT_TO_RIGHT)).into_iter().collect());
    right.step(DT, AWAY);
    // The same spot on the screen either side of the edge, 3 pixels apart.
    let here = surface_at(&left, 1279.0, 500.0) * 1.5;
    let there = surface_at(&right, 1.5, 800.0);
    assert!((here - there).abs() < 3.0, "left side {here}, right side {there} in pixels up from the bottom");
}

#[test]
fn snow_settles_on_a_window_across_displays_of_other_pixels() {
    let none = HashSet::new();
    let (mut left, mut right) = mixed_pair();
    left.update_windows(&[surface(4, 980.0, 600.0, 600.0)], 0.2, &none, &none, false);
    for _ in 0..(60.0 / DT) as usize {
        left.step(DT, AWAY);
        right.set_borrowed(left.lend(4).map(|lent| lent.mapped(LEFT_TO_RIGHT)).into_iter().collect());
        right.step(DT, AWAY);
        for caught in right.take_caught() {
            left.catch(&caught, RIGHT_TO_LEFT);
        }
    }
    let mut buffer = vec![0.0; left.column_count()];
    let piles = left.write_layers(&mut buffer);
    let pile = piles.iter().find(|pile| pile.origin.y > 500.0).expect("snow on the window");
    let (mut sums, mut counts) = ([0.0; 2], [0.0; 2]);
    for i in 0..pile.count {
        let side = usize::from(pile.origin.x + (i as f32 + 0.5) * 2.0 > 1280.0);
        sums[side] += buffer[pile.offset + i] - buffer[pile.offset + pile.count + i];
        counts[side] += 1.0;
    }
    let (near, far) = (sums[0] / counts[0], sums[1] / counts[1]);
    // The right display snows as thickly per point of its own, two thirds as deep in the left one's points.
    assert!(near > 2.0 && far > near * 0.4, "left half {near}, right half {far}");
    assert_finite(&left);
    assert_finite(&right);
}

#[test]
fn a_window_dragged_to_a_display_of_other_pixels_keeps_its_snow() {
    let none = HashSet::new();
    let (mut left, mut right) = mixed_pair();
    left.update_windows(&[surface(4, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut left, 60.0, AWAY);
    left.set_intensity(0.0);
    right.set_intensity(0.0);
    run(&mut left, 10.0, AWAY);
    let resting = ledge(&left);
    let snow = left.take_window(4).expect("snow on the window");
    right.adopt_window(4, snow, LEFT_TO_RIGHT);
    // On the new display the window keeps its size in points, as Windows scales it on the way.
    let top = LEFT_TO_RIGHT.point(V2::new(300.0, 600.0));
    right.update_windows(&[surface(4, top.x, top.y, 500.0)], DT, &none, &none, false);
    run(&mut right, 3.0, AWAY);
    let arrived = piles(&right).iter().find(|(origin, _)| origin.y > 800.0).map_or(0.0, |p| p.1);
    assert!(arrived > resting * 0.9, "resting {resting}, arrived {arrived}");
    assert_finite(&right);
}

fn shake(simulation: &mut SnowSimulation, id: isize) {
    let none = HashSet::new();
    let mut t = 0.0;
    for _ in 0..120 {
        t += DT;
        let x = 300.0 + 150.0 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
        simulation.update_windows(&[surface(id, x, 600.0, 500.0)], DT, &none, &none, false);
        simulation.step(DT, AWAY);
    }
}

fn ledge(simulation: &SnowSimulation) -> f32 {
    piles(simulation).iter().find(|(origin, _)| origin.y > 500.0).map_or(0.0, |p| p.1)
}

/// The same scenarios as the Swift harness, to compare the port against the original.
#[test]
#[ignore]
fn compare_with_swift() {
    let none = HashSet::new();
    let runs = 6;
    let (mut ground, mut ledge_area, mut dropped, mut shake_ratio, mut sweep_peak, mut sweep_ratio) = (0.0, 0.0, 0, 0.0, 0, 0.0);
    for _ in 0..runs {
        let mut a = SnowSimulation::new(1280.0, 800.0, 0.75);
        a.thaws = true;
        run(&mut a, 60.0, AWAY);
        ground += piles(&a)[0].1;

        let mut b = SnowSimulation::new(1280.0, 800.0, 0.75);
        b.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
        run(&mut b, 90.0, AWAY);
        b.set_intensity(0.0);
        run(&mut b, 10.0, AWAY);
        let resting = ledge(&b);
        ledge_area += resting;
        shake(&mut b, 3);
        run(&mut b, 3.0, AWAY);
        shake_ratio += ledge(&b) / resting;
        b.update_windows(&[surface(3, 300.0, 600.0, 500.0)], DT, &none, &none, false);
        run(&mut b, 3.0, AWAY);
        b.update_windows(&[], 0.2, &none, &none, false);
        b.step(DT, AWAY);
        dropped += b.sprite_count();

        let mut c = SnowSimulation::new(1280.0, 800.0, 0.75);
        run(&mut c, 120.0, AWAY);
        c.set_intensity(0.0);
        run(&mut c, 15.0, AWAY);
        let before = piles(&c)[0].1;
        let mut peak = 0;
        for i in 0..120 {
            c.step(DT, V2::new(200.0 + i as f32 * 8.0, 10.0));
            peak = peak.max(c.sprite_count());
        }
        run(&mut c, 10.0, AWAY);
        sweep_peak += peak;
        sweep_ratio += piles(&c)[0].1 / before;
    }
    let n = runs as f32;
    println!(
        "ground60 {:.0}  ledge90 {:.0}  shakeLeft {:.2}  dropped {}  sweepPeak {}  sweepKept {:.3}",
        ground / n,
        ledge_area / n,
        shake_ratio / n,
        dropped / runs,
        sweep_peak / runs,
        sweep_ratio / n
    );
}

#[test]
fn survives_a_long_chaotic_session() {
    let mut simulation = SnowSimulation::new(1920.0, 1080.0, 0.5);
    let none = HashSet::new();
    let mut seed = 12345u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 40) as f32 / (1u32 << 24) as f32
    };
    let mut windows: Vec<(isize, f32, f32, f32)> = Vec::new();
    let mut cursor = V2::new(500.0, 100.0);
    for frame in 0..40_000 {
        if frame % 30 == 0 {
            let roll = next();
            if roll < 0.2 && windows.len() < 6 {
                windows.push((frame as isize + 1, next() * 1600.0 - 100.0, 200.0 + next() * 850.0, 60.0 + next() * 900.0));
            } else if roll < 0.3 && !windows.is_empty() {
                windows.remove((next() * windows.len() as f32) as usize % windows.len());
            }
        }
        for window in &mut windows {
            if next() < 0.05 {
                window.1 += (next() - 0.5) * 400.0;
                window.2 += (next() - 0.5) * 200.0;
            }
            if next() < 0.01 {
                window.3 = 20.0 + next() * 1200.0;
            }
        }
        let surfaces: Vec<WindowSurface> = windows.iter().map(|&(id, x, top, width)| surface(id, x, top, width)).collect();
        simulation.update_windows(&surfaces, DT, &none, &none, false);
        if next() < 0.3 {
            cursor = cursor + V2::new((next() - 0.5) * 300.0, (next() - 0.5) * 200.0);
        }
        if frame % 5000 == 0 {
            simulation.thaws = !simulation.thaws;
            simulation.accumulates = frame % 15000 != 0;
        }
        if frame == 20_000 {
            simulation.resize(1280.0, 720.0);
        }
        let dt = if next() < 0.01 { 0.05 } else { DT };
        simulation.step(dt, cursor);
    }
    assert_finite(&simulation);
}

#[test]
fn snow_falls_off_a_lowered_ground_and_rides_up_on_a_raised_one() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.thaws = false;
    simulation.update_windows(&[surface(1, 200.0, 400.0, 300.0)], 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 15.0, AWAY);
    // An empty layer isn't drawn, so it isn't among the piles.
    let ground = |simulation: &SnowSimulation| piles(simulation).iter().find(|pile| pile.0.y == 0.0).map_or(0.0, |pile| pile.1);
    let window = |simulation: &SnowSimulation| piles(simulation).iter().find(|pile| pile.0.y > 0.0).map_or(0.0, |pile| pile.1);
    let before = ground(&simulation);
    let on_window = window(&simulation);
    assert!(before > 500.0, "no drift on the ground: {before}");
    assert!(on_window > 100.0, "no snow on the window: {on_window}");

    simulation.lower_ground(800.0, 640.0, 40.0);
    // The window stays where it was on the screen, now 40 points further up from the new ground, and its snow with it.
    let lifted = piles(&simulation).iter().find(|pile| pile.0.y > 0.0).map(|pile| pile.0.y);
    assert_eq!(lifted, Some(440.0), "the window's snow didn't stay where it was on the screen");
    simulation.update_windows(&[surface(1, 200.0, 440.0, 300.0)], 1.0 / 60.0, &none, &none, false);
    assert!(ground(&simulation) < 1.0, "the drift stayed on the ground as it dropped");
    assert!(simulation.sprite_count() > 0, "nothing fell");
    run(&mut simulation, 10.0, AWAY);
    let after = ground(&simulation);
    assert!(after > before * 0.6, "the snow didn't land on the new ground: {before} -> {after}");
    let kept = window(&simulation);
    assert!(kept > on_window * 0.9, "the window's snow went with the ground: {on_window} -> {kept}");

    // And back: the drift comes up with the ground onto the taskbar, the window's snow staying put.
    let landed = ground(&simulation);
    simulation.raise_ground(800.0, 600.0, 40.0);
    let lowered = piles(&simulation).iter().find(|pile| pile.0.y > 0.0).map(|pile| pile.0.y);
    assert_eq!(lowered, Some(400.0), "the window's snow didn't stay where it was on the screen");
    assert!(ground(&simulation) > landed * 0.99, "the drift didn't come up with the ground: {landed} -> {}", ground(&simulation));
    simulation.update_windows(&[surface(1, 200.0, 400.0, 300.0)], 1.0 / 60.0, &none, &none, false);
    run(&mut simulation, 5.0, AWAY);
    assert!(window(&simulation) > kept * 0.9, "the window's snow went with the ground");
    assert_finite(&simulation);
}

#[test]
fn a_panel_given_back_takes_up_the_drift_under_it() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.thaws = false;
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 15.0, AWAY);
    let lying = piles(&simulation)[0].1;
    assert!(lying > 500.0, "no drift on the ground: {lying}");

    // The panel comes back along the bottom, 40 points tall, over the drift.
    simulation.lift_onto(9);
    simulation.update_windows(&[surface(9, 0.0, 40.0, 800.0)], 1.0 / 60.0, &none, &none, false);
    let on_panel = piles(&simulation).iter().find(|pile| pile.0.y > 0.0).map_or(0.0, |pile| pile.1);
    let left = piles(&simulation).iter().find(|pile| pile.0.y == 0.0).map_or(0.0, |pile| pile.1);
    assert!(on_panel > lying * 0.9, "the drift didn't come up on the panel: {lying} -> {on_panel}");
    assert!(left < lying * 0.1, "the drift stayed under the panel: {left}");
    assert_finite(&simulation);
}

#[test]
#[ignore]
fn step_cost() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(1920.0, 1080.0, 0.5);
    simulation.thaws = true;
    let window = [WindowSurface {
        id: 1,
        frame: Rect { x: 200.0, y: 300.0, width: 900.0, height: 500.0 },
        blockers: Vec::new(),
        cover: Vec::new(),
        corner_radius: 8.0,
        corner_exponent: 2.5,
    }];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    for _ in 0..60 * 30 {
        simulation.step(1.0 / 30.0, AWAY);
    }
    let steps = 30 * 120;
    let start = std::time::Instant::now();
    for i in 0..steps {
        simulation.step(1.0 / 30.0, V2::new((i % 1900) as f32, 20.0));
    }
    let us = start.elapsed().as_secs_f64() * 1e6 / steps as f64;
    println!("rust: {us:.1} µs per step, {} sprites", simulation.sprite_count());
}

#[test]
fn halloween_runs_from_october_30_to_dawn_on_november_1() {
    use crate::halloween::is_halloween;
    assert!(!is_halloween(10, 29, 23));
    assert!(is_halloween(10, 30, 0));
    assert!(is_halloween(10, 31, 0));
    assert!(is_halloween(10, 31, 23));
    assert!(is_halloween(11, 1, 5));
    assert!(!is_halloween(11, 1, 6));
    assert!(!is_halloween(12, 31, 22));
}

#[test]
fn halloween_comes_with_its_season_and_whenever_turned_on() {
    use crate::halloween::{Choice, Today};
    let season = Today { year: 2027, season: true };
    let later = Today { year: 2027, season: false };
    let next = Today { year: 2028, season: true };

    let fresh = Choice::default();
    assert!(!fresh.wanted(later));
    assert!(fresh.wanted(season));

    let skipped = fresh.toggled(season);
    assert!(!skipped.wanted(season));
    assert!(skipped.wanted(next), "turned off for one season, Halloween comes back the next");
    let back = skipped.toggled(season);
    assert!(back.wanted(season));
    assert!(!back.wanted(later), "turned back on in season, it ends with the season");

    let kept = fresh.toggled(later);
    assert!(kept.wanted(later) && kept.wanted(season));
    let dropped = kept.toggled(season);
    assert!(!dropped.wanted(season) && !dropped.wanted(later));
    assert!(!kept.toggled(later).wanted(later));
}

#[test]
fn halloween_moonlight_only_shows_on_halloween() {
    use crate::halloween::{moonlit, night};
    let light = [1.0, 0.98, 0.95];
    let tint = [0.5, 0.5, 0.5, 0.35];
    assert_eq!(moonlit(light, tint, night(false)), (light, tint));
    let (moon, violet) = moonlit(light, tint, night(true));
    assert!((moon[2] - 1.0).abs() < 1e-5 && moon[0] < 0.8);
    assert!((violet[3] - 0.6).abs() < 1e-5);
}

fn leaf_sprites(simulation: &SnowSimulation) -> usize {
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    sprites[..count].iter().filter(|sprite| sprite.leaf[0] > 0.0).count()
}

#[test]
fn leaves_lie_where_they_land() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.set_mode(Mode::Autumn);
    run(&mut simulation, 90.0, AWAY);
    assert_finite(&simulation);
    assert!(simulation.has_snow(), "no leaves lying");
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    assert!(sprites[..count].iter().all(|sprite| sprite.leaf[0] > 0.0), "snow among the leaves");
    assert!(sprites[..count].iter().any(|sprite| sprite.position.y < 30.0), "nothing on the ground");
}

#[test]
fn cherry_blossom_petals_fall_and_lie_as_leaves_do() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.set_mode(Mode::Blossom);
    assert_eq!(simulation.mode(), Mode::Blossom);
    assert!(simulation.leafy() && simulation.blossom());
    simulation.thaws = true;
    run(&mut simulation, 240.0, AWAY);
    assert_finite(&simulation);
    assert!(simulation.has_snow(), "no petals lying");
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    assert!(sprites[..count].iter().all(|sprite| sprite.leaf[0] > 0.0), "snow among the petals");
    assert!(sprites[..count].iter().any(|sprite| sprite.tint_amount > 0.0), "no petal withering");
    simulation.set_mode(Mode::Autumn);
    assert!(simulation.leafy() && !simulation.blossom());
}

#[test]
fn mixed_snow_and_leaves_fall_together_and_heap_up_mixed() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.set_modes(&[Mode::Snow, Mode::Autumn]);
    assert_eq!(simulation.modes(), [Mode::Snow, Mode::Autumn]);
    assert!(!simulation.leafy(), "snow among the leaves counts as snow");
    run(&mut simulation, 90.0, AWAY);
    assert_finite(&simulation);
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    let leaves = sprites[..count].iter().filter(|sprite| sprite.leaf[0] > 0.0).count();
    assert!(leaves > 0 && leaves < count, "{leaves} leaves among {count} sprites");
    let mut buffer = vec![0.0; simulation.column_count()];
    let layers = simulation.write_layers(&mut buffer);
    let ground = &layers[0];
    assert_eq!(simulation.column_count(), SnowLayer::ROWS * ground.count);
    let shares = &buffer[ground.offset + 2 * ground.count..ground.offset + 3 * ground.count];
    let mixed = shares.iter().filter(|&&share| share > 0.05 && share < 0.95).count();
    assert!(mixed > ground.count / 2, "only {mixed} of {} columns are snow and leaves mixed", ground.count);
}

#[test]
fn a_kind_joining_the_fall_starts_at_once_and_one_leaving_clears_the_screen() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    run(&mut simulation, 30.0, AWAY);
    simulation.switch_modes(&[Mode::Snow, Mode::Autumn], 1.0, false);
    assert_eq!(simulation.modes(), [Mode::Snow, Mode::Autumn], "a kind joining waited for a gust");
    assert!(simulation.has_snow(), "a kind joining cleared the snow");
    run(&mut simulation, 10.0, AWAY);
    assert!(leaf_sprites(&simulation) > 0, "no leaves falling");
    simulation.switch_modes(&[Mode::Autumn], 1.0, false);
    run(&mut simulation, 0.5, AWAY);
    assert_eq!(simulation.modes(), [Mode::Snow, Mode::Autumn], "switched before the gust cleared the screen");
    run(&mut simulation, 8.0, AWAY);
    assert_eq!(simulation.modes(), [Mode::Autumn], "never switched");
    assert!(simulation.leafy());
}

#[test]
fn unwithered_leaves_heap_no_deeper_than_snow() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.set_mode(Mode::Autumn);
    simulation.thaws = false;
    for _ in 0..15 * 60 * 30 {
        simulation.step(1.0 / 30.0, AWAY);
    }
    let mut buffer = vec![0.0; simulation.column_count()];
    let layers = simulation.write_layers(&mut buffer);
    let ground = &layers[0];
    let (surface, bed) = buffer[ground.offset..ground.offset + 2 * ground.count].split_at(ground.count);
    let deepest = surface.iter().zip(bed).map(|(s, b)| s - b).fold(0.0, f32::max);
    assert!(deepest < 60.0, "the leaves heaped {deepest} deep");
}

#[test]
fn a_closed_window_drops_only_its_own_leaves() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.set_mode(Mode::Autumn);
    let none = HashSet::new();
    for _ in 0..(120.0 / DT) as usize {
        simulation.update_windows(&[surface(7, 200.0, 400.0, 400.0)], DT, &none, &none, false);
        simulation.step(DT, AWAY);
    }
    simulation.set_intensity(0.0);
    run(&mut simulation, 30.0, AWAY);
    let before = leaf_sprites(&simulation);
    assert!(before > 10, "too few leaves on the window: {before}");
    simulation.update_windows(&[], DT, &none, &none, false);
    for _ in 0..30 {
        simulation.step(DT, AWAY);
        assert!(leaf_sprites(&simulation) <= before, "leaves out of nowhere");
    }
    let after = leaf_sprites(&simulation);
    assert!(after * 10 >= before * 9, "the leaves vanished with their window: {before} -> {after}");
}

#[test]
fn switching_modes_clears_the_screen_first() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    run(&mut simulation, 30.0, AWAY);
    simulation.switch_mode(Mode::Autumn, 1.0, false);
    run(&mut simulation, 0.5, AWAY);
    assert!(!simulation.leafy(), "switched before the gust cleared the screen");
    run(&mut simulation, 8.0, AWAY);
    assert!(simulation.leafy(), "never switched");
    run(&mut simulation, 10.0, AWAY);
    assert!(leaf_sprites(&simulation) > 0, "no leaves falling");
    assert_eq!(leaf_sprites(&simulation), simulation.sprite_count(), "snow left over");
}

fn jerk(simulation: &mut SnowSimulation, id: isize) {
    let none = HashSet::new();
    let mut t = 0.0;
    for _ in 0..120 {
        t += DT;
        let top = 650.0 + 50.0 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
        simulation.update_windows(&[surface(id, 300.0, top, 500.0)], DT, &none, &none, false);
        simulation.step(DT, AWAY);
    }
    simulation.update_windows(&[surface(id, 300.0, 650.0, 500.0)], DT, &none, &none, false);
}

#[test]
fn a_heap_jerked_off_its_window_comes_off_as_leaves() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    simulation.set_mode(Mode::Autumn);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 650.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 120.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 15.0, AWAY);
    let resting = ledge(&simulation);
    let before = leaf_sprites(&simulation);
    assert!(resting > 50.0, "no heap on the window: {resting}");

    jerk(&mut simulation, 3);
    let left = ledge(&simulation);
    let flying = leaf_sprites(&simulation);
    assert!(left < resting * 0.7, "the heap held on: {resting} -> {left}");
    assert!(flying > before, "the heap came off unseen: {before} leaves -> {flying}");
    assert_finite(&simulation);

    // Thrown about again and again, a heap shows the leaves buried in it, never more than its bulk is worth.
    for _ in 0..6 {
        run(&mut simulation, 30.0, AWAY);
        jerk(&mut simulation, 3);
    }
    run(&mut simulation, 30.0, AWAY);
    let worth = piles(&simulation).iter().map(|pile| pile.1).sum::<f32>() / (5.5 * 11.0);
    let shown = leaf_sprites(&simulation);
    assert!(shown as f32 <= worth * 1.2 + 10.0, "the leaves multiplied: {shown} shown for {worth} worth of bulk");
}

#[test]
fn leaves_stay_put_as_their_window_is_resized() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, 0.75);
    simulation.set_mode(Mode::Autumn);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_intensity(0.0);
    run(&mut simulation, 20.0, AWAY);
    let lying = |simulation: &SnowSimulation| -> Vec<V2> {
        let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
        let count = simulation.write_sprites(&mut sprites);
        sprites[..count].iter().filter(|sprite| sprite.leaf[0] > 0.0 && sprite.position.y > 590.0).map(|sprite| sprite.position).collect()
    };
    let before: Vec<V2> = lying(&simulation).into_iter().filter(|p| p.x > 460.0 && p.x < 780.0).collect();
    assert!(before.len() > 5, "too few leaves on the window: {}", before.len());
    // Where the heap's bulk lies along the screen, away from the end the window loses.
    let centre = |simulation: &SnowSimulation| -> f32 {
        let mut buffer = vec![0.0; simulation.column_count()];
        let piles = simulation.write_layers(&mut buffer);
        let pile = piles.iter().find(|pile| pile.origin.y > 500.0).unwrap();
        let width = (800.0 - pile.origin.x) / pile.count as f32;
        let (surface, bed) = buffer[pile.offset..pile.offset + 2 * pile.count].split_at(pile.count);
        let (mut sum, mut weight) = (0.0, 0.0);
        for i in 0..pile.count {
            let x = pile.origin.x + (i as f32 + 0.5) * width;
            if x > 450.0 && x < 780.0 {
                sum += x * (surface[i] - bed[i]);
                weight += surface[i] - bed[i];
            }
        }
        sum / weight
    };
    let bulk_before = centre(&simulation);

    // The left edge dragged in by a hundred points, the right one left where it is.
    let mut slid: f32 = 0.0;
    for step in 1..=30 {
        let left = 300.0 + 100.0 * step as f32 / 30.0;
        simulation.update_windows(&[surface(3, left, 600.0, 800.0 - left)], DT, &none, &none, false);
        simulation.step(DT, AWAY);
        let mut buffer = vec![0.0; simulation.column_count()];
        for pile in simulation.write_layers(&mut buffer) {
            if pile.origin.y > 500.0 {
                slid = slid.max((pile.origin.x - left).abs());
            }
        }
    }
    assert!(slid < 1.0, "the heap slid {slid} points over the window as it was resized");
    let walked = centre(&simulation) - bulk_before;
    assert!(walked.abs() < 3.0, "the heap walked {walked} points along as the window was resized");
    run(&mut simulation, 0.5, AWAY);
    let after = lying(&simulation);
    let kept = before.iter().filter(|p| after.iter().any(|q| (q.x - p.x).abs() < 3.0 && (q.y - p.y).abs() < 4.0)).count();
    assert!(kept * 10 >= before.len() * 8, "the leaves moved with the window's edge: {kept} of {} stayed", before.len());
}

#[test]
fn snow_pouring_off_a_window_heaps_up_no_higher_than_it_pours_from() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    simulation.thaws = false;
    let none = HashSet::new();
    // The upper window sits in front, over the top of the lower one; snow pours off its right end onto the lower one.
    let windows = || {
        let upper = surface(1, 100.0, 340.0, 300.0);
        let mut lower = surface(2, 200.0, 300.0, 500.0);
        lower.blockers = vec![upper.frame];
        [upper, lower]
    };
    for _ in 0..(900.0 / DT) as usize {
        simulation.update_windows(&windows(), DT, &none, &none, false);
        simulation.step(DT, AWAY);
    }
    let mut buffer = vec![0.0; simulation.column_count()];
    let piles = simulation.write_layers(&mut buffer);
    let lower = piles.iter().find(|pile| (pile.origin.y - 300.0).abs() < 5.0).unwrap();
    let upper = piles.iter().find(|pile| (pile.origin.y - 340.0).abs() < 5.0).unwrap();
    let pours = upper.origin.y + buffer[upper.offset + upper.count - 1];
    let width = 500.0 / lower.count as f32;
    let peak = (0..lower.count)
        .filter(|&i| (400.0..480.0).contains(&(lower.origin.x + (i as f32 + 0.5) * width)))
        .map(|i| lower.origin.y + buffer[lower.offset + i])
        .fold(f32::MIN, f32::max);
    assert!(peak <= pours + 3.0, "the heap under the window's end rose to {peak}, above where the snow pours from at {pours}");
}

fn rain_sprites(simulation: &SnowSimulation, kind: f32) -> Vec<Sprite> {
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    sprites[..count].iter().filter(|sprite| sprite.rain[0] == kind).copied().collect()
}

#[test]
fn rain_runs_down_a_window_and_hangs_under_its_bottom_edge() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(1200.0, 900.0, 0.75);
    simulation.set_mode(Mode::Rain);
    simulation.update_windows(&[surface(1, 200.0, 700.0, 800.0)], 0.2, &none, &none, false);
    let (mut ran, mut hung) = (false, false);
    for _ in 0..(40.0 / DT) as usize {
        simulation.step(DT, AWAY);
        ran |= !rain_sprites(&simulation, 2.0).is_empty();
        for drop in rain_sprites(&simulation, 3.0) {
            hung = true;
            assert!((400.0..410.0).contains(&drop.position.y), "hanging off the edge at {}", drop.position.y);
            assert!((0.0..=2.0).contains(&drop.rain[1]), "hanging stage {}", drop.rain[1]);
        }
    }
    assert!(ran, "no drop ran down the window");
    assert!(hung, "no drop hung under the window");
    assert_finite(&simulation);
}

#[test]
fn rain_spatters_the_glass_and_leaves_wet_uneven_trails() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(1200.0, 900.0, 0.75);
    simulation.set_mode(Mode::Rain);
    simulation.update_windows(&[surface(1, 200.0, 700.0, 800.0)], 0.2, &none, &none, false);
    let mut filmed = false;
    let mut beads: Vec<f32> = Vec::new();
    for _ in 0..(30.0 / DT) as usize {
        simulation.step(DT, AWAY);
        filmed |= !rain_sprites(&simulation, 5.0).is_empty();
        beads.extend(rain_sprites(&simulation, 2.0).iter().filter(|drop| drop.velocity == V2::ZERO).map(|bead| bead.size));
    }
    let spray = rain_sprites(&simulation, 4.0);
    assert!(spray.len() > 200, "only {} droplets of spray", spray.len());
    for droplet in &spray {
        assert!((660.0..700.0).contains(&droplet.position.y), "spray at {}, not just under the top edge", droplet.position.y);
    }
    assert!(filmed, "no drop left a film behind it");
    let smallest = beads.iter().copied().fold(f32::MAX, f32::min);
    let biggest = beads.iter().copied().fold(0.0, f32::max);
    assert!(biggest > smallest * 2.0, "beads all about one size, {smallest} to {biggest}");
    assert!(rain_sprites(&simulation, 2.0).iter().all(|drop| drop.drop[0] > 0.0), "a drop on the glass without an outline of its own");
    assert!(simulation.has_glass_drops(), "nothing on the glass to draw as glass");
    assert_finite(&simulation);
}

#[test]
fn switching_to_rain_clears_the_snow_first() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, 0.75);
    run(&mut simulation, 30.0, AWAY);
    simulation.switch_mode(Mode::Rain, 1.0, false);
    run(&mut simulation, 0.5, AWAY);
    assert_eq!(simulation.mode(), Mode::Snow, "switched before the gust cleared the screen");
    run(&mut simulation, 8.0, AWAY);
    assert_eq!(simulation.mode(), Mode::Rain, "never switched");
    run(&mut simulation, 5.0, AWAY);
    let streaks = rain_sprites(&simulation, 1.0).len();
    assert!(streaks > 0, "no rain falling");
    assert_eq!(streaks, simulation.sprite_count(), "snow left over");
}

#[test]
fn every_language_has_every_text() {
    use crate::text::{LOCALES, parse};
    let english = parse(LOCALES[0].1);
    assert!(english.len() > 50);
    let placeholders =
        |text: &str| text.split('{').skip(1).filter_map(|rest| rest.split_once('}')).map(|(name, _)| name.to_string()).collect::<HashSet<_>>();
    for (code, text) in LOCALES {
        let table = parse(text);
        let mut missing: Vec<_> = english.keys().filter(|key| !table.contains_key(*key)).collect();
        let mut extra: Vec<_> = table.keys().filter(|key| !english.contains_key(*key)).collect();
        missing.sort();
        extra.sort();
        assert!(missing.is_empty() && extra.is_empty(), "{code}: missing {missing:?}, extra {extra:?}");
        for (key, value) in &table {
            assert!(!value.trim().is_empty(), "{code}: {key} is empty");
            assert_eq!(placeholders(value), placeholders(&english[key]), "{code}: {key} placeholders");
        }
    }
}

#[test]
fn reads_strings_tables() {
    let table = crate::text::parse("/* a \"note\" */\n\"a\" = \"one \\\"two\\\"\\nthree\";\n// \"b\" = \"no\";\n\"c\" = \"x\";");
    assert_eq!(table.len(), 2);
    assert_eq!(table["a"], "one \"two\"\nthree");
    assert_eq!(table["c"], "x");
}

#[test]
fn picks_a_language_from_system_tags() {
    use crate::text::language;
    let code = |tag: &str| language(tag).map(|(code, _)| code);
    assert_eq!(code("ru_RU.UTF-8"), Some("ru"));
    assert_eq!(code("uk-UA"), Some("uk"));
    assert_eq!(code("pt-BR"), Some("pt"));
    assert_eq!(code("de_CH@euro"), Some("de"));
    assert_eq!(code("zh-Hans-CN"), Some("zh-Hans"));
    assert_eq!(code("zh_CN"), Some("zh-Hans"));
    assert_eq!(code("zh-Hant-TW"), None);
    assert_eq!(code("zh_TW"), None);
    assert_eq!(code("C"), None);
    assert_eq!(code("nl-NL"), None);
}
