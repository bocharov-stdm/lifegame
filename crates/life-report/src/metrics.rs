//! Сверка с эталонным отпечатком Python-версии (`python/fingerprint.py`).
//!
//! Бит в бит ядра не совпадут (другой генератор случайных чисел), поэтому
//! сравнивается статистика: для каждой метрики берётся разброс по сидам у
//! Python и среднее у Rust. Метрика «сходится», если среднее Rust попадает в
//! диапазон значений отдельных сидов Python.

use std::path::Path;

use life_core::Stats;
use life_sim::{SimResult, StopReason};
use serde_json::Value;

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
    pub seeds: Vec<u64>,
    pub ticks: u64,
    pub sample_every: u64,
    runs: Vec<Run>,
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
            runs.push(Run { extinct: run["stop"] == "вымерли", series });
        }
        Ok(Reference { seeds, ticks, sample_every, runs })
    }
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

pub fn print_comparison(reference: &Reference, results: &[(u64, SimResult)]) {
    let ours: Vec<Run> = results
        .iter()
        .map(|(_, r)| Run { extinct: r.stop == StopReason::Extinct, series: from_stats(&r.history) })
        .collect();

    for window in [Some(3000), None] {
        let cut = |s: &[Point]| -> Vec<Point> {
            s.iter().filter(|p| window.is_none_or(|w| p.tick <= w)).cloned().collect()
        };
        match window {
            Some(w) => println!("\nСверка с Python, первые {w} тиков"),
            None => println!("\nСверка с Python, весь прогон"),
        }
        println!("{:<28} {:>24} {:>10} {:>9}", "метрика", "Python: мин … среднее … макс", "Rust", "сходится");
        let mut agree = 0;
        for (name, f) in METRICS {
            let py: Vec<f64> =
                reference.runs.iter().map(|r| f(&cut(&r.series))).filter(|x| x.is_finite()).collect();
            let rs: Vec<f64> = ours.iter().map(|r| f(&cut(&r.series))).filter(|x| x.is_finite()).collect();
            let (lo, hi) =
                py.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
            let ours_mean = mean(rs.iter().copied());
            let ok = lo <= ours_mean && ours_mean <= hi;
            agree += ok as usize;
            println!(
                "{name:<28} {:>7.2} … {:>6.2} … {:>7.2} {:>10.2} {:>9}",
                lo,
                mean(py.iter().copied()),
                hi,
                ours_mean,
                if ok { "да" } else { "НЕТ" }
            );
        }
        println!("сходится {agree} из {}", METRICS.len());
    }
    let py_ext = reference.runs.iter().filter(|r| r.extinct).count();
    let rs_ext = ours.iter().filter(|r| r.extinct).count();
    println!(
        "\nполное вымирание: Python {py_ext} из {}, Rust {rs_ext} из {}",
        reference.runs.len(),
        ours.len()
    );
}
