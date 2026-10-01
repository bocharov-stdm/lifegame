//! A frame is everything the window needs to draw the world, assembled by the simulation
//! thread. The window never reads `World` directly: it draws the last ready frame, so a slow
//! tick cannot freeze the interface.
//!
//! The frame's size is bounded by the screen, not by the world: only visible creatures get into
//! a frame (selected by the body, not by the centre — a creature's size is a gene, and a big one
//! sticks into the frame even when its centre is far away). If too many are visible, a density
//! map is sent instead of circles — one picture the size of the screen.

use std::sync::Arc;

use life_core::flora::Patch;
use life_core::genome::creature;
use life_core::{Rules, World};
use life_sim::observe::{EventKind, GeneStat, Snapshot, gene_stats};

use crate::history::Sample;

/// We send no more circles in a frame: beyond that, a density map. 32 bytes per creature is
/// 8 MB, which is still easy to upload to the graphics card every frame; and at such a number
/// the circles are smaller than a pixel anyway.
pub const MAX_INSTANCES: usize = 250_000;

/// One circle. Exactly 32 bytes — that is how the shader reads them (`render.rs`).
/// The coordinates are relative to `Frame::origin`; the view, the heading and the ghost's flags
/// are in `meta` (see `motion.rs`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    /// The position at the frame's tick.
    pub x: f32,
    pub y: f32,
    /// The position in the previous frame: the window draws the motion between them.
    pub px: f32,
    pub py: f32,
    /// The body's radius in world units.
    pub r: f32,
    /// RGB and fullness (0‒255) in the last byte.
    pub color: u32,
    /// Seconds since birth at the moment the frame is assembled, for a ghost — since death.
    pub age: f32,
    /// Heading u16 | view << 16 | ghost | starved to death.
    pub meta: u32,
}

/// An RGBA picture stretched over the world's rectangle `rect` (x0, y0, x1, y1).
#[derive(Clone, Debug, Default)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub rect: (f64, f64, f64, f64),
}

/// Which part of the world the window shows now and how many pixels it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRequest {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub px_w: u32,
    pub px_h: u32,
}

impl ViewRequest {
    /// The visible area with a margin of half on each side: until a new frame has come, the camera
    /// manages to shift, and the edges must not be empty.
    pub fn padded(&self) -> (f64, f64, f64, f64) {
        let (dx, dy) = ((self.x1 - self.x0) * 0.5, (self.y1 - self.y0) * 0.5);
        (self.x0 - dx, self.y0 - dy, self.x1 + dx, self.y1 + dy)
    }
}

/// How the game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Extinct,
    Explosion,
}

/// The tempo and the game's state — for the top panel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    pub paused: bool,
    pub speed_index: usize,
    /// How many ticks a second actually come out.
    pub tps: f64,
    /// The tick cannot keep up with the chosen speed.
    pub lagging: bool,
    pub ended: Option<Ending>,
}

/// The world's rectangle: x0, y0, x1, y1 (x0 < x1, y0 < y1).
pub type Area = (f64, f64, f64, f64);

/// A summary of the creatures' genes; None — there is nobody.
pub type GeneSummary = Option<[GeneStat; creature::N]>;

/// A summary for a region of the world (the «Область» tool): who is inside (by the body's
/// centre) and what their genome is — next to the summary for the whole world at the same tick.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionStats {
    pub area: Area,
    pub tick: u64,
    pub plants: usize,
    pub creatures: usize,
    /// The mean tank fullness of the creatures inside, 0..1.
    pub fullness: Option<f64>,
    pub inside: GeneSummary,
    pub world: GeneSummary,
}

