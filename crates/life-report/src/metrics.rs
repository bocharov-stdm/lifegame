//! Эталонный отпечаток баланса: сверка (`--compare`) и запись нового (`--save-reference`).
//!
//! Первый эталон снят с Python-версии (`python/fingerprint.py`, тег python-final).
//! Бит в бит ядра не совпадают (другой генератор случайных чисел), поэтому
//! сравнивается статистика: для каждой метрики берётся разброс по сидам у
//! эталона и среднее у текущего прогона. Метрика «сходится», если среднее
//! попадает в диапазон значений отдельных сидов эталона.
//!
//! После намеренной смены баланса эталон переснимается уже с Rust тем же
//! форматом; в нём записаны условия мира, и сверка на других условиях
//! отказывается работать — иначе «расхождение» мерило бы разницу условий.

use std::path::Path;

use life_core::genome::GENE_KEYS;
use life_core::rules::RULE_KEYS;
use life_core::{Rules, Stats, WorldConfig};
use life_sim::{SimResult, StopReason};
use serde_json::{Map, Value, json};

/// Шаг рядов эталона: кратно периоду деления, чтобы пила деления не шумела.
pub const REFERENCE_SAMPLE: u64 = 60;

/// Ряд одного прогона: тик и численности плюс средний размер.
#[derive(Clone, Debug)]
struct Point {
    tick: u64,
    plants: f64,
    vegetarians: f64,
    predators: f64,
    size: Option<f64>,
}

struct Run {
    extinct: bool,
    series: Vec<Point>,
}

pub struct Reference {
    /// Откуда эталон: «Python» или «Rust».
    pub source: String,
    pub seeds: Vec<u64>,
    pub ticks: u64,
    pub sample_every: u64,
    runs: Vec<Run>,
    /// Условия мира. У Python-эталона их нет — он снят на умолчаниях.
    scale: f64,
    rules: Rules,
    start: (usize, usize),
    predator: (f64, f64),
}

impl Reference {
    pub fn load(path: &Path) -> Result<Reference, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let field = |v: &Value, k: &str| v.get(k).cloned().ok_or(format!("нет поля {k}"));
        let ticks = field(&data, "ticks")?.as_u64().ok_or("ticks — не число")?;
        let sample_every = field(&data, "sample_every")?.as_u64().ok_or("sample_every — не число")?;
        let mut seeds = Vec::new();
        let mut runs = Vec::new();
        for run in field(&data, "runs")?.as_array().ok_or("runs — не список")? {
            seeds.push(field(run, "seed")?.as_u64().ok_or("seed — не число")?);
            let series = field(run, "series")?
                .as_array()
                .ok_or("series — не список")?
                .iter()
                .map(|s| Point {
                    tick: s["tick"].as_u64().unwrap_or(0),
                    plants: s["plants"].as_f64().unwrap_or(0.0),
                    vegetarians: s["vegetarians"].as_f64().unwrap_or(0.0),
                    predators: s["predators"].as_f64().unwrap_or(0.0),
                    size: s["genom"].get(0).and_then(Value::as_f64),
                })
                .collect();
            // Python писал причину остановки русским текстом — формат сохранён
            runs.push(Run { extinct: run["stop"] == StopReason::Extinct.to_string().as_str(), series });
        }

        let world = WorldConfig::default();
        let mut rules = Rules::default();
        if let Some(saved) = data.get("rules").and_then(Value::as_object) {
            for (key, value) in saved {
                let value = value.as_f64().ok_or(format!("правило {key} — не число"))?;
                rules = rules.with(key, value)?;
            }
        }
        let start = &data["start"];
        let num = |v: &Value, default: f64| v.as_f64().unwrap_or(default);
        Ok(Reference {
            source: match data["source"].as_str() {
                Some("rust") => "Rust",
                _ => "Python", // первый эталон поля source не имел
            }
            .to_string(),
            seeds,
            ticks,
            sample_every,
            runs,
            scale: num(&data["scale"], 1.0),
            rules,
            start: (
                num(&start["vegetarians"], world.vegetarians_at_start() as f64) as usize,
                num(&start["predators"], world.predators_at_start() as f64) as usize,
            ),
            predator: (
                num(&start["predator_speed"], world.predator_speed),
                num(&start["predator_vision"], world.predator_vision),
            ),
        })
    }

    /// Совпадают ли условия мира с теми, на которых снят эталон.
    pub fn check_same_world(&self, cfg: &WorldConfig) -> Result<(), String> {
        let mut diff = Vec::new();
        if cfg.scale != self.scale {
            diff.push(format!("масштаб {} (в эталоне {})", cfg.scale, self.scale));
        }
        for key in RULE_KEYS {
            let (ours, theirs) = (cfg.rules.get(key), self.rules.get(key));
            if ours != theirs {
                diff.push(format!(
                    "{key}={} (в эталоне {})",
                    ours.unwrap_or(f64::NAN),
                    theirs.unwrap_or(f64::NAN)
                ));
            }
        }
        let start = (cfg.vegetarians_at_start(), cfg.predators_at_start());
        if start != self.start {
            diff.push(format!("старт {}/{} (в эталоне {}/{})", start.0, start.1, self.start.0, self.start.1));
        }
        if (cfg.predator_speed, cfg.predator_vision) != self.predator {
            diff.push(format!(
                "хищники {}/{} (в эталоне {}/{})",
                cfg.predator_speed, cfg.predator_vision, self.predator.0, self.predator.1
            ));
        }
        if diff.is_empty() { Ok(()) } else { Err(diff.join(", ")) }
    }
}

