use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::hash::{BuildHasher, Hasher};
use std::ops::Range;

use crate::geometry::{Rect, V2, smoothstep};
use crate::layer::SnowLayer;

mod ghosts;
mod pumpkins;

use ghosts::Haunting;
use pumpkins::Pumpkins;

pub type WindowId = isize;

pub const COLUMN_WIDTH: f32 = 2.0;
const GROUND_DEPTH: f32 = 40.0;
const LEDGE_DEPTH: f32 = 14.0;
const THAW_DEPTH: f32 = 8.0;
const LEDGE_THAW_DEPTH: f32 = 4.0;
const THAW_RATE: f32 = 0.02;
const THAW_TIME: f32 = 30.0;
const THAW_BALANCE: f32 = 184.0;
const THAW_EXPONENT: f32 = 0.74;
const DEPOSITION: f32 = 1.1;
const REPOSE: f32 = 0.8;
const ROUNDING: f32 = 0.15;
const CURSOR_RADIUS: f32 = 10.0;
const CURSOR_OFFSET: V2 = V2::new(4.0, -8.0);
const GRAVITY: f32 = 2500.0;
const GRAIN_DRAG: f32 = 0.8;
const MAX_GRAINS: usize = 12000;
const PHASE_WRAP: f32 = 20.0 * PI;
const LEDGE_INSET: f32 = 0.0;
const GRIP: f32 = 3.0;
const SLIP: f32 = 2.0;
const BREAK_LIFT: f32 = 10.0;
/// Snow sticks to a window: it only lets go when pulled away harder than this, on top of its weight.
const ADHESION: f32 = 3500.0;
const SHAKING: f32 = 4.0;
/// Leaves hold together far less than snow: a heap of them flies apart as soon as it lifts off its window at all,
/// and a shake that would only stir snow throws them off.
const LEAF_BREAK_LIFT: f32 = 2.0;
const LEAF_SHAKING: f32 = 0.8;
/// Leaves hardly stick to a window and slide over it easily: shaken, the window moves out from under them.
const LEAF_ADHESION: f32 = 500.0;
const LEAF_GRIP: f32 = 0.8;
const LEAF_SLIP: f32 = 0.5;
/// The height of the display, in points, the way snow comes off a moving window was tuned on, on the Mac.
const TUNED_HEIGHT: f32 = 1440.0;
/// The pointer speeds up less with a flick of the hand on Windows and under libinput than on the Mac, so a window
/// is jerked up and down slower by the same hand: those jolts count for this much more.
const POINTER_GAIN: f32 = 2.0;
const JUMP_SPEED: f32 = 6000.0;
const TRACK_TIME: f32 = 0.06;
const REGROWTH: f32 = 0.4;
const MAX_MOTES: usize = 1500;
const GUST_SWEEP: f32 = 0.6;
const GUST_LENGTH: f32 = 2.2;
const GUST_WIND: f32 = 1000.0;
/// The snowfall is caught this long after the drifts, so it trails behind their powder.
const GUST_LAG: f32 = 0.3;
/// Snow a wall scrapes up packs down to this share of its fresh bulk.
const PACKING: f32 = 0.6;
/// A sliding desktop drags the air along: the wind it stirs up is this share of its speed, up to `SLIDE_WIND_MAX`,
/// and dies down over `SLIDE_CALM` seconds once the desktop stops.
const SLIDE_DRAG: f32 = 0.025;
const SLIDE_WIND_MAX: f32 = 160.0;
const SLIDE_CALM: f32 = 0.35;
/// Leaves fall this much sparser than snow, each one being so much bigger.
const LEAF_SHARE: f32 = 0.07;
/// The share of a heap one fallen leaf makes up.
const LEAF_AREA: f32 = 24.0;
/// Leaves lie loosely, with air between them: the bulk a landing leaf adds to its heap, per point of its length.
const LEAF_BULK: f32 = 5.5;
/// How much of a heap's visible bulk each leaf coming off it stands for: a heap is packed with overlapping leaves,
/// so as many fly off as it looked to hold.
const LEAF_SHOWN: f32 = 30.0;
const MAX_LYING: usize = 1500;
/// The top of a heap is the lying leaves themselves: the pile shader draws the mass under them only this far down.
const LEAF_COVER: f32 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snowfall {
    Off,
    Light,
    Steady,
    Heavy,
    Variable,
}

impl Snowfall {
    pub const ALL: [Snowfall; 5] = [Snowfall::Off, Snowfall::Light, Snowfall::Steady, Snowfall::Heavy, Snowfall::Variable];

    pub fn title(self) -> &'static str {
        match self {
            Snowfall::Off => "Off",
            Snowfall::Light => "Light",
            Snowfall::Steady => "Steady",
            Snowfall::Heavy => "Heavy",
            Snowfall::Variable => "Variable",
        }
    }

    fn flakes_per_megapoint(self) -> f32 {
        match self {
            Snowfall::Off => 0.0,
            Snowfall::Light => 150.0,
            Snowfall::Steady => 380.0,
            Snowfall::Heavy => 800.0,
            Snowfall::Variable => 380.0,
        }
    }
}

/// How briskly the scene changes, which sets how often it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Only snowfall and slow drifts: a low frame rate is indistinguishable.
    Calm,
    /// Loose snow in flight.
    Lively,
    /// The cursor or a window is pushing snow around: drawn as often as the display allows.
    Interactive,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Sprite {
    pub position: V2,
    /// Smears fast snow into a short streak, as a camera would.
    pub velocity: V2,
    pub size: f32,
    pub alpha: f32,
    pub soft: f32,
    /// How much of the tint shows instead of snow white.
    pub tint_amount: f32,
    pub tint: [f32; 3],
    /// Gives a lump of snow its ragged outline and tumble; zero for a round flake.
    pub lump: f32,
    /// Draws a Halloween ghost instead, when the first is above zero: its seed (one and up), the height of the drift's
    /// surface it rises out of (in the same points as `position`), how far its eyes are open, and how far it has taken
    /// shape out of a heap of snow. Packed four to a vector for the graphics card, as are `pose` and `mood`.
    pub ghost: [f32; 4],
    /// A ghost's paws: how far up saying boo, how far its right one is held out at the angle that follows (radians,
    /// zero pointing right, up positive), and how far both are spread for a wave.
    pub pose: [f32; 4],
    /// How far a ghost is into a dab, and how cross it is.
    pub mood: [f32; 2],
    /// Draws a leaf instead, when the first is above zero: its seed, which picks its shape and colour, which way it
    /// points (radians), and how much of its face shows, and which side: one is its front face on, below zero its back.
    pub leaf: [f32; 3],
    /// How fast a leaf turns and tumbles, a second: it blurs over the exposure, as it moves.
    pub spin: [f32; 2],
}

/// A Halloween sign a ghost holds up, or lets fall: the picture of `SIGN_CAPTIONS[caption]`, its stick's foot at
/// `anchor`, turned `angle` radians counterclockwise about it, nothing of it showing below `ground`.
#[derive(Clone, Copy, Debug)]
pub struct SignSprite {
    pub caption: usize,
    pub anchor: V2,
    pub angle: f32,
    pub ground: f32,
}

/// A Halloween jack-o'-lantern sitting in the snow at the bottom of the screen: the middle of its bottom at `anchor`,
/// `width` points wide, its candle's flicker set apart from the other's by `seed`.
#[derive(Clone, Copy, Debug)]
pub struct PumpkinSprite {
    pub anchor: V2,
    pub width: f32,
    pub seed: f32,
}

/// A bit of something that crumbles into snow, such as a window: it keeps its colour for a moment while it falls.
#[derive(Clone, Copy, Debug)]
pub struct ScatterPiece {
    pub position: V2,
    pub velocity: V2,
    pub color: [f32; 3],
}

pub struct PileLayer<'a> {
    pub offset: usize,
    pub count: usize,
    pub origin: V2,
    pub top: f32,
    pub bottom: f32,
    pub occluders: &'a [[f32; 4]],
    /// How far along its window the snow has moved since it came; what is drawn in it moves along.
    pub drift: f32,
}

#[derive(PartialEq)]
pub struct WindowSurface {
    pub id: WindowId,
    pub frame: Rect,
    /// Frames of windows in front of this one.
    pub blockers: Vec<Rect>,
    pub corner_radius: f32,
    pub corner_exponent: f32,
}

#[derive(Clone, Copy)]
struct Flake {
    position: V2,
    velocity: V2,
    fall_speed: f32,
    size: f32,
    depth: f32,
    alpha: f32,
    soft: f32,
    phase: f32,
    sway_rate: f32,
    sway_amount: f32,
    settles: bool,
    /// Scales the snow it leaves where it lands.
    mass: f32,
    tint: [f32; 3],
    tint_amount: f32,
    /// The direction of the gust that caught the flake and carries it off the screen; zero until then.
    blown: f32,
    /// A leaf instead of a snowflake: its seed, which picks its shape and colour; zero for snow.
    leaf: f32,
}

#[derive(Clone, Copy)]
struct Grain {
    position: V2,
    velocity: V2,
    area: f32,
    size: f32,
    /// A lump holds together in flight and bursts into powder where it lands; zero for loose snow.
    lump: f32,
    /// Powder carried off by a gust: it rides the wind off the screen instead of settling.
    blown: bool,
    /// For a leaf, picks its shape and colour.
    seed: f32,
    /// Only the bulk under fallen leaves, with no leaf of its own: unseen, it falls and adds to the heap it lands on.
    bulk: bool,
}

/// A fallen leaf lying flat on a heap, on the ground or on a window.
#[derive(Clone, Copy)]
struct LyingLeaf {
    /// The window it lies on, or zero for the ground.
    layer: WindowId,
    /// From the layer's base along it, and up from the base to where the leaf rests and to the top of its own bulk.
    x: f32,
    rest: f32,
    top: f32,
    size: f32,
    seed: f32,
    tilt: f32,
    /// Half a turn when it came down pointing the other way.
    around: f32,
    /// How it was turned and tumbled in the air, and how far along the surface it slides, while it settles.
    from_turn: f32,
    from_flip: f32,
    slide: f32,
    settling: f32,
    settled: f32,
    life: f32,
    age: f32,
    fade: f32,
}

/// Powder thrown up by a lump bursting: it billows, slows in the air and fades without settling.
#[derive(Clone, Copy)]
struct Mote {
    position: V2,
    velocity: V2,
    size: f32,
    alpha: f32,
    age: f32,
    life: f32,
}

struct Random(u64);

impl Random {
    fn new() -> Random {
        Random(RandomState::new().build_hasher().finish().max(1))
    }

    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lower: f32, upper: f32) -> f32 {
        lower + (upper - lower) * self.next()
    }
}

/// A gust clearing the snow: its front sweeps across from the windward side, lifting the drifts behind it.
#[derive(Clone, Copy)]
struct Gust {
    start: f64,
    direction: f32,
    /// Melts the snow in place instead, for Reduce Motion.
    calm: bool,
}

/// Everything but the layers, so a layer can be borrowed alongside it.
struct Air {
    flakes: Vec<Flake>,
    grains: Vec<Grain>,
    motes: Vec<Mote>,
    random: Random,
    clock: f64,
    strength: f32,
    cursor: Option<V2>,
    cursor_velocity: V2,
    held_layers: Vec<usize>,
    heights_changed: bool,
    windows_changed: bool,
    last_window_motion: f64,
    last_cursor_push: f64,
    /// Snow released while the air is full, laid straight onto the ground at the end of the step.
    overflow: Vec<(f32, f32)>,
    gust: Option<Gust>,
    slide_wind: f32,
    /// Leaves fall instead of snow and heap up where it would.
    autumn: bool,
    /// How many more leaves the heaps can show coming off them, as `spare_leaves` found it, less those shown since.
    leaf_budget: f32,
    /// How much more a window's jolts up and down count than on the Mac the snow was tuned on: across a smaller
    /// display, or by a gentler pointer, the same jerk of the hand moves it fewer points a second, and its snow should
    /// come off as easily. Side to side the snow slides on the window, which shows any such boost as sliding too far.
    jolt_scale: f32,
}

