//! Наблюдение за миром без окна: из чего человек или ИИ понимает, ЧТО в нём
//! происходит и ПОЧЕМУ.
//!
//! - [`Snapshot`] — срез мира: численности, накопленные счётчики рождений и
//!   смертей, разброс каждого гена (не только среднее: среднее прячет раскол
//!   на два вида), где по глубине и по ширине живут существа и растут
//!   растения, сытость.
//! - [`EventTracker`] / [`events`] — хроника по срезам: обвалы и подъёмы численности с причинами,
//!   вымирание, растения у потолка, сдвиги генов, сжатие существ в узкий слой.
//! - [`ascii_map`] — карта мира текстом: слои, скопления, пустые края.

use life_core::config::*;
use life_core::genome::{GeneSpec, Genome, creature};
use life_core::{Counters, World};

/// На сколько полос делится глубина в срезе (0 — поверхность).
pub const DEPTH_BANDS: usize = life_core::flora::DEPTH_BANDS;
/// На сколько полос делится ширина (0 — левый край): видно, куда стянулась
/// жизнь, когда еда распределена по ширине неравномерно.
pub const WIDTH_BANDS: usize = 10;

/// Разброс величины по популяции.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub mean: f64,
}

impl Spread {
    /// None для пустой выборки. Порядок значений портится (сортировка на месте).
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

/// Больше вариантов у гена-выбора не бывает (тест в `tests/observe.rs`
/// сверяет с таблицами генов): доли лежат в массиве, а не в векторе, чтобы
/// срез оставался дешёвым в копировании.
pub const MAX_VARIANTS: usize = 8;

/// Сводка одного гена по популяции.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneStat {
    /// Числовой ген: разброс.
    Number(Spread),
    /// Ген-выбор: доля каждого варианта (0..1) в порядке вариантов.
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

/// Сводка генов популяции по таблице вида: разброс числовых генов (одна
/// сортировка на ген), доли у генов-выборов (один проход). None — никого нет.
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

/// Срез мира на одном тике.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub activities: [usize; 5],
    pub social_counts: life_core::social::Counters,
    /// Root mean square distance of flock members from their mean position.
    pub flock_spread: Option<Spread>,
    /// Radius of the flock circles.
    pub flock_radius: Option<Spread>,
    /// Flocks (two and more) by kind, in `FlockKind::ALL` order.
    pub flock_kinds: [usize; 4],
    /// Share of flock members whose body touches their circle.
    pub inside_share: Option<f64>,
    /// Overlap of circles that must not overlap.
    pub overlaps: life_core::flock::OverlapStats,
    /// Flocks squeezed to at most `MIN_COMPRESS` of their radius, and to at most 80%.
    pub squeezed_flocks: usize,
    pub pressed_flocks: usize,
    /// Battles of flocks going on, and the flocks in them.
    pub battles: usize,
    pub fighting_flocks: usize,
    pub tick: u64,
    /// Во сколько раз мир больше базового: пороги событий растут с площадью.
    pub area: f64,
    pub plants: usize,
    pub corpses: usize,
    /// Hard count limit for memory and work.
    pub plant_cap: usize,
    /// Remaining raw plant energy and its world capacity, before bite yield.
    pub plant_biomass: f64,
    pub plant_biomass_cap: f64,
    pub creatures: usize,
    pub juveniles: usize,
    pub pack_carriers: usize,
    pub pack_share: f64,
    pub pack_members: usize,
    pub flocks: usize,
    /// Накопленные с начала мира; потоки за промежуток — `b.counters.since(&a.counters)`.
    pub counters: Counters,
    /// Сводка каждого гена существ, порядок — таблица `creature::GENES`.
    /// None — существ нет.
    pub genes: Option<[GeneStat; creature::N]>,
    /// Глубина существ, % высоты мира (0 — поверхность, где гуще растения).
    pub depth: Option<Spread>,
    pub creatures_by_depth: [usize; DEPTH_BANDS],
    pub plants_by_depth: [usize; DEPTH_BANDS],
    pub creatures_by_width: [usize; WIDTH_BANDS],
    pub plants_by_width: [usize; WIDTH_BANDS],
    /// Средняя заполненность бака существ, 0..1.
    pub fullness: Option<f64>,
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
        let mut depth: Vec<f64> = herd.iter().map(|v| v.y / h * 100.0).collect();

