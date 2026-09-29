//! Regressions of the life cycle.
use life_core::corpse::Stage;
use life_core::creature::strategy::Divide;
use life_core::creature::{Action, Block, Creature, Diet, Phenotype, Program};
use life_core::genome::creature::Gene;
use life_core::rng::Rng;
use life_core::{CreatureGenome, Rules, Space};

fn with_diet(g: CreatureGenome, diet: Diet) -> CreatureGenome {
    g.with(Gene::Diet, diet as usize as f64)
}

/// A full grown creature whose program divides from 30% of its tank, giving the template's 40%.
fn parent() -> Creature {
    let mut v = Creature::new(
        &Space::default(),
        &Rules::default(),
        CreatureGenome::BASE,
        Some(1000.0),
        Some(1000.0),
        Some(100.0),
        Rng::new(7),
    );
    v.reproduction_wait = 0;
    v.mind.stance.divide = Some(Divide { tank: 0.3, share: 0.4 });
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
    // the `maturation` share goes into growth, the rest into the tank
    let cost = 2.0 * child.pheno.maturation;
    assert!((child.pheno.size - size - cost / life_core::config::GROWTH_ENERGY_PER_SIZE).abs() < 1e-9);
    assert!((child.energy - energy - (2.0 - cost)).abs() < 1e-9);
    let mut quick = child.clone();
    quick.genome = quick.genome.with(Gene::Maturation, 80.0);
    quick.apply_rules(&r, &Space::default());
    let (size, energy) = (quick.pheno.size, quick.energy);
    quick.nourish(2.0, &r);
    assert!((quick.pheno.size - size - 1.6 / life_core::config::GROWTH_ENERGY_PER_SIZE).abs() < 1e-9);
    assert!((quick.energy - energy - 0.4).abs() < 1e-9, "a quick grower fills its tank slower");
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
    assert_eq!(Block::does(Action::Divide).arg(1), 0.4, "the template gives 40%");
    assert_eq!(child.energy, 40.0);
    assert_eq!(p.energy, 50.0);

    let mut p = parent();
    p.mind.stance.divide = Some(Divide { tank: 0.3, share: 0.8 });
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
    let parent_id = w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(20.0));
    w.creatures[0].reproduction_wait = 1000;
    w.creatures[1].parent = parent_id;
    w.creatures[1].birth_size = 20.0;
    w.creatures[1].pheno = Phenotype::at_size(&w.creatures[1].genome, &w.rules, &w.space, 20.0);
    w.step();
    assert_eq!(w.creatures[1].pheno.size, 20.0);
    assert!(w.creatures[1].energy < 20.0);
}

/// Growth is limited by the gene, not by the wall it stands at: the body grows and is pushed
/// inside its new bounds.
#[test]
fn growth_at_a_wall_is_not_stopped_by_it() {
    let r = Rules::default();
    let mut child = parent().maybe_divide(&Space::default(), &r).unwrap();
    child.x = child.pheno.size;
    let y = child.y;
    child.nourish(10000.0, &r);
    assert!(child.adult(), "grown to its gene at the wall");
    assert_eq!((child.x, child.y), (child.pheno.x_lo, y), "pushed inside, along the wall's normal only");
    let size = child.pheno.size;
    child.apply_rules(&r, &Space::default());
    assert_eq!(child.pheno.size, size);
}

