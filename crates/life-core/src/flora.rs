//! Where the food grows: a density profile by depth and by width.
//!
//! A plant is placed along two axes independently: x by the width profile, y by the depth
//! profile (the density is the product of the two profiles). A profile is a function `f(t)` of
//! the share of the way from the near edge (the surface, the left edge) to the far one: `t`
//! from 0 to 1 over the whole length of the axis. The profiles are the world's rules
//! (`Rules::plant_depth`, `plant_width`), they can be changed in the middle of a game; `Flora`
//! is what is derived from them (like a phenotype from a genome), and it is recomputed together
//! with the rules.
//!
//! The distribution of food is a property of the world; a program's layer («слой») is a preference
//! in percent of depth and does not adjust to it.
//!
//! Without patches a plant draws exactly two numbers at any profile, x then y; in patches three
//! (below). The plain exponent by depth and the uniform width use the same expressions as before
//! the profiles, so those rules without patches plant bit for bit as then.
//!
//! Capacity is `PLANT_MAX` (per area) places, *slots*, and a slot holds at most one plant
//! (`World::spawn_plants`): a seed that lands in an occupied slot does not sprout. So a full
//! world follows the profile exactly and a grazed surface cannot hand its room to the deep sea.
//!
//! Without patches (`plant_patches` = 0) the slots are cells of equal fertility — equal steps of
//! each axis's distribution function, narrow where food is rich and wide where it is poor — and a
//! seed is drawn by the profile as above.
//!
//! With patches the food grows in islands and between them. The world is split into coarse
//! *regions* of equal fertility, about two patches each; every region holds the same number of
//! slots, so region by region the food still follows the profile. Part of a region's slots lie in
//! its patches, the rest are scattered over the region by the profile. The part in patches follows
//! the region's light — the depth profile's density at its centre: bright regions keep more in
//! patches, dark ones scatter more, and the world's mean is `plant_patch_share`. A region has one
//! to three patches with their own size, shape and weight: a heavier patch takes more of the
//! region's patch slots, so its seeds fall more often; a patch is smaller the darker it lies
//! (`PATCH_DARK_SIZE`). So deep patches are rare (regions are wide there), small and poor. Patch
//! centres are drawn by the profile inside their region, from a stream keyed by the world's seed
//! (not the world's stream). Slots lie on a sunflower spiral inside the patch, scattered slots on
//! a low-discrepancy sequence over the region; a seed draws three numbers — the slot, uniform over
//! all of them, then a jitter inside the slot.

use std::f64::consts::{PI, TAU};

use crate::config::{
    GAME_PLATEAU, OCEAN_PEAK, OCEAN_SURFACE, PATCH_DARK_SIZE, PATCH_STRETCH, PATCH_WEIGHT_MIN, PLANT_MAX,
    PLANT_RADIUS, WORLD_HEIGHT, WORLD_WIDTH,
};
use crate::plant::Plant;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::space::Space;

/// The law of density along an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Uniform,
    /// A straight line from the near edge to the far one: `1 − (1 − b)·t`.
    Linear,
    /// `e^(−k·t)`: it thins out fast, then a long tail.
    Exp,
    /// `ln(1 + k(1−t)) / ln(1 + k)`: it holds for long, then a drop to the far edge.
    Log,
    /// `1 − a·cos(2π·n·t)`: n rich strips, the peaks in the middles of the strips.
    Waves,
    /// A rough real sea: full food down to `GAME_PLATEAU` of the axis, then `e^(−k·s)` over the
    /// rest (`s` from 0 to 1 there, `k` the steepness): a nearly dead bottom.
    Game,
    /// A real ocean: a little poorer at the very surface, richest at `OCEAN_PEAK` of the axis (the
    /// deep chlorophyll maximum, where light and nutrients from below meet), then `e^(−k·s)` over
    /// the rest (`s` from 0 to 1 below the peak, `k` the steepness).
    Ocean,
}

impl Profile {
    pub const ALL: [Profile; 7] = [
        Profile::Uniform,
        Profile::Linear,
        Profile::Exp,
        Profile::Log,
        Profile::Waves,
        Profile::Game,
        Profile::Ocean,
    ];

    /// The name for a flag: `--rule plant_width_profile=waves`.
    pub fn key(self) -> &'static str {
        match self {
            Profile::Uniform => "uniform",
            Profile::Linear => "linear",
            Profile::Exp => "exp",
            Profile::Log => "log",
            Profile::Waves => "waves",
            Profile::Game => "game",
            Profile::Ocean => "ocean",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Profile::Uniform => "равномерно",
            Profile::Linear => "линейно",
            Profile::Exp => "экспонента",
            Profile::Log => "логарифм",
            Profile::Waves => "волны",
            Profile::Game => "игровое",
            Profile::Ocean => "океаническое",
        }
    }

    /// The profile's number in the rules — the value of the rule `plant_*_profile`.
    pub fn index(self) -> f64 {
        Profile::ALL.iter().position(|p| *p == self).expect("профиль есть в ALL") as f64
    }

    /// A profile by the rule's value (`with` lets only whole numbers through).
    pub fn of(value: f64) -> Profile {
        Profile::ALL[(value.max(0.0) as usize).min(Profile::ALL.len() - 1)]
    }

    pub fn parse(s: &str) -> Option<Profile> {
        let s = s.trim();
        Profile::ALL.into_iter().find(|p| p.key() == s || p.label() == s)
    }
}

/// The exponent's steepness is no more than this. Beyond it all the food lies in a fraction of
/// a percent of the axis, and `e^(−k)` at the far edge goes below the numbers' precision.
pub const MAX_STEEPNESS: f64 = 100.0;
/// Waves — no more than this: the distribution table divides the axis into `TABLE_BINS` parts,
/// and with more frequent waves each would get fewer than 40 bins.
pub const MAX_WAVES: f64 = 100.0;
/// Bins in the distribution table. At a world height of 126 000 (3:2, x1000) this is about
/// 30 px a bin — finer than a plant.
const TABLE_BINS: usize = 4096;

