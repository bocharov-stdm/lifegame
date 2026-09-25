use life_core::{CreatureGenome, Rules, World, WorldConfig, genome::creature::Gene, social::Activity};

fn world() -> World {
    World::new(&WorldConfig {
        seed: 42,
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    })
}

#[test]
fn отдых_стоит_энергии_и_кончается_при_голоде() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.creatures[0].reproduction_wait = 1000;
    let upkeep = w.creatures[0].pheno.slow_upkeep;
    w.step();
    let v = &w.creatures[0];
    assert_eq!(v.mind.social.activity, Activity::Resting);
    assert_eq!((v.x, v.y), (1000.0, 1000.0));
    assert!((v.energy - (100.0 - upkeep)).abs() < 1e-9);
    assert_eq!(v.age, 1.0);
    w.creatures[0].energy = 60.0;
    w.step();
    assert_ne!(w.creatures[0].mind.social.activity, Activity::Resting);
}

#[test]
fn участник_в_своём_круге_отдыхает_на_тех_же_условиях() {
    let mut w = world();
    for x in [1000.0, 1100.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(100.0));
    }
    w.creatures[1].flock = w.creatures[0].flock;
    for v in &mut w.creatures {
        v.reproduction_wait = 1000;
    }
    let upkeep = w.creatures[0].pheno.slow_upkeep;
    w.step();
    assert_eq!(w.creatures[0].mind.social.activity, Activity::Resting);
    assert!((w.creatures[0].energy - (100.0 - upkeep)).abs() < 1e-9);
}

#[test]
fn стайный_видит_растение_даже_при_полной_общительности() {
    let mut w = world();
    let g = CreatureGenome::BASE.with(Gene::Sociability, 100.0);
    w.spawn(g, 1000.0, 1000.0, Some(50.0));
    w.spawn(g, 1000.0, 1040.0, Some(50.0));
    w.creatures[1].flock = w.creatures[0].flock;
    // inside the pair's circle (centre (1000, 1020), radius 85)
    w.plants.push(life_core::plant::Plant::at(1060.0, 1000.0));
    w.step();
    assert!(w.creatures[0].x > 1000.0);
    assert_eq!(w.creatures[0].mind.social.activity, Activity::Feeding);
}

#[test]
fn участник_вдали_от_круга_возвращается_а_в_крайнем_голоде_ест_рядом() {
    for (energy, feeds) in [(80.0, false), (50.0, false), (20.0, true)] {
        let mut w = world();
        let far = w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(energy));
        for x in [2600.0, 2620.0, 2640.0] {
            w.spawn(CreatureGenome::BASE, x, 1000.0, Some(80.0));
        }
        let tag = w.creatures[1].flock;
        for v in &mut w.creatures {
            v.flock = tag;
            v.reproduction_wait = 1000;
        }
        w.plants.push(life_core::plant::Plant::at(1000.0, 1150.0));
        w.step();
        let v = w.creature(far).unwrap();
        let circle = v.circle.expect("участник знает круг стаи");
        assert!(!circle.holds(1000.0, 1000.0, v.pheno.half), "участник вне круга");
        if feeds {
            assert_eq!(v.mind.social.activity, Activity::Feeding, "запас {energy}");
            assert!(v.y > 1000.0);
        } else {
            assert_eq!(v.mind.social.activity, Activity::Gathering, "запас {energy}");
            assert!(v.x > 1000.0, "идёт к кругу");
        }
    }
}

#[test]
fn растение_за_кругом_не_уводит_сытого_участника() {
    let mut w = world();
    for x in [1000.0, 1020.0, 1040.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(60.0));
    }
    let tag = w.creatures[0].flock;
    for v in &mut w.creatures {
        v.flock = tag;
        v.reproduction_wait = 1000;
    }
    w.step();
    let circle = w.creatures[0].circle.unwrap();
    // just outside the circle but well within sight
    let (px, py) = (circle.x + circle.radius + 60.0, circle.y);
    w.plants.push(life_core::plant::Plant::at(px, py));
    for _ in 0..30 {
        w.step();
        for v in &w.creatures {
            assert!(v.mind.social.personal_food.is_none(), "растение за кругом стало личной целью");
            assert!(circle.holds(v.x, v.y, v.pheno.half + v.pheno.speed * 2.0));
        }
    }
    assert!(w.plants[0].alive && w.plants[0].portions == life_core::plant::PORTIONS);
}