        let (mut creatures_by_depth, mut creatures_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        let mut pack_carriers = 0;
        for v in herd {
            creatures_by_depth[band(v.y, h, DEPTH_BANDS)] += 1;
            creatures_by_width[band(v.x, w, WIDTH_BANDS)] += 1;
            pack_carriers += usize::from(v.pheno.pack_instinct);
        }
        let (flocks, pack_members) = world
            .flocks
            .values()
            .filter(|f| f.members >= 2)
            .fold((0, 0), |(groups, members), f| (groups + 1, members + f.members));
        let flocks_now = life_core::flock::summaries(world);
        let mut flock_kinds = [0; 4];
        for f in &flocks_now {
            flock_kinds[f.kind as usize] += 1;
        }
        let inside: usize = flocks_now.iter().map(|f| f.inside).sum();
        let (mut plants_by_depth, mut plants_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        for p in &world.plants {
            plants_by_depth[band(p.y, h, DEPTH_BANDS)] += 1;
            plants_by_width[band(p.x, w, WIDTH_BANDS)] += 1;
        }

        Snapshot {
            activities: {
                let mut counts = [0; 5];
                for v in herd {
                    counts[v.mind.social.activity as usize] += 1;
                }
                counts
            },
            social_counts: world.social_counts,
            flock_spread: Spread::of(&mut flocks_now.iter().map(|s| s.spread).collect::<Vec<_>>()),
            flock_radius: Spread::of(&mut flocks_now.iter().map(|s| s.radius).collect::<Vec<_>>()),
            flock_kinds,
            inside_share: (pack_members > 0).then(|| inside as f64 / pack_members as f64),
            overlaps: life_core::flock::overlap_stats(&world.flocks, &world.space),
            squeezed_flocks: flocks_now
                .iter()
                .filter(|f| f.compress <= life_core::flock::MIN_COMPRESS + 1e-9)
                .count(),
            pressed_flocks: flocks_now.iter().filter(|f| f.compress <= 0.8).count(),
            battles: world.battles.active.len(),
            fighting_flocks: world.battles.active.iter().map(|b| b.sides.len()).sum(),
            tick: world.tick,
            area: world.space.area_ratio(),
            plants: world.plants.len(),
            corpses: world.corpses.len(),
            plant_cap: world.space.per_area(PLANT_MAX),
            plant_biomass: world
                .plants
                .iter()
                .map(|p| {
                    f64::from(p.portions) * world.rules.plant_energy / f64::from(life_core::plant::PORTIONS)
                })
                .sum(),
            plant_biomass_cap: world.space.per_area(PLANT_MAX) as f64 * life_core::config::ENERGY_FROM_PLANT,
            creatures: herd.len(),
            juveniles: herd.iter().filter(|v| !v.adult()).count(),
            pack_carriers,
            pack_share: pack_carriers as f64 / herd.len().max(1) as f64,
            pack_members,
            flocks,
            counters: world.counters,
            genes,
            depth: Spread::of(&mut depth),
            creatures_by_depth,
            plants_by_depth,
            creatures_by_width,
            plants_by_width,
            fullness: average(herd.iter().map(|v| v.energy / v.pheno.max_energy)),
        }
    }
}

// ── хроника ─────────────────────────────────────────────────────────────────

/// Что случилось. Ключ — для машинного разбора (JSON), текст — для чтения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    FlockAlarm,
    FlockCalm,
    FlockSplit,
    CreaturesCrash,
    CreaturesRise,
    CreaturesExtinct,
    PlantsAtCap,
    PlantsEatenAgain,
    GeneShift,
    LayerNarrow,
    LayerWide,
    StrategyShift,
    FlockMove,
    FlockBattle,
    FlockRetreat,
}

