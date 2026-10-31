//! Snow between displays that each have a simulation of their own. A window's snow lies in one of them, which lends
//! it to the displays the window reaches over: they draw it, and the snow falling on it there goes back to the one
//! keeping it. The window's snow goes along when the window moves to another display, and snow thrown past an edge
//! two displays share goes on falling on the other side.

use super::{COLUMN_WIDTH, Flake, Grain, MAX_GRAINS, PileLayer, SnowSimulation, WindowId};
use crate::geometry::V2;
use crate::layer::SnowLayer;

/// A window's snow taken off one display to lie on another. Its times are kept as ages, since each simulation
/// keeps its own clock.
pub struct Carried {
    layer: SnowLayer,
}

/// A window's snow as the display keeping it lends it to a neighbour the window reaches over.
#[derive(Clone, PartialEq)]
pub struct Borrowed {
    id: WindowId,
    origin: V2,
    count: usize,
    /// The smoothed surface of each column, then the bed under it, up from `origin`, as drawn.
    columns: Vec<f32>,
    visible: Vec<bool>,
    top: f32,
    bottom: f32,
    occluders: Vec<[f32; 4]>,
}

impl Borrowed {
    /// Moved into another display's coordinates.
    pub fn shifted(mut self, shift: V2) -> Borrowed {
        self.origin += shift;
        for occluder in &mut self.occluders {
            *occluder = [occluder[0] + shift.x, occluder[1] + shift.y, occluder[2] + shift.x, occluder[3] + shift.y];
        }
        self
    }

    /// Where its snow can show: left, bottom, right, top.
    pub fn bounds(&self) -> [f32; 4] {
        let width = self.count as f32 * COLUMN_WIDTH;
        [self.origin.x, self.origin.y + self.bottom, self.origin.x + width, self.origin.y + self.top + 2.0]
    }

    pub(super) fn span(&self) -> usize {
        self.columns.len()
    }

    pub(super) fn write(&self, buffer: &mut [f32], offset: usize) -> Option<PileLayer<'_>> {
        if self.top <= 0.05 {
            return None;
        }
        buffer.copy_from_slice(&self.columns);
        Some(PileLayer { offset, count: self.count, origin: self.origin, top: self.top, bottom: self.bottom, occluders: &self.occluders })
    }

    /// The column under `x` and its surface, when snow falling from `from` to `to` there comes down on it.
    fn crossed(&self, x: f32, from: f32, to: f32) -> Option<f32> {
        let i = ((x - self.origin.x) / COLUMN_WIDTH).floor();
        if i < 0.0 || i as usize >= self.count || !self.visible[i as usize] {
            return None;
        }
        let (surface, bed) = (self.origin.y + self.columns[i as usize], self.origin.y + self.columns[self.count + i as usize]);
        (from >= bed && to <= surface).then_some(surface)
    }
}

/// Snow that fell on a borrowed window's snow, at `x` in the coordinates of the display it fell on.
pub struct Caught {
    pub id: WindowId,
    x: f32,
    area: f32,
}

/// Snow thrown past an edge shared with another display, for that display to go on with.
pub struct Stray(Piece);

enum Piece {
    Flake(Flake),
    Grain(Grain),
}

impl Stray {
    pub(super) fn flake(flake: Flake) -> Stray {
        Stray(Piece::Flake(flake))
    }

    pub(super) fn grain(grain: Grain) -> Stray {
        Stray(Piece::Grain(grain))
    }

    pub fn position(&self) -> V2 {
        match &self.0 {
            Piece::Flake(flake) => flake.position,
            Piece::Grain(grain) => grain.position,
        }
    }
}

impl SnowSimulation {
    /// Takes the snow off a window that moved to a display with a simulation of its own, so it can lie on there
    /// instead of falling here. `None` without snow kept for that window. Its ghosts settle back into the snow.
    pub fn take_window(&mut self, id: WindowId) -> Option<Carried> {
        let li = self.layer_index(id)?;
        self.release_ghosts(id, li);
        self.release_held();
        self.ids.remove(li);
        let mut layer = self.layers.remove(li);
        layer.moved_at -= self.air.clock;
        layer.returned_at -= self.air.clock;
        self.air.windows_changed = true;
        Some(Carried { layer })
    }

    /// Puts snow taken from another simulation onto its window here. `shift` takes the other simulation's
    /// coordinates to these; the snow keeps its motion, so a drag across displays goes on as one.
    pub fn adopt_window(&mut self, id: WindowId, carried: Carried, shift: V2) {
        if let Some(li) = self.layer_index(id) {
            self.release_held();
            self.ids.remove(li);
            self.layers.remove(li);
        }
        self.dormant.remove(&id);
        self.parking.retain(|entry| entry.0 != id);
        let mut layer = carried.layer;
        layer.moved_at += self.air.clock;
        layer.returned_at += self.air.clock;
        layer.origin += shift;
        layer.track += shift;
        layer.previous_base_y += shift.y;
        for occluder in &mut layer.occluders {
            *occluder = [occluder[0] + shift.x, occluder[1] + shift.y, occluder[2] + shift.x, occluder[3] + shift.y];
        }
        self.layers.push(layer);
        self.ids.push(id);
        self.air.windows_changed = true;
    }