impl RegionStats {
    /// `world_genes` — the summary for the whole world, if it is already computed at this tick (a
    /// sample); otherwise it is computed here.
    pub fn of(world: &World, area: Area, world_genes: Option<GeneSummary>) -> RegionStats {
        let inside = |x: f64, y: f64| x >= area.0 && x <= area.2 && y >= area.1 && y <= area.3;
        let herd = world.creatures.iter().filter(|v| inside(v.x, v.y));
        let (n, sum) = herd.clone().fold((0, 0.0), |(n, s), v| (n + 1, s + v.energy / v.pheno.max_energy));
        let world_genes = world_genes
            .unwrap_or_else(|| gene_stats(&creature::GENES, world.creatures.iter().map(|v| &v.genome)));
        RegionStats {
            area,
            tick: world.tick,
            plants: world.plants.iter().filter(|p| inside(p.x, p.y)).count(),
            creatures: n,
            fullness: (n > 0).then(|| sum / n as f64),
            inside: gene_stats(&creature::GENES, herd.map(|v| &v.genome)),
            world: world_genes,
        }
    }
}

/// The selected creature as it is at the frame's tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected {
    pub age: f64,
    pub health: f64,
    pub max_health: f64,
    pub state: &'static str,
    pub id: u64,
    pub x: f64,
    pub y: f64,
    /// The body's radius.
    pub half: f64,
    pub vision: f64,
    pub speed: f64,
    pub energy: f64,
    pub max_energy: f64,
    /// The energy spent per tick.
    pub upkeep: f64,
    pub genome: [f64; creature::N],
    /// Its depth layer (y from and to), as its program set it this tick.
    pub layer: (f64, f64),
    /// What it bit on the frame's tick or the one before, if anything.
    pub eating: Option<life_core::creature::Morsel>,
    /// Its behaviour programs, the juvenile and the adult one, and the one it lives by now.
    pub programs: [life_core::creature::Program; 2],
    pub stage: usize,
    /// How many ticks each of its modes stays on, from its last decision (0: off).
    pub modes: [u64; life_core::creature::program::MODES],
    /// In the program it lives by: the block that decided this tick (None: none did, it stands),
    /// the settings that applied, and the deciding blocks whose tests held (bit i: block i).
    pub fired: Option<u8>,
    pub applied: u32,
    pub tried: u32,
}

impl Selected {
    /// `decided_by`: the stage its last decision came from, when known — on the tick it grows up it
    /// decided by its juvenile program, and `fired`, `applied` and `tried` are that program's.
    pub fn of(world: &World, id: u64, decided_by: Option<usize>) -> Option<Selected> {
        world.creature(id).map(|v| {
            let stage = decided_by.unwrap_or(v.stage());
            Selected {
                age: v.age,
                health: v.health,
                max_health: v.max_health(),
                state: if v.fleeing() {
                    "убегает"
                } else if v.torpid {
                    "в оцепенении"
                } else if v
                    .mind
                    .fired
                    .and_then(|i| v.programs[stage].blocks().get(usize::from(i)))
                    .map(|b| b.action)
                    == Some(life_core::creature::Action::Ambush)
                {
                    "в засаде"
                } else if v.mind.attack.is_some() {
                    "охотится / защищается"
                } else if !v.adult() {
                    "растёт"
                } else {
                    v.mind.activity.label()
                },
                id,
                x: v.x,
                y: v.y,
                half: v.pheno.half,
                vision: v.pheno.vision,
                speed: v.pheno.speed,
                energy: v.energy,
                max_energy: v.pheno.max_energy,
                upkeep: v.pheno.upkeep,
                genome: v.genome.to_values(),
                layer: v.pheno.layer(v.mind.stance.layer),
                eating: v.meal.filter(|m| m.tick + 1 >= world.tick).map(|m| m.food),
                programs: *v.programs,
                stage,
                modes: v.mind.modes.map(|until| until.saturating_sub(v.mind.tick)),
                fired: v.mind.fired,
                applied: v.mind.applied,
                tried: v.mind.tried,
            }
        })
    }
}

/// A chronicle entry. `kind` belongs to the observer's events (the same as in the report); the
/// game's own events (rules, planting) have none.
#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub tick: u64,
    pub kind: Option<EventKind>,
    pub text: String,
}

