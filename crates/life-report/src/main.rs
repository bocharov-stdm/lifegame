//! Отчёт о балансе без окна — преемник `python/sim_report.py` (тег python-final).
//!
//!     cargo run -p life-report --release                              # сид 1, 600 тиков
//!     cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
//!     cargo run -p life-report --release -- --predators 20 --predator-speed 18
//!     cargo run -p life-report --release -- --rule plant_energy=80 --rule size_power=1.5
//!     cargo run -p life-report --release -- --scale 100 --ticks 2000   # мир в 100 раз больше
//!     cargo run -p life-report --release -- --compare reference/fingerprint.json
//!     cargo run -p life-report --release -- --save-reference reference/fingerprint.json
//!
//! Чтобы понять, что происходило (человеку или ИИ, без окна):
//!
//!     cargo run -p life-report --release -- --ticks 20000 --maps 4       # рассказ + карты
//!     cargo run -p life-report --release -- --seeds 1 2 3 --story        # рассказ по каждому сиду
//!     cargo run -p life-report --release -- --ticks 5000 --json -        # всё в JSON в stdout
//!
//! Сиды считаются параллельно, по одному на ядро. Лимиты те же, что в Python:
//! бюджет работы растёт с числом тиков, и оборванные прогоны сводка
//! перечисляет отдельно, а не выдаёт за здоровые.

mod json;
mod metrics;
mod story;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use life_core::config::*;
use life_core::genome::vegetarian::Gene;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, WorldConfig};
use life_sim::observe::{self, Event, ascii_map};
use life_sim::{Limits, SimResult, simulate};
use rayon::prelude::*;

/// Бюджет «травоядные x растения» на тик — как WORK_PER_TICK в Python.
const WORK_PER_TICK: f64 = 60_000.0;

#[derive(Parser, Debug)]
#[command(about = "Отчёт о балансе Tiny Life без окна")]
struct Args {
    /// Один сид (если не заданы --seeds).
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Несколько сидов — сводная таблица.
    #[arg(long, num_args = 1..)]
    seeds: Vec<u64>,
    /// Тиков на прогон (по умолчанию 600; для --save-reference — 20 000).
    #[arg(long)]
    ticks: Option<u64>,
    /// Срез раз во столько тиков (по умолчанию — 50 срезов на прогон; для
    /// --save-reference — 60, кратно периоду деления).
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    sample: Option<u64>,
    /// Масштаб мира по площади (1 — базовый 6000x4000; от 1 до 10 000).
    #[arg(long, default_value_t = 1.0, value_parser = parse_scale)]
    scale: f64,
    #[arg(long)]
    vegetarians: Option<usize>,
    #[arg(long)]
    predators: Option<usize>,
    #[arg(long, default_value_t = PREDATOR_BASE_SPEED)]
    predator_speed: f64,
    #[arg(long, default_value_t = PREDATOR_BASE_VISION)]
    predator_vision: f64,
    /// Правило мира: имя=число (можно несколько раз).
    #[arg(long = "rule", value_name = "ИМЯ=ЧИСЛО")]
    rules: Vec<String>,
    /// Бюджет работы на весь прогон (по умолчанию растёт с --ticks).
    #[arg(long)]
    max_work: Option<f64>,
    /// Дедлайн одного прогона, секунд.
    #[arg(long, default_value_t = 600)]
    seconds: u64,
    /// Потоков процессора (по умолчанию все).
    #[arg(long)]
    threads: Option<usize>,
    /// Сверить с эталонным отпечатком (reference/fingerprint.json): берутся его
    /// сиды и число тиков; правила, масштаб и старт обязаны совпадать.
    #[arg(long)]
    compare: Option<PathBuf>,
    /// Снять новый эталон с Rust и записать в ФАЙЛ (формат --compare). Сиды по
    /// умолчанию 1‒8. Нужно после намеренной смены баланса.
    #[arg(long, value_name = "ФАЙЛ")]
    save_reference: Option<PathBuf>,
    /// Рассказ о каждом прогоне: причины смертей, промежутки, геном, глубина,
    /// хроника событий. Для одного сида печатается и без флага.
    #[arg(long)]
    story: bool,
    /// Строк в таблице промежутков рассказа.
    #[arg(long, default_value_t = 12)]
    rows: usize,
    /// Карт мира текстом на прогон — через равные промежутки, последняя в конце
    /// (включает рассказ).
    #[arg(long, default_value_t = 0)]
    maps: usize,
    /// Ширина карты, символов.
    #[arg(long, default_value_t = 72)]
    map_width: usize,
    /// Весь отчёт в JSON: каждый срез, хроника, карты. «-» — в stdout вместо текста.
    #[arg(long, value_name = "ФАЙЛ")]
    json: Option<PathBuf>,
}

