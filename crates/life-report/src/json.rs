//! Полный отчёт в JSON — для программ и ИИ: всё, что печатает рассказ, плюс
//! каждый срез целиком. Ключи — машинные (английские, как в эталоне Python),
//! тексты событий — по-русски.

use life_core::genome::{GeneKind, GeneSpec, creature};
use life_core::rules::RULE_KEYS;
use life_core::{Counters, Rules, WorldConfig};
use life_sim::SimResult;
use life_sim::observe::{Event, GeneStat, MAP_LEGEND, Snapshot, Spread};
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
        "born": c.born,
        "starved": c.starved,
        "cannibalized": c.cannibalized,
    })
}

/// Сводка генов по таблице: у числового гена — разброс, у гена-выбора — доли
/// вариантов по их ключам.
fn gene_stats(genes: &[GeneSpec], stats: &[GeneStat]) -> Value {
    let map: Map<_, _> = genes
        .iter()
        .zip(stats)
        .map(|(spec, stat)| {
            let v = match stat {
                GeneStat::Number(s) => spread(s),
                GeneStat::Shares(s) => {
                    let variants = spec.variants().unwrap_or_default();
                    let shares: Map<_, _> = variants
                        .iter()
                        .zip(s)
                        .map(|(v, share)| (v.key.to_string(), json!(r(*share))))
                        .collect();
                    json!({ "shares": shares })
                }
            };
            (spec.key.to_string(), v)
        })
        .collect();
    Value::Object(map)
}

/// Таблица генов: чтобы JSON описывал сам себя.
fn gene_table(genes: &[GeneSpec]) -> Value {
    genes
        .iter()
        .map(|g| {
            let (kind, variants) = match g.kind {
                GeneKind::Absolute => ("absolute", None),
                GeneKind::Percent => ("percent", None),
                GeneKind::Choice(v) => (
                    "choice",
                    Some(v.iter().map(|v| json!({ "key": v.key, "label": v.label })).collect::<Vec<_>>()),
                ),
            };
            let mut row = json!({ "key": g.key, "label": g.label, "kind": kind, "base": g.base });
            if let Some(v) = variants {
                row["variants"] = json!(v);
            }
            row
        })
        .collect()
}

fn snapshot(s: &Snapshot) -> Value {
    let genes = s.genes.map(|g| gene_stats(&creature::GENES, &g));
    json!({
        "tick": s.tick,
        "plants": s.plants,
        "plant_cap": s.plant_cap,
        "creatures": s.creatures,
        "counters": counters(&s.counters),
        "genes": genes,
        "depth_pct": s.depth.as_ref().map(spread),
        "creatures_by_depth": s.creatures_by_depth,
        "plants_by_depth": s.plants_by_depth,
        "creatures_by_width": s.creatures_by_width,
        "plants_by_width": s.plants_by_width,
        "fullness": s.fullness.map(r),
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
    let space = cfg.space();
    json!({
        "format": "life-report/3",
        "world": { "scale": cfg.scale, "shape": cfg.shape.key(), "width": space.width, "height": space.height },
        "ticks": ticks,
        "sample_every": sample_every,
        "rules": rules,
        "start": {
            // настоящие числа, даже если заданы «по умолчанию»: null читателю ничего не говорит
            "creatures": cfg.creatures_at_start(),
            "strategies": cfg.strategies,
        },
        "genes": { "creature": gene_table(&creature::GENES) },
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
                "events": run.events.iter().map(event).collect::<Vec<_>>(),
                "maps": run.maps.iter().map(|(t, rows)| json!({ "tick": t, "rows": rows })).collect::<Vec<_>>(),
                "snapshots": snaps.iter().map(snapshot).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}
