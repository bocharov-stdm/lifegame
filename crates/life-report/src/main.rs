//! A balance report without a window — the successor of `python/sim_report.py` (tag python-final).
//!
//!     cargo run -p life-report --release                              # seed 1, 600 ticks
//!     cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
//!     cargo run -p life-report --release -- --rule plant_energy=80 --rule size_power=1.5
//!     cargo run -p life-report --release -- --scale 100 --ticks 2000   # a world 100 times bigger
//!     cargo run -p life-report --release -- --scale 100 --shape 1:1    # and a square one
//!     cargo run -p life-report --release -- --mix 1 1                # the strategies equally
//!     cargo run -p life-report --release -- --diet-mix 1             # all herbivores
//!     cargo run -p life-report --release -- --compare reference/fingerprint.json
//!     cargo run -p life-report --release -- --save-reference reference/fingerprint.json
//!
//! To understand what happened (for a person or an AI, without a window):
//!
//!     cargo run -p life-report --release -- --ticks 20000 --maps 4       # the story + maps
//!     cargo run -p life-report --release -- --seeds 1 2 3 --story        # a story for every seed
//!     cargo run -p life-report --release -- --ticks 5000 --json -        # everything as JSON to stdout
//!
//! The seeds are computed in parallel, one to a core. The limits are the same as in Python: the
//! work budget grows with the number of ticks, and the summary lists cut-off runs separately
//! instead of passing them off as healthy.

mod json;
mod metrics;
mod story;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use life_core::creature::strategy as creature_strategy;
use life_core::genome::Variant;
use life_core::genome::creature::Gene;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, WorldConfig};
use life_sim::observe::{self, Event, ascii_map};
use life_sim::{Limits, SimResult, simulate};
use rayon::prelude::*;

/// The «creatures x plants» budget per tick — like WORK_PER_TICK in Python.
const WORK_PER_TICK: f64 = 60_000.0;

#[derive(Parser, Debug)]
#[command(about = "Отчёт о балансе lifegame без окна")]
struct Args {
    /// One seed (if --seeds is not given).
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Several seeds — a summary table.
    #[arg(long, num_args = 1..)]
    seeds: Vec<u64>,
    /// Ticks per run (600 by default; for --save-reference — 20 000).
    #[arg(long)]
    ticks: Option<u64>,
    /// A sample once in this many ticks (by default 50 samples per run; for --save-reference — 60,
    /// a multiple of the division period).
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    sample: Option<u64>,
    /// The world's scale by area (1 is the base 6000x4000; from 1 to 10 000).
    #[arg(long, default_value_t = 1.0, value_parser = parse_scale)]
    scale: f64,
    /// The world's shape: 1:1, 3:2, 2:1 or strip (a strip 4000 high, as before the shapes).
    /// At scale 1 a strip and 3:2 are one and the same 6000x4000 world.
    #[arg(long, default_value = "3:2", value_parser = Shape::parse)]
    shape: Shape,
    /// Creatures at the start (by default by the world's area). The old name `--vegetarians` is
    /// understood too.
    #[arg(long, alias = "vegetarians")]
    creatures: Option<usize>,
    /// The starting mix of strategies: the shares of the variants in order (standard, lurker).
    /// For example, `--mix 1 1` — equally. The old name is `--veg-mix`.
    #[arg(long, alias = "veg-mix", num_args = 1.., value_name = "ДОЛИ")]
    mix: Vec<f64>,
    /// The founders' diets: the shares in order (herbivore, omnivore, scavenger, carnivore).
    /// By default 70 30 0 0 — meat diets arise from mutants; `--diet-mix 1` — all herbivores.
    #[arg(long, num_args = 1.., value_name = "ДОЛИ")]
    diet_mix: Vec<f64>,
    /// How many times bigger the meat-eating founders (scavengers, carnivores) start; default 2.
    #[arg(long, value_name = "РАЗ")]
    meat_founders: Option<f64>,
    /// A world rule: name=number (can be repeated). A food profile is also given by name:
    /// `--rule plant_width_profile=waves`.
    #[arg(long = "rule", value_name = "ИМЯ=ЧИСЛО")]
    rules: Vec<String>,
    /// The work budget for the whole run (by default grows with --ticks).
    #[arg(long)]
    max_work: Option<f64>,
    /// The deadline of one run, seconds.
    #[arg(long, default_value_t = 600)]
    seconds: u64,
    /// Processor threads (all by default).
    #[arg(long)]
    threads: Option<usize>,
    /// Check against a reference fingerprint (reference/fingerprint.json): its seeds and number of
    /// ticks are taken; the rules, the scale and the start must match.
    #[arg(long)]
    compare: Option<PathBuf>,
    /// Take a new reference from Rust and write it to a FILE (the --compare format). The default
    /// seeds are 1‒8. Needed after a deliberate change of balance.
    #[arg(long, value_name = "ФАЙЛ")]
    save_reference: Option<PathBuf>,
    /// A story of each run: causes of deaths, intervals, genome, depth, the chronicle of events.
    /// For one seed it is printed even without the flag.
    #[arg(long)]
    story: bool,
    /// Rows in the story's table of intervals.
    #[arg(long, default_value_t = 12)]
    rows: usize,
    /// Text maps of the world per run — at equal intervals, the last at the end (includes the story).
    #[arg(long, default_value_t = 0)]
    maps: usize,
    /// The map's width, characters.
    #[arg(long, default_value_t = 72)]
    map_width: usize,
    /// The whole report as JSON: every sample, the chronicle, maps. «-» — to stdout instead of text.
    #[arg(long, value_name = "ФАЙЛ")]
    json: Option<PathBuf>,
    /// Write «tick of ticks» to this file about once a second (one seed; `life-sweep` shows it).
    #[arg(long, value_name = "ФАЙЛ")]
    progress: Option<PathBuf>,
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
        rules = rules.with_text(key.trim(), value)?;
    }
    Ok(rules)
}