/// A tick is a tick of age for everyone. From 70% of its lifespan a creature weakens linearly, and
/// from 90% its speed, vision, strike and health are 70% of what they were; its body and tank stay.
/// It dies at its lifespan, which the gene sets within 500–10 000 ticks.
#[test]
fn old_age_weakens_and_the_lifespan_is_inherited() {
    use life_core::creature::{Death, vigour};
    use life_core::senses::Blind;
    let r = Rules::default();
    let mut v = parent();
    assert_eq!(v.pheno.lifespan, life_core::config::LIFESPAN_BASE);
    v.step(&Blind);
    assert_eq!(v.age, 1.0);
    assert_eq!((vigour(2099.0, 3000.0), vigour(2100.0, 3000.0)), (1.0, 1.0));
    assert!((vigour(2400.0, 3000.0) - 0.85).abs() < 1e-12);
    assert!((vigour(2700.0, 3000.0) - 0.7).abs() < 1e-12);
    assert!((vigour(2999.0, 3000.0) - 0.7).abs() < 1e-12);

    let (young, health) = (v.pheno, v.max_health());
    v.grow_old(&r);
    assert_eq!(v.pheno, young, "nothing changes before old age");
    v.age = 2700.0;
    v.grow_old(&r);
    let old = v.pheno;
    assert!((old.speed - young.speed * 0.7).abs() < 1e-9);
    assert!((old.vision - young.vision * 0.7).abs() < 1e-9);
    assert!((old.strike() - young.strike() * 0.7).abs() < 1e-9);
    assert!((v.max_health() - health * 0.7).abs() < 1e-9);
    assert!(v.health <= v.max_health());
    assert_eq!(
        (old.size, old.max_energy, old.strike_cost()),
        (young.size, young.max_energy, young.strike_cost())
    );
    assert!(old.upkeep < young.upkeep, "it pays for the speed and sight it has");
    v.apply_rules(&r, &Space::default());
    assert_eq!(v.pheno, old, "new rules keep its age");

    v.age = v.pheno.lifespan - 1.0;
    v.step(&Blind);
    assert_eq!(v.death, Some(Death::OldAge));
    assert!(!v.alive);

    let space = Space::default();
    let lifespan =
        |years: f64| Phenotype::of(&CreatureGenome::BASE.with(Gene::Lifespan, years), &r, &space).lifespan;
    assert_eq!((lifespan(50_000.0), lifespan(10.0), lifespan(4000.0)), (10_000.0, 500.0, 4000.0));
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
    let (e, x, y) = (v.energy, v.x, v.y);
    // warm-blooded, it pays the same at any depth
    let cheaper = v.pheno.temper(v.y).1;
    assert_eq!(cheaper, 1.0);
    // 0.2% of its health a tick: 60 for a hardy herbivore of size 40
    let heal = v.max_health() * 0.002;
    assert!((heal - 0.12).abs() < 1e-12);
    v.step(&life_core::senses::Blind);
    assert!((v.health - 20.0 - heal).abs() < 1e-9);
    // it pays for the step it took
    let upkeep = v.pheno.step_cost((v.x - x).hypot(v.y - y));
    assert!((e - v.energy - upkeep * cheaper - heal).abs() < 1e-9);
}

/// The diet table: a specialist digests its own food fully, the omnivore everything but bones and
/// worse; rot feeds well only the scavenger, bones nobody else.
#[test]
fn диеты_усваивают_по_таблице() {
    let r = Rules::default();
    let plant_bite = r.plant_energy * r.plant_bite_yield / 5.0;
    for (diet, plants, fresh, rot, bones) in [
        (Diet::Herbivore, 1.0, 0.0, 0.0, 0.0),
        (Diet::Omnivore, 0.8, 0.6, 0.2, 0.0),
        (Diet::Carnivore, 0.2, 1.0, 0.3, 0.0),
        (Diet::Scavenger, 0.15, 1.0, 0.9, 0.9),
    ] {
        let mut v = parent();
        v.genome = with_diet(v.genome, diet);
        v.apply_rules(&r, &Space::default());
        assert_eq!(v.pheno.diet, diet);
        v.energy = 0.0;
        v.feed(&r);
        assert!((v.energy - plant_bite * plants).abs() < 1e-9, "{diet:?}: plants {}", v.energy);
        // hungry, it eats whatever it digests
        assert_eq!(v.pheno.corpse_efficiency(Stage::Fresh, true), fresh, "{diet:?}: fresh meat");
        assert_eq!(v.pheno.corpse_efficiency(Stage::Rot, true), rot, "{diet:?}: rot");
        assert_eq!(v.pheno.corpse_efficiency(Stage::Bones, true), bones, "{diet:?}: bones");
        assert_eq!(v.pheno.eats_plants(), plants > 0.0);
        assert_eq!(v.pheno.hunts(), fresh > 0.0);
        assert_eq!(
            v.pheno.strike_cost(),
            v.pheno.size * r.melee_damage_share,
            "{diet:?}: the cost has no bonus"
        );
    }
    // meat eaters strike harder: herbivore < scavenger < omnivore < carnivore (the user's
    // calibration of 2026-09-29 put the omnivore above the scavenger)
    let bonus = |d: Diet| Rules::default().diets[d as usize].strike;
    assert_eq!(bonus(Diet::Herbivore), 1.0);
    assert!(bonus(Diet::Herbivore) < bonus(Diet::Scavenger));
    assert!(bonus(Diet::Scavenger) < bonus(Diet::Omnivore));
    assert!(bonus(Diet::Omnivore) < bonus(Diet::Carnivore));
}

