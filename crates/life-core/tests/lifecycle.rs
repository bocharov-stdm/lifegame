//! Регрессии жизненного цикла.
use life_core::creature::Creature;
use life_core::genome::creature::Gene;
use life_core::rng::Rng;
use life_core::{CreatureGenome, Rules, Space};

fn parent() -> Creature {
    let mut v = Creature::new(
        &Space::default(),
        &Rules::default(),
        CreatureGenome::BASE.with(Gene::ReproThreshold, 30.0),
        Some(1000.0),
        Some(1000.0),
        Some(100.0),
        Rng::new(7),
    );
    v.reproduction_wait = 0;
    v
}

#[test]
fn пища_оплачивает_рост_и_размножение_ждёт_взросления() {
    let r = Rules::default();
    let mut p = parent();
    let mut child = p.maybe_divide(&Space::default(), &r).unwrap();
    assert_eq!(child.pheno.size, child.genome[Gene::Size] * 0.5);
    assert!(!child.adult());
    assert!(child.maybe_divide(&Space::default(), &r).is_none());
    let size = child.pheno.size;
    let energy = child.energy;
    child.nourish(2.0, &r);
    let cost = 2.0 * child.pheno.life_pace / (1.0 + child.pheno.life_pace);
    assert!((child.pheno.size - size - cost / 2.5).abs() < 1e-9);
    assert!((child.energy - energy - (2.0 - cost)).abs() < 1e-9);
    let size = child.pheno.size;
    child.nourish(0.0, &r);
    assert_eq!(child.pheno.size, size);
    child.nourish(10000.0, &r);
    assert!(child.adult());
}

#[test]
fn рост_у_стены_не_перемещает_центр() {
    let r = Rules::default();
    let mut child = parent().maybe_divide(&Space::default(), &r).unwrap();
    child.x = child.pheno.size;
    let before = (child.x, child.y, child.pheno.size);
    child.nourish(10000.0, &r);
    assert_eq!((child.x, child.y, child.pheno.size), before);
    child.apply_rules(&r, &Space::default());
    assert_eq!(child.pheno.size, before.2);
}

#[test]
fn темп_ускоряет_возраст_но_не_скорость() {
    use life_core::senses::Blind;
    let mut slow = parent();
    slow.genome = slow.genome.with(Gene::LifePace, 0.5);
    slow.apply_rules(&Rules::default(), &Space::default());
    let mut fast = parent();
    fast.genome = fast.genome.with(Gene::LifePace, 2.0);
    fast.apply_rules(&Rules::default(), &Space::default());
    slow.step(&Blind);
    fast.step(&Blind);
    assert_eq!(slow.age, 0.5);
    assert_eq!(fast.age, 2.0);
    assert_eq!(slow.pheno.speed, fast.pheno.speed);
    assert_eq!(fast.pheno.upkeep, slow.pheno.upkeep * 4.0);
    fast.age = life_core::config::LIFESPAN - 1.0;
    fast.step(&Blind);
    assert_eq!(fast.death, Some(life_core::creature::Death::OldAge));
    assert!(!fast.alive);
}

#[test]
fn взросление_не_обходит_таймер_рождения() {
    let r = Rules::default();
    let mut child = parent().maybe_divide(&Space::default(), &r).unwrap();
    child.nourish(10000.0, &r);
    assert!(child.adult());
    assert!(child.reproduction_wait > 0);
    assert!(child.maybe_divide(&Space::default(), &r).is_none());
}

#[test]
fn раны_лечатся_за_энергию_после_паузы() {
    let mut v = parent();
    v.health = 20.0;
    v.peaceful_ticks = 0;
    v.step(&life_core::senses::Blind);
    assert_eq!(v.health, 20.0);
    v.peaceful_ticks = 59;
    let e = v.energy;
    v.step(&life_core::senses::Blind);
    assert!((v.health - 20.08).abs() < 1e-9);
    let upkeep = if v.mind.social.activity == life_core::social::Activity::Resting {
        v.pheno.slow_upkeep
    } else {
        v.pheno.upkeep
    };
    assert!((e - v.energy - upkeep - 0.08).abs() < 1e-9);
}

#[test]
fn пищевые_крайности_имеют_компромисс() {
    let r = Rules::default();
    let mut herb = parent();
    herb.genome = herb.genome.with(Gene::Carnivory, 0.0);
    herb.apply_rules(&r, &Space::default());
    herb.energy = 0.0;
    let mut meat = herb.clone();
    meat.genome = meat.genome.with(Gene::Carnivory, 100.0);
    meat.apply_rules(&r, &Space::default());
    herb.feed(1, &r);
    meat.feed(1, &r);
    assert!((meat.energy / herb.energy - 0.2).abs() < 1e-9);
    herb.energy = 0.0;
    meat.energy = 0.0;
    herb.devour(20.0, &r);
    meat.devour(20.0, &r);
    assert_eq!(herb.energy, 4.0);
    assert_eq!(meat.energy, 20.0);
}

