//! Отчёт о балансе без окна — преемник `python/sim_report.py`.
//!
//!     cargo run -p life-report --release                              # сид 1, 600 тиков
//!     cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
//!     cargo run -p life-report --release -- --predators 20 --predator-speed 18
//!     cargo run -p life-report --release -- --rule plant_energy=80 --rule size_power=1.5
//!     cargo run -p life-report --release -- --scale 100 --ticks 2000   # мир в 100 раз больше
//!     cargo run -p life-report --release -- --compare reference/fingerprint.json
//!
//! Сиды считаются параллельно, по одному на ядро. Лимиты те же, что в Python:
//! бюджет работы растёт с числом тиков, и оборванные прогоны сводка
//! перечисляет отдельно, а не выдаёт за здоровые.

mod metrics;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use life_core::config::*;
use life_core::genome::GENE_LABELS;
use life_core::{Rules, WorldConfig};
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
    #[arg(long, default_value_t = 600)]
    ticks: u64,
    /// Снимок раз во столько тиков.
    #[arg(long)]
    sample: Option<u64>,
    /// Масштаб мира по площади (1 — базовый 6000x4000).
    #[arg(long, default_value_t = 1.0)]
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
    /// Сверить с эталонным отпечатком Python-версии (reference/fingerprint.json):
    /// берутся его сиды и число тиков.
    #[arg(long)]
    compare: Option<PathBuf>,
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
    let rules = parse_rules(&args.rules).unwrap_or_else(|e| {
        eprintln!("ошибка: {e}");
        std::process::exit(2);
    });

    let reference = args.compare.as_ref().map(|path| {
        metrics::Reference::load(path).unwrap_or_else(|e| {
            eprintln!("ошибка: {}: {e}", path.display());
            std::process::exit(2);
        })
    });
    if let Some(r) = &reference {
        args.seeds = r.seeds.clone();
        args.ticks = r.ticks;
        args.sample = Some(r.sample_every);
        // эталон снят без бюджета работы — иначе сравнивали бы оборванное с целым
        args.max_work.get_or_insert(1e15);
    }

    let seeds = if args.seeds.is_empty() { vec![args.seed] } else { args.seeds.clone() };
    let sample = args.sample.unwrap_or((args.ticks / 20).max(1));
    let limits = Limits {
        ticks: args.ticks,
        sample_every: sample,
        max_creatures: 50_000,
        max_total_work: args.max_work.unwrap_or(WORK_PER_TICK * args.ticks as f64),
        deadline: Duration::from_secs(args.seconds),
    };

    println!(
        "Мир x{}: {:.0}x{:.0}, {} тиков, сиды {:?}, потоков {}",
        args.scale,
        WORLD_WIDTH * args.scale,
        WORLD_HEIGHT,
        args.ticks,
        seeds,
        rayon::current_num_threads()
    );
    let started = Instant::now();
    let results: Vec<(u64, SimResult)> = seeds
        .par_iter()
        .map(|&seed| {
            let cfg = WorldConfig {
                seed,
                scale: args.scale,
                rules: rules.clone(),
                n_vegetarians: args.vegetarians,
                n_predators: args.predators,
                predator_speed: args.predator_speed,
                predator_vision: args.predator_vision,
            };
            (seed, simulate(&cfg, &limits, |_| {}))
        })
        .collect();

    if results.len() == 1 && reference.is_none() {
        print_trajectory(&results[0].1);
    }
    print_summary(&results);
    if let Some(r) = &reference {
        metrics::print_comparison(r, &results);
    }
    println!("\nвсего {:.1} с", started.elapsed().as_secs_f64());
}

fn print_trajectory(res: &SimResult) {
    println!("\n{:>7} {:>7} {:>6} {:>6}  средний геном", "тик", "растен", "трав", "хищн");
    for s in &res.history {
        let genom = s.avg_genom.map_or("—".to_string(), |g| {
            g.iter().zip(GENE_LABELS).map(|(v, l)| format!("{l} {v:.1}")).collect::<Vec<_>>().join("  ")
        });
        println!("{:>7} {:>7} {:>6} {:>6}  {genom}", s.tick, s.plants, s.vegetarians, s.predators);
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
        let sizes: Vec<f64> = r.history.iter().filter_map(|s| s.avg_genom.map(|g| g[0])).collect();
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