pub struct SnowSimulation {
    width: f32,
    height: f32,
    pace: Pace,
    snowfall: Snowfall,
    density: f32,
    pub thaws: bool,
    pub accumulates: bool,
    /// Side edges, left and right, with no display past them: a window slid past one leaves its snow heaped against the edge.
    pub walls: (bool, bool),
    /// Keeps the snowfall from starting until a scattered window's snow first reaches the bottom of the screen.
    snowfall_held: bool,
    /// Set when the held snowfall starts by itself, so the other displays can start theirs.
    snowfall_released: bool,
    scattered_until: f64,
    /// The ground first, then one ledge per window, with `ids` alongside.
    layers: Vec<SnowLayer>,
    ids: Vec<WindowId>,
    /// Snow on windows that left the screen for another desktop, kept until the window shows up again.
    dormant: HashMap<WindowId, SnowLayer>,
    /// Windows that went off screen without a word yet on why; a desktop switch keeps their snow, anything else drops it.
    parking: Vec<(WindowId, SnowLayer, f64)>,
    space_changed_at: f64,
    target_flakes: usize,
    strength_target: f32,
    next_shift: f64,
    /// Every app window on the screen, maximized and full-screen ones too, left, bottom, right, top in points:
    /// a pumpkin hides from any that covers it.
    pub covers: Vec<[f32; 4]>,
    /// The window that is a panel's shelf along the bottom of the screen, if any: pumpkins over it sit in its snow.
    pub shelf: Option<WindowId>,
    haunting: Haunting,
    lanterns: Pumpkins,
    lying: Vec<LyingLeaf>,
    /// The mode waiting for the gust to clear the screen, so one kind never turns into the other in sight.
    next_autumn: Option<bool>,
    switched_at: f64,
    restyled: bool,
    air: Air,
}

impl SnowSimulation {
    pub fn new(width: f32, height: f32, snowfall: Snowfall) -> SnowSimulation {
        let mut seed = Random::new();
        let ground = SnowLayer::new(V2::ZERO, column_count(width), COLUMN_WIDTH, GROUND_DEPTH, false, (seed.range(0.0, TAU), seed.range(0.0, TAU)));
        let mut simulation = SnowSimulation {
            width,
            height,
            pace: Pace::Calm,
            snowfall,
            density: 1.0,
            thaws: false,
            accumulates: true,
            walls: (false, false),
            snowfall_held: false,
            snowfall_released: false,
            scattered_until: -10.0,
            layers: vec![ground],
            ids: vec![0],
            dormant: HashMap::new(),
            parking: Vec::new(),
            space_changed_at: -10.0,
            target_flakes: 0,
            strength_target: 1.0,
            next_shift: 0.0,
            covers: Vec::new(),
            shelf: None,
            haunting: Haunting::new(),
            lanterns: Pumpkins::default(),
            lying: Vec::new(),
            next_autumn: None,
            switched_at: 0.0,
            restyled: false,
            air: Air {
                flakes: Vec::new(),
                grains: Vec::new(),
                motes: Vec::new(),
                random: Random::new(),
                clock: 0.0,
                strength: 1.0,
                cursor: None,
                cursor_velocity: V2::ZERO,
                held_layers: Vec::new(),
                heights_changed: false,
                windows_changed: false,
                last_window_motion: -10.0,
                last_cursor_push: -10.0,
                overflow: Vec::new(),
                gust: None,
                slide_wind: 0.0,
                autumn: false,
                leaf_budget: 0.0,
                jolt_scale: jolt_scale(height),
            },
        };
        simulation.update_target();
        simulation
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn pace(&self) -> Pace {
        self.pace
    }

    pub fn set_snowfall(&mut self, snowfall: Snowfall) {
        if snowfall == Snowfall::Variable && self.snowfall != Snowfall::Variable {
            self.next_shift = self.air.clock;
        }
        if snowfall != Snowfall::Variable {
            self.air.strength = 1.0;
            self.strength_target = 1.0;
        }
        self.snowfall = snowfall;
        self.update_target();
    }

    pub fn set_snowfall_held(&mut self, held: bool) {
        self.snowfall_held = held;
        self.update_target();
    }

    /// Whether the held snowfall started by itself since the last call.
    pub fn take_snowfall_released(&mut self) -> bool {
        std::mem::take(&mut self.snowfall_released)
    }

    /// Scales the snowfall; flakes come and go through the usual cycle, so a change shows gradually.
    pub fn set_density(&mut self, density: f32) {
        self.density = density;
        self.update_target();
    }

    pub fn has_snow(&self) -> bool {
        !self.lying.is_empty() || self.layers.iter().any(|layer| layer.top > 0.5)
    }

    pub fn autumn(&self) -> bool {
        self.air.autumn
    }

    /// Leaves fall instead of snow, starting at once: what falls now turns into the other kind where it is.
    pub fn set_autumn(&mut self, autumn: bool) {
        if autumn == self.air.autumn {
            return;
        }
        self.air.autumn = autumn;
        self.lying.clear();
        self.update_target();
        let width = self.width;
        let heights: Vec<f32> = self.air.flakes.iter().take(self.target_flakes).map(|flake| flake.position.y).collect();
        self.air.flakes = heights.into_iter().map(|y| self.air.make_flake(width, y)).collect();
        self.restyled = true;
    }

    /// Clears the screen first, with a gust in `direction` (or, `calm`, melting it in place), and lets the other kind
    /// fall only after, from the top.
    pub fn switch_mode(&mut self, autumn: bool, direction: f32, calm: bool) {
        if autumn == self.air.autumn {
            self.next_autumn = None;
            return;
        }
        if self.next_autumn.is_some() {
            return;
        }
        self.next_autumn = Some(autumn);
        self.switched_at = self.air.clock;
        self.blow_away(direction, calm);
    }

    /// Nothing falls, lies, waits for its window or haunts, and nothing will until the snowfall or Halloween is
    /// turned on: the screen can rest.
    pub fn is_empty(&self) -> bool {
        self.snowfall == Snowfall::Off
            && !self.haunting.on
            && self.sprite_count() == 0
            && !self.haunting.moving()
            && self.air.gust.is_none()
            && self.air.overflow.is_empty()
            && self.dormant.is_empty()
            && self.parking.is_empty()
            && self.next_autumn.is_none()
            && self.layers.iter().all(|layer| layer.top <= 0.0)
    }

    pub fn sprite_count(&self) -> usize {
        self.lying.len() + self.air.flakes.len() + self.air.grains.len() + self.air.motes.len() + self.haunting.sprite_count()
    }

    /// Floats the renderer needs per frame: each layer's surface and bed.
    pub fn column_count(&self) -> usize {
        self.layers.iter().map(|layer| 2 * layer.count()).sum()
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        if width == self.width && height == self.height {
            return;
        }
        self.layers[0].resize(column_count(width));
        self.width = width;
        self.height = height;
        self.air.jolt_scale = jolt_scale(height);
        self.update_target();
    }

    /// Returns whether anything visible changed.
    pub fn step(&mut self, dt: f32, cursor: V2) -> bool {
        self.air.clock += dt as f64;
        let was_moving = self.sprite_count() > 0;
        self.air.heights_changed = false;

        self.vary(dt);
        let wind = self.air.slide_wind;
        self.air.slide_wind = if wind.abs() < 1.0 { 0.0 } else { wind * (-dt / SLIDE_CALM).exp() };
        self.air.leaf_budget = self.spare_leaves();
        self.expire_parking();
        for ledge in &mut self.layers[1..] {
            self.air.carry(ledge, dt);
        }
        self.track_cursor(cursor, dt);
        for (li, ledge) in self.layers.iter_mut().enumerate().skip(1) {
            self.air.confine(ledge, li, self.width, self.walls);
        }
        self.blow(dt);
        self.skim(dt);
        self.advance_flakes(dt);
        self.advance_grains(dt);
        self.air.advance_motes(dt);
        self.haunt(dt);
        self.settle(dt);
        self.tend_leaves(dt);
        if let Some(next) = self.next_autumn
            && self.air.gust.is_none()
            && (self.air.grains.is_empty() || self.air.clock - self.switched_at > 6.0)
        {
            self.next_autumn = None;
            self.set_autumn(next);
        }
        self.spawn_flakes();
        if self.haunting.on {
            self.settle_pumpkins(dt);
        }
        for (area, x) in self.air.overflow.drain(..) {
            self.layers[0].deposit(area, x, true);
        }

        for layer in &mut self.layers {
            layer.refresh_top();
            layer.previous_base_y = layer.base().y;
        }
        let air = &self.air;
        // Only a cursor that actually displaces snow needs the full rate; one passing near it or resting in it changes nothing.
        let pushed = air.clock - air.last_window_motion < 0.6
            || air.clock - air.last_cursor_push < 0.3
            || air.gust.is_some()
            || self.haunting.pushing(air.clock);
        // The steady trickle off window corners is a few tiny grains at a time; they read fine at the calm rate.
        let flying = air.grains.len() > 24 || air.clock < self.scattered_until || air.slide_wind.abs() > 20.0;
        self.pace = if pushed {
            Pace::Interactive
        } else if flying {
            Pace::Lively
        } else {
            Pace::Calm
        };
        let changed = was_moving
            || self.sprite_count() > 0
            || self.haunting.moving()
            || self.air.heights_changed
            || self.air.windows_changed
            || self.lanterns.moved
            || self.restyled;
        self.restyled = false;
        self.air.windows_changed = false;
        self.lanterns.moved = false;
        changed
    }

    pub fn write_sprites(&self, buffer: &mut [Sprite]) -> usize {
        let clock = self.air.clock;
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().map(|(li, &id)| (id, li)).collect();
        let lying = self.lying.iter().filter_map(|leaf| {
            let layer = &self.layers[*shown.get(&leaf.layer)?];
            let ease = 1.0 - (1.0 - leaf.settled).powi(3);
            let x = layer.heap_x() + leaf.x - leaf.slide * (1.0 - ease);
            let i = layer.index(x).filter(|&i| layer.visible[i])?;
            let target = resting_turn(leaf, layer, i);
            // Sliding over to where it comes to rest, it skims the ground until it rides up onto the leaves there.
            let rest = if leaf.settled < 1.0 { (layer.surface(i) - layer.base().y).min(leaf.rest) } else { leaf.rest };
            let flat = if leaf.from_flip < 0.0 { -0.7 } else { 0.7 };
            // The heap's mass shows `LEAF_COVER` below its surface: the leaf lies on what shows, or on the bare window.
            let sink = (rest - layer.bed[i]).clamp(0.0, LEAF_COVER);
            Some(Sprite {
                position: V2::new(x, layer.base().y + rest + leaf.size * 0.2 - sink),
                size: leaf.size,
                alpha: leaf.fade,
                leaf: [leaf.seed, leaf.from_turn + (target - leaf.from_turn) * ease, leaf.from_flip + (flat - leaf.from_flip) * ease],
                ..Sprite::default()
            })
        });
        let flakes = self.air.flakes.iter().map(|flake| {
            if flake.leaf > 0.0 {
                return Sprite {
                    position: flake.position,
                    velocity: motion(flake),
                    size: flake.size,
                    alpha: flake.alpha,
                    leaf: [flake.leaf, flake_turn(clock, flake), tumble(clock, flake.leaf)],
                    spin: [flake.phase.cos() * 0.6 * flake.sway_rate + flake.leaf - 0.5, tumble_speed(clock, flake.leaf)],
                    ..Sprite::default()
                };
            }
            Sprite {
                position: flake.position,
                velocity: flake.velocity,
                size: flake.size,
                alpha: flake.alpha,
                soft: flake.soft,
                tint_amount: flake.tint_amount,
                tint: flake.tint,
                ..Sprite::default()
            }
        });
        let autumn = self.air.autumn;
        let grains = self.air.grains.iter().filter(|grain| !grain.bulk).map(|grain| {
            if autumn {
                return Sprite {
                    position: grain.position,
                    velocity: grain.velocity,
                    size: if grain.size >= 5.5 { grain.size } else { 6.0 + grain.seed * 4.5 },
                    alpha: 0.95,
                    leaf: [grain.seed, grain_turn(clock, grain), tumble(clock, grain.seed)],
                    spin: [(grain.seed - 0.5) * 9.0, tumble_speed(clock, grain.seed)],
                    ..Sprite::default()
                };
            }
            Sprite {
                position: grain.position,
                velocity: grain.velocity,
                size: grain.size,
                alpha: if grain.lump > 0.0 {
                    0.97
                } else if grain.blown {
                    0.8
                } else {
                    0.95
                },
                soft: if grain.lump > 0.0 { 0.2 } else { 0.3 },
                tint: [1.0; 3],
                lump: grain.lump,
                ..Sprite::default()
            }
        });
        let motes = self.air.motes.iter().filter(|_| !autumn).map(|mote| {
            let t = mote.age / mote.life;
            let fade = (mote.age / 0.06).min(1.0) * (1.0 - t).powf(1.5);
            Sprite { position: mote.position, size: mote.size * (1.0 + t), alpha: mote.alpha * fade, soft: 1.0, tint: [1.0; 3], ..Sprite::default() }
        });
        let mut count = 0;
        for (slot, sprite) in buffer.iter_mut().zip(lying.chain(flakes).chain(grains).chain(motes)) {
            *slot = sprite;
            count += 1;
        }
        count + self.write_haunting(&mut buffer[count..])
    }

    pub fn write_layers(&self, buffer: &mut [f32]) -> Vec<PileLayer<'_>> {
        let mut result = Vec::new();
        let mut offset = 0;
        for layer in &self.layers {
            let span = 2 * layer.count();
            if layer.top <= 0.05 || offset + span > buffer.len() {
                continue;
            }
            layer.write_smoothed(&mut buffer[offset..offset + span]);
            let grown = smoothstep(0.0, REGROWTH, (self.air.clock - layer.returned_at) as f32);
            if grown < 1.0 {
                let (surface, bed) = buffer[offset..offset + span].split_at_mut(layer.count());
                for (level, bed) in surface.iter_mut().zip(bed.iter()) {
                    *level = bed + (*level - bed) * grown;
                }
            }
            result.push(PileLayer {
                offset,
                count: layer.count(),
                origin: layer.base(),
                top: layer.top,
                bottom: -layer.bed_depth,
                occluders: &layer.occluders,
                drift: layer.drift,
            });
            offset += span;
        }
        result
    }

