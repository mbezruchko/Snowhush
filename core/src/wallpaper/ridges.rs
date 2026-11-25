//! Finding the tops of the things in the wallpaper: where the depth steps nearer going down, followed column by column
//! and moved onto the picture's own edge.

use super::{Grid, Ridge, Shown};

/// How much nearer, in the depth map's own range, a thing must be than what is right above it for its top to count.
const STEP: f32 = 0.035;
/// How far apart in points, across one column, two points of a ridge can still be the same ridge.
const LINK: f32 = 6.0;
/// How far the colour edge may lie from where the depth map puts it, in points: the network's edges are soft and a few
/// of its pixels off.
const REACH: i32 = 12;
/// The fewest points a ridge must run across to be kept.
const SHORTEST: f32 = 64.0;
/// The most columns of ridges kept over the whole screen, to keep the snow on them cheap.
const BUDGET: usize = 5000;
/// Ridges this near the screen's top are under the menu bar; this near its bottom, under the drift.
const TOP_MARGIN: f32 = 40.0;
const BOTTOM_MARGIN: f32 = 48.0;

struct Chain {
    first: usize,
    rows: Vec<f32>,
    steps: Vec<f32>,
    near: Vec<f32>,
    missed: usize,
}

/// A chain moved onto the picture's own edge: its row in each column and how sharp the edge is there.
struct Path {
    first: usize,
    rows: Vec<f32>,
    strengths: Vec<f32>,
    steps: Vec<f32>,
    near: Vec<f32>,
}

struct Piece {
    first: usize,
    rows: Vec<f32>,
    near: Vec<f32>,
    score: f32,
}

pub fn find(shown: &Shown, depth: &Grid, column_width: f32) -> Vec<Ridge> {
    let screen = (shown.width as f32, shown.height as f32);
    let columns = (screen.0 / column_width) as usize;
    let row_height = screen.1 / depth.height as f32;
    let last = depth.height - 1;
    let mut open: Vec<Chain> = Vec::new();
    let mut done: Vec<Chain> = Vec::new();
    let mut profile = vec![0f32; depth.height];
    for c in 0..columns {
        let x = (c as f32 + 0.5) * column_width;
        for (v, value) in profile.iter_mut().enumerate() {
            *value = depth.value_on(x, (v as f32 + 0.5) * row_height, screen);
        }
        // Where the depth steps up going down: something nearer starts there, with something farther above it.
        let mut found: Vec<(f32, f32, f32)> = Vec::new();
        for v in 1..last {
            let rise = profile[(v + 2).min(last)] - profile[v.saturating_sub(2)];
            let before = profile[(v + 1).min(last)] - profile[v.saturating_sub(3)];
            let after = profile[(v + 3).min(last)] - profile[v.saturating_sub(1)];
            if rise > STEP && rise >= before && rise > after {
                found.push(((v as f32 + 0.5) * row_height, rise, profile[(v + 4).min(last)]));
            }
        }
        let mut taken = vec![false; found.len()];
        for chain in &mut open {
            let previous = chain.rows[chain.rows.len() - 1];
            let mut best: Option<usize> = None;
            for (j, point) in found.iter().enumerate() {
                if taken[j] || (point.0 - previous).abs() > LINK * (chain.missed + 1) as f32 {
                    continue;
                }
                if best.is_none_or(|best| (point.0 - previous).abs() < (found[best].0 - previous).abs()) {
                    best = Some(j);
                }
            }
            if let Some(best) = best {
                taken[best] = true;
                // Columns skipped on the way are filled in along a straight line.
                for m in 0..chain.missed {
                    let t = (m + 1) as f32 / (chain.missed + 1) as f32;
                    chain.rows.push(previous + (found[best].0 - previous) * t);
                    chain.steps.push(0.0);
                    chain.near.push(chain.near.last().copied().unwrap_or(0.0));
                }
                chain.rows.push(found[best].0);
                chain.steps.push(found[best].1);
                chain.near.push(found[best].2);
                chain.missed = 0;
            } else {
                chain.missed += 1;
            }
        }
        let (lost, kept): (Vec<Chain>, Vec<Chain>) = open.into_iter().partition(|chain| chain.missed > 3);
        done.extend(lost);
        open = kept;
        for (j, point) in found.iter().enumerate() {
            if !taken[j] {
                open.push(Chain { first: c, rows: vec![point.0], steps: vec![point.1], near: vec![point.2], missed: 0 });
            }
        }
    }
    done.extend(open);
    let minimum = (SHORTEST / column_width) as usize;
    let paths: Vec<Path> = done.iter().filter(|chain| chain.rows.len() >= minimum).map(|chain| snap(chain, shown, column_width)).collect();
    // How sharp the picture's clear edges are: next to them, a fainter edge is a ripple in the dark or a smooth shading,
    // not the top of anything.
    let mut all: Vec<f32> = paths.iter().flat_map(|path| path.strengths.iter().copied()).collect();
    all.sort_by(f32::total_cmp);
    let clear = if all.is_empty() { 0.0 } else { all[((all.len() as f32 * 0.9) as usize).min(all.len() - 1)] };
    let pieces = paths
        .iter()
        .flat_map(|path| cut(path, (0.08 * clear).max(0.035), (0.12 * clear).max(0.06), column_width))
        .filter(|piece| piece.rows.len() >= minimum);
    let mut ridges = Vec::new();
    for piece in pieces {
        // Sunk a little into the thing's top, so no sky shows under the snow.
        let tops: Vec<f32> = piece.rows.iter().map(|row| screen.1 - row - 4.0).collect();
        let lowest = tops.iter().copied().fold(f32::INFINITY, f32::min);
        let highest = tops.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if lowest <= BOTTOM_MARGIN || highest >= screen.1 - TOP_MARGIN {
            continue;
        }
        let near = piece.near.iter().sum::<f32>() / piece.near.len() as f32;
        // Hardly nearer than the sky: the edge of a cloud.
        if near <= 0.1 {
            continue;
        }
        // Up and down all along, as the tops of a row of trees go: no ridgeline for a cap of snow to follow.
        let count = tops.len();
        let climb = (2..2usize.max(count.saturating_sub(2))).map(|k| (tops[k + 2] - tops[k - 2]).abs()).sum::<f32>()
            / (4.0 * column_width * count.saturating_sub(4).max(1) as f32);
        let mut turns = 0;
        let mut heading = 0.0f32;
        for k in (0..count.saturating_sub(4)).step_by(4) {
            let rise = tops[k + 4] - tops[k];
            if rise.abs() <= 3.0 {
                continue;
            }
            if heading != 0.0 && (rise > 0.0) != (heading > 0.0) {
                turns += 1;
            }
            heading = rise;
        }
        let length = count as f32 * column_width;
        if climb >= 0.7 || turns as f32 / length * 100.0 >= 2.0 || (length < 100.0 && climb >= 0.45) {
            continue;
        }
        let ridge = Ridge { left: piece.first as f32 * column_width, tops, nearness: near, room: Vec::new(), mass: Vec::new() };
        ridges.push((ridge, piece.score));
    }
    keep_best(ridges, column_width)
}

