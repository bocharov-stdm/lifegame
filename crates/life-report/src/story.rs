//! Рассказ о прогоне текстом: итог, отчего менялась численность, промежутки,
//! геном, глубина, хроника событий и карты. Написан так, чтобы по нему одному —
//! без графиков и окна — можно было понять, что происходило в мире.

use life_core::flora::{self, Profile};
use life_core::genome::creature::Gene;
use life_core::genome::{GeneSpec, creature};
use life_sim::SimResult;
use life_sim::observe::{Event, GeneStat, MAP_LEGEND, MAX_VARIANTS, Snapshot, Spread, describe_flows};

/// Карта: тик и строки.
pub type Map = (u64, Vec<String>);

fn opt(v: Option<f64>, digits: usize) -> String {
    v.map_or("—".into(), |v| format!("{v:.digits$}"))
}

fn percent(part: u64, whole: u64) -> String {
    if whole == 0 { "—".into() } else { format!("{:.0}%", part as f64 * 100.0 / whole as f64) }
}

fn spread(s: &Spread) -> String {
    format!("{:.1} ({:.1}‒{:.1})", s.p50, s.p10, s.p90)
}

pub fn print_story(seed: u64, res: &SimResult, events: &[Event], maps: &[Map], rows: usize) {
    let snaps = &res.snapshots;
    let (first, last) = (&snaps[0], snaps.last().expect("срезы есть всегда"));
    let c = last.counters.since(&first.counters);

    println!("\n══ сид {seed}: {}, {} тиков, {:.3} мс/тик ══", res.stop, res.ticks_done, res.ms_per_tick());
    let space = res.world.space;
    println!("Мир {:.0}x{:.0}; еда {}.", space.width, space.height, flora::describe(&res.world.rules));
    println!(
        "Итог на тике {}: растений {} из {}, существ {}.",
        last.tick, last.plants, last.plant_cap, last.creatures
    );
    println!(
        "Существа за прогон: {} (из умерших съедено {}).",
        describe_flows(&c),
        percent(c.cannibalized, c.cannibalized + c.starved)
    );
    println!("Растения за прогон: выросло {}, съедено {}.", c.plants_grown, c.plants_eaten);

    print_intervals(snaps, rows);
    print_genome(first, last);
    print_depth(last);
    // по ширине смотреть есть на что, только если еда по ней неравномерна
    if res.world.rules.plant_width.kind() != Profile::Uniform {
        print_width(last);
    }

    println!("\nХроника:");
    if events.is_empty() {
        println!("  событий нет: численности и геном без резких перемен");
    }
    for e in events {
        println!("  тик {:>6}  {}", e.tick, e.text);
    }
    if !res.ok() {
        println!("  тик {:>6}  прогон остановлен: {}", res.ticks_done, res.stop);
    }

    if !maps.is_empty() {
        println!("\nКарты мира ({MAP_LEGEND}):");
        for (tick, lines) in maps {
            println!("\n  тик {tick}");
            for line in lines {
                println!("  {line}");
            }
        }
    }
}

/// Таблица по промежуткам: состояние на конец промежутка и потоки за него.
fn print_intervals(snaps: &[Snapshot], rows: usize) {
    if snaps.len() < 2 || rows == 0 {
        return;
    }
    let steps = snaps.len() - 1;
    let rows = rows.min(steps);
    println!("\nПо промежуткам (численность и геном — на конец, рождения и смерти — за промежуток):");
    println!(
        "{:>13} {:>6} {:>5} │ {:>21} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
        "тики", "растен", "сущ", "сущ: +род −съед −гол", "размер", "скор", "зрение", "слой, %", "сыт"
    );
    let mut from = 0;
    for r in 1..=rows {
        let to = r * steps / rows;
        let (a, b) = (&snaps[from], &snaps[to]);
        let c = b.counters.since(&a.counters);
        let gene = |g: Gene| opt(b.genes.and_then(|s| s[g as usize].spread().map(|x| x.p50)), 1);
        let layer = b.depth.map_or("—".into(), |d| format!("{:.0}‒{:.0}", d.p10, d.p90));
        println!(
            "{:>13} {:>6} {:>5} │ {:>7} {:>6} {:>6} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
            format!("{}‒{}", a.tick, b.tick),
            b.plants,
            b.creatures,
            format!("+{}", c.born),
            format!("−{}", c.cannibalized),
            format!("−{}", c.starved),
            gene(Gene::Size),
            gene(Gene::Speed),
            gene(Gene::Vision),
            layer,
            opt(b.fullness.map(|f| f * 100.0), 0),
        );
        from = to;
    }
    println!("  («−съед» — съедены сородичами; «сыт» — средняя заполненность бака, %)");
}

fn print_genome(first: &Snapshot, last: &Snapshot) {
    println!("\nГеном существ, медиана (10‒90% популяции): начало → конец");
    match (first.genes, last.genes) {
        (Some(a), Some(b)) => print_genes(&creature::GENES, &a, &b),
        (Some(_), None) => println!("  к концу существ не осталось"),
        _ => println!("  существ не было"),
    }
}

/// Гены вида от начала к концу: у числовых — медиана и разброс, у генов-выборов
/// — доли вариантов. Ген-выбор с одним вариантом не печатается: он ничего не
/// различает.
fn print_genes<const N: usize>(genes: &[GeneSpec; N], a: &[GeneStat; N], b: &[GeneStat; N]) {
    for (g, spec) in genes.iter().enumerate() {
        let text = |stat: &GeneStat| match stat {
            GeneStat::Number(s) => spread(s),
            GeneStat::Shares(s) => shares(spec, s),
        };
        if spec.variants().is_some_and(|v| v.len() < 2) {
            continue;
        }
        println!("  {:<13} {:>22} → {}", spec.label, text(&a[g]), text(&b[g]));
    }
}

/// «стандартный 70%, трусливый 30%» — варианты, которые есть в популяции.
fn shares(spec: &GeneSpec, s: &[f64; MAX_VARIANTS]) -> String {
    let variants = spec.variants().unwrap_or_default();
    let parts: Vec<String> = variants
        .iter()
        .zip(s)
        .filter(|&(_, &share)| share > 0.0)
        .map(|(v, share)| format!("{} {:.0}%", v.label, share * 100.0))
        .collect();
    parts.join(", ")
}

/// Кто где по глубине: доли существ и растений в каждой десятой части.
fn print_depth(last: &Snapshot) {
    println!("\nГлубина в конце (0% — поверхность):");
    print_bands(last, "глубина", &last.creatures_by_depth, &last.plants_by_depth);
}

/// То же по ширине, слева направо.
fn print_width(last: &Snapshot) {
    println!("\nШирина в конце (0% — левый край):");
    print_bands(last, "ширина", &last.creatures_by_width, &last.plants_by_width);
}

fn print_bands(last: &Snapshot, axis: &str, creatures: &[usize], plants: &[usize]) {
    let (nv, np) = (last.creatures.max(1) as f64, last.plants.max(1) as f64);
    println!("  {axis:>9} {:>11} {:>9}", "существа", "растения");
    let step = 100 / creatures.len();
    for (b, (v, p)) in creatures.iter().zip(plants).enumerate() {
        println!(
            "  {:>9} {:>10.0}% {:>8.0}%",
            format!("{}‒{}%", b * step, (b + 1) * step),
            *v as f64 * 100.0 / nv,
            *p as f64 * 100.0 / np
        );
    }
}