    /// The snow on a window kept here, to lend to a display it reaches over; `None` for a window without a ledge here.
    pub fn lend(&self, id: WindowId) -> Option<Borrowed> {
        let layer = &self.layers[self.layer_index(id)?];
        let mut columns = vec![0.0; 2 * layer.count()];
        self.write_layer(layer, &mut columns);
        Some(Borrowed {
            id,
            origin: layer.base(),
            count: layer.count(),
            columns,
            visible: layer.visible.clone(),
            top: layer.top,
            bottom: -layer.bed_depth,
            occluders: layer.occluders.clone(),
        })
    }

    /// The snow neighbouring displays lend this one where their windows reach over it, in its coordinates.
    pub fn set_borrowed(&mut self, borrowed: Vec<Borrowed>) {
        if borrowed == self.borrowed {
            return;
        }
        // A window being dragged in from a neighbour moves at the rate of one dragged here.
        if borrowed.iter().any(|lent| self.borrowed.iter().any(|old| old.id == lent.id && old.origin != lent.origin)) {
            self.air.last_window_motion = self.air.clock;
        }
        self.borrowed = borrowed;
        self.borrowed_changed = true;
    }

    /// The snow that fell on borrowed snow since the last call, for the displays keeping it.
    pub fn take_caught(&mut self) -> Vec<Caught> {
        std::mem::take(&mut self.caught)
    }

    /// Lays snow caught on a neighbouring display onto the window it fell on; `shift` takes that display's `x` to this one's.
    pub fn catch(&mut self, caught: &Caught, shift: f32) {
        let Some(li) = self.layer_index(caught.id) else { return };
        let layer = &mut self.layers[li];
        let x = caught.x + shift;
        let fill = layer.index(x).map_or(1.0, |i| layer.fill(i));
        layer.deposit(caught.area * fill, x, true);
        self.air.heights_changed = true;
    }

    /// The snow thrown past a shared edge since the last call, in this simulation's coordinates.
    pub fn take_strays(&mut self) -> Vec<Stray> {
        std::mem::take(&mut self.strays)
    }

    /// Goes on with snow thrown in from a neighbouring display; `shift` takes its coordinates to these.
    pub fn adopt_strays(&mut self, strays: Vec<Stray>, shift: V2) {
        for stray in strays {
            match stray.0 {
                Piece::Flake(mut flake) => {
                    flake.position += shift;
                    self.air.flakes.push(flake);
                }
                Piece::Grain(mut grain) if self.air.grains.len() < MAX_GRAINS => {
                    grain.position += shift;
                    self.air.grains.push(grain);
                }
                Piece::Grain(_) => {}
            }
        }
    }

    /// From where, left to right, a window's snow can be seen: the screen, and the displays beside it.
    pub(super) fn seen_span(&self) -> (f32, f32) {
        (if self.neighbours.0 { f32::NEG_INFINITY } else { 0.0 }, if self.neighbours.1 { f32::INFINITY } else { self.width })
    }

    /// Whether `point` lies past an edge shared with another display.
    pub(super) fn crossed(&self, point: V2) -> bool {
        (self.neighbours.0 && point.x < 0.0) || (self.neighbours.1 && point.x > self.width)
    }

    /// Hands snow falling from `from` to `to` at `x` to the display keeping a borrowed window's snow, when that lies
    /// above `hit`, where it would land here. Returns whether it did.
    pub(super) fn lands_on_borrowed(&mut self, hit: Option<(usize, usize)>, x: f32, from: f32, to: f32, area: f32) -> bool {
        if self.borrowed.is_empty() || self.crossed(V2::new(x, to)) {
            return false;
        }
        let mut highest = hit.map_or(-f32::MAX, |(li, i)| self.layers[li].surface(i));
        let mut caught = None;
        for borrowed in &self.borrowed {
            if let Some(surface) = borrowed.crossed(x, from, to)
                && surface > highest
            {
                highest = surface;
                caught = Some(borrowed.id);
            }
        }
        let Some(id) = caught else { return false };
        self.caught.push(Caught { id, x, area });
        true
    }

    fn layer_index(&self, id: WindowId) -> Option<usize> {
        Some(self.ids.iter().skip(1).position(|&other| other == id)? + 1)
    }

    /// Held columns are kept by index, which taking a layer out would shift.
    fn release_held(&mut self) {
        for held in self.air.held_layers.drain(..) {
            if let Some(layer) = self.layers.get_mut(held) {
                layer.release_support();
            }
        }
    }
}
