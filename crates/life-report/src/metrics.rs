//! The balance's reference fingerprint: checking (`--compare`) and writing a new one (`--save-reference`).
//!
//! Statistics are compared, not bits: for each metric the spread over the reference's seeds and
//! the current run's mean are taken. A metric «converges» if the mean falls within the range of
//! the values of the reference's separate seeds.
//!
//! After a deliberate change of balance the reference is retaken in the same format; it records
//! the world's conditions, and checking under other conditions refuses to work — otherwise a
//! «divergence» would measure the difference of conditions. An older model's reference is refused outright.

use std::path::Path;

use life_core::genome::creature::{GENES, Gene};
use life_core::rules::RULE_KEYS;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space, Stats, WorldConfig};
use life_sim::{SimResult, StopReason};
use serde_json::{Map, Value, json};

/// The step of the reference's series: a multiple of the division period, so that the division's sawtooth
/// does not add noise.
pub const REFERENCE_SAMPLE: u64 = 60;

/// The series of one run: the tick and the counts plus the mean size.
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
    pub seeds: Vec<u64>,
    pub ticks: u64,
    pub sample_every: u64,
    runs: Vec<Run>,
    /// The world's conditions.
    scale: f64,
    /// The world's dimensions. They are compared, not the name of the shape: at x1 a strip and 3:2
    /// are one and the same 6000x4000 world.
    space: Space,
    rules: Rules,
    start: usize,
    strategies: Vec<f64>,
    /// The creatures' genes the reference was taken on.
    pub genes: Vec<String>,
    /// Founders' diets (`WorldConfig::diets`).
    pub diets: Vec<f64>,
    /// How much bigger the meat-eating founders start (`WorldConfig::meat_founder_size`).
    meat_founder_size: f64,
}

impl Reference {
    pub fn load(path: &Path) -> Result<Reference, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if data["model"].as_str() != Some("life-behavior/15") {
            return Err("Эталон другой модели поведения. Пересоздайте его через --save-reference после проверки баланса.".into());
        }
        let field = |v: &Value, k: &str| v.get(k).cloned().ok_or(format!("нет поля {k}"));
        let ticks = field(&data, "ticks")?.as_u64().ok_or("ticks — не число")?;
        let sample_every = field(&data, "sample_every")?.as_u64().ok_or("sample_every — не число")?;
        // The mean genome at the series' points — a list in the order of the reference's genes. We look
        // for the size by name: the reference's gene order may differ from our table.
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
                    creatures: s["creatures"].as_f64().unwrap_or(0.0),
                    size: s["genom"].get(size_at).and_then(Value::as_f64),
                })
                .collect();
            // the stop reason is saved as its text (`StopReason::Display`)
            runs.push(Run { extinct: run["stop"] == StopReason::Extinct.to_string().as_str(), series });
        }

        let world = WorldConfig::default();
        let mut rules = Rules::default();
        if let Some(saved) = data.get("rules").and_then(Value::as_object) {
            let changes = saved
                .iter()
                .map(|(key, value)| Ok((key, value.as_f64().ok_or(format!("правило {key} — не число"))?)))
                .collect::<Result<Vec<_>, String>>()?;
            // checked as a whole: the order of the saved keys does not matter
            rules = rules.with_all(changes)?;
        }
        let start = &data["start"];
        let num = |v: &Value, default: f64| v.as_f64().unwrap_or(default);
        let scale = num(&data["scale"], 1.0);
        let shape = Shape::parse(data["shape"].as_str().ok_or("нет поля shape")?)?;
        if !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
            return Err(format!("масштаб {scale} вне пределов {MIN_SCALE}‒{MAX_SCALE}"));
        }
        Ok(Reference {
            seeds,
            ticks,
            sample_every,
            runs,
            scale,
            space: Space::new(scale, shape),
            rules,
            start: num(&start["creatures"], world.creatures_at_start() as f64) as usize,
            strategies: mix(&start["strategies"])?,
            diets: match &start["diets"] {
                Value::Null => world.diets.clone(),
                saved => mix(saved)?,
            },
            // references from before it was kept took the default
            meat_founder_size: num(&start["meat_founder_size"], world.meat_founder_size),
            genes: data
                .get("genes")
                .and_then(Value::as_array)
                .map(|keys| keys.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
        })
    }

    /// Whether the world's conditions match those the reference was taken on.
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
        if cfg.diets != self.diets {
            diff.push(format!("смесь диет {:?} (в эталоне {:?})", cfg.diets, self.diets));
        }
        if cfg.meat_founder_size != self.meat_founder_size {
            diff.push(format!(
                "размер мясных основателей x{} (в эталоне x{})",
                cfg.meat_founder_size, self.meat_founder_size
            ));
        }
        if diff.is_empty() { Ok(()) } else { Err(diff.join(", ")) }
    }

    /// A warning if the reference was taken on another set of genes: comparing is possible (the
    /// metrics are counts and size), but then a divergence is expected and the reference should be
    /// retaken. A choice gene with one variant is inert (draws no random numbers and tells nothing
    /// apart) and takes no part in the comparison.
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

