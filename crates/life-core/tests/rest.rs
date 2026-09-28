//! Resting, crowding and a newborn's first tick: what the loners kept of the flock tests when the
//! flocks went (tag `flocks-final`).

use life_core::creature::Activity;
use life_core::{CreatureGenome, Rules, World, WorldConfig, genome::creature::Gene};

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
    // standing, it pays only for its body and eyes
    let upkeep = w.creatures[0].pheno.still_upkeep;
    w.step();
    let v = &w.creatures[0];
    assert_eq!(v.mind.activity, Activity::Resting);
    assert_eq!((v.x, v.y), (1000.0, 1000.0));
    assert!((v.energy - (100.0 - upkeep)).abs() < 1e-9);
    assert_eq!(v.age, 1.0);
    w.creatures[0].energy = 60.0;
    w.step();
    assert_ne!(w.creatures[0].mind.activity, Activity::Resting);
}

/// The fullness it rests from is its rest block's test: from 60% a creature three quarters full
/// rests, with the template's 95% it does not; a second block keeps the rest down to 50%.
#[test]
fn the_rest_block_sets_the_fullness_to_rest_from() {
    use life_core::creature::{Action, Block, Cond, Program, Test};
    let early = Program::of(&[
        Block::when(Test::at(Cond::Fullness, 60), Action::Rest),
        Block::when2(Test::is(Cond::Resting), Test::at(Cond::Fullness, 50), Action::Rest),
        Block::does(Action::Wander),
    ]);
    for (program, rests) in [(early, true), (Program::STANDARD, false)] {
        let mut w = world();
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
        let v = &mut w.creatures[0];
        v.programs = [program; 2].into();
        v.reproduction_wait = 1000;
        v.energy = v.pheno.max_energy * 0.75;
        w.step();
        assert_eq!(w.creatures[0].mind.activity == Activity::Resting, rests, "{rests}");
    }
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, None);
    w.creatures[0].programs = [early; 2].into();
    w.creatures[0].reproduction_wait = 1000;
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.75;
    w.step();
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.55;
    w.step();
    assert_eq!(w.creatures[0].mind.activity, Activity::Resting, "rests on above 50%");
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.49;
    w.step();
    assert_ne!(w.creatures[0].mind.activity, Activity::Resting, "gives up below 50%");
}

#[test]
fn новый_отдых_не_начинается_сразу_после_старого() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.creatures[0].reproduction_wait = 1000;
    let mut rested = 0;
    for _ in 0..170 {
        w.creatures[0].energy = 100.0;
        w.step();
        rested += (w.creatures[0].mind.activity == Activity::Resting) as u32;
    }
    // the template rests 90 ticks, then pauses 180
    assert_eq!(rested, 90, "one rest");
    assert_ne!(w.creatures[0].mind.activity, Activity::Resting);
}

/// Creatures crowded on one spot in a corner move the same in two copies of the world, never
/// farther than their speed, never out of the world.
#[test]
fn теснота_совпадение_и_край_не_дают_прыжка() {
    let mut a = world();
    for speed in [5.0, 10.0, 15.0] {
        a.spawn(CreatureGenome::BASE.with(Gene::Speed, speed), 40.0, 40.0, None);
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
fn ребёнок_не_принимает_решений_в_тик_рождения() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.creatures[0].reproduction_wait = 0;
    w.step();
    assert_eq!(w.creatures.len(), 2);
    let child = &w.creatures[1];
    assert_eq!(child.age, 0.0);
    assert_eq!(child.mind, life_core::creature::Mind::default());
    w.step();
    assert!(w.creatures[1].age > 0.0);
}