/// A food profile along one axis — as it is set in the world's rules. The numbers are like the
/// other rules': the profile is a number in `Profile::ALL`, the rest are the profiles'
/// parameters (each reads its own).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodAxis {
    pub profile: f64,
    /// Exponent: the steepness k.
    pub steepness: f64,
    /// Linear: how much food at the far edge, % of the near one.
    pub end: f64,
    /// Logarithm: the bend k.
    pub bend: f64,
    /// Waves: how many rich strips.
    pub waves: f64,
    /// Waves: the amplitude, % (100 — between the strips it is empty).
    pub amplitude: f64,
}

/// The axis parameters in order — the tails of the rules' names `plant_depth_*`, `plant_width_*`.
pub const AXIS_PARAMS: [&str; 6] = ["profile", "steepness", "end", "bend", "waves", "amplitude"];

/// The profile's axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Along {
    Depth,
    Width,
}

/// A food profile's rule: `plant_depth_waves` → (depth, "waves").
pub fn split_key(key: &str) -> Option<(Along, &str)> {
    let (along, param) = if let Some(p) = key.strip_prefix("plant_depth_") {
        (Along::Depth, p)
    } else {
        (Along::Width, key.strip_prefix("plant_width_")?)
    };
    AXIS_PARAMS.contains(&param).then_some((along, param))
}

impl FoodAxis {
    pub fn get(&self, param: &str) -> Option<f64> {
        Some(match param {
            "profile" => self.profile,
            "steepness" => self.steepness,
            "end" => self.end,
            "bend" => self.bend,
            "waves" => self.waves,
            "amplitude" => self.amplitude,
            _ => return None,
        })
    }

    pub fn slot(&mut self, param: &str) -> Option<&mut f64> {
        Some(match param {
            "profile" => &mut self.profile,
            "steepness" => &mut self.steepness,
            "end" => &mut self.end,
            "bend" => &mut self.bend,
            "waves" => &mut self.waves,
            "amplitude" => &mut self.amplitude,
            _ => return None,
        })
    }

    /// The limits past which a parameter loses meaning; Err — what is needed.
    pub fn check(param: &str, v: f64) -> Result<(), String> {
        let whole = v.fract() == 0.0;
        let (ok, need) = match param {
            "profile" => (
                whole && (0.0..Profile::ALL.len() as f64).contains(&v),
                format!(
                    "номер или имя профиля: {}",
                    Profile::ALL.map(|p| format!("{} ({})", p.key(), p.index())).join(", ")
                ),
            ),
            "steepness" => ((0.0..=MAX_STEEPNESS).contains(&v), format!("число от 0 до {MAX_STEEPNESS}")),
            "end" | "amplitude" => ((0.0..=100.0).contains(&v), "процент от 0 до 100".into()),
            "bend" => (v >= 0.0, "число не меньше 0".into()),
            "waves" => (whole && (1.0..=MAX_WAVES).contains(&v), format!("целое число от 1 до {MAX_WAVES}")),
            _ => (false, "известный параметр".into()),
        };
        if ok { Ok(()) } else { Err(need) }
    }

    pub fn kind(&self) -> Profile {
        Profile::of(self.profile)
    }

    /// The density at point `t` (a share of the axis from the near edge), from 0 to 1 — for the
    /// preview. The sampling goes by the same formulas.
    pub fn density(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self.kind() {
            Profile::Uniform => 1.0,
            Profile::Linear => 1.0 - (1.0 - self.end / 100.0) * t,
            Profile::Exp => (-self.steepness * t).exp(),
            Profile::Log => {
                // as k → 0 the logarithm turns into a straight line to zero
                if self.bend < 1e-9 { 1.0 - t } else { (self.bend * (1.0 - t)).ln_1p() / self.bend.ln_1p() }
            }
            Profile::Waves => {
                let a = self.amplitude / 100.0;
                (1.0 - a * (TAU * self.waves * t).cos()) / (1.0 + a)
            }
            Profile::Game => (-self.steepness * ((t - GAME_PLATEAU) / (1.0 - GAME_PLATEAU)).max(0.0)).exp(),
            Profile::Ocean if t < OCEAN_PEAK => OCEAN_SURFACE + (1.0 - OCEAN_SURFACE) * t / OCEAN_PEAK,
            Profile::Ocean => (-self.steepness * (t - OCEAN_PEAK) / (1.0 - OCEAN_PEAK)).exp(),
        }
    }

    /// The profile in words: «экспонента, крутизна 8».
    pub fn describe(&self) -> String {
        let p = self.kind();
        match p {
            Profile::Uniform => p.label().into(),
            Profile::Game => format!(
                "{}, ровно до {:.0}%, дальше крутизна {}",
                p.label(),
                GAME_PLATEAU * 100.0,
                self.steepness
            ),
            Profile::Ocean => format!(
                "{}, больше всего на {:.0}%, дальше крутизна {}",
                p.label(),
                OCEAN_PEAK * 100.0,
                self.steepness
            ),
            Profile::Linear => format!("{}, у дальнего края {:.0}%", p.label(), self.end),
            Profile::Exp => format!("{}, крутизна {}", p.label(), self.steepness),
            Profile::Log => format!("{}, изгиб {}", p.label(), self.bend),
            Profile::Waves => format!("{}, {} × {:.0}%", p.label(), self.waves, self.amplitude),
        }
    }
}

/// How to place a coordinate along an axis.
#[derive(Clone, Debug, PartialEq)]
enum Law {
    Uniform,
    /// The exponent's inverse distribution function — the same formula as before the profiles, so
    /// the default world matches bit for bit.
    Exp {
        lambda: f64,
        e_lo: f64,
        e_hi: f64,
    },
    /// A tabulated distribution function: `cdf[i]` is the share of plants up to the i-th bin
    /// border (from 0 to 1). Inside a bin the density is constant.
    Table(Box<[f64]>),
}

#[derive(Clone, Debug, PartialEq)]
struct Axis {
    lo: f64,
    hi: f64,
    law: Law,
}

