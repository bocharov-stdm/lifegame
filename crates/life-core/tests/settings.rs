//! The settings that took over what genes and the world did until `life-behavior/14`: without its
//! setting a program does not divide, heal, eat on the move, shoot, smooth its step or hold a
//! layer; with it, it does as the old gene or rule did, by its numbers.

use life_core::creature::strategy::Shoot;
use life_core::creature::{Action, Block, Cond, Creature, Program, Programs, Strategy, Test};
use life_core::plant::Plant;
use life_core::rng::Rng;
use life_core::senses::Blind;
use life_core::{CreatureGenome, Rules, Space, World, WorldConfig};

const BASE: CreatureGenome = CreatureGenome::BASE;

/// A world without creatures, where no plant grows by itself (a world starts without plants).
fn world() -> World {
    let rules = Rules::default().with("plant_rate", 0.0).unwrap();
    World::new(&WorldConfig { n_creatures: Some(0), rules, ..Default::default() })
}

/// A base creature (size 40, a store of 100) at (1000, y) with `energy`.
fn creature(y: f64, energy: f64) -> Creature {
    Creature::new(
        &Space::default(),
        &Rules::default(),
        BASE,
        Some(1000.0),
        Some(y),
        Some(energy),
        Rng::new(1),
    )
}

fn living_by(v: &mut Creature, blocks: &[Block]) {
    v.programs = Programs::both(Program::of(blocks));
}

#[test]
fn only_a_program_that_divides_divides_and_from_its_share() {
    for (divide, energy, born) in [
        (None, 100.0, 0),
        (Some(Block::does(Action::Divide)), 100.0, 1),
        // from 95% of the tank with the reserve on top: 90 is not enough
        (Some(Block::does(Action::Divide).with(0, 95)), 90.0, 0),
    ] {
        let mut w = world();
        w.spawn(BASE, 1000.0, 1000.0, Some(energy));
        let blocks: Vec<Block> = divide.into_iter().chain([Block::does(Action::Ambush)]).collect();
        living_by(&mut w.creatures[0], &blocks);
        w.creatures[0].reproduction_wait = 0;
        w.step();
        assert_eq!(w.counters.born, born, "{blocks:?} at {energy}");
    }
}

/// Healing starts the tick after its setting applied, and pays from the tank.
#[test]
fn only_a_program_that_heals_heals() {
    for heals in [true, false] {
        let mut v = creature(1000.0, 100.0);
        let blocks: Vec<Block> = heals
            .then(|| Block::does(Action::Heal))
            .into_iter()
            .chain([Block::does(Action::Ambush)])
            .collect();
        living_by(&mut v, &blocks);
        v.health = 20.0;
        v.step(&Blind);
        assert_eq!(v.health, 20.0, "the setting applies from the next tick");
        let energy = v.energy;
        v.step(&Blind);
        assert_eq!(v.health > 20.0, heals);
        let paid = energy - v.energy - v.pheno.still_upkeep;
        assert!((paid - (v.health - 20.0)).abs() < 1e-9, "a unit of health for a unit of energy: {paid}");
    }
}

/// Without «есть на ходу» it eats only the food its deciding block goes for; with it, what it
/// touches while its tank is no fuller than the setting says.
#[test]
fn without_grazing_it_eats_only_what_its_block_goes_for() {
    let graze = Block::does(Action::Graze);
    let ambush = Block::does(Action::Ambush);
    for (blocks, energy, eats) in [
        (vec![ambush], 40.0, false),
        (vec![Block::does(Action::EatPlant)], 40.0, true),
        (vec![graze, ambush], 40.0, true),
        (vec![graze.with(0, 0), ambush], 40.0, false),
        (vec![graze.with(2, 50), ambush], 80.0, false),
        (vec![graze.with(2, 50), Block::does(Action::EatPlant)], 80.0, true),
    ] {
        let mut w = world();
        w.spawn(BASE, 1000.0, 1000.0, Some(energy));
        living_by(&mut w.creatures[0], &blocks);
        w.plants.push(Plant::at(1000.0, 1000.0));
        w.step();
        assert_eq!(w.counters.plant_bites > 0, eats, "{blocks:?} at {energy}");
    }
}

