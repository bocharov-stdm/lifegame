//! Observing the world without a window: what a person or an AI uses to understand WHAT is
//! happening in it and WHY.
//!
//! - [`Snapshot`] — a sample of the world: the counts, the accumulated counters of births and
//!   deaths, the spread of each gene (not only the mean: the mean hides a split into two
//!   kinds), where by depth and by width the creatures live and the plants grow, fullness.
//! - [`EventTracker`] / [`events`] — a chronicle by samples: collapses and rises of the
//!   population with causes, extinction, plants at the ceiling, gene shifts, creatures squeezing
//!   into a narrow layer.
//! - [`ascii_map`] — a map of the world as text: layers, clusters, empty edges.

use life_core::config::*;
use life_core::creature::{Activity, Creature, Diet};
use life_core::genome::{GeneSpec, Genome, creature};
use life_core::{Counters, World};

/// How many bands the depth is divided into in a sample (0 — the surface).
pub const DEPTH_BANDS: usize = 10;
/// How many bands the width is divided into (0 — the left edge): one sees where life has
/// drawn together when the food is unevenly distributed across the width.
pub const WIDTH_BANDS: usize = 10;

/// The spread of a quantity over the population.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub mean: f64,
}

impl Spread {
    /// None for an empty sample. The order of the values is spoiled (sorted in place).
    pub fn of(values: &mut [f64]) -> Option<Spread> {
        if values.is_empty() {
            return None;
        }
        values.sort_unstable_by(f64::total_cmp);
        let q = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        Some(Spread { p10: q(0.1), p50: q(0.5), p90: q(0.9), mean })
    }
}

/// A choice gene has no more variants than this (a test in `tests/observe.rs` checks it
/// against the gene tables): the shares lie in an array, not a vector, so that a sample stays
/// cheap to copy.
pub const MAX_VARIANTS: usize = 8;

/// A summary of one gene over the population.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneStat {
    /// A numeric gene: the spread.
    Number(Spread),
    /// A choice gene: the share of each variant (0..1) in the order of the variants.
    Shares([f64; MAX_VARIANTS]),
}

impl GeneStat {
    pub fn spread(&self) -> Option<&Spread> {
        match self {
            GeneStat::Number(s) => Some(s),
            GeneStat::Shares(_) => None,
        }
    }

    pub fn shares(&self) -> Option<&[f64; MAX_VARIANTS]> {
        match self {
            GeneStat::Shares(s) => Some(s),
            GeneStat::Number(_) => None,
        }
    }
}

/// A summary of the population's genes by the species' table: the spread of numeric genes (one
/// sort a gene), the shares for choice genes (one pass). None — there is nobody.
pub fn gene_stats<'a, G: Genome, const N: usize>(
    genes: &[GeneSpec; N],
    genomes: impl Iterator<Item = &'a G> + Clone,
) -> Option<[GeneStat; N]> {
    let n = genomes.clone().count();
    if n == 0 {
        return None;
    }
    let mut column = vec![0.0; n];
    Some(std::array::from_fn(|g| match genes[g].variants() {
        Some(_) => {
            let mut shares = [0.0; MAX_VARIANTS];
            for genome in genomes.clone() {
                let k = genome.values()[g] as usize;
                if k < MAX_VARIANTS {
                    shares[k] += 1.0;
                }
            }
            shares.iter_mut().for_each(|s| *s /= n as f64);
            GeneStat::Shares(shares)
        }
        None => {
            for (c, genome) in column.iter_mut().zip(genomes.clone()) {
                *c = genome.values()[g];
            }
            GeneStat::Number(Spread::of(&mut column).expect("популяция не пуста"))
        }
    }))
}

/// Genes the per-diet summary follows: the body, the senses, the cold and the life span.
pub const DIET_GENES: [creature::Gene; 6] = [
    creature::Gene::Size,
    creature::Gene::Speed,
    creature::Gene::Vision,
    creature::Gene::ColdBlood,
    creature::Gene::Burst,
    creature::Gene::Lifespan,
];