impl EventKind {
    pub fn key(self) -> &'static str {
        match self {
            EventKind::FlockAlarm => "flock_alarm",
            EventKind::FlockCalm => "flock_calm",
            EventKind::FlockSplit => "flock_split",
            EventKind::CreaturesCrash => "creatures_crash",
            EventKind::CreaturesRise => "creatures_rise",
            EventKind::CreaturesExtinct => "creatures_extinct",
            EventKind::PlantsAtCap => "plants_at_cap",
            EventKind::PlantsEatenAgain => "plants_eaten_again",
            EventKind::GeneShift => "gene_shift",
            EventKind::LayerNarrow => "layer_narrow",
            EventKind::LayerWide => "layer_wide",
            EventKind::StrategyShift => "strategy_shift",
            EventKind::FlockMove => "flock_move",
            EventKind::FlockBattle => "flock_battle",
            EventKind::FlockRetreat => "flock_retreat",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub kind: EventKind,
    pub text: String,
}

/// Во сколько раз численность должна упасть от пика (или вырасти от дна),
/// чтобы это было событием, а не шумом деления.
const SWING: f64 = 2.0;
/// Ниже этой численности (на базовую площадь) колебания не считаются: десяток
/// существ, ставших пятком, — шум, а не обвал.
const SWING_MIN: f64 = 30.0;
/// Сдвиг гена, который попадает в хронику: для размера, скорости и зрения —
/// относительный, для генов-процентов — в процентных пунктах.
const GENE_SHIFT_REL: f64 = 0.3;
const GENE_SHIFT_PTS: f64 = 15.0;
/// Доля варианта гена-выбора (стратегии) сдвинулась на столько процентных
/// пунктов — или вариант появился либо исчез.
const SHARE_SHIFT_PTS: f64 = 15.0;
/// Слой существ (10‒90% по глубине) уже этого — «сжались», шире второго —
/// «расселились». Зазор между порогами не даёт событию мигать туда-сюда.
const LAYER_NARROW: f64 = 25.0;
const LAYER_WIDE: f64 = 40.0;
/// Растения «у потолка» от этой доли; «снова едят» — ниже второй.
const CAP_HIGH: f64 = 0.95;
const CAP_LOW: f64 = 0.8;

/// Причины перемены численности существ за промежуток.
pub fn describe_flows(c: &Counters) -> String {
    let mut text = format!("родилось {}, умерло с голоду {}", c.born, c.starved);
    if c.old_age > 0 {
        text += &format!(", от старости {}", c.old_age);
    }
    if c.combat > 0 {
        text += &format!(", в бою {}", c.combat);
    }
    // без каннибализма строка короче
    if c.cannibalized > 0 {
        text += &format!(", съедено своими {}", c.cannibalized);
    }
    text
}

/// Сдвиги генов вида с прошлых отметок: (части текста для числовых генов,
/// для генов-выборов). Сдвинувшийся ген переносит свою отметку сюда.
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

/// Колебание популяции: пик и дно с прошлого события.
#[derive(Clone, Debug)]
struct Swing {
    peak: Snapshot,
    trough: Snapshot,
}

/// Хроника, которая пишется по ходу: срез за срезом. Её ведёт и отчёт
/// ([`events`]), и игра — поэтому тексты событий у них одинаковые.
#[derive(Clone, Debug, Default)]
pub struct EventTracker {
    prev: Option<Snapshot>,
    swing: Option<Swing>,
    /// Сводка каждого гена на прошлой отметке и тик этой отметки.
    gene_base: Option<[(GeneStat, u64); creature::N]>,
    narrow: bool,
    capped: bool,
}

impl EventTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Следующий срез (тики должны расти); события между прошлым и этим — в `out`.
    pub fn observe(&mut self, cur: &Snapshot, out: &mut Vec<Event>) {
        let Some(prev) = self.prev.replace(cur.clone()) else {
            self.swing = Some(Swing { peak: cur.clone(), trough: cur.clone() });
            self.gene_base = cur.genes.map(|g| g.map(|s| (s, cur.tick)));
            self.narrow = cur.depth.is_some_and(|d| d.p90 - d.p10 < LAYER_NARROW);
            self.capped = cur.plant_biomass >= cur.plant_biomass_cap * CAP_HIGH;
            return;
        };
        let t = cur.tick;
        let mut push = |kind, text: String| out.push(Event { tick: t, kind, text });
        for (kind, n, label) in [
            (
                EventKind::FlockAlarm,
                cur.social_counts.alarms.saturating_sub(prev.social_counts.alarms),
                "начало тревоги",
            ),
            (
                EventKind::FlockCalm,
                cur.social_counts.alarm_ends.saturating_sub(prev.social_counts.alarm_ends),
                "окончание тревоги",
            ),
            (
                EventKind::FlockSplit,
                cur.social_counts.splits.saturating_sub(prev.social_counts.splits),
                "отделение новой стаи",
            ),
            (
                EventKind::FlockMove,
                cur.social_counts.relocations.saturating_sub(prev.social_counts.relocations),
                "переезд на новое место",
            ),
            (
                EventKind::FlockBattle,
                cur.social_counts.battles.saturating_sub(prev.social_counts.battles),
                "бой за место",
            ),
            (
                EventKind::FlockRetreat,
                cur.social_counts.battle_retreats.saturating_sub(prev.social_counts.battle_retreats),
                "отступление после боя",
            ),
        ] {
            if n > 0 {
                push(kind, format!("Стаи: {label}, событий за промежуток: {n}"));
            }
        }

        // ── численность: обвал и подъём считаются от пика/дна с прошлого события,
        // причины — счётчики между ними
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

        // ── растения у потолка: их растёт больше, чем успевают съесть
        let fill = cur.plant_biomass / cur.plant_biomass_cap;
        if !self.capped && fill >= CAP_HIGH {
            self.capped = true;
            let text = format!(
                "растения упёрлись в предел энергии ({:.0} из {:.0}): существ {} — есть их некому или не там",
                cur.plant_biomass, cur.plant_biomass_cap, cur.creatures
            );
            push(EventKind::PlantsAtCap, text);
        } else if self.capped && fill < CAP_LOW {
            self.capped = false;
            push(
                EventKind::PlantsEatenAgain,
                format!(
                    "растения снова поедаются: {:.0} из {:.0} энергии",
                    cur.plant_biomass, cur.plant_biomass_cap
                ),
            );
        }

        // ── гены: медиана ушла от прошлой отметки — отметка переносится; все
        // сдвиги одного среза — одно событие, иначе хроника тонет в генах
        let (numbers, choices) = gene_shifts(&creature::GENES, cur.genes, &mut self.gene_base, t);
        if !numbers.is_empty() {
            push(EventKind::GeneShift, format!("геном, медиана: {}", numbers.join("; ")));
        }
        if !choices.is_empty() {
            push(EventKind::StrategyShift, format!("существа: {}", choices.join("; ")));
        }

        // ── слой: где по глубине держатся 80% существ
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

/// Хроника прогона по срезам (они должны идти по возрастанию тиков).
pub fn events(snaps: &[Snapshot]) -> Vec<Event> {
    let mut tracker = EventTracker::new();
    let mut out = Vec::new();
    for s in snaps {
        tracker.observe(s, &mut out);
    }
    out
}

// ── карта ───────────────────────────────────────────────────────────────────

/// Легенда к [`ascii_map`].
pub const MAP_LEGEND: &str =
    "O 4+ существ · o 1‒3 существа · : 4+ растений · . 1‒3 растения · верх — поверхность";

/// Карта мира в `cols` колонок. Клетка показывает самое «важное», что в ней
/// есть: существа важнее растений. Слева —
/// глубина в процентах. Строк столько, чтобы пропорции мира сохранились
/// (символ примерно вдвое выше своей ширины), но не меньше 8 и не больше 40.
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
    use life_core::{CreatureGenome, WorldConfig, genome::creature::Gene};

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

    #[test]
    fn носители_стайности_отличаются_от_участников_настоящих_стай() {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        for x in [1000.0, 1100.0, 2000.0] {
            w.spawn(CreatureGenome::BASE, x, 1000.0, None);
        }
        w.spawn(CreatureGenome::BASE.with(Gene::PackInstinct, 0.0), 3000.0, 1000.0, None);
        w.creatures[1].flock = w.creatures[0].flock;
        life_core::flock::update(&mut w.flocks, &mut w.creatures, &w.space, 42, false);
        let s = Snapshot::of(&w);
        assert_eq!((s.creatures, s.pack_carriers, s.pack_members, s.flocks), (4, 3, 2, 1));
        assert_eq!(s.pack_share, 0.75);
    }
}