    /// Turns the pieces into snowflakes that drift down and fade to white; together they leave a small drift.
    pub fn scatter(&mut self, pieces: &[ScatterPiece]) {
        self.scattered_until = self.air.clock + 8.0;
        for piece in pieces {
            let random = &mut self.air.random;
            let depth = random.next().powf(0.7);
            self.air.flakes.push(Flake {
                position: piece.position,
                velocity: piece.velocity,
                fall_speed: 70.0 + depth * 170.0 + random.range(0.0, 40.0),
                size: 1.8 + depth * 2.6,
                depth,
                alpha: 0.5 + depth * 0.45,
                soft: 0.7 - depth * 0.4,
                phase: random.range(0.0, PHASE_WRAP),
                sway_rate: random.range(0.6, 1.8),
                sway_amount: random.range(8.0, 24.0),
                settles: true,
                mass: 0.06,
                tint: piece.color,
                tint_amount: 1.0,
                blown: 0.0,
                leaf: 0.0,
            });
        }
    }

    // Windows

    /// A desktop slid past at `velocity` pixels a second, negative to the left: the air it drags along blows the
    /// falling snow that way for a moment.
    pub fn space_slid(&mut self, velocity: f32) {
        let wind = (velocity * SLIDE_DRAG).clamp(-SLIDE_WIND_MAX, SLIDE_WIND_MAX);
        if wind.abs() > self.air.slide_wind.abs() {
            self.air.slide_wind = wind;
        }
    }

    pub fn space_changed(&mut self) {
        self.space_changed_at = self.air.clock;
        for (id, ledge, _) in self.parking.drain(..) {
            self.dormant.insert(id, ledge);
        }
    }

    pub fn update_windows(
        &mut self,
        surfaces: &[WindowSurface],
        interval: f32,
        parked: &HashSet<WindowId>,
        elsewhere: &HashSet<WindowId>,
        sliding: bool,
    ) {
        self.air.leaf_budget = self.spare_leaves();
        for li in self.air.held_layers.drain(..) {
            if let Some(layer) = self.layers.get_mut(li) {
                layer.release_support();
            }
        }
        let known: HashMap<WindowId, usize> = self.ids.iter().enumerate().skip(1).map(|(li, &id)| (id, li)).collect();
        let mut seen = HashSet::new();
        for surface in surfaces {
            let left = surface.frame.min_x() + LEDGE_INSET;
            let right = surface.frame.max_x() - LEDGE_INSET;
            let top = surface.frame.max_y();
            let columns = ((surface.frame.width - 2.0 * LEDGE_INSET) / COLUMN_WIDTH) as i64;
            if columns < 8 {
                continue;
            }
            let columns = columns as usize;
            seen.insert(surface.id);

            let li = if let Some(&li) = known.get(&surface.id) {
                let ledge = &mut self.layers[li];
                if sliding && columns == ledge.count() {
                    self.air.pan(ledge, left, top);
                } else {
                    let before = V2::new(ledge.heap_x(), ledge.base().y);
                    if self.air.move_ledge(ledge, left, right, top, columns, interval) {
                        // The leaves lying on it are left behind with the rest of its heap.
                        self.drop_leaves(surface.id, before);
                    }
                }
                li
            } else {
                let ledge = match self.dormant.remove(&surface.id).or_else(|| self.unpark(surface.id)) {
                    Some(mut returning) => {
                        restore(&mut returning, left, top, columns);
                        returning.returned_at = self.air.clock;
                        returning
                    }
                    None => {
                        let phases = (self.air.random.range(0.0, TAU), self.air.random.range(0.0, TAU));
                        SnowLayer::new(V2::new(left, top), columns, COLUMN_WIDTH, LEDGE_DEPTH, true, phases)
                    }
                };
                self.layers.push(ledge);
                self.ids.push(surface.id);
                self.layers.len() - 1
            };
            let ledge = &mut self.layers[li];
            ledge.right = right;
            ledge.shape_corners(surface.corner_radius, surface.corner_exponent);
            let blockers: Vec<[f32; 4]> = surface.blockers.iter().map(|b| [b.min_x(), b.min_y(), b.max_x(), b.max_y()]).collect();
            ledge.occluders = blockers.iter().take(16).copied().collect();
            ledge.update_visibility(self.width, &blockers);
        }

        let layers = std::mem::take(&mut self.layers);
        let ids = std::mem::take(&mut self.ids);
        for (li, (mut ledge, id)) in layers.into_iter().zip(ids).enumerate() {
            if li == 0 || seen.contains(&id) {
                self.layers.push(ledge);
                self.ids.push(id);
            } else if elsewhere.contains(&id) {
                self.dormant.insert(id, ledge);
            } else if !parked.contains(&id) {
                let velocity = ledge.track_velocity + ledge.offset_velocity;
                self.drop_leaves(id, V2::new(ledge.heap_x(), ledge.base().y));
                self.air.shed(&mut ledge, 1.0, velocity, false);
            } else if self.air.clock - self.space_changed_at < 0.6 {
                self.dormant.insert(id, ledge);
            } else {
                self.parking.push((id, ledge, self.air.clock));
            }
        }
        if self.dormant.len() > 200
            && let Some(&stale) = self.dormant.keys().next()
        {
            self.dormant.remove(&stale);
        }
    }

    fn unpark(&mut self, id: WindowId) -> Option<SnowLayer> {
        let index = self.parking.iter().position(|entry| entry.0 == id)?;
        Some(self.parking.remove(index).1)
    }

    /// Off-screen windows that no desktop switch accounts for were minimized or hidden: their snow falls.
    fn expire_parking(&mut self) {
        while let Some(first) = self.parking.first()
            && self.air.clock - first.2 > 0.25
        {
            let (id, mut ledge, _) = self.parking.remove(0);
            self.drop_leaves(id, V2::new(ledge.heap_x(), ledge.base().y));
            self.air.shed(&mut ledge, 1.0, V2::ZERO, false);
        }
    }

    // Cursor

    fn track_cursor(&mut self, point: V2, dt: f32) {
        for li in self.air.held_layers.drain(..) {
            self.layers[li].release_support();
        }

        let center = point + CURSOR_OFFSET;
        let Some(previous) = self.air.cursor.replace(center) else { return };

        let delta = center - previous;
        let mut velocity = delta / dt.max(1.0 / 240.0);
        let speed = velocity.length();
        if speed > 3000.0 {
            velocity *= 3000.0 / speed;
        }
        self.air.cursor_velocity += (velocity - self.air.cursor_velocity) * 0.5;

        let low = center.y.min(previous.y) - CURSOR_RADIUS;
        let high = center.y.max(previous.y) + CURSOR_RADIUS;
        let left = center.x.min(previous.x) - CURSOR_RADIUS;
        let right = center.x.max(previous.x) + CURSOR_RADIUS;
        let touched: Vec<usize> = (0..self.layers.len())
            .filter(|&li| {
                let layer = &self.layers[li];
                let base = layer.base();
                low <= base.y + layer.top + 1.0
                    && high >= base.y - layer.bed_depth
                    && right >= base.x
                    && left <= base.x + layer.width()
                    && (li == 0 || layer.top > 0.01)
            })
            .collect();
        if touched.is_empty() {
            return;
        }

        let distance = delta.length();
        if distance > 400.0 {
            for &li in &touched {
                self.air.carve(&mut self.layers[li], li, center, V2::ZERO, 0.0, true);
            }
            return;
        }
        let steps = (distance / (CURSOR_RADIUS * 0.5)) as usize + 1;
        for step in 1..=steps {
            let position = previous + delta * (step as f32 / steps as f32);
            for &li in &touched {
                self.air.carve(&mut self.layers[li], li, position, velocity, distance / steps as f32, step == steps);
            }
        }
    }

    // Particles

    fn advance_flakes(&mut self, dt: f32) {
        let wind = self.air.breeze() + self.air.slide_wind;
        // A desktop's wind is short: the flakes take it up faster than the slow breeze, or it would be gone first.
        let follow = (dt * (1.2 + self.air.slide_wind.abs() * 0.03)).min(1.0);
        let settle = (dt * 1.6).min(1.0);
        let wake = self.air.cursor_wake();

        let mut i = self.air.flakes.len();
        while i > 0 {
            i -= 1;
            let mut flake = self.air.flakes[i];
            if flake.blown != 0.0 {
                // The nearer the flake, the faster the gust seems to carry it.
                flake.velocity.x += (flake.blown * GUST_WIND * (1.3 + 1.3 * flake.depth) - flake.velocity.x) * (dt * 2.5).min(1.0);
            } else {
                flake.velocity.x += (wind * (0.35 + 0.65 * flake.depth) - flake.velocity.x) * follow;
            }
            flake.velocity.y += (-flake.fall_speed - flake.velocity.y) * settle;

            if let Some(wake) = wake {
                stir(&mut flake.velocity, flake.position, wake, dt);
            }

            // Harmonic ratios 2.3 and 1.3 stay continuous across the 20π wrap.
            flake.phase += flake.sway_rate * dt;
            if flake.phase > PHASE_WRAP {
                flake.phase -= PHASE_WRAP;
            }
            if flake.tint_amount > 0.0 {
                flake.tint_amount = (flake.tint_amount - dt * 1.3).max(0.0);
            }
            let previous_y = flake.position.y;
            flake.position += motion(&flake) * dt;

            if flake.blown != 0.0 && (flake.position.x < -30.0 || flake.position.x > self.width + 30.0) {
                self.air.flakes.swap_remove(i);
                continue;
            }
            if flake.position.x < -30.0 {
                flake.position.x += self.width + 60.0;
            } else if flake.position.x > self.width + 30.0 {
                flake.position.x -= self.width + 60.0;
            }

            if flake.settles && self.accumulates && flake.blown == 0.0 {
                if let Some((li, column)) = landing(&self.layers, flake.position.x, previous_y, flake.position.y) {
                    if self.snowfall_held && flake.mass < 1.0 && li == 0 {
                        self.snowfall_held = false;
                        self.snowfall_released = true;
                        self.update_target();
                    }
                    if flake.leaf > 0.0 {
                        let (turn, flip) = (flake_turn(self.air.clock, &flake), tumble(self.air.clock, flake.leaf));
                        self.lay(flake.size, flake.leaf, flake.size * LEAF_BULK, flake.position.x, li, turn, flip, flake.velocity.x);
                    } else {
                        let layer = &mut self.layers[li];
                        let amount = flake.size * flake.size * DEPOSITION * flake.mass * layer.fill(column);
                        layer.deposit(amount, flake.position.x, true);
                    }
                    self.air.heights_changed = true;
                    self.recycle(i);
                    continue;
                }
            } else if flake.position.y < -flake.size * 2.0 {
                self.recycle(i);
                continue;
            }

            self.air.flakes[i] = flake;
        }
    }