/// A summary of one diet's creatures (or of all of them).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DietStat {
    pub creatures: usize,
    /// Mean fullness of the tank, 0..1.
    pub fullness: Option<f64>,
    /// Age, ticks.
    pub age: Option<Spread>,
    /// Depth, % of the world's height.
    pub depth: Option<Spread>,
    /// Spread of each of `DIET_GENES`, in that order.
    pub genes: Option<[Spread; DIET_GENES.len()]>,
}

impl DietStat {
    pub fn of<'a>(herd: impl Iterator<Item = &'a Creature> + Clone, height: f64) -> DietStat {
        let spread =
            |value: &dyn Fn(&Creature) -> f64| Spread::of(&mut herd.clone().map(value).collect::<Vec<_>>());
        DietStat {
            creatures: herd.clone().count(),
            fullness: average(herd.clone().map(|v| v.energy / v.pheno.max_energy)),
            age: spread(&|v| v.age),
            depth: spread(&|v| v.y / height * 100.0),
            genes: DIET_GENES
                .map(|g| spread(&|v| v.genome.values()[g as usize]))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .map(|v| v.try_into().expect("one spread per gene")),
        }
    }
}

/// A sample of the world at one tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Creatures by what they are doing, in `Activity::ALL` order.
    pub activities: [usize; Activity::ALL.len()],
    pub tick: u64,
    /// How many times the world is bigger than the base one: the events' thresholds grow with the area.
    pub area: f64,
    pub plants: usize,
    pub corpses: usize,
    /// This world's plant ceiling.
    pub plant_cap: usize,
    pub creatures: usize,
    pub juveniles: usize,
    /// Creatures whose program (the track they live by now) shoots.
    pub shooters: usize,
    /// Accumulated since the world began; the flows over an interval are `b.counters.since(&a.counters)`.
    pub counters: Counters,
    /// A summary of each gene of the creatures, the order is the table `creature::GENES`.
    /// None — there are no creatures.
    pub genes: Option<[GeneStat; creature::N]>,
    /// The creatures' depth, % of the world's height (0 — the surface, where the plants are thicker).
    pub depth: Option<Spread>,
    pub creatures_by_depth: [usize; DEPTH_BANDS],
    pub plants_by_depth: [usize; DEPTH_BANDS],
    pub creatures_by_width: [usize; WIDTH_BANDS],
    pub plants_by_width: [usize; WIDTH_BANDS],
    /// The mean tank fullness of the creatures, 0..1.
    pub fullness: Option<f64>,
    /// Each diet's creatures, in `Diet` order, and all creatures by the same measures.
    pub diets: [DietStat; 4],
    pub all: DietStat,
}

fn band(at: f64, len: f64, bands: usize) -> usize {
    ((at / len * bands as f64) as usize).min(bands - 1)
}

fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (s, n) = values.fold((0.0, 0usize), |(s, n), x| (s + x, n + 1));
    (n > 0).then(|| s / n as f64)
}