impl Axis {
    /// An axis of length `len`; plants are placed in `[lo, hi]`, the profile by the share of the whole
    /// length.
    fn new(spec: &FoodAxis, len: f64, lo: f64, hi: f64) -> Axis {
        let law = match spec.kind() {
            Profile::Uniform => Law::Uniform,
            // the steepness is almost zero — uniform (otherwise 0/0 in the formula)
            Profile::Exp if spec.steepness < 1e-3 => Law::Uniform,
            Profile::Exp => {
                let lambda = spec.steepness / len;
                Law::Exp { lambda, e_lo: (-lambda * lo).exp(), e_hi: (-lambda * hi).exp() }
            }
            _ => table(spec, len, lo, hi).map_or(Law::Uniform, Law::Table),
        };
        Axis { lo, hi, law }
    }

    /// One random number — one coordinate.
    #[inline]
    fn sample(&self, rng: &mut Rng) -> f64 {
        self.at(rng.random())
    }

    /// The coordinate with share `u` of the plants before it (0 to 1): the inverse of `cdf`.
    /// `Rng::uniform` is `lo + (hi − lo)·u`, so a uniform axis draws the same bits as before.
    #[inline]
    fn at(&self, u: f64) -> f64 {
        match &self.law {
            Law::Uniform => self.lo + (self.hi - self.lo) * u,
            Law::Exp { lambda, e_lo, e_hi } => -(e_lo - u * (e_lo - e_hi)).ln() / lambda,
            Law::Table(cdf) => {
                // the first border up to which there is more than u; the bin is before it.
                // Empty bins (zero density) do not come up: their width by
                // the cdf is zero, and the border after them is no more than u.
                let i = cdf.partition_point(|c| *c <= u).clamp(1, cdf.len() - 1) - 1;
                let within = (u - cdf[i]) / (cdf[i + 1] - cdf[i]);
                let s = (i as f64 + within.clamp(0.0, 1.0)) / (cdf.len() - 1) as f64;
                (self.lo + (self.hi - self.lo) * s).clamp(self.lo, self.hi)
            }
        }
    }

    /// Share of plants before `at` — the inverse of `sample`, from 0 to 1.
    #[inline]
    fn cdf(&self, at: f64) -> f64 {
        let at = at.clamp(self.lo, self.hi);
        let share = match &self.law {
            Law::Uniform => (at - self.lo) / (self.hi - self.lo),
            Law::Exp { lambda, e_lo, e_hi } => (e_lo - (-lambda * at).exp()) / (e_lo - e_hi),
            Law::Table(cdf) => {
                let pos = (at - self.lo) / (self.hi - self.lo) * (cdf.len() - 1) as f64;
                let i = (pos as usize).min(cdf.len() - 2);
                cdf[i] + (cdf[i + 1] - cdf[i]) * (pos - i as f64)
            }
        };
        share.clamp(0.0, 1.0)
    }

    /// Which of `n` equal shares of the axis `at` falls into.
    #[inline]
    fn slot(&self, at: f64, n: usize) -> usize {
        ((self.cdf(at) * n as f64) as usize).min(n - 1)
    }
}

/// The distribution function by bins (the density is at the bins' middles). None if the
/// density is nowhere greater than zero or is not a number.
fn table(spec: &FoodAxis, len: f64, lo: f64, hi: f64) -> Option<Box<[f64]>> {
    let mut cdf = Vec::with_capacity(TABLE_BINS + 1);
    let mut total = 0.0;
    cdf.push(0.0);
    for i in 0..TABLE_BINS {
        let at = lo + (hi - lo) * (i as f64 + 0.5) / TABLE_BINS as f64;
        total += spec.density(at / len).max(0.0);
        cdf.push(total);
    }
    if !(total > 0.0 && total.is_finite()) {
        return None;
    }
    for c in &mut cdf {
        *c /= total;
    }
    Some(cdf.into_boxed_slice())
}

/// Where the food grows in this world: the axes with ready laws and the places for plants.
#[derive(Clone, Debug, PartialEq)]
pub struct Flora {
    x: Axis,
    y: Axis,
    /// Cells of equal fertility across and down — the slots when there are no patches: `nx * ny`
    /// is at least the world's plant cap.
    nx: usize,
    ny: usize,
    /// Patches in slot order; empty — plants are scattered into cells.
    patches: Vec<Patch>,
    /// Slots from `in_patches` on: scattered over their regions (`rx` × `ry`).
    scatters: Vec<Scatter>,
    in_patches: usize,
    rx: usize,
    ry: usize,
    slots: usize,
}

/// A region's scattered slots: `slots` consecutive slots from `first` over region (`i`, `j`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scatter {
    first: usize,
    slots: usize,
    i: usize,
    j: usize,
}

/// The light of a region never counts as less than this: even a dead region has a share in patches
/// the share rule can raise to 100%.
const MIN_LIGHT: f64 = 0.05;
/// The R2 sequence's steps (the plastic number's powers): scattered slots cover a region evenly.
const R2_X: f64 = 0.754_877_666_246_692_7;
const R2_Y: f64 = 0.569_840_290_998_053_2;

/// A patch: an ellipse squashed against the edges of the plant zone (its reach from the centre
/// differs by side), holding `slots` consecutive slots from `first`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Patch {
    pub x: f64,
    pub y: f64,
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
    pub slots: usize,
    first: usize,
    /// The spiral's turn, so patches do not all start their spiral eastwards.
    turn: f64,
    /// How far a plant strays from its slot, across and down: about one slot's width.
    jitter: f64,
}

/// The patch layout's stream: keyed by the world's seed, apart from the world's own stream.
const PATCH_STREAM: u64 = 0xF10A_0000_0000_0000;
/// The sunflower's turn between slots: slots fill the disc evenly at any count.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

