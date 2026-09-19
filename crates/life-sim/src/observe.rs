//! Наблюдение за миром без окна: из чего человек или ИИ понимает, ЧТО в нём
//! происходит и ПОЧЕМУ.
//!
//! - [`Snapshot`] — срез мира: численности, накопленные счётчики рождений и
//!   смертей, разброс каждого гена (не только среднее: среднее прячет раскол
//!   на два вида), где по глубине и по ширине живут травоядные и растут
//!   растения, сытость.
//! - [`EventTracker`] / [`events`] — хроника по срезам: обвалы и подъёмы численности с причинами,
//!   вымирание и возвращение хищников, растения у потолка, сдвиги генов,
//!   сжатие травоядных в узкий слой.
//! - [`ascii_map`] — карта мира текстом: слои, скопления, пустые края.

use life_core::config::*;
use life_core::genome::{GeneSpec, Genome, predator, vegetarian};
use life_core::{Counters, World};

/// На сколько полос делится глубина в срезе (0 — поверхность).
pub const DEPTH_BANDS: usize = 10;
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
    pub tick: u64,
    /// Во сколько раз мир больше базового: пороги событий растут с площадью.
    pub area: f64,
    pub plants: usize,
    /// Потолок растений этого мира.
    pub plant_cap: usize,
    pub vegetarians: usize,
    pub predators: usize,
    /// Накопленные с начала мира; потоки за промежуток — `b.counters.since(&a.counters)`.
    pub counters: Counters,
    pub migrants: u64,
    /// Сводка каждого гена травоядных, порядок — таблица `vegetarian::GENES`.
    /// None — травоядных нет.
    pub genes: Option<[GeneStat; vegetarian::N]>,
    /// Глубина травоядных, % высоты мира (0 — поверхность, где гуще растения).
    pub vegetarian_depth: Option<Spread>,
    pub vegetarians_by_depth: [usize; DEPTH_BANDS],
    pub plants_by_depth: [usize; DEPTH_BANDS],
    pub vegetarians_by_width: [usize; WIDTH_BANDS],
    pub plants_by_width: [usize; WIDTH_BANDS],
    /// Средняя заполненность бака травоядных, 0..1.
    pub vegetarian_fullness: Option<f64>,
    /// Доля голодных хищников — тех, кто сейчас охотится.
    pub predators_hungry: Option<f64>,
    pub predator_fullness: Option<f64>,
    /// Сводка генов хищников (таблица `predator::GENES`). None — хищников нет.
    pub predator_genes: Option<[GeneStat; predator::N]>,
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
        let vegs = &world.vegetarians;
        let preds = &world.predators;

        let genes = gene_stats(&vegetarian::GENES, vegs.iter().map(|v| &v.genome));
        let mut depth: Vec<f64> = vegs.iter().map(|v| v.y / h * 100.0).collect();

        let (mut vegetarians_by_depth, mut vegetarians_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        for v in vegs {
            vegetarians_by_depth[band(v.y, h, DEPTH_BANDS)] += 1;
            vegetarians_by_width[band(v.x, w, WIDTH_BANDS)] += 1;
        }
        let (mut plants_by_depth, mut plants_by_width) = ([0; DEPTH_BANDS], [0; WIDTH_BANDS]);
        for p in &world.plants {
            plants_by_depth[band(p.y, h, DEPTH_BANDS)] += 1;
            plants_by_width[band(p.x, w, WIDTH_BANDS)] += 1;
        }

        Snapshot {
            tick: world.tick,
            area: world.space.area_ratio(),
            plants: world.plants.len(),
            plant_cap: world.space.per_area(PLANT_MAX),
            vegetarians: vegs.len(),
            predators: preds.len(),
            counters: world.counters,
            migrants: world.migrants,
            genes,
            vegetarian_depth: Spread::of(&mut depth),
            vegetarians_by_depth,
            plants_by_depth,
            vegetarians_by_width,
            plants_by_width,
            vegetarian_fullness: average(vegs.iter().map(|v| v.energy / v.pheno.max_energy)),
            predators_hungry: average(preds.iter().map(|p| p.hungry() as u8 as f64)),
            predator_fullness: average(preds.iter().map(|p| p.energy / p.pheno.max_energy)),
            predator_genes: gene_stats(&predator::GENES, preds.iter().map(|p| &p.genome)),
        }
    }
}

// ── хроника ─────────────────────────────────────────────────────────────────

/// Что случилось. Ключ — для машинного разбора (JSON), текст — для чтения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    VegetariansCrash,
    VegetariansRise,
    VegetariansExtinct,
    PredatorsCrash,
    PredatorsRise,
    PredatorsExtinct,
    PredatorsReturn,
    PlantsAtCap,
    PlantsEatenAgain,
    GeneShift,
    LayerNarrow,
    LayerWide,
    PredatorGeneShift,
    StrategyShift,
}