/// A mix of strategies: the shares are non-negative, sum to more than zero, and there are no
/// more than there are variants.
fn check_mix(flag: &str, shares: &[f64], variants: &[Variant]) -> Result<(), String> {
    if shares.len() > variants.len() {
        let names: Vec<&str> = variants.iter().map(|v| v.label).collect();
        return Err(format!("{flag}: вариантов всего {} ({})", variants.len(), names.join(", ")));
    }
    if shares.iter().any(|s| !s.is_finite() || *s < 0.0)
        || (!shares.is_empty() && shares.iter().sum::<f64>() <= 0.0)
    {
        return Err(format!("{flag}: доли — числа не меньше нуля, хотя бы одна больше"));
    }
    Ok(())
}

/// Ticks between two measurements of the tick rate: `pace` in the JSON and the third number of
/// `--progress`, so a run that slows down (a population boom, a degraded search) shows it while it
/// runs and in the sweep's summary, not only as a mean or a cut at the deadline.
const PACE_EVERY: u64 = 500;

/// A run's tick rate: (tick, ms a tick over the `PACE_EVERY` ticks before it).
type Pace = Vec<(u64, f64)>;

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
    check_mix("--mix", &args.mix, &creature_strategy::VARIANTS).unwrap_or_else(|e| fail(e));
    check_mix("--diet-mix", &args.diet_mix, &life_core::genome::creature::DIET_VARIANTS)
        .unwrap_or_else(|e| fail(e));
    // JSON to stdout — and nothing else: text would break the parsing
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
        shape: args.shape,
        rules: rules.clone(),
        n_creatures: args.creatures,
        strategies: args.mix.clone(),
        diets: if args.diet_mix.is_empty() { WorldConfig::default().diets } else { args.diet_mix.clone() },
        meat_founder_size: args.meat_founders.unwrap_or(WorldConfig::default().meat_founder_size),
    };
    if !(base_cfg.meat_founder_size.is_finite() && base_cfg.meat_founder_size > 0.0) {
        fail("--meat-founders: нужно число больше нуля".into());
    }

    let reference = args.compare.as_ref().map(|path| {
        metrics::Reference::load(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())))
    });
    if let Some(r) = &reference {
        // only identical worlds can be compared: otherwise a «divergence» is a difference of
        // conditions, not of balance
        r.check_same_world(&base_cfg).unwrap_or_else(|e| fail(format!("эталон снят на другом мире: {e}")));
        if let Some(note) = r.genes_note() {
            eprintln!("заметка: {note}");
        }
        args.seeds = r.seeds.clone();
        args.ticks = Some(r.ticks);
        args.sample = Some(r.sample_every);
    }
    if reference.is_some() || saving {
        // a reference is taken without a work budget — otherwise a cut-off run would be compared with a whole
        // one
        args.max_work.get_or_insert(1e15);
    }

    let ticks = args.ticks.unwrap_or(600);
    let seeds = if args.seeds.is_empty() { vec![args.seed] } else { args.seeds.clone() };
    if args.progress.is_some() && seeds.len() > 1 {
        fail("--progress: один сид — иначе все сиды пишут в один файл".into());
    }
    let sample = args.sample.unwrap_or((ticks / 50).max(1));
    let limits = Limits {
        ticks,
        sample_every: sample,
        max_creatures: 50_000,
        max_total_work: args.max_work.unwrap_or(WORK_PER_TICK * ticks as f64),
        deadline: Duration::from_secs(args.seconds),
    };

    if !quiet {
        let space = base_cfg.space();
        println!(
            "Мир x{} ({}): {:.0}x{:.0}, {} тиков, сиды {:?}, потоков {}",
            args.scale,
            args.shape.label(),
            space.width,
            space.height,
            ticks,
            seeds,
            rayon::current_num_threads()
        );
    }
    let started = Instant::now();
    let map_every = if args.maps > 0 { (ticks / args.maps as u64).max(1) } else { u64::MAX };
    // per seed: its result, its maps and its tick rate
    let done: Vec<(u64, SimResult, Vec<story::Map>, Pace)> = seeds
        .par_iter()
        .map(|&seed| {
            let cfg = WorldConfig { seed, ..base_cfg.clone() };
            let mut maps = Vec::new();
            let mut written = Instant::now();
            // the tick rate, measured every PACE_EVERY ticks: (tick, ms a tick over the lap)
            let mut pace: Pace = Vec::new();
            let mut lap = (0, Instant::now());
            let res = simulate(&cfg, &limits, |w| {
                if w.tick.is_multiple_of(map_every) {
                    maps.push((w.tick, ascii_map(w, args.map_width)));
                }
                if w.tick >= lap.0 + PACE_EVERY {
                    let ms = lap.1.elapsed().as_secs_f64() * 1000.0 / (w.tick - lap.0) as f64;
                    pace.push((w.tick, ms));
                    lap = (w.tick, Instant::now());
                }
                if let Some(path) = &args.progress
                    && w.tick.is_multiple_of(50)
                    && written.elapsed() >= Duration::from_secs(1)
                {
                    // aside and renamed over: the sweep reads it several times a second and must
                    // never see half a line (a total of 2 would put the run at its end)
                    let tmp = path.with_extension("tick.tmp");
                    let ms = pace.last().map_or(0.0, |p| p.1);
                    if std::fs::write(&tmp, format!("{} {ticks} {ms:.3}", w.tick)).is_ok() {
                        let _ = std::fs::rename(&tmp, path);
                    }
                    written = Instant::now();
                }
            });
            // the last map is always the final state, even if the run was cut off
            if args.maps > 0 && maps.last().map(|m| m.0) != Some(res.world.tick) {
                maps.push((res.world.tick, ascii_map(&res.world, args.map_width)));
            }
            (seed, res, maps, pace)
        })
        .collect();
    let (mut results, mut maps, mut paces) = (Vec::new(), Vec::new(), Vec::new());
    for (seed, res, m, pace) in done {
        results.push((seed, res));
        maps.push(m);
        paces.push(pace);
    }
    let events: Vec<Vec<Event>> = results.iter().map(|(_, r)| observe::events(&r.snapshots)).collect();

    if let Some(path) = &args.json {
        let runs: Vec<json::Run> = results
            .iter()
            .zip(&events)
            .zip(&maps)
            .zip(&paces)
            .map(|((((seed, res), events), maps), pace)| json::Run { seed: *seed, res, events, maps, pace })
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
    // The comparison is a check, not a reference: a divergence from the reference must fail CI.
    if !agrees {
        eprintln!("ошибка: баланс разошёлся с эталоном");
        std::process::exit(1);
    }
}

fn print_summary(results: &[(u64, SimResult)]) {
    println!(
        "\n{:>6} {:<18} {:>7} {:>6} {:>6} {:>7} {:>8} {:>8} {:>8}",
        "сид", "итог", "тиков", "сущ", "растен", "в бою", "разм.макс", "разм.фин", "мс/тик"
    );
    for (seed, r) in results {
        let last = r.last();
        // the share of combat deaths among all deaths
        let c = r.world.counters;
        let deaths = c.starved + c.old_age + c.combat;
        let combat_share = c.combat as f64 / deaths.max(1) as f64;
        let sizes: Vec<f64> =
            r.history.iter().filter_map(|s| s.avg_genom.map(|g| g[Gene::Size as usize])).collect();
        let smax = sizes.iter().copied().fold(f64::NAN, f64::max);
        let sfin = sizes.last().copied().unwrap_or(f64::NAN);
        println!(
            "{seed:>6} {:<18} {:>7} {:>6} {:>6} {:>6.0}% {:>8.1} {:>8.1} {:>8.3}",
            r.stop.to_string(),
            r.ticks_done,
            last.creatures,
            last.plants,
            combat_share * 100.0,
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
