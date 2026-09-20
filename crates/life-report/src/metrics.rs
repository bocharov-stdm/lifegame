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

use life_core::genome::creature::{GENES, Gene};
use life_core::rules::RULE_KEYS;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space, Stats, WorldConfig};
use life_sim::{SimResult, StopReason};
use serde_json::{Map, Value, json};

/// Шаг рядов эталона: кратно периоду деления, чтобы пила деления не шумела.
pub const REFERENCE_SAMPLE: u64 = 60;

/// Ряд одного прогона: тик и численности плюс средний размер.
#[derive(Clone, Debug)]
struct Point {
    tick: u64,
    plants: f64,
    creatures: f64,
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
    /// Размеры мира. Сравниваются они, а не имя формы: при x1 полоса и 3:2 —
    /// один и тот же мир 6000x4000.
    space: Space,
    rules: Rules,
    start: usize,
    strategies: Vec<f64>,
    /// Гены существ, на которых снят эталон (у старого — семь).
    pub genes: Vec<String>,
}

impl Reference {
    pub fn load(path: &Path) -> Result<Reference, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if data["model"].as_str() != Some("life-behavior/1") {
            return Err("Эталон другой модели поведения. Пересоздайте его через --save-reference после проверки баланса.".into());
        }
        let field = |v: &Value, k: &str| v.get(k).cloned().ok_or(format!("нет поля {k}"));
        let ticks = field(&data, "ticks")?.as_u64().ok_or("ticks — не число")?;
        let sample_every = field(&data, "sample_every")?.as_u64().ok_or("sample_every — не число")?;
        // Средний геном в точках ряда — списком по порядку генов эталона. Размер
        // ищем по имени: порядок генов эталона может отличаться от нашей таблицы.
        let size_at = match data.get("genes").and_then(Value::as_array) {
            Some(keys) => keys
                .iter()
                .position(|k| k.as_str() == Some(GENES[Gene::Size as usize].key))
                .ok_or("в эталоне нет гена size")?,
            None => 0,
        };
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
                    // до переименования в «существ» — «vegetarians»
                    creatures: s["creatures"].as_f64().or(s["vegetarians"].as_f64()).unwrap_or(0.0),
                    size: s["genom"].get(size_at).and_then(Value::as_f64),
                })
                .collect();
            // Python писал причину остановки русским текстом — формат сохранён
            runs.push(Run { extinct: run["stop"] == StopReason::Extinct.to_string().as_str(), series });
        }

        let world = WorldConfig::default();
        let mut rules = Rules::default();
        if let Some(saved) = data.get("rules").and_then(Value::as_object) {
            for (key, value) in saved {
                // правила хищников остались в эталонах до тега predators-final:
                // вида больше нет, и правила о нём ничего не значат
                if key.starts_with("predator_") {
                    continue;
                }
                let value = value.as_f64().ok_or(format!("правило {key} — не число"))?;
                rules = rules.with(key, value)?;
            }
        }
        let start = &data["start"];
        let num = |v: &Value, default: f64| v.as_f64().unwrap_or(default);
        // старые эталоны сняты с хищниками: с ними сравнивать нечего
        if num(&start["predators"], 0.0) > 0.0 {
            return Err(
                "эталон снят с хищниками, а их больше нет — переснимите его (--save-reference)".into()
            );
        }
        let scale = num(&data["scale"], 1.0);
        // у эталонов до форм поля нет: они сняты на полосе
        let shape = match data["shape"].as_str() {
            Some(key) => Shape::parse(key)?,
            None => Shape::Strip,
        };
        if !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
            return Err(format!("масштаб {scale} вне пределов {MIN_SCALE}‒{MAX_SCALE}"));
        }
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
            scale,
            space: Space::new(scale, shape),
            rules,
            start: num(&start["creatures"], num(&start["vegetarians"], world.creatures_at_start() as f64))
                as usize,
            strategies: match &start["strategies"] {
                Value::Null => mix(&start["vegetarian_strategies"])?,
                new => mix(new)?,
            },
            genes: data
                .get("genes")
                .and_then(Value::as_array)
                .map(|keys| keys.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
        })
    }

    /// Совпадают ли условия мира с теми, на которых снят эталон.
    pub fn check_same_world(&self, cfg: &WorldConfig) -> Result<(), String> {
        let mut diff = Vec::new();
        let space = cfg.space();
        if space != self.space {
            diff.push(format!(
                "мир x{} {:.0}x{:.0} (в эталоне x{} {:.0}x{:.0})",
                cfg.scale, space.width, space.height, self.scale, self.space.width, self.space.height
            ));
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
        let start = cfg.creatures_at_start();
        if start != self.start {
            diff.push(format!("старт {start} (в эталоне {})", self.start));
        }
        if cfg.strategies != self.strategies {
            diff.push(format!("смесь стратегий {:?} (в эталоне {:?})", cfg.strategies, self.strategies));
        }
        if diff.is_empty() { Ok(()) } else { Err(diff.join(", ")) }
    }

    /// Предупреждение, если эталон снят на другом наборе генов: сравнивать
    /// можно (метрики — численности и размер), но расхождение тогда ожидаемо, и
    /// эталон пора переснять. Ген-выбор с одним вариантом инертен (не тянет
    /// случайных чисел и ничего не различает) и в сравнении не участвует.
    pub fn genes_note(&self) -> Option<String> {
        let inert =
            |key: &str| GENES.iter().any(|g| g.key == key && g.variants().is_some_and(|v| v.len() < 2));
        let ours: Vec<&str> = GENES.iter().map(|g| g.key).filter(|k| !inert(k)).collect();
        let theirs: Vec<&str> = self.genes.iter().map(String::as_str).filter(|k| !inert(k)).collect();
        (!theirs.is_empty() && theirs != ours).then(|| {
            format!(
                "эталон снят на генах существ {theirs:?}, сейчас {ours:?} — после намеренной смены \
                 поведения эталон переснимают (--save-reference)"
            )
        })
    }
}

