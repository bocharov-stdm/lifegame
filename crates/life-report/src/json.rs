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
        "plant_bites": c.plant_bites,
        "meat_bites": c.meat_bites,
        "ranged_shots": c.ranged_shots,
        "territorial_fights": c.territorial_fights,
        "born": c.born,
        "starved": c.starved,
        "old_age": c.old_age,
        "combat": c.combat,
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
        "corpses": s.corpses,
        "plant_cap": s.plant_cap,
        "plant_biomass": s.plant_biomass,
        "plant_biomass_cap": s.plant_biomass_cap,
        "creatures": s.creatures,
        "juveniles": s.juveniles,
        "pack_carriers": s.pack_carriers,
        "pack_share": s.pack_share,
        "pack_members": s.pack_members,
        "flocks": s.flocks,
        "activities": life_core::social::Activity::ALL.iter().enumerate().map(|(i,a)| json!({"name":a.label(),"count":s.activities[i],"share": if s.creatures>0 {s.activities[i] as f64/s.creatures as f64} else {0.0}})).collect::<Vec<_>>(),
        "flock_spread": s.flock_spread.as_ref().map(spread),
        "flock_radius": s.flock_radius.as_ref().map(spread),
        "flock_kinds": life_core::flock::FlockKind::ALL.iter().map(|k| json!({"kind": life_core::genome::creature::FLOCK_KIND_VARIANTS[*k as usize].key, "flocks": s.flock_kinds[*k as usize]})).collect::<Vec<_>>(),
        "inside_share": s.inside_share,
        "overlaps": {"strict_pairs": s.overlaps.strict_pairs, "strict_depth": s.overlaps.strict_depth, "soft_pairs": s.overlaps.soft_pairs, "soft_depth": s.overlaps.soft_depth},
        "social": {"alarms":s.social_counts.alarms,"alarm_ends":s.social_counts.alarm_ends,"interventions":s.social_counts.interventions,"splits":s.social_counts.splits,"departures":s.social_counts.departures,"strays":s.social_counts.strays,"relocations":s.social_counts.relocations,"battles":s.social_counts.battles,"battle_retreats":s.social_counts.battle_retreats},
        "squeezed_flocks": s.squeezed_flocks,
        "pressed_flocks": s.pressed_flocks,
        "battles": s.battles,
        "fighting_flocks": s.fighting_flocks,
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
        "format": "life-report/10",
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
                "social_totals": {
                    "alarms": last.social_counts.alarms - first.social_counts.alarms,
                    "alarm_ends": last.social_counts.alarm_ends - first.social_counts.alarm_ends,
                    "interventions": last.social_counts.interventions - first.social_counts.interventions,
                    "splits": last.social_counts.splits - first.social_counts.splits,
                    "departures": last.social_counts.departures - first.social_counts.departures,
                    "strays": last.social_counts.strays - first.social_counts.strays,
                    "relocations": last.social_counts.relocations - first.social_counts.relocations,
                    "battles": last.social_counts.battles - first.social_counts.battles,
                    "battle_retreats": last.social_counts.battle_retreats - first.social_counts.battle_retreats,
                },
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
        let runs = [Run { seed: cfg.seed, res: &res, events: &[], maps: &[] }];
        let data = report(&cfg, &cfg.rules, 0, 1, &runs);
        assert_eq!(data["format"], "life-report/10");
        let run = &data["runs"][0];
        let snap = &run["snapshots"][0];
        for key in ["plant_bites", "meat_bites", "ranged_shots", "territorial_fights"] {
            assert!(run["totals"][key].as_u64().is_some(), "нет итогового счётчика {key}");
            assert!(snap["counters"][key].as_u64().is_some(), "нет счётчика среза {key}");
        }
        for key in ["pack_instinct", "territoriality", "care"] {
            assert!(data["genes"]["creature"].as_array().unwrap().iter().any(|row| row["key"] == key));
            assert!(snap["genes"][key].is_object(), "нет сводки гена {key}");
        }
        for key in ["pack_carriers", "pack_members", "flocks"] {
            assert!(snap[key].as_u64().is_some(), "нет численности {key}");
        }
        assert!(snap["pack_share"].as_f64().is_some());
        assert_eq!(snap["corpses"], 1);
        assert_eq!(snap["social"]["departures"], 0);
        assert_eq!(run["social_totals"]["departures"], 0);
    }
}