/// A visible corpse: what is left of it sets the mark's opacity, its stage the colour.
#[derive(Clone, Copy, Debug)]
pub struct CorpseMark {
    pub x: f64,
    pub y: f64,
    /// Depth on the previous frame: a rotting corpse sinks, drawn between the two.
    pub py: f64,
    /// Past its fresh time: rot.
    pub rot: bool,
    pub size: f64,
    /// What is left: of the whole meat, or of a skeleton's store.
    pub fullness: f64,
    /// Its bones: drawn pale and smaller.
    pub skeleton: bool,
}

impl CorpseMark {
    /// Fresh meat is red-brown, rot grey-green, bones pale.
    pub fn rgb(&self) -> [u8; 3] {
        match (self.skeleton, self.rot) {
            (true, _) => [206, 198, 172],
            (false, true) => [118, 128, 96],
            (false, false) => [176, 104, 88],
        }
    }
}

/// A short trace of a strike, already assembled by the simulation thread.
#[derive(Clone, Copy, Debug)]
pub struct ShotTrail {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub age: f32,
}

#[derive(Debug, Default)]
pub struct Frame {
    /// The world's number: grows at «Заново» and at a new world. By it the window understands that
    /// the history and the chronicle should start from a clean sheet.
    pub world_gen: u64,
    /// Edits of the world between ticks — a creature planted, new rules: they change it without
    /// a tick, so what was taken of the world at this tick (the census) is taken again.
    pub edits: u64,
    pub seed: u64,
    pub scale: f64,
    /// The rules the world runs by now.
    pub rules: Rules,
    pub tick: u64,
    pub plants: usize,
    pub creatures: usize,
    pub world_w: f64,
    pub world_h: f64,
    pub status: Status,
    /// With the render off the thread does not collect the world's content.
    pub render_world: bool,
    /// A far scale: motionless two-pixel squares without animation.
    pub dots: bool,
    /// The origin of the circles in the world (f64): at ×10 000 the world is 6·10⁷ wide, and in f32
    /// absolute coordinates would lose units of pixels.
    pub origin: (f64, f64),
    /// Plants, then creatures — they are drawn in this order.
    pub instances: Vec<Instance>,
    /// The food patches, when they are new: in the first frame of a world and after a rules
    /// change (frames are never dropped, so the window keeps the last ones it got).
    pub patches: Option<Arc<[Patch]>>,
    pub corpses: Vec<CorpseMark>,
    pub shots: Vec<ShotTrail>,
    /// Instead of circles, when more than `MAX_INSTANCES` are visible.
    pub density: Option<Raster>,
    /// The whole world in big cells; comes not in every frame.
    pub minimap: Option<Raster>,
    pub selected: Option<Selected>,
    /// What is new since the previous frame: the charts' points and the chronicle entries. Frames
    /// are not lost (the thread puts a new one only when the window has taken the previous one), so
    /// increments are enough.
    pub samples: Vec<Sample>,
    /// The world's samples — once in `SNAPSHOT_EVERY` ticks (less often on a huge world).
    pub snapshots: Vec<Snapshot>,
    /// A summary for the given region — when it was recomputed (at setting and at every sample).
    pub region: Option<RegionStats>,
    /// A census of the creatures, when the window asked for one (only while the world stands).
    pub census: Option<crate::census::Census>,
    pub log: Vec<LogEntry>,
    /// When the frame was assembled: from that moment the window counts the circles' age.
    pub built: Option<std::time::Instant>,
    /// How much of the simulation thread's time went into assembling the frame, ms.
    pub build_ms: f64,
    /// The mean price of a tick, ms.
    pub tick_ms: f64,
    /// The price of the last statistics sample, ms.
    pub snapshot_ms: f64,
    /// Each phase's share of a tick (`life_core::profile::Phase`), smoothed like `tick_ms`.
    pub phases: [f64; life_core::profile::Phase::N],
    /// The threads the decisions run on, and whether the simulation thread keeps to the fast cores.
    pub threads: usize,
    pub fast_cores: bool,
}