/// Стартовая смесь стратегий из эталона: список долей, у старого эталона — пустой.
fn mix(v: &Value) -> Result<Vec<f64>, String> {
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Array(a) => {
            a.iter().map(|x| x.as_f64().ok_or("смесь стратегий — не числа".to_string())).collect()
        }
        _ => Err("смесь стратегий — не список".into()),
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
                        "creatures": s.creatures,
                        "genom": s.avg_genom,
                    })
                })
                .collect();
            json!({
                "seed": seed,
                "stop": r.stop.to_string(),
                "ticks_done": r.ticks_done,
                "ms_per_tick": r.ms_per_tick(),
                "series": series,
            })
        })
        .collect();
    let data = json!({
        "source": "rust",
        "model": "life-behavior/1",
        "sample_every": sample_every,
        "ticks": ticks,
        "genes": GENES.iter().map(|g| g.key).collect::<Vec<_>>(),
        "scale": cfg.scale,
        "shape": cfg.shape.key(),
        "rules": rules,
        "start": {
            "creatures": cfg.creatures_at_start(),
            "strategies": cfg.strategies,
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
            creatures: s.creatures as f64,
            size: s.avg_genom.map(|g| g[Gene::Size as usize]),
        })
        .collect()
}

type Metric = (&'static str, fn(&[Point]) -> f64);

fn mean(v: impl Iterator<Item = f64>) -> f64 {
    let (s, n) = v.fold((0.0, 0), |(s, n), x| (s + x, n + 1));
    if n == 0 { f64::NAN } else { s / n as f64 }
}

const METRICS: [Metric; 4] = [
    ("существа, среднее", |s| mean(s.iter().map(|p| p.creatures))),
    ("растения, среднее", |s| mean(s.iter().map(|p| p.plants))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn старый_эталон_отклоняется_до_чтения_рядов() {
        let path = std::env::temp_dir().join(format!("life-old-reference-{}.json", std::process::id()));
        std::fs::write(&path, "{}").unwrap();
        let result = Reference::load(&path);
        std::fs::remove_file(path).unwrap();
        assert!(result.err().unwrap().contains("другой модели поведения"));
    }
}