fn parse_scale(s: &str) -> Result<f64, String> {
    let scale: f64 = s.trim().parse().map_err(|_| format!("«{s}» — не число"))?;
    if (MIN_SCALE..=MAX_SCALE).contains(&scale) {
        Ok(scale)
    } else {
        Err(format!("масштаб должен быть от {MIN_SCALE} до {MAX_SCALE}"))
    }
}

fn parse_rules(pairs: &[String]) -> Result<Rules, String> {
    let mut rules = Rules::default();
    for pair in pairs {
        let (key, value) = pair.split_once('=').ok_or(format!("правило «{pair}»: нужно имя=число"))?;
        let value: f64 = value.trim().parse().map_err(|_| format!("правило {key}: «{value}» — не число"))?;
        rules = rules.with(key.trim(), value)?;
    }
    Ok(rules)
}

fn main() {
    let mut args = Args::parse();
    if let Some(n) = args.threads {
        rayon::ThreadPoolBuilder::new().num_threads(n).build_global().expect("пул потоков");
    }
    let fail = |e: String| -> ! {
        eprintln!("ошибка: {e}");
        std::process::exit(2);
    };
    let rules = parse_rules(&args.rules).unwrap_or_else(|e| fail(e));
    // JSON в stdout — и больше ничего: текст сломал бы разбор
    let quiet = args.json.as_deref().is_some_and(|p| p.as_os_str() == "-");
    if quiet && (args.compare.is_some() || args.save_reference.is_some()) {
        fail("сверку и эталон нельзя печатать вместе с JSON в stdout: укажите --json ФАЙЛ".into());
    }
    if args.compare.is_some() && args.save_reference.is_some() {
        fail("--compare и --save-reference вместе не имеют смысла".into());
    }
    let saving = args.save_reference.is_some();
    if saving {
        if args.seeds.is_empty() {
            args.seeds = (1..=8).collect();
        }
        args.ticks.get_or_insert(20_000);
        args.sample.get_or_insert(metrics::REFERENCE_SAMPLE);
    }
    let base_cfg = WorldConfig {
        seed: 0,
        scale: args.scale,
        rules: rules.clone(),
        n_vegetarians: args.vegetarians,
        n_predators: args.predators,
        predator_speed: args.predator_speed,
        predator_vision: args.predator_vision,
        ..Default::default()
    };

    let reference = args.compare.as_ref().map(|path| {
        metrics::Reference::load(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())))
    });
    if let Some(r) = &reference {
        // сравнивать можно только одинаковые миры: иначе «расхождение» — это
        // разница условий, а не баланса
        r.check_same_world(&base_cfg).unwrap_or_else(|e| fail(format!("эталон снят на другом мире: {e}")));
        if let Some(note) = r.genes_note() {
            eprintln!("заметка: {note}");
        }
        args.seeds = r.seeds.clone();
        args.ticks = Some(r.ticks);
        args.sample = Some(r.sample_every);
    }
    if reference.is_some() || saving {
        // эталон снимается без бюджета работы — иначе сравнивали бы оборванное с целым
        args.max_work.get_or_insert(1e15);
    }

    let ticks = args.ticks.unwrap_or(600);
    let seeds = if args.seeds.is_empty() { vec![args.seed] } else { args.seeds.clone() };
    let sample = args.sample.unwrap_or((ticks / 50).max(1));
    let limits = Limits {
        ticks,
        sample_every: sample,
        max_creatures: 50_000,
        max_total_work: args.max_work.unwrap_or(WORK_PER_TICK * ticks as f64),
        deadline: Duration::from_secs(args.seconds),
    };

    if !quiet {
        println!(
            "Мир x{}: {:.0}x{:.0}, {} тиков, сиды {:?}, потоков {}",
            args.scale,
            WORLD_WIDTH * args.scale,
            WORLD_HEIGHT,
            ticks,
            seeds,
            rayon::current_num_threads()
        );
    }
    let started = Instant::now();
    let map_every = if args.maps > 0 { (ticks / args.maps as u64).max(1) } else { u64::MAX };
    let (results, maps): (Vec<(u64, SimResult)>, Vec<Vec<story::Map>>) = seeds
        .par_iter()
        .map(|&seed| {
            let cfg = WorldConfig { seed, ..base_cfg.clone() };
            let mut maps = Vec::new();
            let res = simulate(&cfg, &limits, |w| {
                if w.tick.is_multiple_of(map_every) {
                    maps.push((w.tick, ascii_map(w, args.map_width)));
                }
            });
            // последняя карта — всегда конечное состояние, даже если прогон оборван
            if args.maps > 0 && maps.last().map(|m| m.0) != Some(res.world.tick) {
                maps.push((res.world.tick, ascii_map(&res.world, args.map_width)));
            }
            ((seed, res), maps)
        })
        .unzip();
    let events: Vec<Vec<Event>> = results.iter().map(|(_, r)| observe::events(&r.snapshots)).collect();

    if let Some(path) = &args.json {
        let runs: Vec<json::Run> = results
            .iter()
            .zip(&events)
            .zip(&maps)
            .map(|(((seed, res), events), maps)| json::Run { seed: *seed, res, events, maps })
            .collect();
        let text = serde_json::to_string_pretty(&json::report(&base_cfg, &rules, ticks, sample, &runs))
            .expect("JSON собирается всегда");
        if quiet {
            println!("{text}");
            return;
        }
        std::fs::write(path, text).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    }

    let story = args.story || args.maps > 0 || (results.len() == 1 && reference.is_none());
    if story {
        for (((seed, res), events), maps) in results.iter().zip(&events).zip(&maps) {
            story::print_story(*seed, res, events, maps, args.rows);
        }
    }
    print_summary(&results);
    let agrees = reference.as_ref().is_none_or(|r| metrics::print_comparison(r, &results));
    if let Some(path) = &args.json {
        println!("\nJSON: {}", path.display());
    }
    if let Some(path) = &args.save_reference {
        metrics::save_reference(path, &base_cfg, ticks, sample, &results)
            .unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
        println!("\nэталон записан: {}", path.display());
    }
    println!("\nвсего {:.1} с", started.elapsed().as_secs_f64());
    // Сверка — проверка, а не справка: расхождение с эталоном должно ронять CI.
    if !agrees {
        eprintln!("ошибка: баланс разошёлся с эталоном");
        std::process::exit(1);
    }
}

