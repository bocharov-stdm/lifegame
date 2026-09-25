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
    assert!((child.pheno.size - size - cost / life_core::config::GROWTH_ENERGY_PER_SIZE).abs() < 1e-9);
    assert!((child.energy - energy - (2.0 - cost)).abs() < 1e-9);
    let size = child.pheno.size;
    child.nourish(0.0, &r);
    assert_eq!(child.pheno.size, size);
    child.nourish(10000.0, &r);
    assert!(child.adult());
}

#[test]
fn рождение_передаёт_энергию_из_резерва_без_потери_при_полном_баке_ребёнка() {
    let r = Rules::default().with("mutation_sigma", 0.0).unwrap();
    let mut p = parent();
    let child = p.maybe_divide(&Space::default(), &r).unwrap();
    assert_eq!(CreatureGenome::BASE[Gene::ReproShare], 40.0);
    assert_eq!(child.energy, 40.0);
    assert_eq!(p.energy, 50.0);

    let mut p = parent();
    p.genome = p.genome.with(Gene::ReproShare, 80.0);
    p.apply_rules(&r, &Space::default());
    let before = p.energy;
    let child = p.maybe_divide(&Space::default(), &r).unwrap();
    assert_eq!(child.energy, child.pheno.max_energy);
    assert!((before - p.energy - child.energy - r.repro_cost).abs() < 1e-9);
    assert!(p.energy >= life_core::config::REPRO_RESERVE);
}

#[test]
fn родитель_больше_не_кормит_подросшего_ребёнка_каждый_тик() {
    use life_core::{World, WorldConfig, creature::Phenotype};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    let parent_id = w.spawn(CreatureGenome::BASE.with(Gene::Care, 100.0), 1000.0, 1000.0, Some(100.0));
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(20.0));
    w.creatures[0].reproduction_wait = 1000;
    w.creatures[1].parent = parent_id;
    w.creatures[1].birth_size = 20.0;
    w.creatures[1].pheno = Phenotype::at_size(&w.creatures[1].genome, &w.rules, &w.space, 20.0);
    w.step();
    assert_eq!(w.creatures[1].pheno.size, 20.0);
    assert!(w.creatures[1].energy < 20.0);
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
    assert!(w.creatures.iter().all(|v| v.health == v.max_health() && v.circle.is_some()));
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
    w.plants.push(life_core::plant::Plant::at(1000.0, 1000.0));
    w.step();
    assert_eq!(w.creatures[0].mind.attack, None);
    assert_eq!(w.counters.plant_bites, 1);
    assert_eq!(w.counters.plants_eaten, 0);
    assert_eq!(w.plants[0].portions, 4);
}

#[test]
fn один_остаток_трупа_получает_едок_с_меньшим_id() {
    use life_core::{World, WorldConfig, corpse::Corpse};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    for _ in 0..2 {
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(30.0));
        w.creatures.last_mut().unwrap().reproduction_wait = 1000;
    }
    w.creatures[1].flock = w.creatures[0].flock;
    let mut corpse = Corpse::from_creature(&w.creatures[0], 0);
    corpse.owner = 99;
    corpse.remaining = 10.0;
    w.corpses.push(corpse);
    w.step();
    assert_eq!(w.counters.meat_bites, 1);
    assert_eq!(w.corpses[0].remaining, 0.0);
    assert!(w.creatures[0].energy > w.creatures[1].energy);
}

#[test]
fn исчерпанный_труп_не_лишает_следующего_едока_доступного_растения() {
    use life_core::{World, WorldConfig, corpse::Corpse, plant::Plant};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    for _ in 0..2 {
        w.spawn(CreatureGenome::BASE.with(Gene::Carnivory, 100.0), 1000.0, 1000.0, Some(30.0));
        w.creatures.last_mut().unwrap().reproduction_wait = 1000;
    }
    w.creatures[1].flock = w.creatures[0].flock;
    let mut corpse = Corpse::from_creature(&w.creatures[0], 0);
    corpse.owner = 99;
    corpse.remaining = 10.0;
    w.corpses.push(corpse);
    w.plants.push(Plant::at(1000.0, 1000.0));

    w.step();

    assert_eq!(w.counters.meat_bites, 1);
    assert_eq!(w.counters.plant_bites, 1);
    assert_eq!(w.plants[0].portions, 4);
    assert_eq!(w.creatures.len(), 2);
    assert!(w.creatures[0].energy > w.creatures[1].energy);
}

#[test]
fn после_чужого_укуса_растения_резерв_трупа_сохраняет_еду_следующему() {
    use life_core::{World, WorldConfig, corpse::Corpse, plant::Plant};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    });
    let plant_eater = CreatureGenome::BASE.with(Gene::Sociability, 0.0);
    let meat_eater = plant_eater.with(Gene::Carnivory, 100.0);
    w.spawn(plant_eater, 1000.0, 1000.0, Some(30.0));
    w.spawn(plant_eater, 1000.0, 1000.0, Some(30.0));
    w.spawn(meat_eater, 1080.0, 1000.0, Some(30.0));
    let flock = w.creatures[0].flock;
    for v in &mut w.creatures {
        v.flock = flock;
        v.reproduction_wait = 1000;
    }
    let mut corpse = Corpse::from_creature(&w.creatures[0], 0);
    corpse.owner = 99;
    corpse.x = 1040.0;
    corpse.remaining = 10.0;
    w.corpses.push(corpse);
    w.plants.push(Plant::at(1000.0, 1000.0));
    w.plants.push(Plant::at(1080.0, 1000.0));

    w.step();

    assert_eq!(w.counters.meat_bites, 1);
    assert_eq!(w.counters.plant_bites, 2);
    assert_eq!(w.plants.iter().map(|p| p.portions).collect::<Vec<_>>(), [4, 4]);
}

#[test]
fn a_pair_gets_a_family_circle_a_loner_none() {
    use life_core::{World, WorldConfig};
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    let id = w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
    w.step();
    assert!(w.creatures[0].circle.is_none());
    w.spawn(CreatureGenome::BASE, 1100.0, 1000.0, None);
    w.creatures[1].flock = id;
    w.step();
    let circle = w.flocks[&id].circle.expect("у пары есть круг");
    assert!(w.creatures.iter().all(|v| v.circle == Some(circle)));
    // A pair is a young family: its circle is at least as wide as the members see.
    let v = &w.creatures[0];
    let want = (v.pheno.flock_spacing * 2f64.sqrt())
        .max(v.pheno.vision)
        .clamp(life_core::flock::MIN_RADIUS, life_core::flock::MAX_RADIUS);
    assert!((circle.radius - want).abs() < 1e-9, "{} vs {want}", circle.radius);
    assert!(circle.x >= circle.radius && circle.x <= w.space.width - circle.radius);
    assert!((w.creatures[0].pheno.layer_lo..=w.creatures[0].pheno.layer_hi).contains(&circle.y));
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
