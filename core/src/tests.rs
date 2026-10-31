use std::collections::HashSet;

use crate::geometry::{Rect, V2};
use crate::simulation::{Pace, ScatterPiece, SnowSimulation, Snowfall, Sprite, WindowSurface};

const DT: f32 = 1.0 / 60.0;
const AWAY: V2 = V2::new(-10_000.0, -10_000.0);

fn run(simulation: &mut SnowSimulation, seconds: f32, cursor: V2) {
    for _ in 0..(seconds / DT) as usize {
        simulation.step(DT, cursor);
    }
}

fn surface(id: isize, x: f32, top: f32, width: f32) -> WindowSurface {
    WindowSurface { id, frame: Rect { x, y: top - 300.0, width, height: 300.0 }, blockers: Vec::new(), corner_radius: 8.0, corner_exponent: 2.5 }
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Off);
    run(&mut simulation, 30.0, AWAY);
    assert_eq!(simulation.sprite_count(), 0);
    assert!(!simulation.has_snow());
}

#[test]
fn cursor_sweeps_the_drift_aside() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    simulation.thaws = false;
    run(&mut simulation, 120.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
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
fn a_calm_gust_melts_the_drifts_where_they_lie() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    let none = HashSet::new();
    let window = [surface(7, 300.0, 600.0, 500.0)];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    let layers = piles(&simulation);
    assert_eq!(layers.len(), 2, "no snow on the window");
    let ledge = layers.iter().find(|(origin, _)| origin.y > 500.0).expect("ledge");
    assert!(ledge.1 > 200.0, "ledge area {}", ledge.1);

    simulation.set_snowfall(Snowfall::Off);
    simulation.update_windows(&[], 0.2, &none, &none, false);
    simulation.step(DT, AWAY);
    assert!(simulation.sprite_count() > 20, "the window's snow didn't fall");
    run(&mut simulation, 5.0, AWAY);
    assert_eq!(piles(&simulation).len(), 1);
    assert_finite(&simulation);
}

#[test]
fn snow_back_from_another_desktop_grows_into_shape() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    let none = HashSet::new();
    let window = [surface(7, 300.0, 600.0, 500.0)];
    simulation.update_windows(&window, 0.2, &none, &none, false);
    run(&mut simulation, 60.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Steady);
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
        let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
        simulation.thaws = false;
        simulation.walls = (true, true);
        let none = HashSet::new();
        simulation.update_windows(&[surface(5, 400.0, 600.0, 500.0)], 0.2, &none, &none, false);
        run(&mut simulation, 90.0, AWAY);
        simulation.set_snowfall(Snowfall::Off);
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
    let mut left = SnowSimulation::new(width, 800.0, Snowfall::Heavy);
    let mut right = SnowSimulation::new(width, 800.0, Snowfall::Off);
    left.thaws = false;
    right.thaws = false;
    left.walls = (true, false);
    right.walls = (false, true);
    left.update_windows(&[surface(4, 300.0, 600.0, window)], 0.2, &none, &none, false);
    run(&mut left, 90.0, AWAY);
    left.set_snowfall(Snowfall::Off);
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
            right.adopt_window(4, snow, V2::new(-width, 0.0));
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
    fn new(snowfall: Snowfall) -> Pair {
        let mut left = SnowSimulation::new(SEAM, 800.0, snowfall);
        let mut right = SnowSimulation::new(SEAM, 800.0, snowfall);
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
        self.right.set_borrowed(self.left.lend(4).map(|lent| lent.shifted(over)).into_iter().collect());
        self.right.step(DT, AWAY);
        for caught in self.right.take_caught() {
            self.left.catch(&caught, SEAM);
        }
        let strays = self.left.take_strays();
        self.right.adopt_strays(strays, over);
        let strays = self.right.take_strays();
        self.left.adopt_strays(strays, V2::new(SEAM, 0.0));
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
    let mut pair = Pair::new(Snowfall::Heavy);
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
    let mut pair = Pair::new(Snowfall::Heavy);
    pair.place(980.0);
    pair.run(60.0);
    pair.left.set_snowfall(Snowfall::Off);
    pair.right.set_snowfall(Snowfall::Off);
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
    let mut left = SnowSimulation::new(SEAM, 800.0, Snowfall::Heavy);
    let mut right = SnowSimulation::new(SEAM, 800.0, Snowfall::Off);
    left.thaws = false;
    let window = [surface(4, 300.0, 600.0, 500.0)];
    left.update_windows(&window, 0.2, &none, &none, false);
    run(&mut left, 60.0, AWAY);
    left.set_snowfall(Snowfall::Off);
    run(&mut left, 10.0, AWAY);
    // Back from another desktop, the snow grows in again by the left display's clock, 70 seconds ahead.
    left.update_windows(&[], 0.2, &none, &HashSet::from([4]), false);
    left.step(DT, AWAY);
    left.update_windows(&window, 0.2, &none, &none, false);
    run(&mut left, 1.0, AWAY);
    let resting = ledge(&left);

    let snow = left.take_window(4).expect("snow on the window");
    right.adopt_window(4, snow, V2::new(-SEAM, 0.0));
    right.update_windows(&[surface(4, 300.0 - SEAM, 600.0, 500.0)], 0.2, &none, &none, false);
    right.step(DT, AWAY);
    let arrived = ledge(&right);
    assert!(arrived > resting * 0.9, "resting {resting}, arrived {arrived}");
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
        let mut a = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
        a.thaws = true;
        run(&mut a, 60.0, AWAY);
        ground += piles(&a)[0].1;

        let mut b = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
        b.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
        run(&mut b, 90.0, AWAY);
        b.set_snowfall(Snowfall::Off);
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

        let mut c = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
        run(&mut c, 120.0, AWAY);
        c.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1920.0, 1080.0, Snowfall::Variable);
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
#[ignore]
fn step_cost() {
    let none = HashSet::new();
    let mut simulation = SnowSimulation::new(1920.0, 1080.0, Snowfall::Steady);
    simulation.thaws = true;
    let window = [WindowSurface {
        id: 1,
        frame: Rect { x: 200.0, y: 300.0, width: 900.0, height: 500.0 },
        blockers: Vec::new(),
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
fn halloween_moonlight_only_shows_on_halloween() {
    use crate::halloween::{moonlit, night};
    let light = [1.0, 0.98, 0.95];
    let tint = [0.5, 0.5, 0.5, 0.35];
    assert_eq!(moonlit(light, tint, night(false)), (light, tint));
    let (moon, violet) = moonlit(light, tint, night(true));
    assert!((moon[2] - 1.0).abs() < 1e-5 && moon[0] < 0.8);
    assert!((violet[3] - 0.6).abs() < 1e-5);
}
