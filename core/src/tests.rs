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
fn a_screen_rests_only_once_its_snow_is_gone_and_none_is_coming() {
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Steady);
    simulation.thaws = true;
    let none = HashSet::new();
    simulation.update_windows(&[surface(7, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 30.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
    run(&mut simulation, 1.0, AWAY);
    assert!(!simulation.is_empty(), "snow still falls and lies");

    simulation.blow_away(1.0, false);
    run(&mut simulation, 30.0, AWAY);
    assert!(simulation.is_empty(), "sprites {}, snow {}", simulation.sprite_count(), simulation.has_snow());
    assert!(!simulation.step(DT, AWAY), "a screen at rest has nothing to redraw");

    simulation.set_halloween(true);
    assert!(!simulation.is_empty(), "Halloween's pumpkins are coming");
    simulation.set_halloween(false);
    simulation.set_snowfall(Snowfall::Light);
    assert!(!simulation.is_empty(), "the snowfall is coming");
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

fn leaf_sprites(simulation: &SnowSimulation) -> usize {
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    sprites[..count].iter().filter(|sprite| sprite.leaf[0] > 0.0).count()
}

#[test]
fn leaves_lie_where_they_land() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, Snowfall::Heavy);
    simulation.set_autumn(true);
    run(&mut simulation, 90.0, AWAY);
    assert_finite(&simulation);
    assert!(simulation.has_snow(), "no leaves lying");
    let mut sprites = vec![Sprite::default(); simulation.sprite_count()];
    let count = simulation.write_sprites(&mut sprites);
    assert!(sprites[..count].iter().all(|sprite| sprite.leaf[0] > 0.0), "snow among the leaves");
    assert!(sprites[..count].iter().any(|sprite| sprite.position.y < 30.0), "nothing on the ground");
}

#[test]
fn a_closed_window_drops_only_its_own_leaves() {
    let mut simulation = SnowSimulation::new(800.0, 600.0, Snowfall::Heavy);
    simulation.set_autumn(true);
    let none = HashSet::new();
    for _ in 0..(120.0 / DT) as usize {
        simulation.update_windows(&[surface(7, 200.0, 400.0, 400.0)], DT, &none, &none, false);
        simulation.step(DT, AWAY);
    }
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(800.0, 600.0, Snowfall::Heavy);
    run(&mut simulation, 30.0, AWAY);
    simulation.switch_mode(true, 1.0, false);
    run(&mut simulation, 0.5, AWAY);
    assert!(!simulation.autumn(), "switched before the gust cleared the screen");
    run(&mut simulation, 8.0, AWAY);
    assert!(simulation.autumn(), "never switched");
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    simulation.set_autumn(true);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 650.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 120.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(1280.0, 800.0, Snowfall::Heavy);
    simulation.set_autumn(true);
    simulation.thaws = false;
    let none = HashSet::new();
    simulation.update_windows(&[surface(3, 300.0, 600.0, 500.0)], 0.2, &none, &none, false);
    run(&mut simulation, 90.0, AWAY);
    simulation.set_snowfall(Snowfall::Off);
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
    let mut simulation = SnowSimulation::new(800.0, 600.0, Snowfall::Heavy);
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