/// A layer setting moves the home band where it wanders; the first layer setting that applies
/// wins, so a conditional one switches the layer; without one the whole depth.
#[test]
fn a_layer_setting_moves_the_band_and_a_conditional_one_switches_it() {
    let mut v = creature(500.0, 100.0);
    living_by(&mut v, &[Block::does(Action::Layer).with(0, 50).with(1, 60), Block::does(Action::Wander)]);
    for _ in 0..400 {
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
    }
    let (lo, hi) = v.pheno.band((0.5, 0.6));
    assert!((lo..=hi).contains(&v.y), "it walked down into its layer: {}", v.y);

    let hungry_up = Block::when(Test::at(Cond::Fullness, 50).not(), Action::Layer).with(0, 0).with(1, 10);
    let mut v = creature(1000.0, 30.0);
    living_by(
        &mut v,
        &[hungry_up, Block::does(Action::Layer).with(0, 50).with(1, 60), Block::does(Action::Ambush)],
    );
    v.step(&Blind);
    assert_eq!(v.mind.stance.layer, (0.0, 0.1), "hungry: the first applies");
    v.energy = 90.0;
    v.step(&Blind);
    assert_eq!(v.mind.stance.layer, (0.5, 0.6), "fed: the second");
    living_by(&mut v, &[Block::does(Action::Ambush)]);
    v.step(&Blind);
    assert_eq!(v.mind.stance.layer, (0.0, 1.0), "none: the whole depth");
    // a reversed layer is swapped
    living_by(&mut v, &[Block::does(Action::Layer).with(0, 70).with(1, 20), Block::does(Action::Ambush)]);
    v.step(&Blind);
    assert_eq!(v.mind.stance.layer, (0.2, 0.7));
}

/// A wander target left in the old layer is dropped the tick the layer changes: the creature turns
/// back to its new layer at once instead of first walking to the old target.
#[test]
fn a_new_layer_drops_the_wander_target_of_the_old_one() {
    let mut v = creature(1000.0, 100.0);
    living_by(&mut v, &[Block::does(Action::Layer).with(0, 0).with(1, 40), Block::does(Action::Wander)]);
    v.step(&Blind);
    // walking up within its old layer, 0–40% of the depth
    v.mind.target = Some((1000.0, 500.0));
    living_by(&mut v, &[Block::does(Action::Layer).with(0, 60).with(1, 100), Block::does(Action::Wander)]);
    let y = v.y;
    v.step(&Blind);
    let (lo, _) = v.pheno.band((0.6, 1.0));
    assert!(v.y > y, "it turned down to its new layer: {y} → {}", v.y);
    assert_eq!(v.mind.target, Some((v.x, lo)), "the new target is the nearest point of the new layer");
}

/// A target picked after eating belongs to the layer it was eaten in: a creature that ate in a
/// temporary layer (a hungry dive to the bottom) drops it once back in its own, instead of walking to
/// the edge of its band nearest to it.
#[test]
fn a_target_picked_after_eating_belongs_to_the_layer_it_ate_in() {
    let home = [Block::does(Action::Layer).with(0, 0).with(1, 40), Block::does(Action::Wander)];
    let mut v = creature(1000.0, 100.0);
    living_by(&mut v, &home);
    v.step(&Blind);
    assert_eq!(v.mind.target_layer, (0.0, 0.4));
    // this tick's settings took it to the bottom, and it ate there
    v.mind.stance.layer = (0.6, 1.0);
    v.feed(&Rules::default());
    assert_eq!(v.mind.target_layer, (0.6, 1.0), "the target is the bottom layer's");
    let (x, _) = v.mind.target.expect("a target after eating");
    v.step(&Blind);
    let (lo, hi) = v.pheno.band((0.0, 0.4));
    let (tx, ty) = v.mind.target.expect("a wander target");
    assert!((lo..=hi).contains(&ty), "a target in its own band");
    assert_ne!((tx, ty), (x, hi), "it dropped the bottom's target, not kept it at the band's edge");
}

/// The band's margin is the body: a creature that grows sees its band narrow, but its layer is the
/// same, so the wander target it walks to stays — only brought within the band and the bounds the
/// grown body reaches (their margin is the body too). A calm walk keeps to the band, so a target
/// left past it would hold the creature standing; with the target in the band it walks on.
#[test]
fn growing_keeps_the_wander_target() {
    for smooth in [false, true] {
        let mut v = creature(1000.0, 100.0);
        let mut blocks =
            vec![Block::does(Action::Layer).with(0, 20).with(1, 60), Block::does(Action::Wander)];
        if smooth {
            blocks.insert(0, Block::does(Action::Smooth));
        }
        living_by(&mut v, &blocks);
        v.step(&Blind);
        let (lo, _) = v.pheno.band((0.2, 0.6));
        let target = (1000.0, lo);
        v.mind.target = Some(target);
        let grown = v.pheno.size * 4.0;
        v.pheno =
            life_core::creature::Phenotype::at_size(&v.genome, &Rules::default(), &Space::default(), grown);
        let (top, _) = v.pheno.band((0.2, 0.6));
        assert!(top - lo > v.pheno.speed, "the band's top moved down past the target, more than a step");
        // inside the grown band right under the target, heading for it
        (v.x, v.y) = (1000.0, top + 1.0);
        v.mind.heading = Some((0.0, -1.0));
        let mut reached = false;
        for _ in 0..60 {
            v.energy = v.pheno.max_energy;
            v.step(&Blind);
            // neither the old target nor it brought to the band's top
            reached |= v.mind.target.is_some_and(|t| t != target && t != (1000.0, top));
        }
        assert!(reached, "smooth {smooth}: it reached its target and drew a new one");
    }
}