#[test]
fn стая_защищает_неродных_и_исчезает_без_участников() {
    use life_core::{World, WorldConfig};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    let a = w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(200.0));
    w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, None);
    w.creatures[1].flock = a;
    w.step();
    assert_eq!(w.creatures.len(), 2);
    assert_eq!(w.counters.combat, 0);
    assert!(w.creatures.iter().all(|v| v.health == v.max_health() && v.flock_goal.is_some()));
    assert_eq!(w.flocks[&a].members, 2);
    for v in &mut w.creatures {
        v.age = life_core::config::LIFESPAN;
    }
    w.step();
    assert!(w.creatures.is_empty() && w.flocks.is_empty());
    assert_eq!(w.counters.old_age, 2);
}

#[test]
fn метка_наследуется_с_редким_отделением() {
    let mut p = parent();
    p.flock = 17;
    let mut same = 0;
    let mut split = 0;
    for _ in 0..1000 {
        p.energy = p.pheno.max_energy;
        p.reproduction_wait = 0;
        let c = p.maybe_divide(&Space::default(), &Rules::default()).unwrap();
        if c.flock == 17 {
            same += 1;
        } else {
            assert_eq!(c.flock, 0);
            split += 1;
        }
    }
    assert!(same > 950 && split > 0 && split < 30);
}

#[test]
fn границы_новых_генов_сохраняются_при_мутации() {
    let mut g = CreatureGenome::BASE;
    let mut rng = Rng::new(42);
    for _ in 0..10000 {
        g = g.mutate(0.8, &mut rng);
        assert!((0.5..=2.0).contains(&g[Gene::LifePace]));
        assert!((1.0..=5.0).contains(&g[Gene::PreyRatio]));
        assert!((0.0..=100.0).contains(&g[Gene::Bravery]));
        assert!((0.0..=100.0).contains(&g[Gene::Carnivory]));
    }
}

#[test]
fn охота_выбирает_добычу_но_сытый_не_начинает() {
    use life_core::{World, WorldConfig};
    for (energy, ratio, expect) in [(100.0, 2.5, true), (250.0, 2.5, false), (100.0, 5.0, false)] {
        let mut w = World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: Rules::default().with("cannibalism", 1.0).unwrap().with("plant_rate", 0.0).unwrap(),
            ..Default::default()
        });
        w.spawn(
            CreatureGenome::BASE.with(Gene::Size, 100.0).with(Gene::PreyRatio, ratio),
            1000.0,
            1000.0,
            Some(energy),
        );
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1200.0, 1000.0, Some(50.0));
        w.step();
        assert_eq!(w.creatures[0].mind.attack == Some(prey), expect);
        if expect {
            assert!(w.creatures[0].x > 1000.0);
        }
    }
}

#[test]
fn близкое_растение_выгоднее_далёкой_добычи() {
    use life_core::{World, WorldConfig};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(100.0));
    w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1200.0, 1000.0, Some(50.0));
    w.plants.push(life_core::plant::Plant { x: 1000.0, y: 1000.0, alive: true, born: 0 });
    w.step();
    assert_eq!(w.creatures[0].mind.attack, None);
    assert_eq!(w.counters.plants_eaten, 1);
}

#[test]
fn цель_стаи_обновляется_а_одиночке_не_навязывается() {
    use life_core::{World, WorldConfig};
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    let id = w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
    w.step();
    assert!(w.creatures[0].flock_goal.is_none());
    w.spawn(CreatureGenome::BASE, 1100.0, 1000.0, None);
    w.creatures[1].flock = id;
    w.step();
    let first = w.flocks[&id].goal;
    w.flocks.get_mut(&id).unwrap().remaining = 1;
    w.step();
    let next = w.flocks[&id].goal;
    assert_ne!((first.tx, first.ty), (next.tx, next.ty));
    assert!((0.0..=w.space.width).contains(&next.tx));
    assert!((w.creatures[0].pheno.layer_lo..=w.creatures[0].pheno.layer_hi).contains(&next.ty));
}

#[test]
fn новые_состояния_конечны_и_смерти_сходятся() {
    use life_core::{World, WorldConfig};
    for seed in 1..=3 {
        let mut w = World::new(&WorldConfig {
            seed,
            rules: Rules::default().with("cannibalism", 1.0).unwrap(),
            ..Default::default()
        });
        let start = w.creatures.len() as u64;
        for _ in 0..2000 {
            w.step();
            for v in &w.creatures {
                assert!(v.alive && v.death.is_none());
                assert!(v.health.is_finite() && v.health > 0.0 && v.health <= v.max_health() + 1e-8);
                assert!(v.energy.is_finite() && v.energy > 0.0 && v.energy <= v.pheno.max_energy + 1e-8);
                assert!(v.age.is_finite() && v.pheno.size <= v.genome[Gene::Size]);
            }
            let c = w.counters;
            assert_eq!(w.creatures.len() as u64, start + c.born - c.starved - c.old_age - c.combat);
        }
    }
}

#[test]
fn погибший_не_размножается() {
    let mut p = parent();
    p.alive = false;
    let energy = p.energy;
    assert!(p.maybe_divide(&Space::default(), &Rules::default()).is_none());
    assert_eq!(p.energy, energy);
}