/// The lab edits a diet's edges: every one of them reaches the phenotype, and only that diet's.
#[test]
fn diet_edges_follow_the_rules() {
    let space = Space::default();
    let r = Rules::default();
    let lab = [
        ("carnivore_strike", 6.0),
        ("carnivore_health", 2.0),
        ("carnivore_size_upkeep", 0.5),
        ("carnivore_speed_upkeep", 0.5),
        ("carnivore_smell", 3.0),
        ("carnivore_plants", 0.5),
        ("carnivore_meat", 0.6),
        ("carnivore_rot", 0.7),
        ("carnivore_young_plants", 0.8),
    ]
    .iter()
    .fold(r.clone(), |r, (k, v)| r.with(k, *v).unwrap());
    let of = |diet: Diet, rules: &Rules| {
        let mut v = parent();
        v.genome = with_diet(v.genome, diet);
        v.apply_rules(rules, &space);
        v.pheno
    };
    let (base, new) = (of(Diet::Carnivore, &r), of(Diet::Carnivore, &lab));
    assert_eq!(new.strike(), base.strike() * 2.0);
    assert_eq!(new.strike_cost(), base.strike_cost(), "the bonus is on damage only");
    assert_eq!((new.health_bonus, new.smell), (2.0, 3.0 * new.vision));
    assert_eq!((new.plant_efficiency, new.meat_efficiency, new.rot_efficiency), (0.5, 0.6, 0.7));
    let young = Phenotype::at_size(&with_diet(parent().genome, Diet::Carnivore), &lab, &space, 10.0);
    assert_eq!(young.plant_efficiency, 0.8, "the juvenile gut is a rule too");
    assert_eq!(new.upkeep, lab.upkeep_diet(new.size, new.speed, new.vision, [0.5, 0.5]));
    assert!(new.upkeep < base.upkeep);
    for diet in [Diet::Herbivore, Diet::Omnivore, Diet::Scavenger] {
        assert_eq!(of(diet, &lab), of(diet, &r), "{diet:?} keeps its edges");
    }
}

/// Every diet has a juvenile gut (the user's calibration of 2026-09-29): until it reaches its own
/// size it digests plants at `young_plants` (100/100/70/70%), then at its grown share — a carnivore
/// at 20% and must hunt. The juvenile gut is a floor under the grown `plants` rule, so a lab change
/// of that rule reaches the young too, and with the floor at 0 a young scavenger at
/// `scavenger_plants=0` leaves plants alone.
#[test]
fn a_young_carnivore_grows_on_plants() {
    let space = Space::default();
    let plants = |r: &Rules, diet: Diet, share: f64| {
        let g = with_diet(parent().genome, diet);
        Phenotype::at_size(&g, r, &space, g[Gene::Size] * share).plant_efficiency
    };
    let r = Rules::default();
    for (diet, young, grown) in [
        (Diet::Herbivore, 1.0, 1.0),
        (Diet::Omnivore, 1.0, 0.8),
        (Diet::Scavenger, 0.7, 0.15),
        (Diet::Carnivore, 0.7, 0.2),
    ] {
        assert_eq!((plants(&r, diet, 0.5), plants(&r, diet, 1.0)), (young, grown), "{diet:?}");
    }
    let c = with_diet(parent().genome, Diet::Carnivore);
    let almost = Phenotype::at_size(&c, &r, &space, c[Gene::Size] - 0.01);
    assert_eq!((almost.plant_efficiency, Phenotype::of(&c, &r, &space).plant_efficiency), (0.7, 0.2));
    // the lab's `plants` reaches the young
    let lab = r
        .with("scavenger_plants", 0.0)
        .unwrap()
        .with("scavenger_young_plants", 0.0)
        .unwrap()
        .with("herbivore_plants", 0.8)
        .unwrap()
        .with("herbivore_young_plants", 0.0)
        .unwrap()
        .with("carnivore_plants", 0.9)
        .unwrap();
    assert_eq!(plants(&lab, Diet::Scavenger, 0.5), 0.0, "a young scavenger leaves plants alone");
    assert_eq!(plants(&lab, Diet::Herbivore, 0.5), 0.8);
    assert_eq!(plants(&lab, Diet::Carnivore, 0.5), 0.9, "the floor never lowers the grown value");
    assert_eq!(plants(&lab.with("carnivore_young_plants", 0.0).unwrap(), Diet::Carnivore, 0.5), 0.9);
}

