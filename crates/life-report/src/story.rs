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
        "Итог на тике {}: растений {} из {}, трупов {}, существ {}.",
        last.tick, last.plants, last.plant_cap, last.corpses, last.creatures
    );
    println!(
        "Существа за прогон: {} (из умерших погибли в бою {}).",
        describe_flows(&c),
        percent(c.combat, c.starved + c.old_age + c.combat)
    );
    println!("Растения за прогон: выросло {}, съедено {}.", c.plants_grown, c.plants_eaten);
    println!(
        "Питание: порций растений {}, трупов {} (из них гнили {}, костей {}). Выстрелов {}.",
        c.plant_bites, c.meat_bites, c.rot_bites, c.bone_bites, c.ranged_shots
    );
    println!(
        "Трупы: появилось {}, убрано {}; из убранных лежали на дне с мясом {}, дошли до костей {}, лежали в среднем {:.0} тиков.",
        c.corpses,
        c.corpses_gone,
        percent(c.corpses_bottom, c.corpses_gone),
        percent(c.skeletons, c.corpses_gone),
        c.corpse_ticks as f64 / c.corpses_gone.max(1) as f64
    );
    print_diets(&c.by_diet, last);

    println!("Молодых {} из {}.", last.juveniles, last.creatures);
    for (i, a) in life_core::creature::Activity::ALL.iter().enumerate() {
        println!("  {}: {}", a.label(), percent(last.activities[i] as u64, last.creatures as u64));
    }
    print_intervals(snaps, rows);
    print_genome(first, last);
    print_programs(&res.world);
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
/// Births and deaths of each diet, and who strikes and kills whom.
fn print_diets(by: &life_core::DietCounters, last: &Snapshot) {
    let names = creature::DIET_VARIANTS.map(|v| v.key);
    println!("By diet: born, died starved / of old age / in combat, alive at the end.");
    for (d, name) in names.iter().enumerate() {
        let [starved, old, combat] = by.deaths[d];
        println!(
            "  {name:<10} born {:>7}, died {starved} / {old} / {combat}, alive {}",
            by.born[d], last.diets[d].creatures
        );
    }
    for (title, table) in [("Strikes", &by.strikes), ("Kills", &by.kills)] {
        println!("{title} (row → column): {}", names.map(|n| format!("{n:>10}")).join(""));
        for (d, name) in names.iter().enumerate() {
            println!("  {name:<10}{}", table[d].map(|n| format!("{n:>10}")).join(""));
        }
    }
}

fn print_intervals(snaps: &[Snapshot], rows: usize) {
    if snaps.len() < 2 || rows == 0 {
        return;
    }
    let steps = snaps.len() - 1;
    let rows = rows.min(steps);
    println!("\nПо промежуткам (численность и геном — на конец, рождения и смерти — за промежуток):");
    println!(
        "{:>13} {:>6} {:>5} │ {:>28} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
        "тики", "растен", "сущ", "сущ: +род −гол −возр −бой", "размер", "скор", "зрение", "слой, %", "сыт"
    );
    let mut from = 0;
    for r in 1..=rows {
        let to = r * steps / rows;
        let (a, b) = (&snaps[from], &snaps[to]);
        let c = b.counters.since(&a.counters);
        let gene = |g: Gene| opt(b.genes.and_then(|s| s[g as usize].spread().map(|x| x.p50)), 1);
        let layer = b.depth.map_or("—".into(), |d| format!("{:.0}‒{:.0}", d.p10, d.p90));
        println!(
            "{:>13} {:>6} {:>5} │ {:>6} {:>6} {:>6} {:>6} │ {:>6} {:>5} {:>6} │ {:>9} {:>5}",
            format!("{}‒{}", a.tick, b.tick),
            b.plants,
            b.creatures,
            format!("+{}", c.born),
            format!("−{}", c.starved),
            format!("−{}", c.old_age),
            format!("−{}", c.combat),
            gene(Gene::Size),
            gene(Gene::Speed),
            gene(Gene::Vision),
            layer,
            opt(b.fullness.map(|f| f * 100.0), 0),
        );
        from = to;
    }
    println!("  («сыт» — средняя заполненность бака, %)");
}

fn print_genome(first: &Snapshot, last: &Snapshot) {
    println!("\nГеном существ, медиана (10‒90% популяции): начало → конец");
    match (first.genes, last.genes) {
        (Some(a), Some(b)) => print_genes(&creature::GENES, &a, &b),
        (Some(_), None) => println!("  к концу существ не осталось"),
        _ => println!("  существ не было"),
    }
}