fn print_summary(results: &[(u64, SimResult)]) {
    println!(
        "\n{:>6} {:<18} {:>7} {:>6} {:>5} {:>6} {:>5} {:>5} {:>8} {:>8} {:>8}",
        "сид", "итог", "тиков", "трав", "хищн", "растен", "хищн%", "мигр", "разм.макс", "разм.фин", "мс/тик"
    );
    for (seed, r) in results {
        let last = r.last();
        let with_pred = r.history.iter().filter(|s| s.predators > 0).count() as f64 / r.history.len() as f64;
        let sizes: Vec<f64> =
            r.history.iter().filter_map(|s| s.avg_genom.map(|g| g[Gene::Size as usize])).collect();
        let smax = sizes.iter().copied().fold(f64::NAN, f64::max);
        let sfin = sizes.last().copied().unwrap_or(f64::NAN);
        println!(
            "{seed:>6} {:<18} {:>7} {:>6} {:>5} {:>6} {:>4.0}% {:>5} {:>8.1} {:>8.1} {:>8.3}",
            r.stop.to_string(),
            r.ticks_done,
            last.vegetarians,
            last.predators,
            last.plants,
            with_pred * 100.0,
            r.world.migrants,
            smax,
            sfin,
            r.ms_per_tick()
        );
    }
    let cut: Vec<String> = results
        .iter()
        .filter(|(_, r)| !r.ok())
        .map(|(s, r)| format!("сид {s}: {} на тике {}", r.stop, r.ticks_done))
        .collect();
    if cut.is_empty() {
        println!("\nвсе прогоны дошли до конца");
    } else {
        println!("\nоборваны: {}", cut.join("; "));
    }
}