/// Without its program's «eat foreign food» setting (the template's, below 30% of the store) a
/// creature eats and goes only for its own food: the scavenger leaves fresh corpses to the hunters
/// and does not hunt, the carnivore leaves rot and bones to the scavengers. With it, anything it
/// digests. The omnivore has no foreign food (and cannot digest bones at all).
#[test]
fn only_its_own_food_without_the_foreign_setting() {
    let r = Rules::default();
    for (diet, own_fresh, own_rot, bones) in [
        (Diet::Omnivore, true, true, false),
        (Diet::Scavenger, false, true, true),
        (Diet::Carnivore, true, false, false),
    ] {
        let mut v = parent();
        v.genome = with_diet(v.genome, diet);
        v.apply_rules(&r, &Space::default());
        for (stage, own) in [(Stage::Fresh, own_fresh), (Stage::Rot, own_rot), (Stage::Bones, bones)] {
            assert_eq!(v.pheno.corpse_efficiency(stage, false) > 0.0, own, "{diet:?} on its own {stage:?}");
        }
        assert!(v.pheno.corpse_efficiency(Stage::Fresh, true) > 0.0, "{diet:?} hungry on fresh");
        assert!(v.pheno.corpse_efficiency(Stage::Rot, true) > 0.0, "{diet:?} hungry on rot");
        assert_eq!(
            v.pheno.corpse_efficiency(Stage::Bones, true) > 0.0,
            bones,
            "{diet:?}: only one digests bones"
        );
        assert_eq!(v.pheno.hunts_now(false), own_fresh, "{diet:?}: hunts on its own food");
        assert!(v.pheno.hunts_now(true), "{diet:?}: hunts when foreign food is allowed");
        assert!(v.pheno.hunts(), "{diet:?}: feared either way");
    }
}

/// Each diet has an edge of its own besides the strike: the herbivore is hardy and carries its
/// size cheaper, the carnivore runs cheaper and smells corpses half as far again as it sees, the
/// scavenger smells them from three times as far. Only the named term of upkeep changes.
#[test]
fn бонусы_диет() {
    let r = Rules::default();
    let space = Space::default();
    let of = |diet: Diet| {
        let mut v = parent();
        v.genome = with_diet(v.genome, diet);
        v.apply_rules(&r, &space);
        v
    };
    let (h, o, s, c) = (of(Diet::Herbivore), of(Diet::Omnivore), of(Diet::Scavenger), of(Diet::Carnivore));
    let (size, speed, vision) = (o.pheno.size, o.pheno.speed, o.pheno.vision);
    assert_eq!(o.pheno.upkeep, r.upkeep(size, speed, vision), "the omnivore pays the base");
    assert_eq!(h.max_health(), 1.5 * o.max_health(), "hardy herbivore");
    assert_eq!(c.max_health(), o.max_health());
    let size_term = r.upkeep(size, speed, vision) - r.upkeep_diet(size, speed, vision, [0.0, 1.0]);
    assert!((o.pheno.upkeep - h.pheno.upkeep - 0.15 * size_term).abs() < 1e-12);
    let speed_term = r.upkeep(size, speed, vision) - r.upkeep_diet(size, speed, vision, [1.0, 0.0]);
    let saved = 1.0 - life_core::config::DIET_SPEED_COST[3];
    assert!((o.pheno.upkeep - c.pheno.upkeep - saved * speed_term).abs() < 1e-12);
    assert_eq!(s.pheno.upkeep, o.pheno.upkeep, "the scavenger pays the base");
    assert_eq!((s.pheno.smell, c.pheno.smell, o.pheno.smell), (3.0 * vision, 1.5 * vision, 1.2 * vision));
}

