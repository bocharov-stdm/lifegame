//! Полный отчёт в JSON — для программ и ИИ: всё, что печатает рассказ, плюс
//! каждый срез целиком. Ключи — машинные (английские, как в эталоне Python),
//! тексты событий — по-русски.

use life_core::genome::{GENE_KEYS, GENE_LABELS};
use life_core::rules::RULE_KEYS;
use life_core::{Counters, Rules, WorldConfig};
use life_sim::SimResult;
use life_sim::observe::{Event, MAP_LEGEND, Snapshot, Spread};
use serde_json::{Map, Value, json};

use crate::story;

/// Три знака после запятой: больше точности отчёту не нужно, а лишние цифры
/// раздувают JSON и мешают его читать.
fn r(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn spread(s: &Spread) -> Value {
    json!({ "p10": r(s.p10), "p50": r(s.p50), "p90": r(s.p90), "mean": r(s.mean) })
}

fn counters(c: &Counters) -> Value {
    json!({
        "plants_grown": c.plants_grown,
        "plants_eaten": c.plants_eaten,
        "vegetarians_born": c.vegetarians_born,
        "vegetarians_eaten": c.vegetarians_eaten,
        "vegetarians_starved": c.vegetarians_starved,
        "predators_born": c.predators_born,
        "predators_starved": c.predators_starved,
    })
}

fn snapshot(s: &Snapshot) -> Value {
    let genes = s
        .genes
        .map(|g| GENE_KEYS.iter().zip(&g).map(|(k, v)| (k.to_string(), spread(v))).collect::<Map<_, _>>());
    json!({
        "tick": s.tick,
        "plants": s.plants,
        "plant_cap": s.plant_cap,
        "vegetarians": s.vegetarians,
        "predators": s.predators,
        "migrants": s.migrants,
        "counters": counters(&s.counters),
        "genes": genes,
        "vegetarian_depth_pct": s.vegetarian_depth.as_ref().map(spread),
        "vegetarians_by_depth": s.vegetarians_by_depth,
        "plants_by_depth": s.plants_by_depth,
        "vegetarian_fullness": s.vegetarian_fullness.map(r),
        "predators_hungry": s.predators_hungry.map(r),
        "predator_fullness": s.predator_fullness.map(r),
        "predator_speed": s.predator_speed.map(r),
        "predator_vision": s.predator_vision.map(r),
    })
}

fn event(e: &Event) -> Value {
    json!({ "tick": e.tick, "kind": e.kind.key(), "text": e.text })
}

pub struct Run<'a> {
    pub seed: u64,
    pub res: &'a SimResult,
    pub events: &'a [Event],
    pub maps: &'a [story::Map],
}

pub fn report(cfg: &WorldConfig, rules: &Rules, ticks: u64, sample_every: u64, runs: &[Run]) -> Value {
    let rules: Map<_, _> = RULE_KEYS.iter().map(|k| (k.to_string(), json!(rules.get(k)))).collect();
    let space = life_core::Space::scaled(cfg.scale);
    json!({
        "format": "life-report/1",
        "world": { "scale": cfg.scale, "width": space.width, "height": space.height },
        "ticks": ticks,
        "sample_every": sample_every,
        "rules": rules,
        "start": {
            "vegetarians": cfg.n_vegetarians,
            "predators": cfg.n_predators,
            "predator_speed": cfg.predator_speed,
            "predator_vision": cfg.predator_vision,
        },
        "gene_keys": GENE_KEYS,
        "gene_labels": GENE_LABELS,
        "map_legend": MAP_LEGEND,
        "runs": runs.iter().map(|run| {
            let snaps = &run.res.snapshots;
            let (first, last) = (&snaps[0], snaps.last().expect("срезы есть всегда"));
            json!({
                "seed": run.seed,
                "stop": run.res.stop.key(),
                "stop_text": run.res.stop.to_string(),
                "ticks_done": run.res.ticks_done,
                "ms_per_tick": r(run.res.ms_per_tick()),
                "totals": counters(&last.counters.since(&first.counters)),
                "migrants": last.migrants,
                "events": run.events.iter().map(event).collect::<Vec<_>>(),
                "maps": run.maps.iter().map(|(t, rows)| json!({ "tick": t, "rows": rows })).collect::<Vec<_>>(),
                "snapshots": snaps.iter().map(snapshot).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}