/// A start mix (strategies, diets) of the reference: a list of shares; missing — empty.
fn mix(v: &Value) -> Result<Vec<f64>, String> {
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Array(a) => {
            a.iter().map(|x| x.as_f64().ok_or("смесь стратегий — не числа".to_string())).collect()
        }
        _ => Err("смесь стратегий — не список".into()),
    }
}

/// A seed a guard cut short (the deadline, the population ceiling, the work budget) went fewer ticks
/// than the reference's whole runs: its shortened series is no evidence either way, so the
/// comparison is refused. An extinct world is an outcome and is compared (the reference counts its
/// extinctions too).
pub fn cut_short(results: &[(u64, SimResult)], ticks: u64) -> Option<String> {
    results
        .iter()
        .find(|(_, r)| r.stop != StopReason::Extinct && (!r.ok() || r.ticks_done != ticks))
        .map(|(seed, r)| format!("сид {seed} остановился ({}) на тике {} из {ticks}", r.stop, r.ticks_done))
}

/// Write a reference from the current runs — in the format that `Reference::load` reads.
pub fn save_reference(
    path: &Path,
    cfg: &WorldConfig,
    ticks: u64,
    sample_every: u64,
    results: &[(u64, SimResult)],
) -> Result<(), String> {
    if results.is_empty() {
        return Err("эталон нельзя снять без прогонов".into());
    }
    // a seed a guard cut short is no evidence, as for `--compare`; a world that died out is an
    // outcome, and the reference keeps it (the validation allows one of eight)
    if let Some(why) = cut_short(results, ticks) {
        return Err(format!("эталон не записан: {why}"));
    }
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
                        "genom": s.avg_genom.as_ref().map(|g| g.as_slice()),
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
        "model": "life-behavior/15",
        "sample_every": sample_every,
        "ticks": ticks,
        "genes": GENES.iter().map(|g| g.key).collect::<Vec<_>>(),
        "scale": cfg.scale,
        "shape": cfg.shape.key(),
        "rules": rules,
        "start": {
            "creatures": cfg.creatures_at_start(),
            "strategies": cfg.strategies,
            "diets": cfg.diets,
            "meat_founder_size": cfg.meat_founder_size,
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

/// Prints the comparison; true — all metrics converged in both windows.
pub fn print_comparison(reference: &Reference, results: &[(u64, SimResult)]) -> bool {
    let mut all_agree = true;
    let ours: Vec<Run> = results
        .iter()
        .map(|(_, r)| Run { extinct: r.stop == StopReason::Extinct, series: from_stats(&r.history) })
        .collect();

    for window in [Some(3000), None] {
        let cut = |s: &[Point]| -> Vec<Point> {
            s.iter().filter(|p| window.is_none_or(|w| p.tick <= w)).cloned().collect()
        };
        match window {
            Some(w) => println!("\nСверка с эталоном, первые {w} тиков"),
            None => println!("\nСверка с эталоном, весь прогон"),
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
    use life_core::World;
    use life_sim::{Limits, run};

    /// A reference of another model (or of none) is refused before its series are read.
    #[test]
    fn a_reference_of_another_model_is_refused() {
        let path = std::env::temp_dir().join(format!("life-old-reference-{}.json", std::process::id()));
        for value in [
            "{}",
            r#"{"model":"life-behavior/1"}"#,
            r#"{"model":"life-behavior/9"}"#,
            r#"{"model":"life-behavior/10"}"#,
            r#"{"model":"life-behavior/12"}"#,
            r#"{"model":"life-behavior/13"}"#,
            r#"{"model":"life-behavior/14"}"#,
        ] {
            std::fs::write(&path, value).unwrap();
            assert!(Reference::load(&path).err().unwrap().contains("другой модели поведения"));
        }
        std::fs::remove_file(path).unwrap();
    }

    /// The rules and the world's shape of a reference are read; a rule it names that no longer
    /// exists (combat used to be the rule `cannibalism`) refuses the whole reference.
    #[test]
    fn the_rules_of_a_reference_are_read_and_a_removed_rule_refuses_it() {
        let path = std::env::temp_dir().join(format!("life-rules-reference-{}.json", std::process::id()));
        let reference = |rules: &str| {
            format!(
                r#"{{"model":"life-behavior/15","ticks":1,"sample_every":1,"shape":"3:2","runs":[],"rules":{{{rules}}}}}"#
            )
        };
        std::fs::write(&path, reference(r#""cost_scale":3"#)).unwrap();
        let loaded = Reference::load(&path).expect("a reference with a rule is read");
        assert_eq!(loaded.rules, Rules::default().with("cost_scale", 3.0).unwrap());
        assert_eq!(loaded.space, Space::default());
        std::fs::write(&path, reference(r#""cannibalism":1"#)).unwrap();
        assert!(Reference::load(&path).err().unwrap().contains("нет такого правила"));
        std::fs::remove_file(path).unwrap();
    }

    /// The meat founders' size is a world condition: a reference taken at another one is refused;
    /// one from before it was kept took the default.
    #[test]
    fn the_meat_founders_size_is_part_of_the_world() {
        let path = std::env::temp_dir().join(format!("life-founders-reference-{}.json", std::process::id()));
        let reference = |start: &str| {
            format!(
                r#"{{"model":"life-behavior/15","ticks":1,"sample_every":1,"shape":"3:2","runs":[],"start":{{{start}}}}}"#
            )
        };
        let cfg = WorldConfig::default();
        std::fs::write(&path, reference("")).unwrap();
        assert_eq!(Reference::load(&path).unwrap().check_same_world(&cfg), Ok(()), "no field: the default");
        std::fs::write(&path, reference(r#""meat_founder_size":1"#)).unwrap();
        let other = Reference::load(&path).unwrap();
        assert!(other.check_same_world(&cfg).unwrap_err().contains("мясных основателей"));
        assert_eq!(other.check_same_world(&WorldConfig { meat_founder_size: 1.0, ..cfg }), Ok(()));
        std::fs::remove_file(path).unwrap();
    }

    /// A run cut by a guard is no evidence against whole reference runs; a whole run and an
    /// extinct world are compared.
    #[test]
    fn a_cut_run_is_not_compared_but_an_extinct_one_is() {
        let run_with =
            |cfg: WorldConfig, limits: Limits| (cfg.seed, run(World::new(&cfg), &limits, &mut |_| {}));
        let limits =
            Limits { ticks: 20, deadline: std::time::Duration::from_secs(600), ..Default::default() };
        let whole = run_with(WorldConfig::default(), limits.clone());
        let extinct = run_with(WorldConfig { n_creatures: Some(0), ..Default::default() }, limits.clone());
        assert_eq!(extinct.1.stop, StopReason::Extinct);
        assert_eq!(cut_short(&[whole, extinct], 20), None);
        let late = run_with(
            WorldConfig { seed: 3, ..Default::default() },
            Limits { deadline: std::time::Duration::ZERO, ..limits },
        );
        assert_eq!(late.1.stop, StopReason::Deadline);
        let why = cut_short(&[late], 20).expect("a cut run is refused");
        assert!(why.contains("сид 3") && why.contains("из 20"), "{why}");
    }

    #[test]
    fn оборванный_прогон_не_становится_эталоном() {
        let cfg = WorldConfig::default();
        let limits = Limits { ticks: 20, deadline: std::time::Duration::ZERO, ..Default::default() };
        let result = run(World::new(&cfg), &limits, &mut |_| {});
        assert_eq!(result.stop, StopReason::Deadline);
        let path =
            std::env::temp_dir().join(format!("life-incomplete-reference-{}.json", std::process::id()));
        let error = save_reference(&path, &cfg, 20, REFERENCE_SAMPLE, &[(cfg.seed, result)]).unwrap_err();
        assert!(error.contains("эталон не записан") && error.contains("сид"), "{error}");
        assert!(!path.exists());
    }

    /// A world that died out is an outcome, not a cut run: the reference keeps it.
    #[test]
    fn an_extinct_world_goes_into_the_reference() {
        let cfg = WorldConfig { n_creatures: Some(0), ..Default::default() };
        let result = run(World::new(&cfg), &Limits { ticks: 20, ..Default::default() }, &mut |_| {});
        assert_eq!(result.stop, StopReason::Extinct);
        let path = std::env::temp_dir().join(format!("life-extinct-reference-{}.json", std::process::id()));
        save_reference(&path, &cfg, 20, REFERENCE_SAMPLE, &[(cfg.seed, result)]).unwrap();
        let loaded = Reference::load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(loaded.is_ok(), "{:?}", loaded.err());
    }
}
