//! Тесты наблюдателя: срезы, хроника, карта. Хроника — то, по чему ИИ или
//! человек судит о прогоне без окна, поэтому ложное или пропущенное событие
//! здесь так же плохо, как ошибка в движке.

use life_core::config::*;
use life_core::genome::creature::Gene;
use life_core::{World, WorldConfig};
use life_sim::observe::{DEPTH_BANDS, EventKind, GeneStat, MAX_VARIANTS, Snapshot, ascii_map, events};
use life_sim::{Limits, simulate};

fn world() -> World {
    World::new(&WorldConfig { seed: 2, n_creatures: Some(400), ..Default::default() })
}

fn kinds(snaps: &[Snapshot]) -> Vec<EventKind> {
    events(snaps).into_iter().map(|e| e.kind).collect()
}

#[test]
fn срез_раскладывает_всех_по_глубине() {
    let w = world();
    let s = Snapshot::of(&w);
    assert_eq!(s.creatures_by_depth.iter().sum::<usize>(), w.creatures.len());
    assert_eq!(s.plants_by_depth.iter().sum::<usize>(), w.plants.len());
    assert_eq!(s.creatures_by_depth.len(), DEPTH_BANDS);
    assert_eq!(s.creatures_by_width.iter().sum::<usize>(), w.creatures.len());
    assert_eq!(s.plants_by_width.iter().sum::<usize>(), w.plants.len());
    let g = s.genes.expect("существа есть");
    for (index, (stat, base)) in g.iter().zip(life_core::CreatureGenome::BASE.to_values()).enumerate() {
        match stat {
            GeneStat::Number(s) => assert_eq!(s.p50, base, "на старте геном у всех базовый"),
            GeneStat::Shares(s) => {
                for (variant, &share) in s.iter().enumerate() {
                    let count =
                        w.creatures.iter().filter(|v| v.genome.to_values()[index] == variant as f64).count();
                    assert_eq!(share, count as f64 / w.creatures.len() as f64);
                }
                if index == Gene::Strategy as usize {
                    assert_eq!(s[base as usize], 1.0, "начальная стратегия задана конфигурацией");
                }
            }
        }
    }
    let packs = g[Gene::PackInstinct as usize].shares().unwrap()[1];
    let shooters = g[Gene::Shooter as usize].shares().unwrap()[1];
    assert!((0.4..=0.6).contains(&packs), "половина основателей стайные: {packs}");
    assert!((0.02..=0.08).contains(&shooters), "редкие стрелки у основателей: {shooters}");

    let mut empty = w.clone();
    empty.creatures.clear();
    let s = Snapshot::of(&empty);
    assert!(s.genes.is_none() && s.depth.is_none() && s.fullness.is_none());
}

/// Одна волна по ширине — богатая полоса посередине: полосы среза это видят.
#[test]
fn срез_видит_еду_по_ширине() {
    let rules = life_core::Rules::default()
        .with_text("plant_width_profile", "waves")
        .and_then(|r| r.with("plant_width_waves", 1.0))
        .and_then(|r| r.with("plant_width_amplitude", 100.0))
        .unwrap();
    let mut w = World::new(&WorldConfig { rules, n_creatures: Some(0), ..Default::default() });
    for _ in 0..500 {
        w.step();
    }
    let s = Snapshot::of(&w);
    assert_eq!(s.plants_by_width.iter().sum::<usize>(), w.plants.len());
    let (edges, middle) =
        (s.plants_by_width[0] + s.plants_by_width[9], s.plants_by_width[4] + s.plants_by_width[5]);
    assert!(middle > 10 * edges, "середина {middle}, края {edges}: {:?}", s.plants_by_width);
}

#[test]
fn хроника_видит_обвал_и_вымирание() {
    let mut w = world();
    let before = Snapshot::of(&w);
    w.creatures.truncate(100);
    w.tick = 60;
    let after = Snapshot::of(&w);
    assert_eq!(kinds(&[before.clone(), after.clone()]), [EventKind::CreaturesCrash]);
    w.creatures.clear();
    w.tick = 120;
    let k = kinds(&[before.clone(), after, Snapshot::of(&w)]);
    assert_eq!(k, [EventKind::CreaturesCrash, EventKind::CreaturesExtinct]);

    // без перемен — без событий
    let mut same = before.clone();
    same.tick = 60;
    assert!(kinds(&[before, same]).is_empty());
}

/// Мелкие колебания не событие: 25 существ, ставших десятью, — шум.
#[test]
fn мелочь_не_попадает_в_хронику() {
    let mut w = world();
    w.creatures.truncate(25);
    // Проверяем именно численность: случайный состав маленькой выборки
    // основателей может сам по себе дать заметный сдвиг долей стайности.
    for v in &mut w.creatures {
        v.genome = life_core::CreatureGenome::BASE;
    }
    let before = Snapshot::of(&w);
    w.creatures.truncate(10);
    w.tick = 60;
    assert!(kinds(&[before, Snapshot::of(&w)]).is_empty());
}

#[test]
fn хроника_видит_растения_у_потолка() {
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
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
fn карта_ставит_существо_на_место() {
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    w.plants.clear();
    w.spawn(life_core::CreatureGenome::BASE, 0.0, 0.0, None);
    let map = ascii_map(&w, 60);
    let rows = &map[1..map.len() - 1];
    assert!(rows.iter().all(|r| r.chars().count() == map[0].chars().count()), "строки разной длины");
    assert!(rows[0].contains("|o "), "существо у поверхности слева не нарисовано: {}", rows[0]);
    assert_eq!(map.iter().filter(|r| r.contains('o')).count(), 1);
}

/// Срезы идут в те же моменты, что история, а хроника — по порядку тиков.
#[test]
fn прогон_снимает_срезы_вместе_с_историей() {
    let limits = Limits { ticks: 3000, sample_every: 100, ..Default::default() };
    let res = simulate(&WorldConfig { seed: 1, ..Default::default() }, &limits, |_| {});
    assert_eq!(res.snapshots.len(), res.history.len());
    for (s, h) in res.snapshots.iter().zip(&res.history) {
        assert_eq!((s.tick, s.creatures, s.plants), (h.tick, h.creatures, h.plants));
    }
    let ev = events(&res.snapshots);
    assert!(ev.windows(2).all(|p| p[0].tick <= p[1].tick));
    assert!(ev.iter().all(|e| !e.text.is_empty()));
}

/// Стратегия расползлась по популяции — хроника это видит.
#[test]
fn хроника_видит_смену_стратегий() {
    use life_core::genome::creature::Gene;
    let mut w = world();
    let before = Snapshot::of(&w);
    let half = w.creatures.len() / 2;
    for v in &mut w.creatures[..half] {
        v.genome = v.genome.with(Gene::Strategy, 1.0);
    }
    w.tick = 60;
    let e = events(&[before, Snapshot::of(&w)]);
    let shifts: Vec<&str> =
        e.iter().filter(|e| e.kind == EventKind::StrategyShift).map(|e| e.text.as_str()).collect();
    assert_eq!(shifts.len(), 1, "{shifts:?}");
    assert!(shifts[0].starts_with("существа") && shifts[0].contains("затаившийся"), "{shifts:?}");
}

/// Доли гена-выбора лежат в массиве на `MAX_VARIANTS`: вариантов в таблице
/// не больше.
#[test]
fn вариантов_не_больше_места_в_срезе() {
    for spec in life_core::genome::creature::GENES.iter() {
        let n = spec.variants().map_or(0, <[_]>::len);
        assert!(n <= MAX_VARIANTS, "у гена {} {n} вариантов", spec.key);
    }
}