#[test]
fn мягкое_расхождение_не_отталкивает_от_видимой_еды() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(50.0));
    let tag = w.creatures[0].flock;
    for y in [1000.0, 1002.0, 1004.0, 1006.0, 1008.0, 1010.0, 1012.0] {
        w.spawn(CreatureGenome::BASE, 1050.0, y, Some(50.0));
        w.creatures.last_mut().unwrap().flock = tag;
    }
    w.plants.push(life_core::plant::Plant::at(1200.0, 1000.0));
    w.step();
    assert!(w.creatures[0].x > 1000.0);
}

#[test]
fn теснота_совпадение_и_край_не_дают_прыжка() {
    let mut a = world();
    for speed in [5.0, 10.0, 15.0] {
        a.spawn(CreatureGenome::BASE.with(Gene::Speed, speed), 40.0, 40.0, None);
    }
    for v in &mut a.creatures {
        v.flock = 1;
    }
    let mut b = a.clone();
    for _ in 0..60 {
        let before: Vec<_> = a.creatures.iter().map(|v| (v.x, v.y)).collect();
        a.step();
        b.step();
        for ((v, u), (x, y)) in a.creatures.iter().zip(&b.creatures).zip(before) {
            assert_eq!((v.x, v.y), (u.x, u.y));
            assert!((v.x - x).hypot(v.y - y) <= v.pheno.speed + 1e-9);
            assert!(v.x.is_finite() && v.y.is_finite());
            assert!(v.x >= v.pheno.x_lo && v.y >= v.pheno.y_lo);
        }
    }
}

#[test]
fn общительность_наследуется_в_границах() {
    let mut rng = life_core::rng::Rng::new(42);
    let mut g = CreatureGenome::BASE;
    for _ in 0..1000 {
        g = g.mutate(0.3, &mut rng);
        assert!((0.0..=100.0).contains(&g[Gene::Sociability]));
    }
}

#[test]
fn сведения_о_еде_не_ретранслируются_и_истекают() {
    use life_core::{
        grid::Grid,
        social::{Food, prepare},
    };
    let mut w = world();
    for x in [1000.0, 1300.0, 1600.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
    }
    for v in &mut w.creatures {
        v.flock = 1;
    }
    let food = Food { x: 800.0, y: 1000.0, tick: 1, observer: w.creatures[0].id };
    w.creatures[0].mind.social.observed_food = Some(food);
    let mut grid = Grid::new(100.0);
    grid.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    prepare(&mut w.creatures, &grid, 1);
    assert_eq!(w.creatures[1].mind.social.food, Some(food));
    assert!(w.creatures[2].mind.social.food.is_none());
    prepare(&mut w.creatures, &grid, 2);
    assert!(w.creatures[2].mind.social.food.is_none());
    assert_eq!(w.creatures[1].mind.social.food.unwrap().tick, 1);
    prepare(&mut w.creatures, &grid, 181);
    assert!(w.creatures.iter().all(|v| v.mind.social.food.is_none()));
}

#[test]
fn пустое_кормовое_место_забывается() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
    w.creatures[0].mind.social.food =
        Some(life_core::social::Food { x: 1000.0, y: 1000.0, tick: 0, observer: 99 });
    w.step();
    assert!(w.creatures[0].mind.social.food.is_none());
}

#[test]
fn тревога_не_ретранслируется_и_выключается_правилом() {
    use life_core::{
        grid::Grid,
        social::{Alarm, prepare},
    };
    let mut w = world();
    for x in [1000.0, 1300.0, 1600.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
    }
    for v in &mut w.creatures {
        v.flock = 1;
    }
    w.creatures[0].mind.social.observed_alarm = Some(Alarm { enemy: 99, x: 800.0, y: 1000.0, tick: 1 });
    let mut g = Grid::new(100.0);
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    for t in 1..=61 {
        prepare(&mut w.creatures, &g, t);
        assert!(w.creatures[2].mind.social.alarm.is_none());
    }
    assert!(w.creatures[1].mind.social.alarm.is_none());
    w.creatures[0].mind.social.alarm = w.creatures[0].mind.social.observed_alarm;
    w.set_rules(w.rules.clone());
    assert!(
        w.creatures.iter().all(|v| v.mind.social.alarm.is_none() && v.mind.social.observed_alarm.is_none())
    );
}

