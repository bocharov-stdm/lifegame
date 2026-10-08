//! The full report as JSON — for programs and AIs: everything the story prints, plus every
//! sample in full. The keys are machine ones (English), the event texts Russian
//! (`life_sim::observe`).

use life_core::genome::{GeneKind, GeneSpec, creature};
use life_core::rules::{DIETS, RULE_KEYS};
use life_core::world::DietCounters;
use life_core::{Counters, Rules, WorldConfig};
use life_sim::SimResult;
use life_sim::observe::{Event, GeneStat, MAP_LEGEND, Snapshot, Spread};
use serde_json::{Map, Value, json};

use crate::story;

/// Three decimals: the report needs no more precision, and extra digits bloat the JSON and make
/// it hard to read.
fn r(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn spread(s: &Spread) -> Value {
    json!({ "p10": r(s.p10), "p50": r(s.p50), "p90": r(s.p90), "mean": r(s.mean) })
}

/// Flows by diet keyed by diet: `born[diet]`, `deaths[diet]` (starved, old age, combat), and the
/// matrices `strikes[striker][target]`, `kills[killer][victim]`.
fn by_diet(c: &DietCounters) -> Value {
    let row =
        |r: &[u64; 4]| Value::Object(DIETS.iter().zip(r).map(|(d, n)| (d.to_string(), json!(n))).collect());
    let keyed =
        |f: &dyn Fn(usize) -> Value| Value::Object((0..4).map(|d| (DIETS[d].to_string(), f(d))).collect());
    json!({
        "born": row(&c.born),
        "deaths": keyed(&|d| json!({ "starved": c.deaths[d][0], "old_age": c.deaths[d][1], "combat": c.deaths[d][2] })),
        "strikes": keyed(&|d| row(&c.strikes[d])),
        "kills": keyed(&|d| row(&c.kills[d])),
    })
}

fn counters(c: &Counters) -> Value {
    json!({
        "plants_grown": c.plants_grown,
        "plants_eaten": c.plants_eaten,
        "plant_bites": c.plant_bites,
        "meat_bites": c.meat_bites,
        "rot_bites": c.rot_bites,
        "bone_bites": c.bone_bites,
        "ranged_shots": c.ranged_shots,
        "born": c.born,
        "starved": c.starved,
        "old_age": c.old_age,
        "combat": c.combat,
        "corpses": c.corpses,
        "corpses_gone": c.corpses_gone,
        "corpses_bottom": c.corpses_bottom,
        "skeletons": c.skeletons,
        "corpse_ticks": c.corpse_ticks,
        "by_diet": by_diet(&c.by_diet),
    })
}

/// A summary of the genes by the table: a spread for a numeric gene, the shares of the variants
/// by their keys for a choice gene.
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

/// The gene table: so that the JSON describes itself.
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
        "corpses": s.corpses,
        "plant_cap": s.plant_cap,
        "creatures": s.creatures,
        // exact, not creatures × a rounded share: 99 of 10 000 must not read as 1%
        "diet_creatures": Value::Object(
            DIETS.iter().zip(&s.diets).map(|(d, stat)| (d.to_string(), json!(stat.creatures))).collect()
        ),
        "juveniles": s.juveniles,
        "activities": life_core::creature::Activity::ALL.iter().enumerate().map(|(i,a)| json!({"name":a.label(),"count":s.activities[i],"share": if s.creatures>0 {s.activities[i] as f64/s.creatures as f64} else {0.0}})).collect::<Vec<_>>(),
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
    /// The tick rate over the run: (tick, ms a tick over the lap before it).
    pub pace: &'a [(u64, f64)],
}

pub fn report(cfg: &WorldConfig, rules: &Rules, ticks: u64, sample_every: u64, runs: &[Run]) -> Value {
    let rules: Map<_, _> = RULE_KEYS.iter().map(|k| (k.to_string(), json!(rules.get(k)))).collect();
    let space = cfg.space();
    json!({
        "format": "life-report/12",
        "world": { "scale": cfg.scale, "shape": cfg.shape.key(), "width": space.width, "height": space.height },
        "ticks": ticks,
        "sample_every": sample_every,
        "rules": rules,
        "start": {
            // real numbers even when they are «default»: null tells the reader nothing
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
                "pace": run.pace.iter().map(|&(tick, ms)| json!([tick, r(ms)])).collect::<Vec<_>>(),
                "totals": counters(&last.counters.since(&first.counters)),
                "events": run.events.iter().map(event).collect::<Vec<_>>(),
                "maps": run.maps.iter().map(|(t, rows)| json!({ "tick": t, "rows": rows })).collect::<Vec<_>>(),
                "snapshots": snaps.iter().map(snapshot).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::{World, corpse::Corpse};
    use life_sim::{Limits, run};

    #[test]
    fn отчёт_содержит_новые_поля_питания_и_боя() {
        let cfg = WorldConfig::default();
        let mut world = World::new(&cfg);
        let dead = world.creatures.pop().unwrap();
        world.corpses.push(Corpse::from_creature(&dead, 0));
        let res = run(world, &Limits { ticks: 0, ..Limits::default() }, &mut |_| {});
        let runs =
            [Run { seed: cfg.seed, res: &res, events: &[], maps: &[], pace: &[(500, 2.5), (1000, 7.25)] }];
        let data = report(&cfg, &cfg.rules, 0, 1, &runs);
        assert_eq!(data["format"], "life-report/12");
        let run = &data["runs"][0];
        assert_eq!(run["pace"], json!([[500, 2.5], [1000, 7.25]]), "the tick rate by laps");
        let snap = &run["snapshots"][0];
        for key in [
            "plant_bites",
            "meat_bites",
            "rot_bites",
            "bone_bites",
            "ranged_shots",
            "corpses",
            "corpses_gone",
            "corpses_bottom",
            "skeletons",
            "corpse_ticks",
        ] {
            assert!(run["totals"][key].as_u64().is_some(), "нет итогового счётчика {key}");
            assert!(snap["counters"][key].as_u64().is_some(), "нет счётчика среза {key}");
        }
        for key in ["size", "burst", "diet"] {
            assert!(data["genes"]["creature"].as_array().unwrap().iter().any(|row| row["key"] == key));
            assert!(snap["genes"][key].is_object(), "нет сводки гена {key}");
        }
        assert!(snap["activities"].as_array().is_some_and(|a| !a.is_empty()));
        for diet in DIETS {
            assert!(run["totals"]["by_diet"]["born"][diet].as_u64().is_some(), "no births of {diet}");
            assert!(run["totals"]["by_diet"]["kills"]["carnivore"][diet].as_u64().is_some());
            assert!(snap["counters"]["by_diet"]["deaths"][diet]["starved"].as_u64().is_some());
        }
        let by_diet: u64 = DIETS.iter().map(|d| snap["diet_creatures"][d].as_u64().unwrap()).sum();
        assert_eq!(Some(by_diet), snap["creatures"].as_u64(), "every creature counted by its diet");
        assert_eq!(snap["corpses"], 1);
    }
}