    fn advance_grains(&mut self, dt: f32) {
        if self.air.grains.is_empty() {
            return;
        }
        let damping = (-GRAIN_DRAG * dt).exp();
        let air_drag = (-3.2 * dt).exp();
        let wind = self.air.breeze() + self.air.slide_wind;
        let time = (self.air.clock % 1000.0) as f32;
        let wake = self.air.cursor_wake();
        let mut i = self.air.grains.len();
        while i > 0 {
            i -= 1;
            let mut grain = self.air.grains[i];
            let previous_y = grain.position.y;
            if grain.blown {
                // Light powder: the wind keeps it moving toward the edge it was blown to, and it barely sinks.
                let speed = if grain.velocity.x < 0.0 { -GUST_WIND * 1.6 } else { GUST_WIND * 1.6 };
                grain.velocity.x += (speed - grain.velocity.x) * (dt * 2.0).min(1.0);
                grain.velocity.y -= GRAVITY * 0.08 * dt;
            } else if self.air.autumn {
                // A leaf catches the air: flung up, it soon loses its speed and flutters down, swinging from side to side.
                let phase = time * (1.2 + grain.seed) + grain.seed * 40.0;
                let fall = (35.0 + 30.0 * grain.seed) * (0.3 + 1.1 * phase.cos().abs());
                grain.velocity.x = (grain.velocity.x - wind) * air_drag + wind;
                grain.velocity.y = (grain.velocity.y + fall) * air_drag - fall;
                // Leaves falling off a window or swept out of a heap ride the cursor's wake as the falling ones do.
                if let Some(wake) = wake
                    && !grain.bulk
                {
                    stir(&mut grain.velocity, grain.position, wake, dt);
                }
                // As wide as a falling leaf's swing: tossed up, it glides off to one side and back as it comes down.
                grain.position.x += phase.cos() * 60.0 * dt;
            } else {
                grain.velocity.y -= GRAVITY * dt;
                grain.velocity *= damping;
            }
            grain.position += grain.velocity * dt;

            if grain.position.x < -10.0 || grain.position.x > self.width + 10.0 || grain.position.y < -10.0 {
                self.air.grains.swap_remove(i);
            } else if !grain.blown
                && grain.velocity.y <= 0.0
                && let Some((li, column)) = landing(&self.layers, grain.position.x, previous_y, grain.position.y)
            {
                let layer = &self.layers[li];
                if self.air.autumn && !grain.bulk && grain.lump <= 0.0 && layer.agitation > LEAF_SHAKING {
                    // A shaken window lets no leaf settle: it hops off again the way the window goes, and so works its
                    // way off the edges.
                    let random = &mut self.air.random;
                    grain.position.y = layer.surface(column) + 1.0;
                    let window = layer.track_velocity;
                    grain.velocity = V2::new(
                        window.x * random.range(0.6, 1.0) + random.range(-120.0, 120.0),
                        random.range(80.0, 180.0) + window.y.max(0.0) * random.range(0.6, 1.0),
                    );
                    self.air.grains[i] = grain;
                    continue;
                }
                if self.air.autumn && !grain.bulk && grain.lump <= 0.0 {
                    let size = if grain.size >= 5.5 { grain.size } else { 6.0 + grain.seed * 4.5 };
                    let (turn, flip) = (grain_turn(self.air.clock, &grain), tumble(self.air.clock, grain.seed));
                    self.lay(size, grain.seed, grain.area, grain.position.x, li, turn, flip, grain.velocity.x);
                    self.air.grains.swap_remove(i);
                    continue;
                }
                let layer = &mut self.layers[li];
                if grain.lump > 0.0 {
                    self.air.burst(&grain, layer);
                } else {
                    let missed = layer.deposit(grain.area, grain.position.x, true);
                    self.air.drop_past(layer, missed, grain.position.x, grain.velocity.x * 0.5);
                }
                self.air.heights_changed = true;
                self.air.grains.swap_remove(i);
            } else {
                self.air.grains[i] = grain;
            }
        }
    }

    fn recycle(&mut self, i: usize) {
        if self.air.flakes.len() <= self.target_flakes && !self.holding() {
            let y = self.height + self.air.random.range(4.0, 40.0);
            self.air.flakes[i] = self.air.make_flake(self.width, y);
        } else {
            self.air.flakes.swap_remove(i);
        }
    }

    /// No new snow falls until the gust has carried off the last flake, then the snowfall starts again from the top.
    fn spawn_flakes(&mut self) {
        if self.holding() {
            return;
        }
        while self.air.flakes.len() < self.target_flakes {
            let y = self.height + self.air.random.range(0.0, self.height * 0.8);
            let flake = self.air.make_flake(self.width, y);
            self.air.flakes.push(flake);
        }
    }

    /// No new snow falls while a gust clears the screen or the mode waits to switch.
    fn holding(&self) -> bool {
        self.next_autumn.is_some() || self.air.blowing()
    }

    /// Wanders between light lulls, steady snow and heavy flurries.
    fn vary(&mut self, dt: f32) {
        // In case none of the scattered snow makes it to the bottom, say because it all came to rest on windows.
        if self.snowfall_held && self.scattered_until > 0.0 && self.air.clock > self.scattered_until {
            self.snowfall_held = false;
            self.snowfall_released = true;
            self.update_target();
        }
        if self.snowfall != Snowfall::Variable {
            self.strength_target = 1.0;
        } else if self.air.clock >= self.next_shift {
            let random = &mut self.air.random;
            let roll = random.next();
            self.strength_target = if roll < 0.25 {
                random.range(0.25, 0.5)
            } else if roll < 0.75 {
                random.range(0.7, 1.3)
            } else {
                random.range(1.5, 2.1)
            };
            self.next_shift = self.air.clock + random.range(60.0, 240.0) as f64;
        }
        if (self.strength_target - self.air.strength).abs() <= 0.001 {
            return;
        }
        self.air.strength += (self.strength_target - self.air.strength) * (1.0 - (-dt / 25.0).exp());
        self.update_target();
    }

    // Gust

    /// The way the breeze blows now: 1 to the right, -1 to the left.
    pub fn breeze_direction(&self) -> f32 {
        if self.air.breeze() < 0.0 { -1.0 } else { 1.0 }
    }

    /// Clears all the snow: a gust in `direction` lifts it off the ground and the windows and blows it off the screen,
    /// or, `calm`, it melts away where it lies.
    pub fn blow_away(&mut self, direction: f32, calm: bool) {
        let direction = if direction < 0.0 { -1.0 } else { 1.0 };
        self.air.gust = Some(Gust { start: self.air.clock, direction, calm });
        // Snow kept for windows on other desktops goes too, out of sight.
        self.dormant.clear();
        for (_, ledge, _) in &mut self.parking {
            for i in 0..ledge.count() {
                ledge.set(i, 0.0);
            }
        }
    }

    fn blow(&mut self, dt: f32) {
        let Some(gust) = self.air.gust else { return };
        let age = (self.air.clock - gust.start) as f32;
        if age >= GUST_LENGTH && !self.air.flakes.iter().any(|flake| flake.blown != 0.0) {
            self.air.gust = None;
            return;
        }
        let margin = 40.0;
        let front = gust_front(self.width, age, gust.direction, margin);
        // Whatever the front has long passed is gone by now, however deep it was.
        let finish = age > GUST_SWEEP + if gust.calm { 0.6 } else { 0.8 };
        self.sweep_haunting(front, gust.direction, gust.calm, finish);
        if !gust.calm {
            let trailing = gust_front(self.width, age - GUST_LAG, gust.direction, margin);
            for flake in self.air.flakes.iter_mut().filter(|flake| flake.blown == 0.0) {
                // Each flake is caught a little further behind, so the snowfall doesn't go as one straight line.
                let stagger = flake.phase / PHASE_WRAP * 240.0;
                if finish || (trailing - flake.position.x) * gust.direction > stagger {
                    flake.blown = gust.direction;
                }
            }
            // Whatever else is in the air, like the snow off a window just closed, goes along with the snowfall.
            let autumn = self.air.autumn;
            let random = &mut self.air.random;
            for grain in self.air.grains.iter_mut().filter(|grain| !grain.blown) {
                if !(finish || (trailing - grain.position.x) * gust.direction > 0.0) {
                    continue;
                }
                grain.blown = true;
                grain.area = 0.0;
                grain.velocity = if autumn {
                    V2::new(gust.direction * random.range(500.0, 1100.0), random.range(60.0, 260.0))
                } else {
                    V2::new(gust.direction * random.range(800.0, 1600.0), random.range(60.0, 300.0))
                };
            }
        }
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().map(|(li, &id)| (id, li)).collect();
        for k in (0..self.lying.len()).rev() {
            let leaf = self.lying[k];
            let Some(&li) = shown.get(&leaf.layer) else {
                self.lying.remove(k);
                continue;
            };
            let x = self.layers[li].heap_x() + leaf.x;
            if !(finish || gust.calm || (front - x) * gust.direction > 0.0) {
                continue;
            }
            if gust.calm {
                self.lying.remove(k);
            } else {
                let random = &mut self.air.random;
                let velocity = V2::new(gust.direction * random.range(500.0, 1100.0), random.range(60.0, 260.0));
                self.fly(k, li, velocity, true);
            }
        }
        for layer in &mut self.layers {
            let mut dust = 0.0;
            for i in 0..layer.count() {
                let height = layer.heights[i];
                if height <= 0.0 {
                    continue;
                }
                let x = layer.center_x(i);
                if !gust.calm && (front - x) * gust.direction <= 0.0 {
                    continue;
                }
                let surface = layer.surface(i);
                let rate = if gust.calm { height * 2.5 + 6.0 } else { height * 8.0 + 50.0 };
                let loss = if finish { height } else { height.min(rate * dt) };
                layer.set(i, height - loss);
                self.air.heights_changed = true;
                if gust.calm || !layer.visible[i] {
                    continue;
                }
                dust += loss * layer.column_width;
                while dust >= 3.5 {
                    self.air.lift(V2::new(x, surface), gust.direction);
                    dust -= 3.5;
                }
            }
        }
    }

    /// The wind of a sliding desktop skims the drifts, most off the crests that stand above the snow around them:
    /// a little powder flies a short way downwind and settles again.
    fn skim(&mut self, dt: f32) {
        let share = self.air.slide_wind.abs() / SLIDE_WIND_MAX;
        if share <= 0.1 {
            return;
        }
        if self.air.autumn {
            // It lifts some of the leaves lying on top, never the mass under them.
            let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().map(|(li, &id)| (id, li)).collect();
            for k in (0..self.lying.len()).rev() {
                if self.air.random.next() >= share * dt {
                    continue;
                }
                let leaf = self.lying[k];
                let Some(&li) = shown.get(&leaf.layer) else { continue };
                let layer = &self.layers[li];
                let Some(i) = layer.index(layer.heap_x() + leaf.x) else { continue };
                if !layer.visible[i] || layer.surface(i) - layer.base().y >= leaf.top + 2.0 {
                    continue;
                }
                let random = &mut self.air.random;
                let velocity = V2::new(self.air.slide_wind * random.range(1.5, 3.0), random.range(30.0, 120.0));
                self.fly(k, li, velocity, false);
            }
            return;
        }
        for layer in &mut self.layers {
            let count = layer.count();
            let mut dust = 0.0;
            for i in 0..count {
                let height = layer.heights[i];
                if !layer.visible[i] || height <= 1.5 {
                    continue;
                }
                let around = (layer.heights[i.saturating_sub(12)] + layer.heights[(i + 12).min(count - 1)]) / 2.0;
                let loss = (height * 0.2).min((0.2 + (height - around).max(0.0) * 1.5) * share * dt);
                layer.set(i, height - loss);
                self.air.heights_changed = true;
                dust += loss * layer.column_width;
                while dust >= 1.5 {
                    self.air.puff(V2::new(layer.center_x(i), layer.surface(i)), 1.5);
                    dust -= 1.5;
                }
            }
        }
    }

    // Piles