#[test]
fn прикрытие_ограничено_двумя_здоровыми_и_временем() {
    use life_core::{
        grid::Grid,
        social::{Alarm, prepare_aid},
    };
    let mut w = world();
    for x in [1000.0, 1050.0, 1100.0, 1150.0, 1200.0, 1250.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(80.0));
    }
    for v in &mut w.creatures[..5] {
        v.flock = 1;
    }
    w.creatures[1].health = 1.0;
    let enemy = w.creatures[5].id;
    w.creatures[0].mind.social.hit = Some(Alarm { enemy, x: 1250.0, y: 1000.0, tick: 1 });
    let mut g = Grid::new(100.0);
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    assert_eq!(prepare_aid(&mut w.creatures, &g, 1), 2);
    assert!(w.creatures[1].mind.social.aid.is_none());
    assert!(w.creatures[2].mind.social.aid.is_some() && w.creatures[3].mind.social.aid.is_some());
    assert!(w.creatures[4].mind.social.aid.is_none());
    prepare_aid(&mut w.creatures, &g, 31);
    assert!(w.creatures.iter().all(|v| v.mind.social.aid.is_none()));
    w.creatures[0].mind.social.hit.as_mut().unwrap().tick = 100;
    prepare_aid(&mut w.creatures, &g, 100);
    for t in 101..=190 {
        w.creatures[0].mind.social.hit.as_mut().unwrap().tick = t;
        prepare_aid(&mut w.creatures, &g, t);
    }
    assert!(w.creatures[2].mind.social.aid.is_none() && w.creatures[3].mind.social.aid.is_none());
}

#[test]
fn отделение_требует_устойчивой_тройки_и_сохраняет_родство() {
    use life_core::{flock::split, grid::Grid};
    let mut w = world();
    for x in [1000.0, 1010.0, 1020.0, 3000.0, 3010.0, 3020.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
    }
    for v in &mut w.creatures {
        v.flock = 1;
        v.parent = 999;
    }
    let mut g = Grid::new(100.0);
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    let mut watches = Vec::new();
    let mut next = 10;
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 60), 0);
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 600), 0);
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 660), 1);
    assert!(w.creatures[..3].iter().all(|v| v.flock == 1));
    assert!(w.creatures[3..].iter().all(|v| v.flock == 10 && v.parent == 999));
    assert_eq!(next, 11);
    assert!(watches.is_empty());
}

#[test]
fn соединение_и_смерть_якоря_сбрасывают_ожидание() {
    use life_core::{flock::split, grid::Grid};
    let mut w = world();
    for x in [1000.0, 1010.0, 1020.0, 1030.0, 3000.0, 3010.0, 3020.0, 3030.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
    }
    for v in &mut w.creatures {
        v.flock = 1;
    }
    let mut g = Grid::new(100.0);
    let mut watches = Vec::new();
    let mut next = 10;
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    split(&mut w.creatures, &g, &mut watches, &mut next, 60);
    w.creatures.remove(4);
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 660), 0);
    assert_eq!(watches.iter().filter(|watch| watch.since == 660).count(), 1);
    for v in &mut w.creatures {
        v.x = 1000.0;
    }
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    split(&mut w.creatures, &g, &mut watches, &mut next, 720);
    assert!(watches.is_empty());
}

#[test]
fn смена_крупнейшей_компоненты_не_обнуляет_отделение() {
    use life_core::{flock::split, grid::Grid};
    let mut w = world();
    for x in [1000.0, 1010.0, 1020.0, 1030.0, 3000.0, 3010.0, 3020.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
    }
    let tag = w.creatures[0].flock;
    for v in &mut w.creatures {
        v.flock = tag;
    }
    let mut g = Grid::new(100.0);
    let mut watches = Vec::new();
    let mut next = 100;
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 60), 0);
    assert_eq!(watches.len(), 2);
    for x in [3030.0, 3040.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, None);
        w.creatures.last_mut().unwrap().flock = tag;
    }
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 360), 0);
    assert_eq!(split(&mut w.creatures, &g, &mut watches, &mut next, 660), 1);
    assert!(w.creatures[..4].iter().all(|v| v.flock == 100));
    assert!(w.creatures[4..].iter().all(|v| v.flock == tag));
}