/// The behaviour programs alive at the end, the juvenile track and the adult one apart. The numbers
/// of every mutating child drift, so programs are grouped by their shape (`Program::shape`: tests,
/// actions and flags, no numbers): how many shapes, how many keep the founders' one, and the three
/// most common, each with the medians of its numbers. `METRIC` lines give a sweep the same: shapes,
/// the template's share, the medians of the hunt's ratio and of how far the threat tests look, and
/// the shares that remember (a working «режим» setting) and switch their layer by a condition.
fn print_programs(world: &life_core::World) {
    use life_core::creature::{ADULT, Action, JUVENILE, Program, Strategy};
    let total = world.creatures.len();
    if total == 0 {
        return;
    }
    let median = |mut xs: Vec<f64>| {
        xs.sort_by(f64::total_cmp);
        xs.get(xs.len() / 2).copied().unwrap_or(f64::NAN)
    };
    // both templates have one shape: the lurker differs only in a number; a shooting founder has
    // one more block, and a founder's layer is a number
    let template = Program::STANDARD.shape();
    let shooter = Program::founder(Strategy::Standard, (5, 100), true).shape();
    for (stage, track, key) in [(JUVENILE, "детская", "juvenile"), (ADULT, "взрослая", "adult")]
    {
        let mut groups: std::collections::BTreeMap<Vec<u64>, Vec<Program>> = Default::default();
        for v in &world.creatures {
            let p = v.programs[stage];
            groups.entry(p.shape()).or_default().push(p);
        }
        let on_template =
            [&template, &shooter].map(|s| groups.get(s).map_or(0, Vec::len)).iter().sum::<usize>();
        let changed = world.creatures.iter().filter(|v| v.programs[stage].changes > 0).count();
        println!(
            "\nПрограммы поведения к концу, {track} дорожка: {} форм у {total} существ; форма шаблона у {}, \
             мутировавших {}",
            groups.len(),
            percent(on_template as u64, total as u64),
            percent(changed as u64, total as u64)
        );
        let mut top: Vec<(&Vec<u64>, &Vec<Program>)> = groups.iter().collect();
        top.sort_by_key(|g| std::cmp::Reverse(g.1.len()));
        for (shape, group) in top.into_iter().take(3) {
            let name = if *shape == template {
                " — форма шаблонов основателей"
            } else if *shape == shooter {
                " — форма основателей-стрелков"
            } else {
                ""
            };
            println!("  {}{name}, числа — медианы:", percent(group.len() as u64, total as u64));
            let middle = Program::median(group).expect("a group has one shape");
            for line in middle.describe() {
                println!("    {line}");
            }
        }
        let programs = || world.creatures.iter().map(|v| v.programs[stage]);
        let hunt = median(programs().filter_map(|p| p.hunt_ratio()).collect());
        let threat = median(programs().map(|p| p.threat_range()).collect());
        let having = |works: &dyn Fn(&Program, usize) -> bool| {
            let n = programs().filter(|p| (0..p.blocks().len()).any(|i| p.live(i) && works(p, i))).count();
            n as f64 / total as f64
        };
        let modes = having(&|p, i| p.blocks()[i].action == Action::Mode);
        let layers = having(&|p, i| {
            let b = &p.blocks()[i];
            b.action == Action::Layer && b.when.iter().any(|t| !t.always())
        });
        // how fit for evolution the programs are: their length, the dead blocks among them (off or
        // never reached), the memories they keep, and how far the numbers of the biggest shape
        // have spread (0: copies, about 0.5: random)
        let blocks = median(programs().map(|p| p.blocks().len() as f64).collect());
        let (all, dead) = programs().fold((0, 0), |(all, dead), p| {
            let n = p.blocks().len();
            (all + n, dead + (0..n).filter(|&i| !p.live(i)).count())
        });
        let dead_share = dead as f64 / all.max(1) as f64;
        let memories = median(
            programs()
                .map(|p| {
                    (0..p.blocks().len())
                        .filter(|&i| p.live(i) && p.blocks()[i].action == Action::Mode)
                        .count() as f64
                })
                .collect(),
        );
        let spread =
            groups.values().max_by_key(|g| g.len()).and_then(|g| Program::spread(g)).unwrap_or(f64::NAN);
        println!(
            "  блоков в программе {blocks:.0}, мёртвых {}, режимов {memories:.0}, разброс чисел главной формы {spread:.2}",
            percent((dead_share * 10_000.0).round() as u64, 10_000)
        );
        println!("METRIC {key}_shapes {}", groups.len());
        println!("METRIC {key}_template_share {:.4}", on_template as f64 / total as f64);
        println!("METRIC {key}_hunt_ratio {hunt:.3}");
        println!("METRIC {key}_threat_range {threat:.3}");
        println!("METRIC {key}_mode_share {modes:.4}");
        println!("METRIC {key}_conditional_layer_share {layers:.4}");
        println!("METRIC {key}_blocks {blocks:.1}");
        println!("METRIC {key}_dead_share {dead_share:.4}");
        println!("METRIC {key}_modes {memories:.1}");
        println!("METRIC {key}_spread {spread:.4}");
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