/// Cold deep water: a cold-blooded body is slower and cheaper below the thermocline, by its gene
/// times the coldness; a warm-blooded one is the same everywhere. It saves, it never gains.
#[test]
fn a_cold_blooded_body_is_slower_and_cheaper_in_the_cold() {
    use life_core::config::{COLD_SAVING, COLD_SLOWING, THERMO_BOTTOM, THERMO_TOP};
    let (r, space) = (Rules::default(), Space::default());
    let body = |cold: f64| {
        let mut v = parent();
        v.genome = v.genome.with(Gene::ColdBlood, cold);
        v.apply_rules(&r, &space);
        v
    };
    let (warm, cold, half) = (body(0.0), body(100.0), body(50.0));
    let (top, bottom) = (space.height * THERMO_TOP / 100.0, space.height * THERMO_BOTTOM / 100.0);
    for v in [&warm, &cold, &half] {
        assert_eq!(v.pheno.temper(top), (1.0, 1.0), "warm water");
        assert_eq!(v.pheno.upkeep, warm.pheno.upkeep, "the gene is free");
    }
    assert_eq!(warm.pheno.temper(space.height), (1.0, 1.0), "warm-blooded");
    assert_eq!(cold.pheno.temper(bottom), (1.0 - COLD_SLOWING, 1.0 - COLD_SAVING));
    let (slower, cheaper) = half.pheno.temper(space.height);
    assert!(
        (slower - (1.0 - COLD_SLOWING / 2.0)).abs() < 1e-12
            && (cheaper - (1.0 - COLD_SAVING / 2.0)).abs() < 1e-12
    );
    assert!((cold.pheno.coldness((top + bottom) / 2.0) - 0.5).abs() < 1e-12, "a smooth step");
    // a step on the bottom: the cold-blooded one goes shorter, pays for that step, and half of it
    let step = |mut v: life_core::creature::Creature| {
        (v.x, v.y, v.energy) = (3000.0, space.height - 100.0, 50.0);
        v.step(&life_core::senses::Blind);
        ((v.x - 3000.0).hypot(v.y - (space.height - 100.0)), 50.0 - v.energy, v.pheno)
    };
    let (warm_moved, warm_paid, pheno) = step(warm.clone());
    let (cold_moved, cold_paid, _) = step(cold.clone());
    assert!((warm_paid - pheno.upkeep).abs() < 1e-9, "{warm_paid}");
    assert!((cold_paid - pheno.step_cost(cold_moved) * (1.0 - COLD_SAVING)).abs() < 1e-9, "{cold_paid}");
    assert!(warm_moved > 0.0 && (cold_moved - warm_moved * (1.0 - COLD_SLOWING)).abs() < 1e-9);
}

/// The reach setting (`Action::Reach`): how far beyond its layer a creature goes for food it sees.
/// It takes the best food within its reach, not the nearest dropped.
#[test]
fn the_reach_setting_limits_where_it_goes_for_food() {
    use life_core::senses::Taste;
    use life_core::{World, WorldConfig, plant::Plant};
    let space = Space::default();
    let (lo, hi, reach) = (space.height * 0.1, space.height * 0.3, space.height * 0.05);
    let p = Taste { foreign: false, reach, layer: (lo, hi) };
    assert!(p.admits(lo - reach + 1.0) && p.admits(hi + reach - 1.0));
    assert!(!p.admits(lo - reach - 1.0) && !p.admits(hi + reach + 1.0));
    let free = Taste { layer: (0.0, space.height), ..p };
    assert!(free.admits(space.height) && free.admits(0.0), "the whole depth its layer");
    assert!(Taste { reach: f64::INFINITY, ..p }.admits(space.height), "without the setting: anywhere");

    // on its layer's lower edge: the nearer plant is past its reach, the farther one inside it
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    w.spawn(CreatureGenome::BASE, 1000.0, hi, Some(40.0));
    let near = Program::of(&[
        Block::does(Action::Layer).with(0, 10).with(1, 30),
        Block::does(Action::Reach).with(0, 5),
        Block::does(Action::EatPlant),
        Block::does(Action::Wander),
    ]);
    w.creatures[0].programs = [near; 2].into();
    w.plants.push(Plant::at(1000.0, hi + reach + 100.0));
    w.plants.push(Plant::at(1000.0, hi - 200.0));
    w.step();
    assert_eq!(w.creatures[0].mind.personal_food, Some((1000.0, hi - 200.0)));
}