#[test]
fn затаившийся_по_тревоге_идёт_на_полной_скорости() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE.with(Gene::Strategy, 1.0), 1000.0, 1000.0, None);
    let v = &mut w.creatures[0];
    v.mind.social.alarm = Some(life_core::social::Alarm { enemy: 99, x: 800.0, y: 1000.0, tick: 0 });
    v.step(&life_core::senses::Blind);
    assert!(v.fleeing());
    assert!((v.x - 1000.0 - v.pheno.speed).abs() < 1e-9);
    assert_eq!(v.y, 1000.0);
}

#[test]
fn новый_отдых_не_начинается_сразу_после_старого() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.creatures[0].reproduction_wait = 1000;
    for _ in 0..170 {
        w.creatures[0].energy = 100.0;
        w.step();
    }
    assert_eq!(w.creatures[0].mind.social.rest_count, 1);
    assert_ne!(w.creatures[0].mind.social.activity, Activity::Resting);
}

#[test]
fn нулевая_общительность_не_принимает_сообщения() {
    use life_core::creature::{Me, Mind};
    for (s, min, max) in [(0.0, 0, 0), (50.0, 400, 600), (100.0, 1000, 1000)] {
        let mut w = world();
        w.spawn(CreatureGenome::BASE.with(Gene::Sociability, s), 1000.0, 1000.0, None);
        let v = &w.creatures[0];
        let me = Me {
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
            pheno: &v.pheno,
            health_share: 1.0,
        };
        let mut mind = Mind::default();
        let mut accepted = 0;
        for t in 0..1000 {
            mind.social.tick = t * 180;
            accepted += life_core::social::follows_reports(&me, &mind) as usize;
        }
        assert!((min..=max).contains(&accepted));
    }
}

#[test]
fn восемь_соседей_совпадают_с_перебором_включая_границу() {
    use life_core::{grid::Grid, rng::Rng, social::neighbors};
    let mut w = world();
    let mut rng = Rng::new(42);
    for _ in 0..100 {
        w.spawn(CreatureGenome::BASE, rng.uniform(1000.0, 2000.0), rng.uniform(1000.0, 2000.0), None);
    }
    for (i, v) in w.creatures.iter_mut().enumerate() {
        v.flock = (i % 3) as u64 + 1;
    }
    w.creatures[0].x = 1000.0;
    w.creatures[0].y = 1000.0;
    w.creatures[3].x = 1400.0;
    w.creatures[3].y = 1000.0;
    let mut g = Grid::new(100.0);
    g.rebuild(&w.space, w.creatures.iter().map(|v| (v.x, v.y)));
    for (i, v) in w.creatures.iter().enumerate() {
        let mut expected: Vec<_> = w
            .creatures
            .iter()
            .enumerate()
            .filter(|(j, u)| i != *j && u.flock == v.flock)
            .map(|(j, u)| ((u.x - v.x).powi(2) + (u.y - v.y).powi(2), u.id, j))
            .filter(|&(d, _, _)| d <= v.pheno.vision2)
            .collect();
        expected.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        assert_eq!(
            neighbors(i, &w.creatures, &g),
            expected.into_iter().take(8).map(|(_, _, j)| j).collect::<Vec<_>>()
        );
    }
}

#[test]
fn ребёнок_не_принимает_решений_в_тик_рождения() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.creatures[0].reproduction_wait = 0;
    w.step();
    assert_eq!(w.creatures.len(), 2);
    let child = &w.creatures[1];
    assert_eq!(child.age, 0.0);
    assert_eq!(child.mind.social, life_core::social::Memory::default());
    assert!(child.mind.attack.is_none());
    w.step();
    assert!(w.creatures[1].age > 0.0);
}