/// Записать эталон с текущих прогонов — в формате, который читает `Reference::load`.
pub fn save_reference(
    path: &Path,
    cfg: &WorldConfig,
    ticks: u64,
    sample_every: u64,
    results: &[(u64, SimResult)],
) -> Result<(), String> {
    let rules: Map<_, _> = RULE_KEYS.iter().map(|k| (k.to_string(), json!(cfg.rules.get(k)))).collect();
    let runs: Vec<Value> = results
        .iter()
        .map(|(seed, r)| {
            let series: Vec<Value> = r
                .history
                .iter()
                .map(|s| {
                    json!({
                        "tick": s.tick,
                        "plants": s.plants,
                        "vegetarians": s.vegetarians,
                        "predators": s.predators,
                        "genom": s.avg_genom,
                    })
                })
                .collect();
            json!({
                "seed": seed,
                "stop": r.stop.to_string(),
                "ticks_done": r.ticks_done,
                "migrants": r.world.migrants,
                "ms_per_tick": r.ms_per_tick(),
                "series": series,
            })
        })
        .collect();
    let data = json!({
        "source": "rust",
        "sample_every": sample_every,
        "ticks": ticks,
        "genes": GENE_KEYS,
        "scale": cfg.scale,
        "rules": rules,
        "start": {
            "vegetarians": cfg.vegetarians_at_start(),
            "predators": cfg.predators_at_start(),
            "predator_speed": cfg.predator_speed,
            "predator_vision": cfg.predator_vision,
        },
        "runs": runs,
    });
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn from_stats(history: &[Stats]) -> Vec<Point> {
    history
        .iter()
        .map(|s| Point {
            tick: s.tick,
            plants: s.plants as f64,
            vegetarians: s.vegetarians as f64,
            predators: s.predators as f64,
            size: s.avg_genom.map(|g| g[0]),
        })
        .collect()
}

type Metric = (&'static str, fn(&[Point]) -> f64);

fn mean(v: impl Iterator<Item = f64>) -> f64 {
    let (s, n) = v.fold((0.0, 0), |(s, n), x| (s + x, n + 1));
    if n == 0 { f64::NAN } else { s / n as f64 }
}

const METRICS: [Metric; 6] = [
    ("травоядные, среднее", |s| mean(s.iter().map(|p| p.vegetarians))),
    ("хищники, среднее", |s| mean(s.iter().map(|p| p.predators))),
    ("растения, среднее", |s| mean(s.iter().map(|p| p.plants))),
    ("доля времени с хищниками", |s| {
        mean(s.iter().map(|p| (p.predators > 0.0) as u8 as f64))
    }),
    ("средний размер, максимум", |s| {
        s.iter().filter_map(|p| p.size).fold(f64::NAN, f64::max)
    }),
    ("средний размер, финал", |s| s.iter().rev().find_map(|p| p.size).unwrap_or(f64::NAN)),
];

/// Печатает сверку; true — все метрики сошлись в обоих окнах.
pub fn print_comparison(reference: &Reference, results: &[(u64, SimResult)]) -> bool {
    let mut all_agree = true;
    let ours: Vec<Run> = results
        .iter()
        .map(|(_, r)| Run { extinct: r.stop == StopReason::Extinct, series: from_stats(&r.history) })
        .collect();
    let source = &reference.source;

    for window in [Some(3000), None] {
        let cut = |s: &[Point]| -> Vec<Point> {
            s.iter().filter(|p| window.is_none_or(|w| p.tick <= w)).cloned().collect()
        };
        match window {
            Some(w) => println!("\nСверка с эталоном ({source}), первые {w} тиков"),
            None => println!("\nСверка с эталоном ({source}), весь прогон"),
        }
        println!(
            "{:<28} {:>24} {:>10} {:>9}",
            "метрика", "эталон: мин … среднее … макс", "сейчас", "сходится"
        );
        let mut agree = 0;
        for (name, f) in METRICS {
            let theirs: Vec<f64> =
                reference.runs.iter().map(|r| f(&cut(&r.series))).filter(|x| x.is_finite()).collect();
            let rs: Vec<f64> = ours.iter().map(|r| f(&cut(&r.series))).filter(|x| x.is_finite()).collect();
            let (lo, hi) =
                theirs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
            let ours_mean = mean(rs.iter().copied());
            let ok = lo <= ours_mean && ours_mean <= hi;
            agree += ok as usize;
            println!(
                "{name:<28} {:>7.2} … {:>6.2} … {:>7.2} {:>10.2} {:>9}",
                lo,
                mean(theirs.iter().copied()),
                hi,
                ours_mean,
                if ok { "да" } else { "НЕТ" }
            );
        }
        println!("сходится {agree} из {}", METRICS.len());
        all_agree &= agree == METRICS.len();
    }
    let ref_ext = reference.runs.iter().filter(|r| r.extinct).count();
    let our_ext = ours.iter().filter(|r| r.extinct).count();
    println!(
        "\nполное вымирание: эталон {ref_ext} из {}, сейчас {our_ext} из {}",
        reference.runs.len(),
        ours.len()
    );
    all_agree
}