impl Flora {
    pub fn new(rules: &Rules, space: &Space, seed: u64) -> Flora {
        // plants grow up to the surface: the plant zone is the world less a plant's radius
        let r = PLANT_RADIUS;
        // rows by the world's proportions: with uniform food the cells are near squares
        let cap = space.per_area(PLANT_MAX);
        let ny = rows(cap, space);
        let x = Axis::new(&rules.plant_width, space.width, r, space.width - r);
        let y = Axis::new(&rules.plant_depth, space.height, r, space.height - r);
        let Layout { patches, scatters, rx, ry } = layout(&x, &y, rules, space, seed, cap);
        let nx = cap.div_ceil(ny);
        let scattered = patches.is_empty() && scatters.is_empty();
        let slots = if scattered { nx * ny } else { cap };
        let in_patches = patches.last().map_or(0, |p| p.first + p.slots);
        Flora { x, y, nx, ny, patches, scatters, in_patches, rx, ry, slots }
    }

    /// How many places for plants the world has (one plant each).
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// The patches; empty when plants are scattered.
    pub fn patches(&self) -> &[Patch] {
        &self.patches
    }

    /// The cell a plant at (x, y) occupies when there are no patches.
    #[inline]
    fn cell(&self, x: f64, y: f64) -> usize {
        self.x.slot(x, self.nx) + self.nx * self.y.slot(y, self.ny)
    }

    /// A new plant and its place. Without patches — two random numbers, x and then y; in patches —
    /// three: the place, then the scatter inside the place across and into the depth.
    #[inline]
    pub fn plant(&self, rng: &mut Rng) -> Plant {
        if self.patches.is_empty() && self.scatters.is_empty() {
            let x = self.x.sample(rng);
            let y = self.y.sample(rng);
            return Plant::in_slot(x, y, self.cell(x, y));
        }
        let slot = ((rng.random() * self.slots as f64) as usize).min(self.slots - 1);
        let (jx, jy) = (rng.random(), rng.random());
        let (x, y) = self.slot_at(slot, jx, jy);
        Plant::in_slot(x, y, slot)
    }

    /// Where a plant in `slot` grows; `jx`, `jy` from 0 to 1 place it inside the slot.
    #[inline]
    fn slot_at(&self, slot: usize, jx: f64, jy: f64) -> (f64, f64) {
        if slot >= self.in_patches {
            return self.scattered_at(slot, jx, jy);
        }
        let p = &self.patches[self.patches.partition_point(|p| p.first <= slot) - 1];
        let k = slot - p.first;
        let rho = ((k as f64 + 0.5) / p.slots as f64).sqrt();
        let (sin, cos) = (p.turn + k as f64 * GOLDEN_ANGLE).sin_cos();
        let dx = rho * cos * if cos < 0.0 { p.left } else { p.right };
        let dy = rho * sin * if sin < 0.0 { p.up } else { p.down };
        let x = p.x + dx + p.jitter * (jx - 0.5);
        let y = p.y + dy + p.jitter * (jy - 0.5);
        (x.clamp(self.x.lo, self.x.hi), y.clamp(self.y.lo, self.y.hi))
    }

    /// Where a plant in a scattered slot grows: a fixed point of its region in the coordinates of
    /// the distribution functions, so by the profile, moved by up to a slot's width.
    #[inline]
    fn scattered_at(&self, slot: usize, jx: f64, jy: f64) -> (f64, f64) {
        let s = &self.scatters[self.scatters.partition_point(|s| s.first <= slot) - 1];
        let q = (slot - s.first + 1) as f64;
        let spread = 1.0 / (s.slots as f64).sqrt();
        let u = (0.5 + q * R2_X + spread * (jx - 0.5)).rem_euclid(1.0);
        let v = (0.5 + q * R2_Y + spread * (jy - 0.5)).rem_euclid(1.0);
        (self.x.at((s.i as f64 + u) / self.rx as f64), self.y.at((s.j as f64 + v) / self.ry as f64))
    }
}

/// The patches and the scattered slots of a world with patches; empty without them.
struct Layout {
    patches: Vec<Patch>,
    scatters: Vec<Scatter>,
    rx: usize,
    ry: usize,
}