    fn settle(&mut self, dt: f32) {
        // Melting nearly keeps pace with the snowfall, so heavier snowfall builds drifts only a little higher, and the
        // far sparser leaves wither as slowly.
        let falling = self.snowfall.flakes_per_megapoint() * self.air.strength * self.density * if self.air.autumn { LEAF_SHARE } else { 1.0 };
        let melt_pace = if falling > 0.0 { (falling / THAW_BALANCE).powf(THAW_EXPONENT) } else { 1.0 };
        let outside: Vec<[f32; 2]> = (0..self.layers.len())
            .map(|li| if li == 0 { [0.0; 2] } else { [outside(&self.layers, li, true), outside(&self.layers, li, false)] })
            .collect();
        for (layer, outside) in self.layers.iter_mut().zip(outside) {
            layer.outside = outside;
        }
        for (li, layer) in self.layers.iter_mut().enumerate() {
            if !self.accumulates || self.thaws {
                // Withered leaves leave nothing behind.
                let floor = if !self.accumulates || self.air.autumn {
                    0.0
                } else if li == 0 {
                    THAW_DEPTH
                } else {
                    LEDGE_THAW_DEPTH
                };
                if layer.thaw(dt * melt_pace, floor, THAW_RATE, THAW_TIME) {
                    self.air.heights_changed = true;
                }
            }
            // Shaken harder than gravity, a granular pile fluidizes and slumps to a flatter slope.
            // Leaves catch on one another and heap up steeper than snow.
            let steepest = if self.air.autumn { REPOSE * 1.8 } else { REPOSE };
            let repose = if layer.agitation > SHAKING { steepest * 0.35 } else { steepest };
            if layer.relax(repose, ROUNDING) {
                self.air.heights_changed = true;
            }
            if layer.spill_left > 1.5 {
                let velocity = layer.track_velocity + V2::new(-6.0, -15.0);
                self.air.release(layer.spill_left, edge_point(layer, true), velocity, V2::new(0.25, 1.0), 0.0);
                layer.spill_left = 0.0;
            }
            if layer.spill_right > 1.5 {
                let velocity = layer.track_velocity + V2::new(6.0, -15.0);
                self.air.release(layer.spill_right, edge_point(layer, false), velocity, V2::new(0.25, 1.0), 0.0);
                layer.spill_right = 0.0;
            }
        }
    }

    fn update_target(&mut self) {
        let per_megapoint = if self.snowfall_held { 0.0 } else { self.snowfall.flakes_per_megapoint() };
        let share = if self.air.autumn { LEAF_SHARE } else { 1.0 };
        self.target_flakes = (per_megapoint * share * self.air.strength * self.density * self.width * self.height / 1_000_000.0) as usize;
    }

    // Leaves

    /// A leaf comes down, slides a little the way it was going and settles flat on whatever lies there, adding its
    /// bulk to the heap under it so the next one lands on top.
    #[allow(clippy::too_many_arguments)]
    fn lay(&mut self, size: f32, seed: f32, area: f32, x: f32, li: usize, turn: f32, flip: f32, drift: f32) {
        let slide = (drift * 0.06).clamp(-8.0, 8.0);
        let spot = x + slide;
        let layer = &mut self.layers[li];
        let i = layer.clamped_index(spot);
        let rest = layer.surface(i) - layer.base().y;
        let steps = ((size / 5.0) as usize).max(1);
        if area > 0.0 {
            for step in 0..steps {
                let along = (step as f32 + 0.5) / steps as f32 - 0.5;
                let missed = layer.deposit(area / steps as f32, spot + along * size * 0.8, true);
                self.air.drop_past(layer, missed, spot, 0.0);
            }
        }
        let random = &mut self.air.random;
        let tilt = random.range(-0.2, 0.2);
        let left = i.saturating_sub(2);
        let right = (i + 2).min(layer.count() - 1);
        let slope = (layer.surface(right) - layer.surface(left)) / ((right - left).max(1) as f32 * layer.column_width);
        let flat = slope.atan() + tilt;
        let around = if wrapped(flat - turn).abs() <= PI / 2.0 { 0.0 } else { PI };
        let target = flat + around;
        let leaf = LyingLeaf {
            layer: self.ids[li],
            x: spot - layer.heap_x(),
            rest,
            top: layer.surface(i) - layer.base().y,
            size,
            seed,
            tilt,
            around,
            from_turn: target - wrapped(target - turn),
            from_flip: flip,
            slide,
            settling: 0.45 + slide.abs() / 90.0,
            settled: 0.0,
            life: random.range(90.0, 180.0),
            age: 0.0,
            fade: 1.0,
        };
        self.lying.push(leaf);
        if self.lying.len() > MAX_LYING {
            self.lying.remove(0);
        }
        self.air.heights_changed = true;
    }

    /// How many more leaves the heaps can show coming off them: as many as all the bulk lying about is worth, less
    /// the leaves already shown, lying or in the air. Leaves flown off a heap leave their bulk in it, so without
    /// this a heap thrown about again and again would show more and more of them.
    fn spare_leaves(&self) -> f32 {
        if !self.air.autumn {
            return 0.0;
        }
        let mut bulk: f32 = self.air.grains.iter().map(|grain| grain.area).sum();
        for layer in &self.layers {
            bulk += layer.heights.iter().map(|h| h.max(0.0)).sum::<f32>() * layer.column_width;
        }
        let shown = self.lying.len() + self.air.grains.iter().filter(|grain| !grain.bulk).count();
        bulk / LEAF_SHOWN - shown as f32
    }

    /// Sends a lying leaf off its heap into the air.
    fn fly(&mut self, k: usize, li: usize, velocity: V2, blown: bool) {
        let layer = &self.layers[li];
        let from = V2::new(layer.heap_x(), layer.base().y);
        self.fly_from(k, from, velocity, blown);
    }

    /// The leaves lying on a window that closed fall from where it was.
    /// `from` is where the heap's own positions start, and its base's height.
    fn drop_leaves(&mut self, id: WindowId, from: V2) {
        for k in (0..self.lying.len()).rev() {
            if self.lying[k].layer == id {
                self.fly_from(k, from, V2::ZERO, false);
            }
        }
    }

    fn fly_from(&mut self, k: usize, from: V2, velocity: V2, blown: bool) {
        let leaf = self.lying.remove(k);
        self.air.heights_changed = true;
        if self.air.grains.len() >= MAX_GRAINS {
            return;
        }
        self.air.grains.push(Grain {
            position: V2::new(from.x + leaf.x, from.y + leaf.rest + 1.0),
            velocity,
            area: 0.0,
            size: leaf.size,
            lump: 0.0,
            blown,
            seed: leaf.seed,
            bulk: false,
        });
    }

    /// Lying leaves fly up when the cursor kicks them or the heap under them comes off, sink with it as it settles,
    /// sink out of sight deep under the leaves that land on them, and wither away while the snow melts.
    fn tend_leaves(&mut self, dt: f32) {
        for layer in &mut self.layers {
            layer.lying_leaves = 0.0;
        }
        if self.lying.is_empty() {
            return;
        }
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().map(|(li, &id)| (id, li)).collect();
        for leaf in &self.lying {
            if let Some(&li) = shown.get(&leaf.layer) {
                self.layers[li].lying_leaves += 1.0;
            }
        }
        let speed = self.air.cursor_velocity.length();
        let mut k = self.lying.len();
        while k > 0 {
            k -= 1;
            let mut leaf = self.lying[k];
            let Some(&li) = shown.get(&leaf.layer) else {
                let kept = self.dormant.contains_key(&leaf.layer) || self.parking.iter().any(|entry| entry.0 == leaf.layer);
                if !kept {
                    self.lying.remove(k);
                }
                continue;
            };
            let layer = &self.layers[li];
            let x = layer.heap_x() + leaf.x;
            let Some(i) = layer.index(x) else {
                let velocity = V2::new(layer.track_velocity.x, 0.0);
                self.fly(k, li, velocity, false);
                continue;
            };
            let surface = layer.surface(i) - layer.base().y;
            let point = V2::new(x, layer.base().y + leaf.rest);
            if let Some(cursor) = self.air.cursor
                && speed > 8.0
                && (point - cursor).length() < CURSOR_RADIUS + leaf.size * 0.4
            {
                let velocity = self.air.swept(self.air.cursor_velocity);
                self.fly(k, li, velocity, false);
                self.air.last_cursor_push = self.air.clock;
                continue;
            }
            // The heap came off its window: every leaf on it goes along, down to the ones lying on the window itself.
            if let Some(thrown) = layer.thrown {
                let random = &mut self.air.random;
                let velocity = thrown + V2::new(random.range(-40.0, 40.0), random.range(10.0, 60.0));
                self.fly(k, li, velocity, false);
                continue;
            }
            // Shaken, the leaves on top go as often as the columns of the heap under them do.
            if self.air.random.next() < leaf_shake_rate(layer.agitation) * 2.0 * dt {
                let random = &mut self.air.random;
                let velocity = layer.track_velocity + layer.offset_velocity + V2::new(random.range(-80.0, 80.0), random.range(20.0, 80.0));
                self.fly(k, li, velocity, false);
                continue;
            }
            if surface < leaf.rest - 3.0 {
                // It goes the way the heap under it went, not the window.
                let carried = layer.track_velocity + layer.offset_velocity;
                let random = &mut self.air.random;
                let tossed = if layer.agitation > LEAF_SHAKING {
                    V2::new(random.range(-40.0, 40.0), random.range(20.0, 80.0))
                } else {
                    V2::new(random.range(-15.0, 15.0), 0.0)
                };
                self.fly(k, li, carried + tossed, false);
                continue;
            }
            // Deep in a heap a leaf is hidden by the ones above it and the mass drawn under them.
            let buried = surface > leaf.top + 10.0;
            if surface < leaf.top {
                let sink = (leaf.top - surface).min(20.0 * dt);
                leaf.top -= sink;
                leaf.rest -= sink;
                self.air.heights_changed = true;
            }
            if leaf.settled < 1.0 {
                leaf.settled = (leaf.settled + dt / leaf.settling).min(1.0);
                self.air.heights_changed = true;
            }
            if buried {
                leaf.fade -= dt * 2.0;
                self.air.heights_changed = true;
            } else if self.thaws {
                leaf.age += dt;
                if leaf.age > leaf.life {
                    leaf.fade -= dt / 4.0;
                    self.air.heights_changed = true;
                }
            }
            if leaf.fade <= 0.0 {
                self.lying.remove(k);
            } else {
                self.lying[k] = leaf;
            }
        }
    }
}

impl Air {
    /// Carries snow along with a window that the whole desktop is sliding past: nothing pushes the snow, so it doesn't react.
    fn pan(&mut self, ledge: &mut SnowLayer, left: f32, top: f32) {
        let shift = V2::new(left, top) - ledge.origin;
        if shift == V2::ZERO {
            return;
        }
        ledge.origin += shift;
        ledge.track += shift;
        ledge.previous_base_y = ledge.base().y;
        if ledge.top > 0.05 {
            self.last_window_motion = self.clock;
            self.windows_changed = true;
        }
    }

    /// Whether the window jumped, leaving its snow behind.
    fn move_ledge(&mut self, ledge: &mut SnowLayer, left: f32, right: f32, top: f32, columns: usize, interval: f32) -> bool {
        let old_left = ledge.origin.x;
        // The window's own edges, not the snow's: rounded down to whole columns, the snow's right end would twitch as
        // the left edge is dragged, and shake the snow about.
        let old_right = if ledge.right > 0.0 { ledge.right } else { old_left + ledge.width() };
        // A resize keeps the edge that didn't move fixed; a plain move shifts both edges alike.
        let shift = if (left - old_left).abs() <= (right - old_right).abs() { left - old_left } else { right - old_right };
        let displacement = V2::new(shift, top - ledge.origin.y);
        let resized = columns != ledge.count() || (left - old_left - shift).abs() > 0.5;
        if displacement == V2::ZERO && !resized {
            return false;
        }

        let elapsed = interval.max(1.0 / 120.0);
        // Only a motion that began unobserved, between slow polls, has to be picked up mid-flight.
        let was_resting = self.clock - ledge.moved_at > 0.3 && interval > 0.05;
        ledge.moved_at = self.clock;
        if ledge.top > 0.05 {
            self.last_window_motion = self.clock;
            self.windows_changed = true;
        }

        let jumped = displacement.length() / elapsed > JUMP_SPEED;
        if jumped {
            // The window vanished from under the snow and reappeared elsewhere: nothing carried the snow along.
            self.shed(ledge, 1.0, V2::ZERO, true);
            ledge.offset = V2::ZERO;
            ledge.offset_velocity = V2::ZERO;
            ledge.track_velocity = V2::ZERO;
        }

        if resized {
            // The snow stays put as the window's edge moves under it: it shifts along the window by whole columns, as
            // many as the drift it has built up comes to, so that rounding never adds up and walks it off.
            let before = (ledge.drift / COLUMN_WIDTH).round();
            ledge.drift += old_left + shift - left;
            let by = ((ledge.drift / COLUMN_WIDTH).round() - before) as isize;
            let mut fresh = vec![0.0; columns];
            for i in 0..ledge.count() {
                let h = ledge.heights[i];
                if h <= 0.0 {
                    continue;
                }
                let j = i as isize + by;
                if j >= 0 && (j as usize) < columns {
                    fresh[j as usize] += h;
                } else {
                    let position = V2::new(ledge.center_x(i), ledge.floor(i));
                    let leaves = (h - LEAF_COVER).max(0.0) / h;
                    self.release_leaves(h * COLUMN_WIDTH, leaves, position, ledge.track_velocity, V2::new(COLUMN_WIDTH / 2.0, h), 0.0);
                }
            }
            ledge.replace(fresh);
        }
        ledge.origin = V2::new(left, top);
        // Resized, the window doesn't move under its snow: the edge dragged is no jolt to it.
        ledge.track.x += left - old_left - shift;
        if jumped {
            ledge.track = ledge.origin;
        } else if was_resting {
            // Join the motion at its current pace, with the lag a steady follow would have, instead of as a jolt.
            ledge.track_velocity = displacement / elapsed;
            ledge.track = ledge.origin - ledge.track_velocity * TRACK_TIME;
        }
        jumped
    }