/// Light instances for a far scale: no matching of frames, ghosts, headings and sorting.
/// Bodies and plants keep their colours.
pub fn dots(world: &World, rect: (f64, f64, f64, f64), out: &mut Vec<Instance>) -> bool {
    use crate::motion::{DOT_BIT, KIND_CREATURE, KIND_PLANT, OLD};

    let (x0, y0, x1, y1) = rect;
    out.clear();
    let mut add = |x: f64, y: f64, color: u32, kind: u32| {
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return true;
        }
        out.push(Instance {
            x: (x - x0) as f32,
            y: (y - y0) as f32,
            px: (x - x0) as f32,
            py: (y - y0) as f32,
            r: 1.0,
            color,
            age: OLD,
            meta: (kind << 16) | DOT_BIT,
        });
        out.len() <= MAX_INSTANCES
    };
    let plant = rgba(plant_color(), 255);
    for p in &world.plants {
        if !add(p.x, p.y, plant, KIND_PLANT) {
            out.clear();
            return false;
        }
    }
    let color = rgba(CREATURE_COLOR, 255);
    for v in &world.creatures {
        if !add(v.x, v.y, color, KIND_CREATURE) {
            out.clear();
            return false;
        }
    }
    true
}

// ── colours (the palette of app/theme.py) ───────────────────────────────────

pub const WORLD_TOP: [u8; 3] = [31, 38, 47];
pub const WORLD_BOTTOM: [u8; 3] = [15, 18, 23];
pub const PLANT_COLOR: [u8; 3] = [93, 211, 158];
pub const CREATURE_COLOR: [u8; 3] = [205, 134, 255];

pub fn lerp(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
}

/// A colour and a byte in the last channel (for creatures — fullness) as one u32.
pub fn rgba(c: [u8; 3], a: u8) -> u32 {
    u32::from_le_bytes([c[0], c[1], c[2], a])
}

/// Sprouts are dark green: there are many of them, and they must not compete with those
/// that move. Charts and counters keep the bright `PLANT_COLOR`.
pub const SPROUT_COLOR: [u8; 3] = [64, 150, 84];

/// Plants are dimmer than animals: there are many of them, and they must not compete with those that move.
pub fn plant_color() -> [u8; 3] {
    lerp(WORLD_BOTTOM, SPROUT_COLOR, 0.75)
}

/// The density map: how many plants and creatures are in each cell of the world's rectangle,
/// in colour. Computed in one pass over the world, so its price does not depend on how many
/// creatures are visible.
pub fn density(world: &World, rect: (f64, f64, f64, f64), w: usize, h: usize, out: Raster) -> Raster {
    let (x0, y0, x1, y1) = rect;
    let (sx, sy) = (w as f64 / (x1 - x0), h as f64 / (y1 - y0));
    let mut counts = vec![[0u32; 2]; w * h];
    let mut hues = vec![[0u64; 3]; w * h];
    let mut add = |x: f64, y: f64, kind: usize, color: [u8; 3]| {
        let (cx, cy) = ((x - x0) * sx, (y - y0) * sy);
        if cx >= 0.0 && cy >= 0.0 && (cx as usize) < w && (cy as usize) < h {
            let i = cy as usize * w + cx as usize;
            counts[i][kind] += 1;
            if kind == 1 {
                for (sum, value) in hues[i].iter_mut().zip(color) {
                    *sum += value as u64;
                }
            }
        }
    };
    world.plants.iter().for_each(|p| add(p.x, p.y, 0, PLANT_COLOR));
    world.creatures.iter().for_each(|v| add(v.x, v.y, 1, CREATURE_COLOR));

    let mut rgba = out.rgba;
    rgba.clear();
    rgba.reserve(w * h * 4);
    for (c, hue) in counts.iter().zip(&hues) {
        let colors = [PLANT_COLOR, hue.map(|sum| (sum / c[1].max(1) as u64) as u8)];
        // Brightness by the logarithm: a lone creature is visible, and a crowd does not blind.
        let k = c.map(|n| if n == 0 { 0.0 } else { (0.6 + (n as f64).log2() / 10.0).min(1.0) });
        // Creatures on top of plants.
        let mut px = [0.0f64; 3];
        let mut alpha = 0.0f64;
        for (kind, &a) in k.iter().enumerate() {
            for (ch, v) in px.iter_mut().enumerate() {
                *v = *v * (1.0 - a) + colors[kind][ch] as f64 * a;
            }
            alpha = alpha * (1.0 - a) + a;
        }
        // A premultiplied colour: that is how egui blends it.
        rgba.extend([px[0] as u8, px[1] as u8, px[2] as u8, (alpha * 255.0) as u8]);
    }
    Raster { w, h, rgba, rect }
}