impl EventKind {
    pub fn key(self) -> &'static str {
        match self {
            EventKind::VegetariansCrash => "vegetarians_crash",
            EventKind::VegetariansRise => "vegetarians_rise",
            EventKind::VegetariansExtinct => "vegetarians_extinct",
            EventKind::PredatorsCrash => "predators_crash",
            EventKind::PredatorsRise => "predators_rise",
            EventKind::PredatorsExtinct => "predators_extinct",
            EventKind::PredatorsReturn => "predators_return",
            EventKind::PlantsAtCap => "plants_at_cap",
            EventKind::PlantsEatenAgain => "plants_eaten_again",
            EventKind::GeneShift => "gene_shift",
            EventKind::LayerNarrow => "layer_narrow",
            EventKind::LayerWide => "layer_wide",
            EventKind::PredatorGeneShift => "predator_gene_shift",
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

/// Во сколько раз численность должна упасть от пика (или вырасти от дна),
/// чтобы это было событием, а не шумом деления.
const SWING: f64 = 2.0;
/// Ниже этой численности (на базовую площадь) колебания не считаются: пять
/// хищников, ставших двумя, — шум, а не обвал.
const SWING_MIN_VEGETARIANS: f64 = 30.0;
const SWING_MIN_PREDATORS: f64 = 4.0;
/// Сдвиг гена, который попадает в хронику: для размера, скорости и зрения —
/// относительный, для генов-процентов — в процентных пунктах.
const GENE_SHIFT_REL: f64 = 0.3;
const GENE_SHIFT_PTS: f64 = 15.0;
/// Доля варианта гена-выбора (стратегии) сдвинулась на столько процентных
/// пунктов — или вариант появился либо исчез.
const SHARE_SHIFT_PTS: f64 = 15.0;
/// Слой травоядных (10‒90% по глубине) уже этого — «сжались», шире второго —
/// «расселились». Зазор между порогами не даёт событию мигать туда-сюда.
const LAYER_NARROW: f64 = 25.0;
const LAYER_WIDE: f64 = 40.0;
/// Растения «у потолка» от этой доли; «снова едят» — ниже второй.
const CAP_HIGH: f64 = 0.95;
const CAP_LOW: f64 = 0.8;

/// Причины перемены численности травоядных за промежуток.
pub fn vegetarian_flows(c: &Counters) -> String {
    let mut text = format!(
        "родилось {}, съедено хищниками {}, умерло с голоду {}",
        c.vegetarians_born, c.vegetarians_eaten, c.vegetarians_starved
    );
    // без каннибализма строка прежняя
    if c.vegetarians_cannibalized > 0 {
        text += &format!(", съедено своими {}", c.vegetarians_cannibalized);
    }
    text
}

pub fn predator_flows(c: &Counters, migrants: u64) -> String {
    format!(
        "родилось {}, пришло мигрантов {migrants}, умерло с голоду {}",
        c.predators_born, c.predators_starved
    )
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

/// Колебание одной популяции: пик и дно с прошлого события.
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
    /// Травоядные и хищники.
    swings: Vec<Swing>,
    /// Сводка каждого гена на прошлой отметке и тик этой отметки.
    gene_base: Option<[(GeneStat, u64); vegetarian::N]>,
    predator_gene_base: Option<[(GeneStat, u64); predator::N]>,
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
            self.swings = vec![Swing { peak: cur.clone(), trough: cur.clone() }; 2];
            self.gene_base = cur.genes.map(|g| g.map(|s| (s, cur.tick)));
            self.predator_gene_base = cur.predator_genes.map(|g| g.map(|s| (s, cur.tick)));
            self.narrow = cur.vegetarian_depth.is_some_and(|d| d.p90 - d.p10 < LAYER_NARROW);
            self.capped = cur.plants as f64 >= cur.plant_cap as f64 * CAP_HIGH;
            return;
        };
        let t = cur.tick;
        let mut push = |kind, text: String| out.push(Event { tick: t, kind, text });

        // ── численности: обвал и подъём считаются от пика/дна с прошлого события,
        // причины — счётчики между ними
        for (species, swing) in self.swings.iter_mut().enumerate() {
            let count = |s: &Snapshot| if species == 0 { s.vegetarians } else { s.predators };
            let min = cur.area * if species == 0 { SWING_MIN_VEGETARIANS } else { SWING_MIN_PREDATORS };
            let n = count(cur);
            if count(&swing.peak) < n {
                swing.peak = cur.clone();
            }
            if count(&swing.trough) > n {
                swing.trough = cur.clone();
            }
            let (peak, trough) = (&swing.peak, &swing.trough);
            let flows = |from: &Snapshot| {
                let c = cur.counters.since(&from.counters);
                if species == 0 {
                    vegetarian_flows(&c)
                } else {
                    predator_flows(&c, cur.migrants - from.migrants)
                }
            };
            let name = if species == 0 { "травоядных" } else { "хищников" };

            let event = if n == 0 && count(&prev) > 0 {
                let (kind, who) = if species == 0 {
                    (EventKind::VegetariansExtinct, "травоядные вымерли")
                } else {
                    (EventKind::PredatorsExtinct, "хищники вымерли")
                };
                Some((
                    kind,
                    format!("{who} (пик {} на тике {}); с пика: {}", count(peak), peak.tick, flows(peak)),
                ))
            } else if n > 0 && count(&prev) == 0 && species == 1 {
                Some((EventKind::PredatorsReturn, format!("хищники вернулись: {n}; {}", flows(&prev))))
            } else if count(peak) as f64 >= min && n as f64 * SWING <= count(peak) as f64 {
                let kind = if species == 0 { EventKind::VegetariansCrash } else { EventKind::PredatorsCrash };
                let text = format!(
                    "обвал {name}: {} (тик {}) → {n}; за это время {}",
                    count(peak),
                    peak.tick,
                    flows(peak)
                );
                Some((kind, text))
            } else if n as f64 >= min && count(trough) as f64 * SWING <= n as f64 && count(trough) > 0 {
                let kind = if species == 0 { EventKind::VegetariansRise } else { EventKind::PredatorsRise };
                let text = format!(
                    "подъём {name}: {} (тик {}) → {n}; за это время {}",
                    count(trough),
                    trough.tick,
                    flows(trough)
                );
                Some((kind, text))
            } else {
                None
            };
            if let Some((kind, text)) = event {
                push(kind, text);
                *swing = Swing { peak: cur.clone(), trough: cur.clone() };
            }
        }

        // ── растения у потолка: их растёт больше, чем успевают съесть
        let fill = cur.plants as f64 / cur.plant_cap as f64;
        if !self.capped && fill >= CAP_HIGH {
            self.capped = true;
            let text = format!(
                "растения упёрлись в потолок ({} из {}): травоядных {} — есть их некому или не там",
                cur.plants, cur.plant_cap, cur.vegetarians
            );
            push(EventKind::PlantsAtCap, text);
        } else if self.capped && fill < CAP_LOW {
            self.capped = false;
            push(
                EventKind::PlantsEatenAgain,
                format!("растения снова поедаются: {} из {}", cur.plants, cur.plant_cap),
            );
        }

        // ── гены: медиана ушла от прошлой отметки — отметка переносится; все
        // сдвиги одного среза — одно событие, иначе хроника тонет в генах
        let (numbers, choices) = gene_shifts(&vegetarian::GENES, cur.genes, &mut self.gene_base, t);
        if !numbers.is_empty() {
            push(EventKind::GeneShift, format!("геном, медиана: {}", numbers.join("; ")));
        }
        if !choices.is_empty() {
            push(EventKind::StrategyShift, format!("травоядные: {}", choices.join("; ")));
        }
        let (numbers, choices) =
            gene_shifts(&predator::GENES, cur.predator_genes, &mut self.predator_gene_base, t);
        if !numbers.is_empty() {
            push(EventKind::PredatorGeneShift, format!("хищники, медиана: {}", numbers.join("; ")));
        }
        if !choices.is_empty() {
            push(EventKind::StrategyShift, format!("хищники: {}", choices.join("; ")));
        }

        // ── слой: где по глубине держатся 80% травоядных
        if let Some(d) = cur.vegetarian_depth {
            let width = d.p90 - d.p10;
            let text = |what: &str| {
                format!(
                    "травоядные {what}: 80% живут на глубине {:.0}‒{:.0}% (медиана {:.0}%)",
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
    "X хищник · O 4+ травоядных · o 1‒3 травоядных · : 4+ растений · . 1‒3 растения · верх — поверхность";

/// Карта мира в `cols` колонок. Клетка показывает самое «важное», что в ней
/// есть: хищник важнее травоядных, травоядные важнее растений. Слева —
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
    let (mut plants, mut vegs, mut preds) =
        (vec![0u32; rows * cols], vec![0u32; rows * cols], vec![0u32; rows * cols]);
    for p in &world.plants {
        plants[cell(p.x, p.y)] += 1;
    }
    for v in &world.vegetarians {
        vegs[cell(v.x, v.y)] += 1;
    }
    for p in &world.predators {
        preds[cell(p.x, p.y)] += 1;
    }

    let border = format!("     +{}+", "-".repeat(cols));
    let mut out = vec![border.clone()];
    for r in 0..rows {
        let line: String = (0..cols)
            .map(|c| {
                let i = r * cols + c;
                match (preds[i], vegs[i], plants[i]) {
                    (p, _, _) if p > 0 => 'X',
                    (_, v, _) if v >= 4 => 'O',
                    (_, v, _) if v > 0 => 'o',
                    (_, _, n) if n >= 4 => ':',
                    (_, _, n) if n > 0 => '.',
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
}
