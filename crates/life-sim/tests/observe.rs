//! Тесты наблюдателя: срезы, хроника, карта. Хроника — то, по чему ИИ или
//! человек судит о прогоне без окна, поэтому ложное или пропущенное событие
//! здесь так же плохо, как ошибка в движке.

use life_core::config::*;
use life_core::{World, WorldConfig};
use life_sim::observe::{DEPTH_BANDS, EventKind, Snapshot, ascii_map, events};
use life_sim::{Limits, simulate};

fn world() -> World {
    World::new(&WorldConfig { seed: 2, n_vegetarians: Some(400), ..Default::default() })
}

fn kinds(snaps: &[Snapshot]) -> Vec<EventKind> {
    events(snaps).into_iter().map(|e| e.kind).collect()
}

#[test]
fn срез_раскладывает_всех_по_глубине() {
    let w = world();
    let s = Snapshot::of(&w);
    assert_eq!(s.vegetarians_by_depth.iter().sum::<usize>(), w.vegetarians.len());
    assert_eq!(s.plants_by_depth.iter().sum::<usize>(), w.plants.len());
    assert_eq!(s.vegetarians_by_depth.len(), DEPTH_BANDS);
    let g = s.genes.expect("травоядные есть");
    assert_eq!(g[0].p50, VEGETARIAN_BASE_GENOM[0], "на старте геном у всех базовый");

    let mut empty = w.clone();
    empty.vegetarians.clear();
    let s = Snapshot::of(&empty);
    assert!(s.genes.is_none() && s.vegetarian_depth.is_none() && s.vegetarian_fullness.is_none());
}

#[test]
fn хроника_видит_вымирание_хищников_и_обвал() {
    let mut w = world();
    let before = Snapshot::of(&w);
    w.predators.clear();
    w.vegetarians.truncate(100);
    w.tick = 60;
    let after = Snapshot::of(&w);
    let k = kinds(&[before.clone(), after]);
    assert!(k.contains(&EventKind::PredatorsExtinct), "{k:?}");
    assert!(k.contains(&EventKind::VegetariansCrash), "{k:?}");

    // без перемен — без событий
    let mut same = before.clone();
    same.tick = 60;
    assert!(kinds(&[before, same]).is_empty());
}

/// Мелкие колебания не событие: 5 хищников, ставших двумя, — шум.
#[test]
fn мелочь_не_попадает_в_хронику() {
    let mut w = world();
    w.predators.truncate(3);
    let before = Snapshot::of(&w);
    w.predators.truncate(1);
    w.tick = 60;
    assert!(kinds(&[before, Snapshot::of(&w)]).is_empty());
}

#[test]
fn хроника_видит_растения_у_потолка() {
    let mut w =
        World::new(&WorldConfig { n_vegetarians: Some(0), n_predators: Some(0), ..Default::default() });
    let mut snaps = vec![Snapshot::of(&w)];
    for _ in 0..10 {
        for _ in 0..100 {
            w.step();
        }
        snaps.push(Snapshot::of(&w));
    }
    assert_eq!(w.plants.len(), PLANT_MAX);
    let k = kinds(&snaps);
    assert_eq!(k.iter().filter(|&&e| e == EventKind::PlantsAtCap).count(), 1, "{k:?}");
}

#[test]
fn карта_ставит_хищника_на_место() {
    let mut w =
        World::new(&WorldConfig { n_vegetarians: Some(0), n_predators: Some(0), ..Default::default() });
    w.plants.clear();
    w.spawn_predator(PREDATOR_DIAM, PREDATOR_DIAM, None);
    let map = ascii_map(&w, 60);
    let rows = &map[1..map.len() - 1];
    assert!(rows.iter().all(|r| r.chars().count() == map[0].chars().count()), "строки разной длины");
    assert!(rows[0].contains("|X "), "хищник у поверхности слева не нарисован: {}", rows[0]);
    assert_eq!(map.iter().filter(|r| r.contains('X')).count(), 1);
}

/// Срезы идут в те же моменты, что история, а хроника — по порядку тиков.
#[test]
fn прогон_снимает_срезы_вместе_с_историей() {
    let limits = Limits { ticks: 3000, sample_every: 100, ..Default::default() };
    let res = simulate(&WorldConfig { seed: 1, ..Default::default() }, &limits, |_| {});
    assert_eq!(res.snapshots.len(), res.history.len());
    for (s, h) in res.snapshots.iter().zip(&res.history) {
        assert_eq!(
            (s.tick, s.vegetarians, s.predators, s.plants),
            (h.tick, h.vegetarians, h.predators, h.plants)
        );
    }
    let ev = events(&res.snapshots);
    assert!(ev.windows(2).all(|p| p[0].tick <= p[1].tick));
    assert!(ev.iter().all(|e| !e.text.is_empty()));
}