/// The minimap's size in cells: 480 across, in height by the world's proportions, but no fewer
/// than 8 rows (at ×1000 the world is 1500 times wider than tall).
pub fn minimap_size(world_w: f64, world_h: f64) -> (usize, usize) {
    let w = 480;
    let h = ((w as f64 * world_h / world_w).round() as usize).clamp(8, 320);
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::WorldConfig;

    #[test]
    fn кружок_ровно_32_байта() {
        assert_eq!(std::mem::size_of::<Instance>(), 32);
    }

    /// A guard on the frame's speed: 200 thousand visible creatures are assembled into a frame
    /// fast — together with matching against the previous frame (`motion.rs`) — and a whole ×400
    /// world (600 thousand plants) goes into a density map, and the frame weighs no more than a
    /// couple of megabytes, not tens.
    #[test]
    fn кадр_огромного_мира_быстрый_и_лёгкий() {
        use crate::motion::Motion;
        use life_core::rng::Rng;

        let mut world = World::new(&WorldConfig { scale: 400.0, n_creatures: Some(0), ..Default::default() });
        let mut rng = Rng::new(9);
        let flora = world.flora().clone();
        world.plants =
            (0..world.space.per_area(life_core::config::PLANT_MAX)).map(|_| flora.plant(&mut rng)).collect();
        let (w, h) = (world.space.width, world.space.height);

        // a view of a part of the world: ~200 thousand plants in the frame; all were born on one tick
        // — the worst case for matching plants (one big group)
        let part = (0.0, 0.0, w / 3.0, h);
        let mut out = Vec::new();
        let mut motion = Motion::default();
        assert!(motion.collect(&world, part, &mut out));
        world.plants.retain(|p| p.x.to_bits() % 7 != 0); // a part was eaten
        let start = std::time::Instant::now();
        assert!(motion.collect(&world, part, &mut out));
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("  [кадр] {} кружков за {ms:.1} мс", out.len());
        assert!(out.len() > 150_000);
        assert!(ms < 60.0, "сборка кадра {ms:.1} мс — окно получало бы кадры редко");

        // the whole world: more circles than the ceiling — a density map the size of the screen
        assert!(!motion.collect(&world, (0.0, 0.0, w, h), &mut out));
        let start = std::time::Instant::now();
        let r = density(&world, (0.0, 0.0, w, h), 960, 300, Raster::default());
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("  [плотность] {}x{} за {ms:.1} мс", r.w, r.h);
        assert!(r.rgba.len() <= 2 * 1024 * 1024);
        assert!(ms < 60.0, "карта плотности {ms:.1} мс");
    }

    #[test]
    fn плотность_считает_всех() {
        let world = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
        let (w, h) = minimap_size(world.space.width, world.space.height);
        let r = density(&world, (0.0, 0.0, world.space.width, world.space.height), w, h, Raster::default());
        assert_eq!(r.rgba.len(), w * h * 4);
        let lit = r.rgba.chunks(4).filter(|p| p[3] > 0).count();
        assert!(lit >= 1, "стартовые существа видны на миникарте");
    }
}