/// Moves a chain from the depth map's soft edge onto the picture's own: the row in each column, within `REACH`, where
/// the colour changes most from above to below, with the path kept smooth.
fn snap(chain: &Chain, shown: &Shown, column_width: f32) -> Path {
    let count = chain.rows.len();
    let span = (2 * REACH + 1) as usize;
    let width = column_width as usize;
    let mut edge = vec![0f32; count * span];
    for i in 0..count {
        let center = chain.rows[i].round() as i32;
        let x0 = (chain.first + i) * width;
        for s in 0..span {
            edge[i * span + s] = contrast(shown, x0, width, center - REACH + s as i32);
        }
    }
    // Viterbi: each column picks a row; a column's row may move at most `turn` rows from its neighbour's, and pays for
    // moving at all.
    let turn = 3;
    let bend = 0.004;
    let lower = 0.006;
    let mut total = vec![0f32; count * span];
    let mut from = vec![0usize; count * span];
    for s in 0..span {
        total[s] = edge[s] + lower * (s as i32 - REACH) as f32;
    }
    for i in 1..count {
        let shift = chain.rows[i].round() as i32 - chain.rows[i - 1].round() as i32;
        for s in 0..span {
            let (mut best, mut pick) = (f32::MIN, 0);
            for d in -turn..=turn {
                let p = s as i32 + shift + d;
                if p < 0 || p >= span as i32 {
                    continue;
                }
                let value = total[(i - 1) * span + p as usize] - bend * (d * d) as f32;
                if value > best {
                    best = value;
                    pick = p as usize;
                }
            }
            // A haze or glow above a thing's top is part of what lies behind it: of two edges alike, the lower wins.
            total[i * span + s] = best + edge[i * span + s] + lower * (s as i32 - REACH) as f32;
            from[i * span + s] = pick;
        }
    }
    let mut rows = vec![0f32; count];
    let mut strengths = vec![0f32; count];
    let tail = &total[(count - 1) * span..count * span];
    let mut s = (1..span).fold(0, |best, k| if tail[k] > tail[best] { k } else { best });
    for i in (0..count).rev() {
        rows[i] = (chain.rows[i].round() as i32 - REACH + s as i32) as f32;
        strengths[i] = edge[i * span + s];
        if i > 0 {
            s = from[i * span + s];
        }
    }
    // A little smoothing takes out the stair steps of whole rows.
    let mut smooth = rows.clone();
    for i in 1..count.saturating_sub(1) {
        smooth[i] = 0.25 * rows[i - 1] + 0.5 * rows[i] + 0.25 * rows[i + 1];
    }
    Path { first: chain.first, rows: smooth, strengths, steps: chain.steps.clone(), near: chain.near.clone() }
}