impl Snapshot {
    pub fn of(world: &World) -> Snapshot {
        let (w, h) = (world.space.width, world.space.height);
        let herd = &world.creatures;

        let genes = gene_stats(&creature::GENES, herd.iter().map(|v| &v.genome));
        // everyone by the diets' measures: the world's fullness and depth are its
        let all = DietStat::of(herd.iter(), h);

        let (mut creatures_by_depth, mut creatures_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        let mut activities = [0; Activity::ALL.len()];
        for v in herd {
            creatures_by_depth[band(v.y, h, DEPTH_BANDS)] += 1;
            creatures_by_width[band(v.x, w, WIDTH_BANDS)] += 1;
            activities[v.mind.activity as usize] += 1;
        }
        let (mut plants_by_depth, mut plants_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        for p in &world.plants {
            plants_by_depth[band(p.y, h, DEPTH_BANDS)] += 1;
            plants_by_width[band(p.x, w, WIDTH_BANDS)] += 1;
        }

        Snapshot {
            activities,
            tick: world.tick,
            area: world.space.area_ratio(),
            plants: world.plants.len(),
            corpses: world.corpses.len(),
            plant_cap: world.space.per_area(PLANT_MAX),
            creatures: herd.len(),
            juveniles: herd.iter().filter(|v| !v.adult()).count(),
            shooters: herd.iter().filter(|v| v.program().shoots()).count(),
            counters: world.counters,
            genes,
            depth: all.depth,
            creatures_by_depth,
            plants_by_depth,
            creatures_by_width,
            plants_by_width,
            fullness: all.fullness,
            diets: Diet::ALL.map(|d| DietStat::of(herd.iter().filter(|v| v.pheno.diet == d), h)),
            all,
        }
    }
}

// ── chronicle ───────────────────────────────────────────────────────────────

/// What happened. The key is for machine parsing (JSON), the text is for reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    CreaturesCrash,
    CreaturesRise,
    CreaturesExtinct,
    PlantsAtCap,
    PlantsEatenAgain,
    GeneShift,
    LayerNarrow,
    LayerWide,
    StrategyShift,
}

impl EventKind {
    pub fn key(self) -> &'static str {
        match self {
            EventKind::CreaturesCrash => "creatures_crash",
            EventKind::CreaturesRise => "creatures_rise",
            EventKind::CreaturesExtinct => "creatures_extinct",
            EventKind::PlantsAtCap => "plants_at_cap",
            EventKind::PlantsEatenAgain => "plants_eaten_again",
            EventKind::GeneShift => "gene_shift",
            EventKind::LayerNarrow => "layer_narrow",
            EventKind::LayerWide => "layer_wide",
            EventKind::StrategyShift => "strategy_shift",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub kind: EventKind,
    pub text: String,
}

/// By how many times the population must fall from a peak (or rise from a bottom) for it to be
/// an event, not the division's noise.
const SWING: f64 = 2.0;
/// Below this population (per base area) swings do not count: a dozen creatures becoming five
/// is noise, not a collapse.
const SWING_MIN: f64 = 30.0;
/// The shift of a gene that gets into the chronicle: for size, speed and sight — relative, for
/// percent genes — in percentage points.
const GENE_SHIFT_REL: f64 = 0.3;
const GENE_SHIFT_PTS: f64 = 15.0;
/// The share of a choice gene's (a strategy's) variant has shifted by this many percentage
/// points — or a variant has appeared or vanished.
const SHARE_SHIFT_PTS: f64 = 15.0;
/// A creatures' layer (10‒90% by depth) already this — «squeezed», wider than the second —
/// «spread out». The gap between the thresholds keeps the event from blinking back and forth.
const LAYER_NARROW: f64 = 25.0;
const LAYER_WIDE: f64 = 40.0;
/// Plants are «at the ceiling» from this share; «eaten again» — below the second.
const CAP_HIGH: f64 = 0.95;
const CAP_LOW: f64 = 0.8;

/// The causes of the creatures' population change over an interval.
pub fn describe_flows(c: &Counters) -> String {
    let mut text = format!("родилось {}, умерло с голоду {}", c.born, c.starved);
    if c.old_age > 0 {
        text += &format!(", от старости {}", c.old_age);
    }
    if c.combat > 0 {
        text += &format!(", в бою {}", c.combat);
    }
    text
}

/// The species' gene shifts since the previous marks: (the parts of text for numeric genes, for
/// choice genes). A gene that has shifted carries its mark over here.
fn gene_shifts<const N: usize>(
    genes: &[GeneSpec; N],
    cur: Option<[GeneStat; N]>,
    base: &mut Option<[(GeneStat, u64); N]>,
    t: u64,
) -> (Vec<String>, Vec<String>) {
    let (mut numbers, mut choices) = (Vec::new(), Vec::new());
    let Some(cur) = cur else { return (numbers, choices) };
    let Some(base) = base.as_mut() else {
        *base = Some(cur.map(|s| (s, t)));
        return (numbers, choices);
    };
    for (g, spec) in genes.iter().enumerate() {
        let (was, since) = base[g];
        match (cur[g], was) {
            (GeneStat::Number(s), GeneStat::Number(w)) => {
                let (now, was) = (s.p50, w.p50);
                let percent = spec.is_percent();
                let shifted = if percent {
                    (now - was).abs() >= GENE_SHIFT_PTS
                } else {
                    was > 0.0 && (now / was - 1.0).abs() >= GENE_SHIFT_REL
                };
                if shifted {
                    let change = if percent {
                        format!("{:+.0} п.п.", now - was)
                    } else {
                        format!("{:+.0}%", (now / was - 1.0) * 100.0)
                    };
                    numbers.push(format!(
                        "{} {was:.1}→{now:.1} ({change} с тика {since}; 10‒90%: {:.1}‒{:.1})",
                        spec.label, s.p10, s.p90
                    ));
                    base[g] = (cur[g], t);
                }
            }
            (GeneStat::Shares(now), GeneStat::Shares(was)) => {
                let variants = spec.variants().unwrap_or_default();
                let moved = variants.iter().enumerate().any(|(k, _)| {
                    (now[k] - was[k]).abs() * 100.0 >= SHARE_SHIFT_PTS || (now[k] > 0.0) != (was[k] > 0.0)
                });
                if moved {
                    let parts: Vec<String> = variants
                        .iter()
                        .enumerate()
                        .filter(|&(k, _)| now[k] > 0.0 || was[k] > 0.0)
                        .map(|(k, v)| format!("{} {:.0}→{:.0}%", v.label, was[k] * 100.0, now[k] * 100.0))
                        .collect();
                    choices.push(format!("{} {} (с тика {since})", spec.label, parts.join(", ")));
                    base[g] = (cur[g], t);
                }
            }
            _ => {}
        }
    }
    (numbers, choices)
}

/// A population swing: the peak and the bottom since the previous event.
#[derive(Clone, Debug)]
struct Swing {
    peak: Snapshot,
    trough: Snapshot,
}

/// A chronicle that is written along the way: sample after sample. Both the report ([`events`])
/// and the game keep it — so the events' texts are the same in both.
#[derive(Clone, Debug, Default)]
pub struct EventTracker {
    prev: Option<Snapshot>,
    swing: Option<Swing>,
    /// A summary of each gene at the previous mark and the tick of that mark.
    gene_base: Option<[(GeneStat, u64); creature::N]>,
    narrow: bool,
    capped: bool,
}

impl EventTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// The next sample (the ticks must grow); the events between the previous and this one go to `out`.
    pub fn observe(&mut self, cur: &Snapshot, out: &mut Vec<Event>) {
        let Some(prev) = self.prev.replace(cur.clone()) else {
            self.swing = Some(Swing { peak: cur.clone(), trough: cur.clone() });
            self.gene_base = cur.genes.map(|g| g.map(|s| (s, cur.tick)));
            self.narrow = cur.depth.is_some_and(|d| d.p90 - d.p10 < LAYER_NARROW);
            self.capped = cur.plants as f64 >= cur.plant_cap as f64 * CAP_HIGH;
            return;
        };
        let t = cur.tick;
        let mut push = |kind, text: String| out.push(Event { tick: t, kind, text });

        // ── the population: a collapse and a rise are counted from the peak/bottom since the previous
        // event, the causes are the counters between them
        let swing = self.swing.get_or_insert_with(|| Swing { peak: prev.clone(), trough: prev.clone() });
        let min = cur.area * SWING_MIN;
        let n = cur.creatures;
        if swing.peak.creatures < n {
            swing.peak = cur.clone();
        }
        if swing.trough.creatures > n {
            swing.trough = cur.clone();
        }
        let (peak, trough) = (&swing.peak, &swing.trough);
        let flows = |from: &Snapshot| describe_flows(&cur.counters.since(&from.counters));
        let event = if n == 0 && prev.creatures > 0 {
            Some((
                EventKind::CreaturesExtinct,
                format!(
                    "существа вымерли (пик {} на тике {}); с пика: {}",
                    peak.creatures,
                    peak.tick,
                    flows(peak)
                ),
            ))
        } else if peak.creatures as f64 >= min && n as f64 * SWING <= peak.creatures as f64 {
            let text = format!(
                "обвал существ: {} (тик {}) → {n}; за это время {}",
                peak.creatures,
                peak.tick,
                flows(peak)
            );
            Some((EventKind::CreaturesCrash, text))
        } else if n as f64 >= min && trough.creatures as f64 * SWING <= n as f64 && trough.creatures > 0 {
            let text = format!(
                "подъём существ: {} (тик {}) → {n}; за это время {}",
                trough.creatures,
                trough.tick,
                flows(trough)
            );
            Some((EventKind::CreaturesRise, text))
        } else {
            None
        };
        if let Some((kind, text)) = event {
            push(kind, text);
            *swing = Swing { peak: cur.clone(), trough: cur.clone() };
        }

        // ── plants at the ceiling: more grow than can be eaten
        let fill = cur.plants as f64 / cur.plant_cap as f64;
        if !self.capped && fill >= CAP_HIGH {
            self.capped = true;
            let text = format!(
                "растения упёрлись в потолок ({} из {}): существ {} — есть их некому или не там",
                cur.plants, cur.plant_cap, cur.creatures
            );
            push(EventKind::PlantsAtCap, text);
        } else if self.capped && fill < CAP_LOW {
            self.capped = false;
            push(
                EventKind::PlantsEatenAgain,
                format!("растения снова поедаются: {} из {}", cur.plants, cur.plant_cap),
            );
        }

        // ── genes: the median has moved away from the previous mark — the mark is carried over; all
        // shifts of one sample are one event, otherwise the chronicle drowns in genes
        let (numbers, choices) = gene_shifts(&creature::GENES, cur.genes, &mut self.gene_base, t);
        if !numbers.is_empty() {
            push(EventKind::GeneShift, format!("геном, медиана: {}", numbers.join("; ")));
        }
        if !choices.is_empty() {
            push(EventKind::StrategyShift, format!("существа: {}", choices.join("; ")));
        }

        // ── the layer: where by depth 80% of the creatures keep
        if let Some(d) = cur.depth {
            let width = d.p90 - d.p10;
            let text = |what: &str| {
                format!(
                    "существа {what}: 80% живут на глубине {:.0}‒{:.0}% (медиана {:.0}%)",
                    d.p10, d.p90, d.p50
                )
            };
            if !self.narrow && width < LAYER_NARROW {
                self.narrow = true;
                push(EventKind::LayerNarrow, text("сжались в узкий слой"));
            } else if self.narrow && width > LAYER_WIDE {
                self.narrow = false;
                push(EventKind::LayerWide, text("снова расселились по глубине"));
            }
        }
    }
}

/// A run's chronicle by samples (they must go in ascending ticks).
pub fn events(snaps: &[Snapshot]) -> Vec<Event> {
    let mut tracker = EventTracker::new();
    let mut out = Vec::new();
    for s in snaps {
        tracker.observe(s, &mut out);
    }
    out
}

// ── map ─────────────────────────────────────────────────────────────────────

/// A legend for [`ascii_map`].
pub const MAP_LEGEND: &str =
    "O 4+ существ · o 1‒3 существа · : 4+ растений · . 1‒3 растения · верх — поверхность";

/// A map of the world in `cols` columns. A cell shows the most «important» thing in it:
/// creatures are more important than plants. On the left is the depth in percent. There are as
/// many rows as keep the world's proportions (a character is about twice as high as it is wide),
/// but no fewer than 8 and no more than 40.
pub fn ascii_map(world: &World, cols: usize) -> Vec<String> {
    let cols = cols.max(8);
    let (w, h) = (world.space.width, world.space.height);
    let rows = ((cols as f64 * h / w / 2.0).round() as usize).clamp(8, 40);
    let cell = |x: f64, y: f64| {
        let c = ((x / w * cols as f64) as usize).min(cols - 1);
        let r = ((y / h * rows as f64) as usize).min(rows - 1);
        r * cols + c
    };
    let (mut plants, mut herd) = (vec![0u32; rows * cols], vec![0u32; rows * cols]);
    for p in &world.plants {
        plants[cell(p.x, p.y)] += 1;
    }
    for v in &world.creatures {
        herd[cell(v.x, v.y)] += 1;
    }

    let border = format!("     +{}+", "-".repeat(cols));
    let mut out = vec![border.clone()];
    for r in 0..rows {
        let line: String = (0..cols)
            .map(|c| {
                let i = r * cols + c;
                match (herd[i], plants[i]) {
                    (v, _) if v >= 4 => 'O',
                    (v, _) if v > 0 => 'o',
                    (_, n) if n >= 4 => ':',
                    (_, n) if n > 0 => '.',
                    _ => ' ',
                }
            })
            .collect();
        out.push(format!("{:>3}% |{line}|", r * 100 / rows));
    }
    out.push(border);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::genome::{GeneKind, Mutation, Variant};
    use life_core::{CreatureGenome, WorldConfig};

    const THREE: [Variant; 3] = [
        Variant { key: "a", label: "первый", about: "" },
        Variant { key: "b", label: "второй", about: "" },
        Variant { key: "c", label: "третий", about: "" },
    ];
    const STRATEGY: [GeneSpec; 1] = [GeneSpec {
        key: "strategy",
        label: "стратегия",
        about: "",
        kind: GeneKind::Choice(&THREE),
        base: 0.0,
        mutation: Mutation::Switch { chance: 0.1 },
    }];

    fn shares(v: [f64; 3]) -> Option<[GeneStat; 1]> {
        let mut s = [0.0; MAX_VARIANTS];
        s[..3].copy_from_slice(&v);
        Some([GeneStat::Shares(s)])
    }

    #[test]
    fn сдвиг_долей_стратегий_попадает_в_хронику() {
        let mut base = None;
        let (_, c) = gene_shifts(&STRATEGY, shares([1.0, 0.0, 0.0]), &mut base, 0);
        assert!(c.is_empty(), "первый срез — только отметка");

        let (_, c) = gene_shifts(&STRATEGY, shares([0.95, 0.05, 0.0]), &mut base, 60);
        assert_eq!(c, ["стратегия первый 100→95%, второй 0→5% (с тика 0)"], "вариант появился");

        let (_, c) = gene_shifts(&STRATEGY, shares([0.9, 0.1, 0.0]), &mut base, 120);
        assert!(c.is_empty(), "5 п.п. — шум");

        let (_, c) = gene_shifts(&STRATEGY, shares([0.7, 0.3, 0.0]), &mut base, 180);
        assert_eq!(c, ["стратегия первый 95→70%, второй 5→30% (с тика 60)"]);
    }

    /// The snapshot counts every creature under what it is doing.
    #[test]
    fn activities_count_every_creature() {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        for x in [1000.0, 1100.0, 2000.0] {
            w.spawn(CreatureGenome::BASE, x, 1000.0, None);
        }
        w.creatures[1].mind.activity = Activity::Resting;
        let s = Snapshot::of(&w);
        assert_eq!(s.activities.iter().sum::<usize>(), 3);
        assert_eq!(s.activities[Activity::Resting as usize], 1);
    }
}