/// A target the body reached when small may lie past the bounds of the grown one (the margin is the
/// body): it is brought within them, so the creature does not stand at the wall short of it.
#[test]
fn a_grown_body_at_a_wall_does_not_stand_short_of_its_target() {
    let mut v = creature(1000.0, 100.0);
    living_by(&mut v, &[Block::does(Action::Wander)]);
    v.step(&Blind);
    // a body of 20 reaches x 20 and walked to 25; grown to 40 it stands at the wall, x 40
    v.pheno = life_core::creature::Phenotype::at_size(&v.genome, &Rules::default(), &Space::default(), 40.0);
    (v.x, v.y) = (40.0, 1000.0);
    v.mind.target = Some((25.0, 1000.0));
    let start = (v.x, v.y);
    for _ in 0..5 {
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
    }
    assert_ne!((v.x, v.y), start, "it wandered on");
}

/// A parent defends its young child that met a hunter: its template goes for the hunter; a child
/// it no longer knows, it leaves.
#[test]
fn a_parent_defends_its_young_child_that_met_a_threat() {
    use life_core::creature::{Diet, Sighting};
    use life_core::genome::creature::Gene;
    for knows in [true, false] {
        let mut w = world();
        let parent = w.spawn(BASE, 1000.0, 1000.0, Some(90.0));
        w.creatures[0].mind.stance.spare = if knows { 1.0 } else { 0.0 }; // as its last tick set it
        let child = w.spawn(BASE, 1060.0, 1000.0, Some(20.0));
        let hunter = BASE.with(Gene::Size, 100.0).with(Gene::Diet, Diet::Carnivore as usize as f64);
        let enemy = w.spawn(hunter, 1160.0, 1000.0, Some(200.0));
        {
            let young = &mut w.creatures[1];
            young.parent = parent;
            young.pheno = life_core::creature::Phenotype::at_size(&young.genome, &w.rules, &w.space, 20.0);
            young.mind.alarm = Some(Sighting { enemy, x: 1160.0, y: 1000.0, tick: 0 });
        }
        // the hunter stands still: it hunts nobody, so the parent does not run from it
        w.creatures[2].programs = Programs::both(Program::of(&[Block::does(Action::Ambush)]));
        w.step();
        let p = &w.creatures[0];
        assert_eq!(p.mind.aid.map(|a| (a.victim, a.enemy)), knows.then_some((child, enemy)), "knows {knows}");
        assert_eq!(p.mind.attack == Some(enemy), knows);
    }
}

/// Only a program with «стрелять» arms itself; a founder that shoots gets it after its settings.
#[test]
fn only_a_program_that_shoots_is_armed() {
    let mut v = creature(1000.0, 100.0);
    v.step(&Blind);
    assert_eq!(v.mind.stance.shoot, None, "the template does not shoot");
    let shooter = Program::founder(Strategy::Standard, (5, 100), true);
    assert!(shooter.shoots() && !Program::STANDARD.shoots());
    let at = shooter.blocks().iter().position(|b| b.action == Action::Shoot).unwrap();
    assert!(shooter.blocks()[..at].iter().all(|b| b.action.is_setting()));
    assert!(!shooter.blocks()[at + 1].action.is_setting(), "right after the other settings");
    v.programs = Programs::both(shooter);
    v.step(&Blind);
    assert_eq!(v.mind.stance.shoot, Some(Shoot { from: 0.5, keep: 0.5 }), "the old genes' bases");
}

/// Without «плавный ход» a calm step goes straight for its target and holds no course; with it, it
/// holds one.
#[test]
fn only_a_smooth_program_holds_a_course() {
    for smooth in [false, true] {
        let mut v = creature(1000.0, 60.0);
        let blocks: Vec<Block> = smooth
            .then(|| Block::does(Action::Smooth))
            .into_iter()
            .chain([Block::does(Action::Wander)])
            .collect();
        living_by(&mut v, &blocks);
        for _ in 0..5 {
            v.step(&Blind);
        }
        assert_eq!(v.mind.course.is_some(), smooth);
    }
}