/// Cuts a path where the picture shows no edge to sit on for a while, or where it climbs too steeply for snow, and keeps
/// the pieces whose edge is clear enough on the whole.
fn cut(path: &Path, weak: f32, faint: f32, column_width: f32) -> Vec<Piece> {
    let count = path.rows.len();
    let mut pieces = Vec::new();
    let close = |begin: usize, end: usize, pieces: &mut Vec<Piece>| {
        if end <= begin {
            return;
        }
        let range = begin..end;
        let n = range.len() as f32;
        let mean = path.strengths[range.clone()].iter().sum::<f32>() / n;
        let rise = path.steps[range.clone()].iter().sum::<f32>() / n;
        if mean < faint {
            return;
        }
        pieces.push(Piece {
            first: path.first + begin,
            rows: path.rows[range.clone()].to_vec(),
            near: path.near[range].to_vec(),
            score: mean * rise * n,
        });
    };
    // Snow slides off anything steeper than about 56 degrees, and a path that steep is mostly noise anyway.
    let steepest = 1.5 * column_width;
    let steep = |i: usize| {
        let (a, b) = (i.saturating_sub(2), (i + 2).min(count - 1));
        (path.rows[b] - path.rows[a]).abs() > steepest * (b - a).max(1) as f32
    };
    let mut start: Option<usize> = None;
    let mut gap = 0;
    for i in 0..count {
        if path.strengths[i] >= weak && !steep(i) {
            if start.is_none() {
                start = Some(i);
            }
            gap = 0;
        } else if let Some(begin) = start {
            gap += 1;
            if gap >= 4 || steep(i) {
                close(begin, i + 1 - gap, &mut pieces);
                start = None;
                gap = 0;
            }
        }
    }
    if let Some(begin) = start {
        close(begin, count - gap, &mut pieces);
    }
    pieces
}

/// How different the few rows just above `row` are from the few just below, over a column: the red, green and blue
/// differences added up, 0 to 3.
fn contrast(shown: &Shown, x: usize, width: usize, row: i32) -> f32 {
    if row < 3 || row + 3 >= shown.height as i32 || x + width > shown.width {
        return 0.0;
    }
    let row = row as usize;
    let (mut above, mut below) = ([0f32; 3], [0f32; 3]);
    let p = &shown.pixels;
    for dx in 0..width {
        for dy in 1..=3 {
            let a = ((row - dy) * shown.width + x + dx) * 4;
            let b = ((row + dy) * shown.width + x + dx) * 4;
            for c in 0..3 {
                above[c] += p[a + c] as f32;
                below[c] += p[b + c] as f32;
            }
        }
    }
    ((above[0] - below[0]).abs() + (above[1] - below[1]).abs() + (above[2] - below[2]).abs()) / (3 * width * 255) as f32
}

/// Drops ridges that double others, then keeps the strongest within the budget.
fn keep_best(mut ridges: Vec<(Ridge, f32)>, column_width: f32) -> Vec<Ridge> {
    ridges.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<Ridge> = Vec::new();
    let mut used = 0;
    for (ridge, _) in ridges {
        let doubled = kept.iter().any(|other| {
            let first = ridge.left.max(other.left);
            let last = (ridge.left + ridge.tops.len() as f32 * column_width).min(other.left + other.tops.len() as f32 * column_width);
            if last - first <= 0.0 {
                return false;
            }
            let (mut close, mut all) = (0, 0);
            let mut x = first;
            while x < last {
                let a = ridge.tops[(((x - ridge.left) / column_width) as usize).min(ridge.tops.len() - 1)];
                let b = other.tops[(((x - other.left) / column_width) as usize).min(other.tops.len() - 1)];
                if (a - b).abs() < 6.0 {
                    close += 1;
                }
                all += 1;
                x += column_width;
            }
            close * 2 > all
        });
        if doubled || used + ridge.tops.len() > BUDGET {
            continue;
        }
        used += ridge.tops.len();
        kept.push(ridge);
    }
    kept
}