    /// Moves a ledge's snow as a body riding on its window: it feels gravity plus the inertia of the
    /// window's acceleration, sticks by friction, slides when that's exceeded, and flies free when the
    /// window drops away faster than gravity or stops short after rising.
    fn carry(&mut self, ledge: &mut SnowLayer, dt: f32) {
        ledge.thrown = None;
        let previous_velocity = ledge.track_velocity;
        smooth_damp(&mut ledge.track, &mut ledge.track_velocity, ledge.origin, TRACK_TIME, dt);
        let mut acceleration = (ledge.track_velocity - previous_velocity) / dt;
        acceleration.y *= self.jolt_scale;
        // Agitation is vibration: two consecutive back-and-forth swings, not a single start or stop.
        ledge.agitation *= (-dt / 0.3).exp();
        let magnitude = acceleration.length();
        if magnitude > 1.0 && acceleration.dot(ledge.swing_direction) < 0.0 {
            ledge.agitation = ledge.agitation.max(ledge.previous_swing_peak.min(ledge.swing_peak) / GRAVITY);
            ledge.previous_swing_peak = ledge.swing_peak;
            ledge.swing_peak = magnitude;
            ledge.swing_direction = acceleration / magnitude;
        } else {
            ledge.swing_peak = ledge.swing_peak.max(magnitude);
            if magnitude > 1.0 && ledge.swing_direction == V2::ZERO {
                ledge.swing_direction = acceleration / magnitude;
            }
        }

        if ledge.top <= 0.05 {
            ledge.offset = V2::ZERO;
            ledge.offset_velocity = V2::ZERO;
            return;
        }

        let mut lift = ledge.offset.y;
        let mut lift_velocity = ledge.offset_velocity.y;
        let (stick, hold, rub) = if self.autumn { (LEAF_ADHESION, LEAF_GRIP, LEAF_SLIP) } else { (ADHESION, GRIP, SLIP) };
        let fall = -GRAVITY - acceleration.y;
        if lift > 0.0 || fall > stick {
            lift_velocity += fall * dt;
            lift += lift_velocity * dt;
            if lift <= 0.0 {
                lift = 0.0;
                lift_velocity = 0.0;
            }
        }

        let pressure = if lift > 0.0 { 0.0 } else { (GRAVITY + stick + acceleration.y).max(0.0) };
        let drive = -acceleration.x;
        let mut slide_velocity = ledge.offset_velocity.x;
        if !(slide_velocity == 0.0 && drive.abs() <= hold * pressure) {
            let direction: f32 = if slide_velocity != 0.0 {
                slide_velocity.signum()
            } else if drive > 0.0 {
                1.0
            } else {
                -1.0
            };
            let next = slide_velocity + (drive - direction * rub * pressure) * dt;
            slide_velocity = if pressure > 0.0 && next * direction <= 0.0 { 0.0 } else { next };
        }

        let mut slide = ledge.offset.x + slide_velocity * dt;
        let carried = ledge.track_velocity + V2::new(slide_velocity, lift_velocity);
        while slide.abs() >= COLUMN_WIDTH {
            let direction = if slide > 0.0 { 1 } else { -1 };
            let (height, x, y) = ledge.slide_column(direction);
            slide -= direction as f32 * COLUMN_WIDTH;
            ledge.drift += direction as f32 * COLUMN_WIDTH;
            if height > 0.01 {
                let x = x + direction as f32 * COLUMN_WIDTH;
                let leaves = (height - LEAF_COVER).max(0.0) / height;
                self.release_leaves(height * COLUMN_WIDTH, leaves, V2::new(x, y), carried, V2::new(COLUMN_WIDTH / 2.0, height), 0.0);
            }
        }

        let moved = lift != ledge.offset.y || slide != ledge.offset.x;
        ledge.offset = V2::new(slide, lift);
        ledge.offset_velocity = V2::new(slide_velocity, lift_velocity);
        if moved {
            self.heights_changed = true;
        }

        let shakes = if self.autumn { LEAF_SHAKING } else { SHAKING };
        if lift > if self.autumn { LEAF_BREAK_LIFT } else { BREAK_LIFT } {
            self.shed(ledge, 1.0, carried, true);
            ledge.offset = V2::ZERO;
            ledge.offset_velocity = V2::ZERO;
            ledge.thrown = Some(carried);
        } else if ledge.agitation > shakes {
            // A granular bed shaken well past gravity throws grains off its surface.
            let rate = if self.autumn { leaf_shake_rate(ledge.agitation) } else { (0.06 * (ledge.agitation - shakes)).min(0.8) };
            let picks = (ledge.count() as f32 * rate * dt * 2.0 + self.random.next()) as usize;
            for _ in 0..picks {
                let column = ((self.random.next() * ledge.count() as f32) as usize).min(ledge.count() - 1);
                let h = ledge.heights[column];
                if h <= 0.2 {
                    continue;
                }
                ledge.set(column, h * 0.5);
                let velocity = carried + V2::new(self.random.range(-80.0, 80.0), self.random.range(50.0, 200.0));
                let position = V2::new(ledge.center_x(column), ledge.floor(column) + h * 0.5);
                let leaves = (h - LEAF_COVER).max(0.0).min(h * 0.5) / (h * 0.5);
                self.release_leaves(h * 0.5 * COLUMN_WIDTH, leaves, position, velocity, V2::new(COLUMN_WIDTH / 2.0, h * 0.5), 0.25);
            }
            if picks > 0 {
                self.heights_changed = true;
            }
        }
    }

    /// The part of a window slid past a wall carries no snow: the wall scrapes it off, and it heaps up against the
    /// wall, which holds up the heap's face as the cursor holds up the snow it touches.
    fn confine(&mut self, ledge: &mut SnowLayer, li: usize, width: f32, walls: (bool, bool)) {
        let count = ledge.count() as i64;
        let base = ledge.base().x;
        let first = if walls.0 { ((-base / COLUMN_WIDTH - 0.5).ceil() as i64).clamp(0, count) } else { 0 };
        let last = if walls.1 { (((width - base) / COLUMN_WIDTH - 0.5).floor() as i64).clamp(-1, count - 1) } else { count - 1 };
        if first == 0 && last == count - 1 {
            return;
        }
        if first > last {
            // Nothing of the window is left this side of the wall: its snow falls down the wall.
            let x = if first > 0 { 1.0 } else { width - 1.0 };
            for i in 0..ledge.count() {
                let h = ledge.heights[i];
                if h <= 0.05 {
                    continue;
                }
                ledge.set(i, 0.0);
                self.release(h * COLUMN_WIDTH, V2::new(x, ledge.floor(i)), V2::ZERO, V2::new(1.0, h), 0.0);
            }
            self.heights_changed = true;
            return;
        }
        if !self.held_layers.contains(&li) {
            self.held_layers.push(li);
        }
        self.scrape(ledge, 0..first as usize, first as usize, 1);
        self.scrape(ledge, (last + 1) as usize..ledge.count(), last as usize, -1);
    }

    /// Heaps the snow on `columns`, past a wall, onto column `edge` just inside it, packing down what wasn't packed
    /// already. Scraped off fast, some of it crumbles off the heap and tumbles back over the window.
    fn scrape(&mut self, ledge: &mut SnowLayer, columns: Range<usize>, edge: usize, inward: i32) {
        let mut area = 0.0;
        for i in columns {
            let packed = ledge.packed[i].min(ledge.heights[i]);
            area += (packed + (ledge.heights[i] - packed) * PACKING) * COLUMN_WIDTH;
            ledge.set(i, 0.0);
            ledge.hold(i);
        }
        if area <= 0.01 {
            return;
        }
        self.heights_changed = true;
        let direction = inward as f32;
        let speed = (-direction * (ledge.track_velocity.x + ledge.offset_velocity.x)).max(0.0);
        let crumbled = area * 0.4 * smoothstep(200.0, 1200.0, speed);
        if crumbled > 0.5 {
            let position = V2::new(ledge.center_x(edge), ledge.surface(edge));
            let velocity = V2::new(direction * speed * 0.25, 60.0 + speed * 0.12);
            self.release(crumbled, position, velocity, V2::new(COLUMN_WIDTH, 2.0), 0.6);
        }
        ledge.heap(area - crumbled, edge, inward, REPOSE * COLUMN_WIDTH);
    }

    /// Turns part of a ledge's snow into falling snow where it lies: a few columns at a time hold together
    /// as a lump, the rest pours off as powder. A heap of leaves thrown off `as_leaves` shows as the leaves it is
    /// made of; one left by a window closing or hiding goes unseen, leaving only the leaves lying on it to fall.
    fn shed(&mut self, ledge: &mut SnowLayer, share: f32, velocity: V2, as_leaves: bool) {
        let shown = if self.autumn && as_leaves {
            // The leaves lying on top go along with the rest, and count among the leaves the heap shows.
            let area = ledge.heights.iter().map(|h| h.max(0.0)).sum::<f32>() * share * COLUMN_WIDTH;
            let seen = ledge.heights.iter().map(|h| (h - LEAF_COVER).max(0.0)).sum::<f32>() * share * COLUMN_WIDTH;
            if area > 0.0 { (seen / LEAF_SHOWN - ledge.lying_leaves).max(0.0) * LEAF_SHOWN / area } else { 0.0 }
        } else {
            0.0
        };
        let mut start = 0;
        while start < ledge.count() {
            let end = ledge.count().min(start + self.random.range(2.0, 7.0) as usize);
            let mut area = 0.0;
            let mut lowest = f32::MAX;
            let mut highest = -f32::MAX;
            for i in start..end {
                let h = ledge.heights[i];
                if h <= 0.05 {
                    continue;
                }
                area += h * share * COLUMN_WIDTH;
                lowest = lowest.min(ledge.floor(i) + h * (1.0 - share));
                highest = highest.max(ledge.floor(i) + h);
            }
            let cohesion = if area > 14.0 && !self.autumn { self.random.range(0.4, 0.7) } else { 0.0 };
            for i in start..end {
                let h = ledge.heights[i];
                if h <= 0.05 {
                    continue;
                }
                let amount = h * share;
                ledge.set(i, h - amount);
                let position = V2::new(ledge.center_x(i), ledge.floor(i) + h - amount);
                self.release_leaves(amount * COLUMN_WIDTH * (1.0 - cohesion), shown, position, velocity, V2::new(COLUMN_WIDTH / 2.0, amount), 0.25);
            }
            if cohesion > 0.0 {
                let left = ledge.center_x(start);
                let right = ledge.center_x(end - 1);
                self.throw_lump(area * cohesion, V2::new((left + right) / 2.0, (lowest + highest) / 2.0), velocity);
            }
            start = end;
        }
        self.heights_changed = true;
    }

    fn throw_lump(&mut self, area: f32, position: V2, velocity: V2) {
        if self.autumn {
            self.release(area, position, velocity, V2::new(4.0, 2.0), 0.5);
            return;
        }
        let pieces = (area / 90.0) as usize + 1;
        let share = area / pieces as f32;
        for _ in 0..pieces {
            if self.grains.len() >= MAX_GRAINS {
                break;
            }
            let random = &mut self.random;
            self.grains.push(Grain {
                position: position + V2::new(random.range(-2.0, 2.0), random.range(-1.0, 1.0)),
                // Lumps break away a moment before the powder around them, so a falling sheet comes apart.
                velocity: velocity * (1.0 + random.range(-0.2, 0.2)) + V2::new(random.range(-25.0, 25.0), random.range(-80.0, 0.0)),
                area: share,
                size: (share.sqrt() * 1.05).min(11.0),
                lump: random.range(0.05, 1.0),
                blown: false,
                seed: random.range(0.001, 1.0),
                bulk: false,
            });
        }
    }