#[test]
fn границы_новых_генов_сохраняются_при_мутации() {
    let mut g = CreatureGenome::BASE;
    let mut rng = Rng::new(42);
    for _ in 0..10000 {
        g = g.mutate(0.8, &mut rng);
        assert!((0.0..=100.0).contains(&g[Gene::Maturation]));
        assert!((500.0..=10_000.0).contains(&g[Gene::Lifespan]));
        assert!((0..4).contains(&(g[Gene::Diet] as usize)) && g[Gene::Diet].fract() == 0.0);
    }
}

#[test]
fn охота_выбирает_добычу_но_сытый_не_начинает() {
    use life_core::{World, WorldConfig};
    // the ratio of the hunter's hunt block alone decides: there is no world floor under it
    for (energy, ratio, size, expect) in [
        (100.0, 2.5, 30.0, true),
        (250.0, 2.5, 30.0, false),
        (100.0, 5.0, 30.0, false),
        (100.0, 1.5, 50.0, true),
        (100.0, 2.5, 50.0, false),
    ] {
        let mut w = World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: Rules::default().with("plant_rate", 0.0).unwrap(),
            ..Default::default()
        });
        w.spawn(
            with_diet(CreatureGenome::BASE.with(Gene::Size, 100.0), Diet::Carnivore),
            1000.0,
            1000.0,
            Some(energy),
        );
        let hunt = Program::STANDARD.tuned(Action::Hunt, |b| b.args[0] = (ratio * 100.0) as u16);
        w.creatures[0].programs = [hunt; 2].into();
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, size), 1200.0, 1000.0, Some(50.0));
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
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    // an omnivore: it could take either
    w.spawn(
        with_diet(CreatureGenome::BASE.with(Gene::Size, 100.0), Diet::Omnivore),
        1000.0,
        1000.0,
        Some(100.0),
    );
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
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    for _ in 0..2 {
        w.spawn(with_diet(CreatureGenome::BASE, Diet::Carnivore), 1000.0, 1000.0, Some(30.0));
        w.creatures.last_mut().unwrap().reproduction_wait = 1000;
    }
    let mut corpse = Corpse::from_creature(&w.creatures[0], 0);
    corpse.owner = 99;
    // one bite of flesh is left: a spawned body is no meat, so the corpse is its tank of 30, 3 of
    // it the bones, which carnivores cannot eat
    corpse.remaining = 3.0 + 10.0;
    w.corpses.push(corpse);
    w.step();
    assert_eq!((w.counters.meat_bites, w.counters.bone_bites), (1, 0));
    assert!(w.corpses[0].skeleton.is_some() && (w.corpses[0].remaining - 3.0).abs() < 1e-9, "bones left");
    assert!(w.creatures[0].energy > w.creatures[1].energy);
}