/// Each region's share of slots in patches by its light: `min(1, c · light)`, with `c` such that
/// the mean over the regions is `share`.
fn patch_shares(light: &[f64], share: f64) -> Vec<f64> {
    if share >= 1.0 || share <= 0.0 {
        return vec![share.clamp(0.0, 1.0); light.len()];
    }
    let mean = |c: f64| light.iter().map(|l| (c * l).min(1.0)).sum::<f64>() / light.len() as f64;
    // at `hi` every region is all patches, so the mean is 1 > share
    let (mut lo, mut hi) = (0.0, 1.0 / light.iter().fold(f64::MAX, |m, &l| m.min(l)));
    for _ in 0..64 {
        let mid = (lo + hi) / 2.0;
        if mean(mid) < share {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    light.iter().map(|l| (hi * l).min(1.0)).collect()
}

/// Rows of a grid of `n` cells by the world's proportions.
fn rows(n: usize, space: &Space) -> usize {
    ((n as f64 * space.height / space.width).sqrt().round() as usize).clamp(1, n)
}

/// Patches over regions of equal fertility, in slot order, then the regions' scattered slots;
/// empty when the rules ask for no patches.
fn layout(x: &Axis, y: &Axis, rules: &Rules, space: &Space, seed: u64, cap: usize) -> Layout {
    let wanted = (rules.plant_patches * space.area_ratio()).round() as usize;
    if wanted == 0 {
        return Layout { patches: Vec::new(), scatters: Vec::new(), rx: 0, ry: 0 };
    }
    // about two patches a region, and every region holds at least one slot
    let ry = rows(wanted.div_ceil(2).min(cap), space);
    let rx = wanted.div_ceil(2).min(cap).div_ceil(ry);
    let regions = (rx * ry).min(cap);
    // light: the depth profile's density, at a region's centre and at a patch's
    let light = |at: f64| rules.plant_depth.density(at / space.height).max(0.0);
    let lights: Vec<f64> =
        (0..regions).map(|r| light(y.at(((r / rx) as f64 + 0.5) / ry as f64)).max(MIN_LIGHT)).collect();
    let shares = patch_shares(&lights, rules.plant_patch_share / 100.0);
    let mut rng = Rng::keyed(seed, PATCH_STREAM);
    let mut patches = Vec::with_capacity(2 * regions);
    let mut scattered = Vec::with_capacity(regions);
    let mut first = 0;
    for (r, share) in shares.iter().enumerate() {
        let (i, j) = ((r % rx) as f64, (r / rx) as f64);
        // every region its own equal share of the slots, part of them in its patches
        let all = (r + 1) * cap / regions - r * cap / regions;
        let own = ((all as f64 * share).round() as usize).min(all);
        scattered.push((r % rx, r / rx, all - own));
        let n = (1 + (rng.random() * 3.0) as usize).min(3).min(own);
        let mut drawn = [(0.0, Patch::default()); 3];
        for (weight, p) in drawn.iter_mut().take(n) {
            // the centre by the profile inside the region
            p.x = x.at((i + rng.random()) / rx as f64);
            p.y = y.at((j + rng.random()) / ry as f64);
            let size =
                rules.plant_patch_size * rng.uniform(0.5, 1.5) * light(p.y).sqrt().max(PATCH_DARK_SIZE);
            let stretch = PATCH_STRETCH.powf(rng.uniform(-1.0, 1.0)).sqrt();
            let (wide, tall) = (size * stretch, size / stretch);
            (p.left, p.right) = (wide.min(p.x - x.lo), wide.min(x.hi - p.x));
            (p.up, p.down) = (tall.min(p.y - y.lo), tall.min(y.hi - p.y));
            p.turn = rng.uniform(0.0, TAU);
            *weight = rng.uniform(PATCH_WEIGHT_MIN, 1.0);
            p.jitter = (PI * wide * tall).sqrt();
        }
        // the region's slots by weight, at least one each
        let total: f64 = drawn[..n].iter().map(|(w, _)| w).sum();
        let (mut before, mut sum) = (0, 0.0);
        for (q, (weight, p)) in drawn[..n].iter_mut().enumerate() {
            sum += *weight;
            let upto =
                if q + 1 == n { own } else { q + 1 + ((own - n) as f64 * sum / total).round() as usize };
            p.first = first + before;
            p.slots = upto - before;
            p.jitter /= (p.slots as f64).sqrt();
            before = upto;
            patches.push(*p);
        }
        first += own;
    }
    let mut scatters = Vec::with_capacity(regions);
    for (i, j, slots) in scattered {
        if slots > 0 {
            scatters.push(Scatter { first, slots, i, j });
            first += slots;
        }
    }
    debug_assert_eq!(first, cap);
    Layout { patches, scatters, rx, ry }
}

/// The density of food at a point (shares of the width and depth) by the rules, from 0 to 1 —
/// for the preview: the window has no need to build tables.
pub fn density(rules: &Rules, tx: f64, ty: f64) -> f64 {
    rules.plant_width.density(tx) * rules.plant_depth.density(ty)
}

/// «по глубине — игровое; по ширине — равномерно; заросли — 24 на участок 6000×4000, радиус ~200».
pub fn describe(rules: &Rules) -> String {
    let patches = if rules.plant_patches == 0.0 {
        "россыпью".to_string()
    } else {
        format!(
            "заросли — {} на участок {WORLD_WIDTH}×{WORLD_HEIGHT}, радиус ~{}, в них {}% растений",
            rules.plant_patches, rules.plant_patch_size, rules.plant_patch_share
        )
    };
    format!(
        "по глубине — {}; по ширине — {}; {patches}",
        rules.plant_depth.describe(),
        rules.plant_width.describe()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PLANT_DEPTH_DECAY;
    use crate::space::Shape;

    fn rules(pairs: &[(&str, f64)]) -> Rules {
        pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
    }

    /// The same rules with plants scattered by the profile, no patches.
    fn scattered(r: &Rules) -> Rules {
        r.with("plant_patches", 0.0).unwrap()
    }

    /// All profiles at extreme parameters, along both axes.
    fn every_profile() -> Vec<Rules> {
        let mut out = Vec::new();
        for p in Profile::ALL {
            for axis in ["depth", "width"] {
                let key = |param: &str| format!("plant_{axis}_{param}");
                for (param, values) in [
                    ("steepness", &[0.0, 1e-4, 0.5, 8.0, MAX_STEEPNESS][..]),
                    ("end", &[0.0, 10.0, 100.0][..]),
                    ("bend", &[0.0, 1e-12, 20.0, 1e9][..]),
                    ("waves", &[1.0, 3.0, MAX_WAVES][..]),
                    ("amplitude", &[0.0, 80.0, 100.0][..]),
                ] {
                    for &v in values {
                        out.push(rules(&[(&key("profile"), p.index()), (&key(param), v)]));
                    }
                }
            }
        }
        out
    }

    /// The exponent without patches — the default before the «игровое» profile — plants exactly
    /// as before profiles: the same formula, the same random numbers, the same bits.
    #[test]
    fn экспонента_россыпью_как_до_профилей_бит_в_бит() {
        let space = Space::default();
        let exp = scattered(&rules(&[("plant_depth_profile", Profile::Exp.index())]));
        let flora = Flora::new(&exp, &space, 1);
        let (mut a, mut b) = (Rng::new(11), Rng::new(11));
        for _ in 0..10_000 {
            // the former Plant::random
            let lambda = PLANT_DEPTH_DECAY / space.height;
            let e_top = (-lambda * PLANT_RADIUS).exp();
            let e_bottom = (-lambda * (space.height - PLANT_RADIUS)).exp();
            let x = a.uniform(PLANT_RADIUS, space.width - PLANT_RADIUS);
            let u = a.random();
            let y = -(e_top - u * (e_top - e_bottom)).ln() / lambda;

            let p = flora.plant(&mut b);
            assert_eq!((p.x.to_bits(), p.y.to_bits()), (x.to_bits(), y.to_bits()));
        }
    }

    /// Any profile draws exactly two numbers without patches and three in patches: changing the
    /// profile on the fly does not shift the world's stream more than it changes the plants themselves.
    #[test]
    fn два_числа_на_растение_россыпью_и_три_в_зарослях() {
        let space = Space::new(10.0, Shape::Square);
        for r in every_profile() {
            for (r, draws) in [(scattered(&r), 2), (r, 3)] {
                let flora = Flora::new(&r, &space, 1);
                let (mut a, mut b) = (Rng::new(5), Rng::new(5));
                for _ in 0..100 {
                    flora.plant(&mut a);
                    for _ in 0..draws {
                        b.random();
                    }
                }
                assert_eq!(a.next_u64(), b.next_u64(), "{r:?}");
            }
        }
    }

    #[test]
    fn растения_в_границах_при_любых_профилях() {
        for shape in Shape::ALL {
            let space = Space::new(7.0, shape);
            for r in every_profile().into_iter().flat_map(|r| [scattered(&r), r]) {
                let flora = Flora::new(&r, &space, 3);
                let mut rng = Rng::new(3);
                for _ in 0..2000 {
                    let p = flora.plant(&mut rng);
                    let eps = 1e-9 * space.width.max(space.height);
                    assert!(
                        p.x >= PLANT_RADIUS - eps && p.x <= space.width - PLANT_RADIUS + eps,
                        "x={} {r:?}",
                        p.x
                    );
                    assert!(
                        p.y >= PLANT_RADIUS - eps && p.y <= space.height - PLANT_RADIUS + eps,
                        "y={} {r:?}",
                        p.y
                    );
                }
            }
        }
    }

    /// The share of the sample by strips matches the density's integral.
    fn check_shape(axis: &str, r: &Rules) {
        let space = Space::new(1.0, Shape::R3x2);
        let flora = Flora::new(&scattered(r), &space, 1);
        let spec = if axis == "depth" { r.plant_depth } else { r.plant_width };
        let (len, lo) =
            if axis == "depth" { (space.height, PLANT_RADIUS) } else { (space.width, PLANT_RADIUS) };
        let hi = len - PLANT_RADIUS;
        const BANDS: usize = 20;
        const N: usize = 200_000;
        let mut got = [0usize; BANDS];
        let mut rng = Rng::new(17);
        for _ in 0..N {
            let p = flora.plant(&mut rng);
            let v = if axis == "depth" { p.y } else { p.x };
            got[(((v - lo) / (hi - lo) * BANDS as f64) as usize).min(BANDS - 1)] += 1;
        }
        // the density's integral over a strip — by a fine sum
        let fine = 200;
        let mass: Vec<f64> = (0..BANDS)
            .map(|b| {
                (0..fine)
                    .map(|i| {
                        let at = lo + (hi - lo) * (b as f64 + (i as f64 + 0.5) / fine as f64) / BANDS as f64;
                        spec.density(at / len)
                    })
                    .sum::<f64>()
            })
            .collect();
        let total: f64 = mass.iter().sum();
        for b in 0..BANDS {
            let expected = mass[b] / total;
            let share = got[b] as f64 / N as f64;
            assert!(
                (share - expected).abs() < 0.006,
                "{axis} {}: полоса {b} — {share:.4}, ожидалось {expected:.4}",
                spec.describe()
            );
        }
    }

    #[test]
    fn выборка_повторяет_плотность() {
        for axis in ["depth", "width"] {
            let key = |param: &str| format!("plant_{axis}_{param}");
            for pairs in [
                vec![(key("profile"), Profile::Uniform.index())],
                vec![(key("profile"), Profile::Linear.index()), (key("end"), 10.0)],
                vec![(key("profile"), Profile::Exp.index()), (key("steepness"), 3.0)],
                vec![(key("profile"), Profile::Log.index()), (key("bend"), 20.0)],
                vec![
                    (key("profile"), Profile::Waves.index()),
                    (key("waves"), 3.0),
                    (key("amplitude"), 100.0),
                ],
                vec![(key("profile"), Profile::Game.index())],
            ] {
                let pairs: Vec<(&str, f64)> = pairs.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                check_shape(axis, &rules(&pairs));
            }
        }
    }

    /// Preview: «игровое» is full at the rich edge, right up to the surface; the default ocean is
    /// full at its peak.
    #[test]
    fn плотность_для_предпросмотра_от_нуля_до_единицы() {
        let r = rules(&[("plant_depth_profile", Profile::Game.index())]);
        assert_eq!(density(&r, 0.5, 0.0), 1.0);
        assert_eq!(density(&r, 0.5, 0.05), 1.0);
        assert_eq!(density(&Rules::default(), 0.5, OCEAN_PEAK), 1.0);
        let exp = rules(&[("plant_depth_profile", Profile::Exp.index())]);
        assert!((density(&exp, 0.5, 0.05) - (-8.0 * 0.05_f64).exp()).abs() < 1e-12);
        for r in every_profile() {
            for i in 0..=50 {
                let d = density(&r, i as f64 / 50.0, 0.05 + 0.95 * i as f64 / 50.0);
                assert!((0.0..=1.0 + 1e-12).contains(&d), "{d} {r:?}");
            }
        }
    }

    #[test]
    fn профиль_словами() {
        assert_eq!(
            describe(&Rules::default()),
            "по глубине — океаническое, больше всего на 15%, дальше крутизна 8; по ширине — равномерно; заросли — 24 на участок 6000×4000, радиус ~200, в них 60% растений"
        );
        assert_eq!(
            describe(&scattered(&rules(&[("plant_depth_profile", Profile::Exp.index())]))),
            "по глубине — экспонента, крутизна 8; по ширине — равномерно; россыпью"
        );
    }

    /// The distribution function undoes sampling: a plant drawn with `u` sits at share `u`.
    #[test]
    fn cdf_inverts_sampling_for_every_profile() {
        let space = Space::new(7.0, Shape::Square);
        for r in every_profile() {
            let flora = Flora::new(&scattered(&r), &space, 1);
            let (mut a, mut b) = (Rng::new(17), Rng::new(17));
            for _ in 0..500 {
                let (ux, uy) = (a.random(), a.random());
                let p = flora.plant(&mut b);
                assert!((flora.x.cdf(p.x) - ux).abs() < 1e-6, "x: {ux} {r:?}");
                assert!((flora.y.cdf(p.y) - uy).abs() < 1e-6, "y: {uy} {r:?}");
            }
        }
    }

    /// Every slot is equally fertile: seeds spread evenly over slots, whatever the profile, in
    /// cells and in patches.
    #[test]
    fn seeds_fall_evenly_into_slots() {
        let space = Space::default();
        for r in [
            Rules::default(),
            rules(&[("plant_depth_profile", Profile::Waves.index()), ("plant_depth_amplitude", 100.0)]),
            rules(&[("plant_width_profile", Profile::Linear.index()), ("plant_width_end", 0.0)]),
        ]
        .into_iter()
        .flat_map(|r| [scattered(&r), r])
        {
            let flora = Flora::new(&r, &space, 1);
            if flora.patches.is_empty() {
                assert!(flora.slots() >= PLANT_MAX && flora.slots() < PLANT_MAX + flora.ny);
            } else {
                assert_eq!(flora.slots(), PLANT_MAX);
            }
            let mut hits = vec![0u32; flora.slots()];
            let mut rng = Rng::new(21);
            for _ in 0..100 * flora.slots() {
                let p = flora.plant(&mut rng);
                hits[p.slot().expect("a seed has a slot")] += 1;
            }
            // Poisson(100): 6 sigma either way
            assert!(
                hits.iter().all(|&n| (40..=160).contains(&n)),
                "{:?}..{:?} {r:?}",
                hits.iter().min(),
                hits.iter().max()
            );
        }
    }

    #[test]
    fn игровой_профиль_сытый_верх_и_мёртвое_дно() {
        let game = rules(&[("plant_depth_profile", Profile::Game.index())]).plant_depth;
        assert_eq!(game.kind(), Profile::Game);
        for t in [0.0, 0.1, GAME_PLATEAU] {
            assert_eq!(game.density(t), 1.0, "flat to {GAME_PLATEAU}: {t}");
        }
        let middle = (GAME_PLATEAU + 1.0) / 2.0;
        assert!((game.density(middle) - (-game.steepness / 2.0).exp()).abs() < 1e-12, "the exponent below");
        assert!((game.density(1.0) - (-8.0_f64).exp()).abs() < 1e-12, "the bottom is nearly dead");
        // continuous and never rising with depth
        let mut last = 1.0;
        for i in 0..=10_000 {
            let d = game.density(i as f64 / 10_000.0);
            assert!(d <= last + 1e-12 && last - d < 2e-3, "{i}: {last} -> {d}");
            last = d;
        }
        assert_eq!(game.describe(), "игровое, ровно до 20%, дальше крутизна 8");
        assert_eq!(Profile::parse("game"), Some(Profile::Game));
    }

    /// The ocean: poorer at the very surface, richest at `OCEAN_PEAK`, the exponent below; the
    /// plants grown follow it.
    #[test]
    fn the_ocean_profile_peaks_under_the_surface() {
        let r = Rules::default();
        assert_eq!(r.plant_depth.kind(), Profile::Ocean, "the default since the ocean reform");
        let ocean = r.plant_depth;
        assert_eq!(ocean.density(0.0), OCEAN_SURFACE);
        assert_eq!(ocean.density(OCEAN_PEAK), 1.0);
        let middle = (OCEAN_PEAK + 1.0) / 2.0;
        assert!((ocean.density(middle) - (-ocean.steepness / 2.0).exp()).abs() < 1e-12, "the exponent below");
        // continuous: rising to the peak, never rising below it
        let mut last = OCEAN_SURFACE;
        for i in 1..=10_000 {
            let t = i as f64 / 10_000.0;
            let d = ocean.density(t);
            assert!((d - last).abs() < 3e-3, "{t}: {last} -> {d}");
            assert!(if t <= OCEAN_PEAK { d >= last } else { d <= last }, "{t}: {last} -> {d}");
            last = d;
        }
        assert_eq!(ocean.describe(), "океаническое, больше всего на 15%, дальше крутизна 8");
        assert_eq!(Profile::parse("ocean"), Some(Profile::Ocean));
        assert_eq!(Profile::parse("океаническое"), Some(Profile::Ocean));

        // plants by 5% of depth: the densest band is at the peak, the surface one is poorer
        let space = Space::default();
        let flora = Flora::new(&scattered(&r), &space, 1);
        let mut bands = [0u32; 20];
        let mut rng = Rng::new(5);
        for _ in 0..50_000 {
            let y = flora.plant(&mut rng).y / space.height;
            bands[((y * 20.0) as usize).min(19)] += 1;
        }
        let densest = (0..20).max_by_key(|&i| bands[i]).unwrap();
        assert!((2..=3).contains(&densest), "the peak at 15%: {bands:?}");
        assert!(bands[0] < bands[densest] * 3 / 4, "the surface is poorer: {bands:?}");
        assert!(bands[19] * 100 < bands[densest], "a nearly dead bottom: {bands:?}");
    }

    /// Patches differ in size, shape and weight; every region holds the same number of slots;
    /// the layout depends on the seed only.
    #[test]
    fn заросли_разные_области_равные() {
        let space = Space::default();
        let r = Rules::default();
        let flora = Flora::new(&r, &space, 7);
        assert_eq!(flora, Flora::new(&r, &space, 7), "the same seed, the same layout");
        assert_ne!(flora.patches, Flora::new(&r, &space, 8).patches, "another seed, other patches");
        let n = flora.patches.len();
        assert!((16..=36).contains(&n), "about {} patches: {n}", r.plant_patches);
        let slots: Vec<usize> = flora.patches.iter().map(|p| p.slots).collect();
        let (lo, hi) = (*slots.iter().min().unwrap(), *slots.iter().max().unwrap());
        assert!(lo >= 1 && hi > 2 * lo, "patches differ in weight: {slots:?}");
        let widths: Vec<f64> = flora.patches.iter().map(|p| p.left + p.right).collect();
        let (w_lo, w_hi) = widths.iter().fold((f64::MAX, 0.0_f64), |(a, b), w| (a.min(*w), b.max(*w)));
        assert!(w_hi > 2.0 * w_lo, "patches differ in size: {widths:?}");
        // slots run on without gaps, patch after patch, then the scattered ones region by region
        let mut next = 0;
        for p in &flora.patches {
            assert_eq!(p.first, next);
            next += p.slots;
            // squashed against the plant zone, never past it
            assert!(p.x - p.left >= flora.x.lo - 1e-9 && p.x + p.right <= flora.x.hi + 1e-9);
            assert!(p.y - p.up >= flora.y.lo - 1e-9 && p.y + p.down <= flora.y.hi + 1e-9);
        }
        assert_eq!(next, flora.in_patches);
        for s in &flora.scatters {
            assert_eq!(s.first, next);
            next += s.slots;
        }
        assert_eq!(next, flora.slots());
        // every region holds the same number of slots, patches and scattered together
        let regions = flora.rx * flora.ry;
        let mut per_region = vec![0; regions];
        let region = |x: f64, y: f64| {
            let (i, j) = (flora.x.slot(x, flora.rx), flora.y.slot(y, flora.ry));
            i + flora.rx * j
        };
        for p in &flora.patches {
            per_region[region(p.x, p.y)] += p.slots;
        }
        for s in &flora.scatters {
            per_region[s.i + flora.rx * s.j] += s.slots;
        }
        let (lo, hi) = (per_region.iter().min().unwrap(), per_region.iter().max().unwrap());
        assert!(hi - lo <= 1, "regions differ: {per_region:?}");
    }

    /// `plant_patch_share` of the slots lie in patches, the rest scattered; 0 and 100 are all one
    /// or all the other. Deeper, in less light, patches are rarer, smaller and poorer.
    #[test]
    fn заросли_глубже_реже_мельче_беднее() {
        let space = Space::default();
        for (share, want) in [(60.0, 900), (0.0, 0), (100.0, PLANT_MAX), (35.0, 525)] {
            let r = rules(&[("plant_patch_share", share)]);
            let flora = Flora::new(&r, &space, 7);
            assert!(flora.in_patches.abs_diff(want) <= flora.rx * flora.ry, "{share}%: {}", flora.in_patches);
            assert_eq!(flora.slots(), PLANT_MAX);
            // every seed lands in the plant zone, patch or not
            let mut rng = Rng::new(3);
            for _ in 0..3000 {
                let p = flora.plant(&mut rng);
                assert!(p.x >= flora.x.lo && p.x <= flora.x.hi && p.y >= flora.y.lo && p.y <= flora.y.hi);
            }
        }
        // over many layouts: patches in the upper third against those below the middle
        let (mut upper, mut lower) = ([0.0; 3], [0.0; 3]); // count, mean width, mean slots
        for seed in 0..40 {
            for p in Flora::new(&Rules::default(), &space, seed).patches {
                let t = p.y / space.height;
                let side = if t < 1.0 / 3.0 {
                    &mut upper
                } else if t > 0.5 {
                    &mut lower
                } else {
                    continue;
                };
                side[0] += 1.0;
                side[1] += p.left + p.right;
                side[2] += p.slots as f64;
            }
        }
        let mean = |s: [f64; 3]| (s[1] / s[0], s[2] / s[0]);
        let ((w_up, n_up), (w_low, n_low)) = (mean(upper), mean(lower));
        assert!(lower[0] < upper[0] * 0.8, "deep patches rarer: {} vs {}", lower[0], upper[0]);
        assert!(w_low < w_up * 0.85, "deep patches smaller: {w_low:.0} vs {w_up:.0}");
        assert!(n_low < n_up * 0.7, "deep patches poorer: {n_low:.1} vs {n_up:.1}");
    }

    /// Patches do not break the profile: averaged over layouts, bands of depth and of width
    /// get about the profile's share of the plants.
    #[test]
    fn заросли_не_ломают_профиль() {
        let space = Space::default();
        let top = PLANT_RADIUS / space.height;
        const BANDS: usize = 5;
        // the profile's mass over a share of its axis
        let mass = |spec: &FoodAxis, from: f64, to: f64| -> f64 {
            (0..1000).map(|i| spec.density(from + (to - from) * (i as f64 + 0.5) / 1000.0)).sum()
        };
        for r in [
            Rules::default(),
            rules(&[("plant_depth_profile", Profile::Exp.index())]),
            rules(&[("plant_width_profile", Profile::Linear.index()), ("plant_width_end", 0.0)]),
        ] {
            let mut down = [0.0; BANDS];
            let mut across = [0.0; BANDS];
            let (seeds, each) = (30, 10_000);
            for seed in 0..seeds {
                let flora = Flora::new(&r, &space, seed);
                let mut rng = Rng::new(seed);
                for _ in 0..each {
                    let p = flora.plant(&mut rng);
                    let t = (p.y / space.height - top) / (1.0 - top);
                    down[((t * BANDS as f64) as usize).min(BANDS - 1)] += 1.0;
                    across[((p.x / space.width * BANDS as f64) as usize).min(BANDS - 1)] += 1.0;
                }
            }
            let step = (1.0 - top) / BANDS as f64;
            let depth: Vec<f64> = (0..BANDS)
                .map(|b| mass(&r.plant_depth, top + step * b as f64, top + step * (b + 1) as f64))
                .collect();
            let width: Vec<f64> = (0..BANDS)
                .map(|b| mass(&r.plant_width, b as f64 / BANDS as f64, (b + 1) as f64 / BANDS as f64))
                .collect();
            let total = (seeds * each) as f64;
            for (name, got, want) in [("depth", down, depth), ("width", across, width)] {
                let sum: f64 = want.iter().sum();
                for b in 0..BANDS {
                    let (share, expected) = (got[b] / total, want[b] / sum);
                    assert!(
                        (share - expected).abs() < 0.03,
                        "{name} {}: band {b} — {share:.3}, the profile gives {expected:.3}",
                        describe(&r)
                    );
                }
            }
        }
    }
}