    /// A lump hitting the snow: most of it packs in where it lands, a few crumbs hop off and a puff of powder rises.
    fn burst(&mut self, lump: &Grain, layer: &mut SnowLayer) {
        let impact = (-lump.velocity.y).max(0.0);
        let force = (impact / 900.0).clamp(0.35, 1.3);
        let crumbs = lump.area * 0.18;
        for spot in [-0.3, 0.0, 0.3] {
            let x = lump.position.x + spot * lump.size;
            let missed = layer.deposit((lump.area - crumbs) / 3.0, x, true);
            self.drop_past(layer, missed, x, lump.velocity.x * 0.3);
        }
        let surface = layer.index(lump.position.x).map_or(lump.position.y, |i| layer.surface(i));
        let velocity = V2::new(lump.velocity.x * 0.3, impact * self.random.range(0.08, 0.16));
        self.release(crumbs, V2::new(lump.position.x, surface + 1.0), velocity, V2::new(lump.size * 0.4, 1.0), 0.6);

        let count = MAX_MOTES.saturating_sub(self.motes.len()).min((2.0 + lump.size * 0.6 * force) as usize);
        for _ in 0..count {
            let random = &mut self.random;
            let angle = random.range(0.15, PI - 0.15);
            let speed = random.range(25.0, 110.0) * force;
            self.motes.push(Mote {
                position: V2::new(lump.position.x + random.range(-0.5, 0.5) * lump.size, surface + random.range(0.0, lump.size * 0.4)),
                velocity: V2::new(angle.cos() * speed * 1.4 + lump.velocity.x * 0.2, angle.sin() * speed * 0.6),
                size: random.range(0.3, 0.6) * lump.size + 1.0,
                alpha: random.range(0.1, 0.22),
                age: 0.0,
                life: random.range(0.45, 0.9),
            });
        }
    }

    /// `scatter` varies each clump's speed by that fraction, so a thrown sheet of snow breaks up instead of flying as one band.
    /// As `release`, with `leaves` of what comes off a heap of leaves showing as the leaves it is made of.
    fn release_leaves(&mut self, area: f32, leaves: f32, position: V2, velocity: V2, spread: V2, scatter: f32) {
        if self.autumn && leaves > 0.0 && area > 0.02 {
            let count = ((area * leaves / LEAF_SHOWN + self.random.next()) as usize)
                .min(MAX_GRAINS.saturating_sub(self.grains.len()))
                .min(self.leaf_budget.max(0.0) as usize);
            self.leaf_budget -= count as f32;
            // The leaves carry the bulk along, so the heap makes up again just where and when they land.
            for _ in 0..count {
                let depth = self.random.range(0.3, 1.0);
                let size = self.leaf_length(depth);
                let random = &mut self.random;
                self.grains.push(Grain {
                    position: position + V2::new(random.range(-spread.x, spread.x), random.range(0.0, spread.y)),
                    velocity: velocity * (1.0 + random.range(-scatter, scatter)) + V2::new(random.range(-40.0, 40.0), random.range(10.0, 60.0)),
                    area: area / count as f32,
                    size,
                    lump: 0.0,
                    blown: false,
                    seed: random.range(0.001, 1.0),
                    bulk: false,
                });
            }
            if count > 0 {
                return;
            }
        }
        self.release(area, position, velocity, spread, scatter);
    }

    fn release(&mut self, area: f32, position: V2, velocity: V2, spread: V2, scatter: f32) {
        if area <= 0.02 {
            return;
        }
        let room = MAX_GRAINS.saturating_sub(self.grains.len());
        if room == 0 {
            self.overflow.push((area, position.x));
            return;
        }
        if self.autumn {
            // The bulk goes unseen and makes up a heap again wherever it lands; whatever leaves show come off above.
            let count = room.min((area / LEAF_AREA) as usize + 1).min(4);
            let share = area / count as f32;
            for _ in 0..count {
                let random = &mut self.random;
                self.grains.push(Grain {
                    position: position + V2::new(random.range(-spread.x, spread.x), random.range(0.0, spread.y)),
                    velocity: velocity * (1.0 + random.range(-scatter, scatter)) + V2::new(random.range(-30.0, 30.0), random.range(0.0, 40.0)),
                    area: share,
                    size: 2.0,
                    lump: 0.0,
                    blown: false,
                    seed: random.range(0.001, 1.0),
                    bulk: true,
                });
            }
            return;
        }
        let count = room.min(((area / 3.0) as usize).clamp(1, 6));
        let share = area / count as f32;
        for _ in 0..count {
            let random = &mut self.random;
            let jitter = V2::new(random.range(-12.0, 12.0) - 40.0 * scatter * random.range(-1.0, 1.0), random.range(-10.0, 8.0));
            self.grains.push(Grain {
                position: position + V2::new(random.range(-spread.x, spread.x), random.range(0.0, spread.y)),
                velocity: velocity * (1.0 + random.range(-scatter, scatter)) + jitter,
                area: share,
                size: (share.sqrt() * 1.05).clamp(1.1, 5.0),
                lump: 0.0,
                blown: false,
                seed: random.range(0.001, 1.0),
                bulk: false,
            });
        }
    }

    /// Snow that lands past the end of a ledge drops off that end instead.
    fn drop_past(&mut self, layer: &SnowLayer, area: f32, x: f32, velocity: f32) {
        if !layer.opens_at_edges || area <= 0.05 {
            return;
        }
        let left_side = x < layer.base().x + layer.width() / 2.0;
        self.release(area, edge_point(layer, left_side), V2::new(velocity, -10.0), V2::new(0.25, 1.0), 0.0);
    }

    fn carve(&mut self, layer: &mut SnowLayer, li: usize, center: V2, velocity: V2, travel: f32, last: bool) {
        let radius = CURSOR_RADIUS;
        let base = layer.base();
        let lower = (((center.x - radius - base.x) / COLUMN_WIDTH).floor() as i64).max(0);
        let upper = (((center.x + radius - base.x) / COLUMN_WIDTH).floor() as i64).min(layer.count() as i64 - 1);
        if lower > upper {
            return;
        }

        let local = center.y - base.y;
        let spray_share = if self.autumn { smoothstep(60.0, 500.0, velocity.length()) } else { smoothstep(60.0, 420.0, velocity.length()) };
        let mut pushed_left = 0.0;
        let mut pushed_right = 0.0;

        for i in lower as usize..=upper as usize {
            if !layer.visible[i] {
                continue;
            }
            let x = layer.center_x(i);
            let dx = x - center.x;
            let reach = radius * radius - dx * dx;
            if reach <= 0.0 {
                continue;
            }
            let half = reach.sqrt();
            // Heights here are measured up from the bed under this column.
            let bed_level = layer.bed[i];
            let bottom = (local - half - bed_level).max(0.0);
            let top = local + half - bed_level;
            if top <= bottom {
                continue;
            }
            let h = layer.heights[i];
            let buried = h > top;
            if last && !buried && h >= bottom - 1.0 {
                if !self.held_layers.contains(&li) {
                    self.held_layers.push(li);
                }
                layer.hold(i);
            }
            if h <= bottom + 0.01 {
                continue;
            }

            // A buried cursor only displaces the snow it sweeps through; the rest rests on top of it.
            let removed = if buried { (top - bottom) * (travel / (2.0 * radius)).min(1.0) } else { h - bottom };
            if removed <= 0.001 {
                continue;
            }
            layer.set(i, h - removed);
            self.last_cursor_push = self.clock;
            self.heights_changed = true;

            let mut area = removed * COLUMN_WIDTH;
            if self.autumn {
                // The lying leaves on top fly off by themselves; only the mass under them is swept out.
                let hidden = (LEAF_COVER - if buried { h - top } else { 0.0 }).max(0.0);
                area = (removed - hidden).max(0.0) * COLUMN_WIDTH;
            }
            let surface = layer.floor(i) + if buried { h - removed } else { h };
            let sprayed = if spray_share > 0.0 { self.emit(area * spray_share, V2::new(x, surface), center, velocity) } else { 0.0 };
            let side = if velocity.x.abs() > 40.0 { velocity.x } else { dx };
            if side < 0.0 {
                pushed_left += area - sprayed;
            } else {
                pushed_right += area - sprayed;
            }
        }

        for (area, x) in [(pushed_left, center.x - radius - COLUMN_WIDTH * 3.0), (pushed_right, center.x + radius + COLUMN_WIDTH * 3.0)] {
            if area <= 0.0 {
                continue;
            }
            let missed = layer.deposit(area, x, true);
            self.drop_past(layer, missed, x, velocity.x * 0.5);
        }
    }

    fn emit(&mut self, area: f32, origin: V2, center: V2, velocity: V2) -> f32 {
        if area <= 0.6 || self.grains.len() >= MAX_GRAINS {
            return 0.0;
        }
        let count = ((area / 1.6) as usize + 1).min(4);
        let share = area / count as f32;
        let speed = velocity.length();
        let offset = origin - center;
        let length = offset.length();
        let outward = if length > 0.001 { offset / length } else { V2::new(0.0, 1.0) };

        if self.autumn {
            // Leaves swept out of a heap skim along the way the cursor goes, a whole leaf at a time.
            let leaves = ((area / LEAF_AREA + self.random.next()) as usize).min(3);
            if leaves == 0 {
                return 0.0;
            }
            for _ in 0..leaves {
                let push = velocity + outward * (speed * self.random.range(0.05, 0.15));
                let velocity = self.swept(push);
                let depth = self.random.next().powf(1.2);
                let size = self.leaf_length(depth);
                let random = &mut self.random;
                self.grains.push(Grain {
                    position: origin + V2::new(random.range(-2.0, 2.0), random.range(0.0, 3.0)),
                    velocity,
                    area: area / leaves as f32,
                    size,
                    lump: 0.0,
                    blown: false,
                    seed: random.range(0.001, 1.0),
                    bulk: false,
                });
            }
            return area;
        }

        for _ in 0..count {
            let random = &mut self.random;
            let mut v = velocity * random.range(0.55, 1.25) + outward * (speed * random.range(0.05, 0.25));
            v.y += speed * random.range(0.02, 0.2);
            v += V2::new(random.range(-30.0, 30.0), random.range(-20.0, 40.0));
            self.grains.push(Grain {
                position: origin + V2::new(random.range(-1.0, 1.0), random.range(0.0, 2.0)),
                velocity: v,
                area: share,
                size: (share.sqrt() * 1.05).clamp(1.1, 4.2),
                lump: 0.0,
                blown: false,
                seed: random.range(0.001, 1.0),
                bulk: false,
            });
        }
        area
    }

    fn advance_motes(&mut self, dt: f32) {
        if self.motes.is_empty() {
            return;
        }
        let damping = (-3.2 * dt).exp();
        let drift = self.wind_speed() * 0.4;
        let mut i = self.motes.len();
        while i > 0 {
            i -= 1;
            let mote = &mut self.motes[i];
            mote.age += dt;
            if mote.age >= mote.life {
                self.motes.swap_remove(i);
            } else {
                mote.velocity.y -= 35.0 * dt;
                mote.velocity *= damping;
                mote.position += (mote.velocity + V2::new(drift, 0.0)) * dt;
            }
        }
    }

    fn make_flake(&mut self, width: f32, y: f32) -> Flake {
        if self.autumn {
            return self.make_leaf(width, y);
        }
        let wind = self.wind_speed();
        let random = &mut self.random;
        let near = random.next() < 0.02;
        let depth = if near { 1.0 } else { random.next().powf(1.6) };
        let fall_speed = if near { random.range(75.0, 100.0) } else { 16.0 + depth * 50.0 + random.range(0.0, 10.0) };
        Flake {
            position: V2::new(random.range(-30.0, width + 30.0), y),
            velocity: V2::new(wind * (0.35 + 0.65 * depth), -fall_speed),
            fall_speed,
            size: if near { random.range(8.0, 13.0) } else { 1.4 + depth * 4.4 + random.range(0.0, 1.0) },
            depth,
            alpha: if near { 0.28 } else { 0.4 + depth * 0.5 },
            soft: if near { 0.95 } else { 0.75 - depth * 0.45 },
            phase: random.range(0.0, PHASE_WRAP),
            sway_rate: random.range(0.5, 1.4),
            sway_amount: if near { random.range(10.0, 20.0) } else { random.range(4.0, 9.0) + depth * 10.0 },
            settles: !near,
            mass: 1.0,
            tint: [1.0; 3],
            tint_amount: 0.0,
            blown: 0.0,
            leaf: 0.0,
        }
    }