#[test]
fn исчерпанный_труп_не_лишает_следующего_едока_доступного_растения() {
    use life_core::{World, WorldConfig, corpse::Corpse, plant::Plant};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    // omnivores: the fresh corpse is worth more to them than the plant
    for _ in 0..2 {
        w.spawn(with_diet(CreatureGenome::BASE, Diet::Omnivore), 1000.0, 1000.0, Some(30.0));
        w.creatures.last_mut().unwrap().reproduction_wait = 1000;
    }
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

/// One that loses its plant to an earlier id eats the corpse it touches instead, even a rotten
/// one it likes less; a herbivore beside it never touches the corpse.
#[test]
fn потерявший_растение_ест_труп_который_касается() {
    use life_core::{World, WorldConfig, corpse::Corpse, plant::Plant};
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    let herbivore = CreatureGenome::BASE;
    w.spawn(herbivore, 1000.0, 1000.0, Some(30.0));
    w.spawn(with_diet(herbivore, Diet::Omnivore), 1000.0, 1000.0, Some(30.0));
    w.spawn(herbivore, 1080.0, 1000.0, Some(30.0));
    for v in &mut w.creatures {
        v.reproduction_wait = 1000;
    }
    // long dead and fully rotten, but lying here: the omnivore prefers the plant
    let mut corpse = Corpse::from_creature(&w.creatures[0], 0);
    corpse.owner = 99;
    corpse.x = 1040.0;
    corpse.bottom = corpse.y0;
    corpse.remaining = 40.0;
    // its decay is counted up to now: what is left is the 40 above
    corpse.last_decay = 700;
    w.corpses.push(corpse);
    w.plants.push(Plant::at(1000.0, 1000.0));
    w.tick = 700;

    w.step();

    assert_eq!((w.counters.plant_bites, w.counters.meat_bites, w.counters.rot_bites), (1, 1, 1));
    assert_eq!(w.plants[0].portions, 4);
    use life_core::creature::Morsel;
    assert!(matches!(w.creatures[0].meal.map(|m| m.food), Some(Morsel::Plant)));
    assert!(matches!(w.creatures[1].meal.map(|m| m.food), Some(Morsel::Corpse { stage: Stage::Rot })));
    assert!(w.creatures[2].meal.is_none(), "the herbivore beside the corpse ate nothing");
}

#[test]
fn новые_состояния_конечны_и_смерти_сходятся() {
    use life_core::{World, WorldConfig};
    for seed in 1..=3 {
        let mut w = World::new(&WorldConfig { seed, ..Default::default() });
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

/// The corpse counters add up: every corpse that appeared was removed or still lies; every one
/// comes to bones before it goes (a short clock, so that many go within the run).
#[test]
fn счётчики_трупов_сходятся() {
    use life_core::{World, WorldConfig};
    let rules = Rules::default().with("corpse_decay", 600.0).unwrap().with("corpse_bones", 600.0).unwrap();
    let mut w = World::new(&WorldConfig { seed: 3, rules, ..Default::default() });
    for _ in 0..3000 {
        w.step();
    }
    let c = w.counters;
    assert_eq!(c.corpses, c.corpses_gone + w.corpses.len() as u64);
    assert!(c.corpses_gone > 100, "{c:?}");
    assert_eq!(c.skeletons, c.corpses_gone, "all came to bones: {c:?}");
    assert!(c.corpses_bottom <= c.corpses_gone);
    assert!(c.corpse_ticks >= c.corpses_gone, "a corpse lies at least a tick");
    assert!(w.corpses.iter().filter_map(|k| k.skeleton).all(|s| s.rest >= s.y1));
}

/// The lab's body prices: each term's price scales its term alone, the mass power can drop the
/// body's weight from the price of running; at 1 and 1 they keep config's bits.
#[test]
fn body_prices_scale_their_own_terms() {
    let base = Rules::default();
    let (size, speed, vision) = (80.0, 20.0, 400.0);
    let terms = |r: &Rules| {
        let all = r.upkeep(size, speed, vision);
        let no_size =
            r.upkeep(size, speed, vision) - r.with("size_cost", 0.0).unwrap().upkeep(size, speed, vision);
        (all, no_size)
    };
    let (all, size_term) = terms(&base);
    let doubled = base.with("size_cost", 2.0).unwrap();
    assert!(
        (doubled.upkeep(size, speed, vision) - (all + size_term)).abs() < 1e-12,
        "only the size term doubles"
    );
    let massless = base.with("speed_mass_power", 0.0).unwrap();
    let running = |r: &Rules| {
        r.upkeep(size, speed, vision) - r.with("speed_cost", 0.0).unwrap().upkeep(size, speed, vision)
    };
    assert!(
        (running(&base) / running(&massless) - 2.0).abs() < 1e-12,
        "twice the base size runs twice as dear"
    );
    let same = base.with("size_cost", 1.0).unwrap().with("speed_mass_power", 1.0).unwrap();
    assert_eq!(same.upkeep(size, speed, vision).to_bits(), all.to_bits());
}

/// Heredity from the rules: with every child a copy nothing mutates; with no copies and a certain
/// diet step every mutating child changes its diet; the mutability floor holds.
#[test]
fn heredity_follows_the_rules() {
    use life_core::genome::Heredity;
    let rules = Rules::default();
    let parent = CreatureGenome::BASE.with(Gene::Mutability, 0.0);
    let mut rng = Rng::new(3);
    let copies = Heredity::of(&rules.with("clone_share", 1.0).unwrap());
    assert!((0..200).all(|_| parent.mutate_by(&copies, &mut rng) == parent));
    let restless = Heredity::of(
        &rules
            .with("clone_share", 0.0)
            .unwrap()
            .with("diet_step", 1.0)
            .unwrap()
            .with("diet_meat_step", 1.0)
            .unwrap(),
    );
    for _ in 0..200 {
        let child = parent.mutate_by(&restless, &mut rng);
        assert_ne!(child[Gene::Diet], parent[Gene::Diet], "the diet always steps");
        assert!(child[Gene::Mutability] >= rules.min_mutability, "the floor holds");
    }
    assert_eq!(Heredity::of(&rules), Heredity::with_sigma(rules.mutation_sigma), "defaults are config's");
}

/// The herbivore's leaps past the omnivore are rules: at 0 it never leaps, whatever `diet_jump`
/// says (they replace its general jump), and a certain leap always lands where the rule says.
#[test]
fn herbivore_leaps_follow_the_rules() {
    use life_core::genome::Heredity;
    let base = Rules::default().with("clone_share", 0.0).unwrap().with("diet_step", 0.0).unwrap();
    let base = base.with("diet_meat_step", 0.0).unwrap();
    let herbivore = CreatureGenome::BASE;
    let mut rng = Rng::new(5);
    let children = |rules: &Rules, rng: &mut Rng| {
        let h = Heredity::of(rules);
        let mut seen = [0; 4];
        for _ in 0..20_000 {
            seen[herbivore.mutate_by(&h, rng)[Gene::Diet] as usize] += 1;
        }
        seen
    };
    let none = base.with("diet_leap_carnivore", 0.0).unwrap().with("diet_leap_scavenger", 0.0).unwrap();
    assert_eq!(children(&none.with("diet_jump", 1.0).unwrap(), &mut rng), [20_000, 0, 0, 0]);
    let to_carnivore = none.with("diet_leap_carnivore", 1.0).unwrap();
    assert_eq!(children(&to_carnivore, &mut rng), [0, 0, 0, 20_000]);
    let seen = children(&none.with("diet_leap_scavenger", 0.25).unwrap(), &mut rng);
    assert!(seen[1] == 0 && seen[3] == 0 && (4500..5500).contains(&seen[2]), "{seen:?}");
}

/// A corpse keeps the clock of the rules it died under.
#[test]
fn corpses_follow_the_rules_clock() {
    use life_core::corpse::{Corpse, CorpseClock};
    let rules = Rules::default()
        .with("corpse_fresh", 10.0)
        .unwrap()
        .with("corpse_decay", 20.0)
        .unwrap()
        .with("corpse_bones", 30.0)
        .unwrap()
        .with("corpse_bones_sink", 7.0)
        .unwrap();
    let v = parent();
    let mut c = Corpse::from_creature_in(&v, 0, CorpseClock::of(&rules));
    assert_eq!((c.stage(10), c.stage(11)), (Stage::Fresh, Stage::Rot), "rot by the rules, not by config");
    assert_eq!(Corpse::from_creature(&v, 0).stage(11), Stage::Fresh, "config's corpse is still fresh");
    assert!(c.decay(20) && c.stage(20) == Stage::Bones, "bones by the rules");
    assert_eq!(c.y_at(21) - c.y_at(20), 7.0, "sinking at the rules' speed");
    assert!(c.decay(49) && !c.decay(50), "gone by the rules");
}
