//! Рассказ о прогоне текстом: итог, отчего менялась численность, промежутки,
//! геном, глубина, хроника событий и карты. Написан так, чтобы по нему одному —
//! без графиков и окна — можно было понять, что происходило в мире.

use life_core::genome::GENE_LABELS;
use life_sim::SimResult;
use life_sim::observe::{DEPTH_BANDS, Event, MAP_LEGEND, Snapshot, Spread, predator_flows, vegetarian_flows};

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
    println!(
        "Итог на тике {}: растений {} из {}, травоядных {}, хищников {}.",
        last.tick, last.plants, last.plant_cap, last.vegetarians, last.predators
    );
    println!(
        "Травоядные за прогон: {} (из умерших съедено {}).",
        vegetarian_flows(&c),
        percent(c.vegetarians_eaten, c.vegetarians_eaten + c.vegetarians_starved)
    );
    println!("Хищники за прогон: {}.", predator_flows(&c, last.migrants));
    println!("Растения за прогон: выросло {}, съедено {}.", c.plants_grown, c.plants_eaten);

    print_intervals(snaps, rows);
    print_genome(first, last);
    print_depth(last);

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
        "{:>13} {:>6} {:>5} {:>4} │ {:>21} │ {:>13} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
        "тики",
        "растен",
        "трав",
        "хищ",
        "трав: +род −съед −гол",
        "хищ: +род −гол",
        "размер",
        "скор",
        "зрение",
        "слой, %",
        "сыт"
    );
    let mut from = 0;
    for r in 1..=rows {
        let to = r * steps / rows;
        let (a, b) = (&snaps[from], &snaps[to]);
        let c = b.counters.since(&a.counters);
        let gene = |g: usize| opt(b.genes.map(|s| s[g].p50), 1);
        let layer = b.vegetarian_depth.map_or("—".into(), |d| format!("{:.0}‒{:.0}", d.p10, d.p90));
        println!(
            "{:>13} {:>6} {:>5} {:>4} │ {:>7} {:>6} {:>6} │ {:>6} {:>6} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
            format!("{}‒{}", a.tick, b.tick),
            b.plants,
            b.vegetarians,
            b.predators,
            format!("+{}", c.vegetarians_born),
            format!("−{}", c.vegetarians_eaten),
            format!("−{}", c.vegetarians_starved),
            format!("+{}", c.predators_born + (b.migrants - a.migrants)),
            format!("−{}", c.predators_starved),
            gene(0),
            gene(1),
            gene(2),
            layer,
            opt(b.vegetarian_fullness.map(|f| f * 100.0), 0),
        );
        from = to;
    }
    println!("  (у хищников «+род» включает мигрантов; «сыт» — средняя заполненность бака травоядных, %)");
}

fn print_genome(first: &Snapshot, last: &Snapshot) {
    println!("\nГеном травоядных, медиана (10‒90% популяции): начало → конец");
    match (first.genes, last.genes) {
        (Some(a), Some(b)) => {
            for (g, label) in GENE_LABELS.iter().enumerate() {
                println!("  {label:<13} {:>22} → {}", spread(&a[g]), spread(&b[g]));
            }
        }
        (Some(_), None) => println!("  к концу травоядных не осталось"),
        _ => println!("  травоядных не было"),
    }
    println!(
        "Хищники в конце: скорость {}, зрение {}, голодных {}, заполненность бака {}.",
        opt(last.predator_speed, 1),
        opt(last.predator_vision, 0),
        opt(last.predators_hungry.map(|h| h * 100.0), 0) + "%",
        opt(last.predator_fullness.map(|f| f * 100.0), 0) + "%"
    );
}

/// Кто где по глубине: доли травоядных и растений в каждой десятой части.
fn print_depth(last: &Snapshot) {
    let (nv, np) = (last.vegetarians.max(1) as f64, last.plants.max(1) as f64);
    println!("\nГлубина в конце (0% — поверхность, где растений больше всего):");
    println!("  {:>9} {:>11} {:>9}", "глубина", "травоядные", "растения");
    for b in 0..DEPTH_BANDS {
        let step = 100 / DEPTH_BANDS;
        println!(
            "  {:>9} {:>10.0}% {:>8.0}%",
            format!("{}‒{}%", b * step, (b + 1) * step),
            last.vegetarians_by_depth[b] as f64 * 100.0 / nv,
            last.plants_by_depth[b] as f64 * 100.0 / np
        );
    }
}