    fn make_leaf(&mut self, width: f32, y: f32) -> Flake {
        let wind = self.wind_speed();
        let depth = self.random.next().powf(1.2);
        let size = self.leaf_length(depth);
        let random = &mut self.random;
        let fall_speed = 24.0 + depth * 36.0 + random.range(0.0, 12.0);
        Flake {
            position: V2::new(random.range(-30.0, width + 30.0), y),
            velocity: V2::new(wind * (0.35 + 0.65 * depth), -fall_speed),
            fall_speed,
            size,
            depth,
            alpha: 0.88 + depth * 0.12,
            soft: 0.0,
            phase: random.range(0.0, PHASE_WRAP),
            sway_rate: random.range(1.1, 2.0),
            sway_amount: random.range(26.0, 46.0) + depth * 20.0,
            settles: true,
            mass: 1.0,
            tint: [1.0; 3],
            tint_amount: 0.0,
            blown: 0.0,
            leaf: random.range(0.001, 1.0),
        }
    }

    fn leaf_length(&mut self, depth: f32) -> f32 {
        7.0 + depth * 7.0 + self.random.range(0.0, 2.5)
    }

    /// A leaf the cursor sweeps along: it goes the cursor's way a little slower, barely lifting, and the air soon
    /// slows it down, so it lands a short way off.
    fn swept(&mut self, velocity: V2) -> V2 {
        let random = &mut self.random;
        let mut v = velocity * random.range(0.5, 0.9);
        let speed = v.length();
        if speed > 450.0 {
            v *= 450.0 / speed;
        }
        v + V2::new(random.range(-20.0, 20.0), random.range(10.0, 35.0) + speed * random.range(0.03, 0.1))
    }

    /// The air a moving cursor drags along, which carries off what falls through it: where it is and how fast it
    /// goes; none while the cursor is still.
    fn cursor_wake(&self) -> Option<(V2, V2)> {
        let cursor = self.cursor?;
        let speed = self.cursor_velocity.length();
        if speed <= 20.0 {
            return None;
        }
        Some((cursor, if speed > 500.0 { self.cursor_velocity * (500.0 / speed) } else { self.cursor_velocity }))
    }

    fn wind_speed(&self) -> f32 {
        self.breeze() + self.gust_speed() + self.slide_wind
    }

    fn breeze(&self) -> f32 {
        let t = self.clock;
        let breeze = (10.0 * (t * 0.07).sin() + 6.0 * (t * 0.19 + 1.7).sin() + 3.0 * (t * 0.43 + 4.1).sin()) as f32;
        breeze * (0.6 + 0.4 * self.strength)
    }

    fn gust_speed(&self) -> f32 {
        let Some(gust) = self.gust.filter(|gust| !gust.calm) else { return 0.0 };
        let age = (self.clock - gust.start) as f32;
        gust.direction * GUST_WIND * smoothstep(0.0, 0.12, age) * (1.0 - smoothstep(0.9, GUST_LENGTH, age))
    }

    fn blowing(&self) -> bool {
        self.gust.is_some_and(|gust| !gust.calm)
    }

    /// A pinch of snow torn off by the gust: a grain of powder, now and then with a faint puff around it.
    fn lift(&mut self, position: V2, direction: f32) {
        // Of a heap of leaves only the lying ones fly off; the bulk under them just goes.
        if self.grains.len() >= MAX_GRAINS || self.autumn {
            return;
        }
        let random = &mut self.random;
        self.grains.push(Grain {
            position: position + V2::new(random.range(-1.0, 1.0), random.range(-3.0, 0.0)),
            velocity: V2::new(direction * random.range(800.0, 1800.0), random.range(60.0, 420.0)),
            area: 0.0,
            size: random.range(1.1, 2.6),
            lump: 0.0,
            blown: true,
            seed: random.range(0.001, 1.0),
            bulk: false,
        });
        if random.next() >= 0.35 || self.motes.len() >= MAX_MOTES {
            return;
        }
        self.motes.push(Mote {
            position,
            velocity: V2::new(direction * random.range(200.0, 600.0), random.range(20.0, 120.0)),
            size: random.range(6.0, 14.0),
            alpha: random.range(0.06, 0.14),
            age: 0.0,
            life: random.range(0.6, 1.3),
        });
    }

    /// A pinch of powder off a crest, thrown downwind by a sliding desktop's wind; it lands again where it falls.
    fn puff(&mut self, position: V2, area: f32) {
        if self.grains.len() >= MAX_GRAINS {
            return;
        }
        let wind = self.slide_wind;
        let random = &mut self.random;
        self.grains.push(Grain {
            position: position + V2::new(random.range(-1.0, 1.0), random.range(-2.0, 0.0)),
            velocity: V2::new(wind * random.range(1.5, 3.0), random.range(60.0, 260.0)),
            area,
            size: random.range(1.1, 2.4),
            lump: 0.0,
            blown: false,
            seed: random.range(0.001, 1.0),
            bulk: false,
        });
        if random.next() >= 0.3 || self.motes.len() >= MAX_MOTES {
            return;
        }
        self.motes.push(Mote {
            position,
            velocity: V2::new(wind * random.range(0.8, 2.0), random.range(20.0, 90.0)),
            size: random.range(6.0, 13.0),
            alpha: random.range(0.06, 0.12),
            age: 0.0,
            life: random.range(0.5, 1.0),
        });
    }
}

/// How a flake actually moves: carried along, swaying from side to side, and for a leaf, swinging like a pendulum,
/// dropping fastest through the middle of each swing.
fn motion(flake: &Flake) -> V2 {
    let sway = (flake.phase.cos() + 0.3 * (flake.phase * 2.3).cos()) * flake.sway_amount;
    let flutter = if flake.leaf > 0.0 { 0.3 + 1.1 * flake.phase.cos().abs() } else { 1.0 + 0.12 * (flake.phase * 1.3).sin() };
    V2::new(flake.velocity.x + sway, flake.velocity.y * flutter)
}

/// Pulls a velocity toward the cursor's wake, the more the nearer the cursor.
fn stir(velocity: &mut V2, position: V2, wake: (V2, V2), dt: f32) {
    let radius: f32 = 80.0;
    let distance_squared = (position - wake.0).length_squared();
    if distance_squared >= radius * radius {
        return;
    }
    let falloff = 1.0 - distance_squared.sqrt() / radius;
    *velocity += (wake.1 - *velocity) * (falloff * falloff * 4.0 * dt).min(1.0);
}

fn spin(clock: f64, rate: f32) -> f32 {
    ((clock * rate as f64) % std::f64::consts::TAU) as f32
}

/// How a leaf in the air tumbles about its length.
fn tumble(clock: f64, seed: f32) -> f32 {
    (spin(clock, 0.5 + seed * 1.1) + seed * 50.0).cos()
}

fn tumble_speed(clock: f64, seed: f32) -> f32 {
    -(spin(clock, 0.5 + seed * 1.1) + seed * 50.0).sin() * (0.5 + seed * 1.1)
}

fn flake_turn(clock: f64, flake: &Flake) -> f32 {
    flake.leaf * 40.0 + flake.phase.sin() * 0.6 + spin(clock, flake.leaf - 0.5)
}

fn grain_turn(clock: f64, grain: &Grain) -> f32 {
    grain.seed * 40.0 + spin(clock, (grain.seed - 0.5) * 9.0)
}

/// A lying leaf follows the slope under it.
fn resting_turn(leaf: &LyingLeaf, layer: &SnowLayer, i: usize) -> f32 {
    let left = i.saturating_sub(2);
    let right = (i + 2).min(layer.count() - 1);
    let slope = (layer.surface(right) - layer.surface(left)) / ((right - left).max(1) as f32 * layer.column_width);
    slope.atan() + leaf.tilt + leaf.around
}

fn wrapped(angle: f32) -> f32 {
    let a = angle % TAU;
    if a > PI {
        a - TAU
    } else if a < -PI {
        a + TAU
    } else {
        a
    }
}

/// Where the gust's front is `age` seconds in, coming in from `margin` past the windward edge of a display `width` wide.
/// It hits at full speed and eases off as it crosses.
fn gust_front(width: f32, age: f32, direction: f32, margin: f32) -> f32 {
    let t = (age / GUST_SWEEP).clamp(0.0, 1.0);
    let travel = (width + 2.0 * margin) * (1.0 - (1.0 - t) * (1.0 - t));
    if direction > 0.0 { -margin + travel } else { width + margin - travel }
}

/// Puts snow that waited off screen back on its window, wherever the window is now, without treating the jump as motion.
fn restore(ledge: &mut SnowLayer, left: f32, top: f32, columns: usize) {
    ledge.resize(columns);
    ledge.origin = V2::new(left, top);
    ledge.track = ledge.origin;
    ledge.track_velocity = V2::ZERO;
    ledge.offset = V2::ZERO;
    ledge.offset_velocity = V2::ZERO;
    ledge.previous_base_y = top;
    ledge.moved_at = -1.0;
}

/// The highest visible snow surface crossed while falling from `from` to `to` at `x`, allowing for the surface moving meanwhile.
fn landing(layers: &[SnowLayer], x: f32, from: f32, to: f32) -> Option<(usize, usize)> {
    let mut hit = None;
    let mut highest = -f32::MAX;
    for (li, ledge) in layers.iter().enumerate().skip(1) {
        let Some(i) = ledge.index(x) else { continue };
        if !ledge.visible[i] {
            continue;
        }
        let surface = ledge.surface(i);
        // Entering the snow from above the window's edge counts, even if the surface rose this frame.
        if from >= ledge.previous_base_y + ledge.bed[i] && to <= surface && surface > highest {
            hit = Some((li, i));
            highest = surface;
        }
    }
    if hit.is_some() {
        return hit;
    }
    let ground = &layers[0];
    let i = ground.clamped_index(x);
    (to <= ground.surface(i)).then_some((0, i))
}

/// Where snow leaves an open layer: just past its end, at the height the snow reaches on the end column,
/// which on a rounded corner lies well down the curve.
fn edge_point(layer: &SnowLayer, left: bool) -> V2 {
    let end = if left { 0 } else { layer.count() - 1 };
    let x = if left { layer.base().x - 0.75 } else { layer.base().x + layer.width() + 0.75 };
    V2::new(x, layer.surface(end))
}

/// Critically damped follow of `target` (the SmoothDamp approximation), stable at any time step.
fn smooth_damp(current: &mut V2, velocity: &mut V2, target: V2, time: f32, dt: f32) {
    let omega = 2.0 / time;
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = *current - target;
    let temp = (*velocity + change * omega) * dt;
    *velocity = (*velocity - temp * omega) * decay;
    *current = target + (change + temp) * decay;
}

/// How fast a shaken heap of leaves comes apart: the share of its columns, and of the leaves lying on it, thrown off
/// in half a second.
fn leaf_shake_rate(agitation: f32) -> f32 {
    (0.3 * (agitation - LEAF_SHAKING).max(0.0)).min(0.8)
}

/// How high the snow lying just past one end of a window's ledge comes up that end, from below or beside it: a heap
/// grown up to a window's end props up the snow on it, so the window stops pouring onto the heap.
fn outside(layers: &[SnowLayer], li: usize, left: bool) -> f32 {
    let ledge = &layers[li];
    let end = if left { 0 } else { ledge.count() - 1 };
    let floor = ledge.floor(end);
    let x = if left { ledge.base().x - ledge.column_width * 0.5 } else { ledge.base().x + ledge.width() + ledge.column_width * 0.5 };
    let highest = layers
        .iter()
        .enumerate()
        .filter(|&(other, _)| other != li)
        .filter_map(|(_, layer)| layer.index(x).filter(|&i| layer.floor(i) <= floor).map(|i| layer.surface(i)))
        .fold(f32::MIN, f32::max);
    (highest - floor).max(0.0)
}

fn jolt_scale(height: f32) -> f32 {
    (TUNED_HEIGHT / height.max(1.0)).clamp(0.5, 2.0) * POINTER_GAIN
}

fn column_count(width: f32) -> usize {
    (width / COLUMN_WIDTH).ceil() as usize + 1
}
