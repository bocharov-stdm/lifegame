//! Engine tests: regressions (each closes a bug that once happened) and invariants.
//! No unbounded loops: every run is capped by a tick count.

use life_core::config::*;
use life_core::creature::Creature;
use life_core::creature::Kinship;
use life_core::genome::creature::{GENES, Gene};
use life_core::grid::Grid;
use life_core::plant::Plant;
use life_core::rng::Rng;
use life_core::senses::{Blind, Threat, senses_from};
use life_core::{Counters, CreatureGenome, Genome, Rules, Shape, Space, World, WorldConfig};

const BASE: CreatureGenome = CreatureGenome::BASE;

fn genom(size: f64) -> CreatureGenome {
    BASE.with(Gene::Size, size)
}

/// A carnivore: fresh meat is its own food, so it hunts whatever its fullness. The base genome is a
/// herbivore, which takes fresh meat (10%) only under its template's hunger, as foreign food.
fn hunter(size: f64) -> CreatureGenome {
    genom(size).with(Gene::Diet, life_core::creature::Diet::Carnivore as usize as f64)
}

/// An empty world: neither creatures nor plants (a world starts without plants).
fn empty_world(rules: Rules) -> World {
    World::new(&WorldConfig { seed: 3, rules, n_creatures: Some(0), ..Default::default() })
}

/// The player's world in small (`CLAUDE.md`, baseline conditions): the game's prices and food, 2:1,
/// half lurkers and every diet among the founders, so the hunt, corpses, rot and bones all happen
/// within a short run. ×3 keeps it quick.
fn game_world(seed: u64) -> WorldConfig {
    let rules =
        [("cost_scale", 2.0), ("speed_cost", 0.5), ("plant_rate", 0.5), ("plant_depth_steepness", 5.0)]
            .iter()
            .fold(Rules::default(), |r, &(k, v)| r.with(k, v).unwrap());
    WorldConfig {
        seed,
        scale: 3.0,
        shape: Shape::R2x1,
        rules,
        strategies: vec![1.0, 1.0],
        diets: vec![55.0, 25.0, 10.0, 10.0],
        ..Default::default()
    }
}

fn creature(x: f64, y: f64, g: CreatureGenome) -> Creature {
    Creature::new(&Space::default(), &Rules::default(), g, Some(x), Some(y), None, Rng::new(0))
}

/// Its programs: the template in the layer from `lo` to `hi` % of the depth.
fn layered(v: &mut Creature, lo: u16, hi: u16) {
    use life_core::creature::{Program, Programs, Strategy};
    v.programs = Programs::both(Program::founder(Strategy::Standard, (lo, hi), false));
}

/// Its home band: its adult program's home layer less a margin for the body.
fn band(v: &Creature) -> (f64, f64) {
    v.pheno.band(v.programs[life_core::creature::ADULT].home_layer())
}

/// A program that divides from `tank` of the store giving the child `share`, as the stance of a
/// tick it applied.
fn dividing(v: &mut Creature, tank: f64, share: f64) {
    v.mind.stance.divide = Some(life_core::creature::strategy::Divide { tank, share });
}

// ── regressions ─────────────────────────────────────────────────────────────

/// The plants' pace is an expected number per tick, not a probability (it was: exactly 1 a tick).
/// Seeds landing in occupied slots are lost, so an empty world fills
/// logistically: `cap * (1 - exp(-rate * t / cap))`.
#[test]
fn темп_растений_это_ожидаемое_число() {
    let mut w = empty_world(Rules::default());
    let ticks = 200;
    for _ in 0..ticks {
        w.step();
    }
    let slots = w.flora().slots() as f64;
    let expected = slots * (1.0 - (-PLANT_SPAWN_CHANCE * ticks as f64 / slots).exp());
    let got = w.plants.len() as f64;
    assert!((got - expected).abs() < 40.0, "{got} plants, expected {expected:.0}");
}

/// The band 20‒30% of depth.
fn in_band(p: &Plant, height: f64) -> bool {
    (0.2..0.3).contains(&(p.y / height))
}

/// Plants scattered by the profile, no patches.
fn scattered() -> Rules {
    Rules::default().with("plant_patches", 0.0).unwrap()
}

/// Without creatures plants fill their slots up to the cap, one plant per slot, in patches and
/// scattered.
#[test]
fn plants_fill_slots_up_to_cap() {
    for rules in [Rules::default(), scattered()] {
        let mut w = empty_world(rules);
        for _ in 0..3000 {
            w.step();
        }
        assert!(w.plants.len() <= PLANT_MAX && w.plants.len() >= PLANT_MAX * 9 / 10, "{}", w.plants.len());
        let mut slots: Vec<usize> = w.plants.iter().map(|p| p.slot().expect("grown in a slot")).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), w.plants.len(), "two plants share a slot");
    }
}

/// The surface is grazed bare every tick: the deep sea keeps only its own share
/// of the cap instead of growing a forest into the room the surface left — about
/// 14% below 30% depth with the exponent, about 12% with the «игровое» profile (flat to 20%, the
/// exponent below; patches blur the line a little).
#[test]
fn deep_plants_do_not_take_over_when_surface_is_eaten() {
    let exp = scattered().with("plant_depth_profile", life_core::flora::Profile::Exp.index()).unwrap();
    for (rules, share) in [(exp, 0.07..0.2), (Rules::default(), 0.1..0.26)] {
        let mut w = empty_world(rules);
        for _ in 0..3000 {
            w.step();
            let h = w.space.height;
            w.plants.retain(|p| p.y >= 0.3 * h);
        }
        let deep = w.plants.len() as f64 / PLANT_MAX as f64;
        assert!(share.contains(&deep), "{deep:.3} of the cap below 30% depth, expected {share:?}");
    }
}

/// A band cleared in a full world grows back into the slots it freed, and only
/// there. (Slots on the band's edges straddle it: their new plant may sprout
/// just outside, so the band's own count comes back a little lower.)
#[test]
fn cleared_band_recovers() {
    let mut w = empty_world(Rules::default());
    for _ in 0..4000 {
        w.step();
    }
    let h = w.space.height;
    let cells_of = |w: &World, keep: &dyn Fn(&Plant) -> bool| {
        let mut c: Vec<usize> = w.plants.iter().filter(|p| keep(p)).filter_map(|p| p.slot()).collect();
        c.sort_unstable();
        c
    };
    let cleared = cells_of(&w, &|p| in_band(p, h));
    let band = cleared.len();
    w.plants.retain(|p| !in_band(p, h));
    for _ in 0..3000 {
        w.step();
    }
    let now = cells_of(&w, &|_| true);
    let back = cleared.iter().filter(|c| now.binary_search(c).is_ok()).count();
    assert!(back * 100 >= band * 95, "{back} of {band} cleared slots taken again");
    let after = w.plants.iter().filter(|p| in_band(p, h)).count();
    assert!(after * 10 >= band * 8, "band {band} -> {after}");
}

// ── hunting ────────────────────────────────────────────────────────────────

/// A world with a big hunter and a neighbour of the given size at the given distance. Tick 1 is
/// no division tick: only the strikes count.
fn hunter_world(small: f64, dx: f64) -> World {
    let mut w = empty_world(Rules::default());
    w.tick = 1;
    w.spawn(hunter(100.0), 3000.0, 2000.0, Some(100.0));
    w.spawn(genom(small), 3000.0 + dx, 2000.0, None);
    w
}

/// A hunter strikes the small one beside it: twice smaller it survives the first strike,
/// 6.7 times smaller it dies of it (`MELEE_SIZE_POWER`) and leaves a corpse.
#[test]
fn a_hunter_strikes_the_small_one_beside_it() {
    let mut w = hunter_world(50.0, 10.0);
    w.step();
    assert_eq!(w.creatures.len(), 2, "twice smaller, full health survives one strike");
    assert!(w.creatures[1].health < w.creatures[1].max_health());
    assert_eq!(w.counters.combat, 0);
    let mut w = hunter_world(15.0, 10.0);
    w.step();
    assert_eq!(w.creatures.len(), 1, "6.7 times smaller, one strike kills");
    assert_eq!((w.counters.combat, w.corpses.len()), (1, 1));
}

#[test]
fn a_hunter_leaves_the_big_and_the_distant_alone() {
    for (small, dx, why) in [
        (70.0, 10.0, "only 1.4 times smaller: not prey for the template's hunt block (1.5)"),
        (30.0, 400.0, "far away: a strike needs contact"),
    ] {
        let mut w = hunter_world(small, dx);
        w.step();
        assert_eq!(w.creatures.len(), 2, "{why}");
        assert_eq!(w.counters.combat, 0, "{why}");
        assert!(w.creatures[1].health == w.creatures[1].max_health(), "{why}");
    }
}

// ── kinship and flight ─────────────────────────────────────────────────────

/// A child knows its parent. Family is a parent and its growing child while the parent still
/// knows it (base care: until the child is adult); siblings and grandchildren are strangers.
#[test]
fn kinship_is_a_parent_and_its_growing_child() {
    let mut w = empty_world(Rules::default());
    let id = w.spawn(BASE, 3000.0, 2000.0, None);
    let (s, r) = (w.space, w.rules.clone());
    let parent = &mut w.creatures[0];
    dividing(parent, 0.3, 0.4);
    parent.mind.stance.spare = 1.0; // the template's «щадить детей»: until grown
    let stance = parent.mind.stance;
    assert_eq!(parent.parent, 0, "a spawned creature has no parent");
    let mut kids = Vec::new();
    for n in 0..2 {
        parent.reproduction_wait = 0;
        parent.energy = parent.pheno.max_energy;
        let mut kid = parent.maybe_divide(&s, &r).expect("a full parent did not divide");
        kid.id = 100 + n; // the world numbers them
        kids.push(kid);
    }
    let parent = parent.kinship();
    let (a, b) = (kids[0].kinship(), kids[1].kinship());
    assert_eq!(a.parent, id, "the child remembers its parent");
    assert!(a.growth < 1.0, "a newborn is not adult");
    assert!(parent.kin(a) && a.kin(parent), "a parent and its newborn are family");
    assert!(!a.kin(b), "siblings are strangers");
    kids[0].nourish(10000.0, &r);
    kids[0].mind.stance = stance;
    assert!(kids[0].adult());
    assert!(!parent.kin(kids[0].kinship()), "the parent forgets its grown child");
    kids[0].reproduction_wait = 0;
    kids[0].energy = kids[0].pheno.max_energy;
    let mut grandchild = kids[0].maybe_divide(&s, &r).expect("a full child did not divide");
    grandchild.id = 200;
    let grandchild = grandchild.kinship();
    assert!(kids[0].kinship().kin(grandchild), "a child and its own newborn are family");
    assert!(
        !parent.kin(grandchild) && !b.kin(grandchild),
        "a grandchild is a stranger to grandparent and uncle"
    );
    let strangers = (Kinship { id: 7, ..Default::default() }, Kinship { id: 8, ..Default::default() });
    assert!(!strangers.0.kin(strangers.1), "founders without a parent are strangers");
}

/// How long a parent knows its child is its program's: «щадить детей» sets the growth up to which
/// it does, as it applied last tick; without it not at all.
#[test]
fn a_careless_parent_knows_only_its_tiny_children() {
    use life_core::creature::{Action, Block, Program};
    let mut w = empty_world(Rules::default());
    for spare in [Some(10), Some(100), None] {
        w.spawn(BASE, 3000.0, 2000.0, None);
        let mut blocks = vec![Block::does(Action::Ambush)];
        if let Some(until) = spare {
            blocks.insert(0, Block::does(Action::Spare).with(0, until));
        }
        let v = w.creatures.last_mut().unwrap();
        v.programs = [Program::of(&blocks); 2].into();
        v.step(&Blind);
    }
    let parents: Vec<Kinship> = w.creatures.iter().map(|v| v.kinship()).collect();
    assert_eq!(parents.iter().map(|k| k.knows_until).collect::<Vec<_>>(), [0.1, 1.0, 0.0]);
    for (growth, known) in [(0.05, [true, true, false]), (0.5, [false, true, false]), (1.0, [false; 3])] {
        for (p, known) in parents.iter().zip(known) {
            let child = Kinship { id: 99, parent: p.id, growth, knows_until: 1.0 };
            assert_eq!(p.kin(child), known, "spares until {} growth {growth}", p.knows_until);
        }
    }
}

/// A world with a big creature of size `big`, hunting, and a small one (30) `dx` to the right;
/// `kin` sets their kinship by hand. The big one is on the hunt: the template flees a hunter from
/// farther than a passer-by that hunts nobody (`Cond::HunterNear` 33% of sight, `ThreatNear` 16%).
fn threat_world(big: f64, dx: f64, kin: impl Fn(&mut World)) -> World {
    let mut w = hunter_world(30.0, dx);
    let small = w.creatures[1].id;
    let v = &mut w.creatures[0];
    v.genome = hunter(big);
    v.pheno = life_core::creature::Phenotype::of(&v.genome, &w.rules, &w.space);
    v.mind.attack = Some(small);
    kin(&mut w);
    w
}

/// A world tick: the small one's shift in x and y and whether it is now running.
fn small_step(mut w: World) -> (f64, f64, bool) {
    let (x0, y0) = (w.creatures[1].x, w.creatures[1].y);
    w.step();
    let v = &w.creatures[1];
    (v.x - x0, v.y - y0, v.fleeing())
}

#[test]
fn мелкий_бежит_от_крупного_чужака() {
    // to the big one's body edge 150 - 50 = 100: closer than a third of sight (133)
    let w = threat_world(100.0, 150.0, |_| {});
    let speed = w.creatures[1].pheno.speed;
    let (dx, dy, fleeing) = small_step(w);
    assert!(fleeing, "мелкий не испугался");
    assert!((dx - speed).abs() < 1e-9 && dy.abs() < 1e-9, "бежал не прочь: ({dx}, {dy})");
}

/// The small one is still growing: its body is half of its inherited size.
fn growing(w: &mut World) {
    let v = &mut w.creatures[1];
    v.genome = v.genome.with(Gene::Size, v.pheno.size * 2.0);
}

#[test]
fn does_not_flee_a_parent_that_knows_it_nor_an_equal_or_distant_one() {
    let parent = |w: &mut World| {
        growing(w);
        w.creatures[1].parent = w.creatures[0].id;
        w.creatures[0].mind.stance.spare = 1.0; // the template knows its child until grown
    };
    let cases: [(World, &str); 3] = [
        (threat_world(100.0, 150.0, parent), "from its parent"),
        (threat_world(40.0, 150.0, |_| {}), "from one only 1.3 times bigger"),
        (threat_world(100.0, 250.0, |_| {}), "from one 200 away, beyond a third of its vision"),
    ];
    for (w, why) in cases {
        let (_, _, fleeing) = small_step(w);
        assert!(!fleeing, "flees {why}");
    }
    let grown_child = |w: &mut World| w.creatures[1].parent = w.creatures[0].id; // adult: forgotten
    let adult_child = |w: &mut World| w.creatures[0].parent = w.creatures[1].id;
    let brothers = |w: &mut World| {
        w.creatures[0].parent = 999;
        w.creatures[1].parent = 999;
    };
    let strangers = |w: &mut World| w.creatures[1].parent = 999;
    for (w, why) in [
        (threat_world(100.0, 150.0, strangers), "a stranger"),
        (threat_world(100.0, 150.0, brothers), "a brother"),
        (threat_world(100.0, 150.0, grown_child), "a parent that forgot it"),
        (threat_world(100.0, 150.0, adult_child), "its own adult child"),
    ] {
        let (_, _, fleeing) = small_step(w);
        assert!(fleeing, "does not flee {why}");
    }
}

/// A fright lasts FLEE_TICKS ticks after the tick the threat was seen, even when it has gone out
/// of sight — even past visible food (the templates' alarm mode); then the creature goes to the
/// food again. A lurker runs at full speed.
#[test]
fn бежит_ещё_после_пропажи_угрозы() {
    let food = |_: f64, _: f64, _: f64| Some((900.0, 1000.0));
    let threat = Threat { id: 999, x: 960.0, y: 1000.0, gap: 20.0, half: 20.0 };
    for g in [BASE, LURKER] {
        let mut v = creature(1000.0, 1000.0, g);
        v.health = v.max_health() * 0.1;
        let x0 = v.x;
        let (d, _) = move_once(&mut v, &senses_from(food).with_threat(threat));
        assert!(v.x > x0 && close(d, v.pheno.speed), "не побежал прочь на полной скорости");
        for t in 0..FLEE_TICKS {
            let x = v.x;
            move_once(&mut v, &senses_from(food));
            assert!(v.x > x, "stopped running at tick {t}");
        }
        let x = v.x;
        move_once(&mut v, &senses_from(food));
        assert!(v.x < x && !v.fleeing(), "the fright is over, yet it does not go to the food");
    }
    // the threat in view, but beyond the threshold — to the food
    let mut v = creature(1000.0, 1000.0, BASE);
    // the template flees from a hunter within a third of its sight
    let far = Threat { gap: v.pheno.vision * FLEE_SIGHT_SHARE + 1.0, ..threat };
    move_once(&mut v, &senses_from(food).with_threat(far));
    assert!(v.x < 1000.0 && !v.fleeing(), "испугался далёкого");
}

/// A healthy creature in contact with a threat strikes back — unless the threat is its fight-back
/// block's ratio (1.5 in the template) times bigger than it: then it runs.
#[test]
fn strikes_back_an_equal_but_runs_from_one_out_of_its_league() {
    for (half, fights) in [(20.0, true), (29.0, true), (30.0, false), (120.0, false)] {
        let mut v = creature(1000.0, 1000.0, BASE);
        assert_eq!(v.pheno.half, 20.0);
        let threat = Threat { id: 999, x: 1000.0 + 20.0 + half - 1.0, y: 1000.0, gap: 19.0, half };
        let before = v.x;
        v.step(&senses_from(|_, _, _| None).with_threat(threat));
        assert_eq!(v.mind.attack == Some(999), fights, "enemy radius {half}");
        assert_eq!(v.fleeing(), !fights, "enemy radius {half}");
        if !fights {
            assert!(v.x < before, "ran towards the enemy");
        }
    }
}

#[test]
fn a_hunter_spares_its_growing_child_but_not_a_brother() {
    let as_child = |w: &mut World| {
        growing(w);
        w.creatures[1].parent = w.creatures[0].id;
    };
    let mut w = threat_world(100.0, 10.0, as_child);
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.3;
    w.step();
    assert_eq!(w.creatures[1].health, w.creatures[1].max_health(), "the hunter struck its own growing child");
    let as_brother = |w: &mut World| {
        w.creatures[0].parent = 999;
        w.creatures[1].parent = 999;
    };
    let mut w = threat_world(100.0, 10.0, as_brother);
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.3;
    let brother = w.creatures[1].id;
    w.step();
    assert!(
        w.creature(brother).is_none_or(|v| v.health < v.max_health()),
        "a hungry hunter spared a brother"
    );
}

/// A world with flight is deterministic: relatives are seen by the snapshot at the start of the phase.
#[test]
fn мир_с_бегством_детерминирован() {
    let run = || {
        let mut w = World::new(&WorldConfig { seed: 9, ..Default::default() });
        let mut fled = 0;
        for _ in 0..2000 {
            w.step();
            fled += w.creatures.iter().filter(|v| v.fleeing()).count();
        }
        (w.stats(), w.counters, fled)
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b);
    assert!(a.2 > 0, "за 2000 тиков никто ни разу не испугался");
}

/// A creature that starved to death on its own move neither eats nor divides.
#[test]
fn умерший_от_голода_не_ест() {
    let mut w = empty_world(Rules::default());
    w.spawn(genom(40.0), 1000.0, 1000.0, None);
    let v = &mut w.creatures[0];
    v.energy = v.pheno.upkeep / 2.0; // this move is the last
    let (x, y) = (v.x, v.y);
    w.plants.push(Plant::at(x, y));
    w.plants.push(Plant::at(x + 5.0, y));
    w.step();
    assert!(w.creatures.is_empty());
    assert!(w.plants.iter().filter(|p| p.y == y).count() == 2, "труп съел растение");
}

/// After a division the parent keeps a reserve (it was: it gave everything and died).
#[test]
fn родитель_сохраняет_резерв() {
    // Without mutation the child's capacity is known (half of 40 by 2.5), so a large share
    // really takes the parent below its reserve and the division is refused.
    let (s, r) = (Space::default(), Rules::default().with("mutation_sigma", 0.0).unwrap());
    let (mut divided, mut blocked) = (0, 0);
    for share in [10.0, 30.0, 50.0, 70.0, 90.0] {
        let mut p = Creature::new(&s, &r, BASE, Some(1000.0), Some(1000.0), Some(60.0), Rng::new(0));
        dividing(&mut p, 0.3, share / 100.0);
        p.reproduction_wait = 0;
        match p.maybe_divide(&s, &r) {
            Some(_) => {
                divided += 1;
                assert!(p.energy >= REPRO_RESERVE, "доля {share}%: осталось {}", p.energy);
            }
            None => {
                blocked += 1;
                assert_eq!(p.energy, 60.0, "деления не было, а энергия ушла");
            }
        }
    }
    assert!(divided > 0 && blocked > 0, "одна из веток не проверена");
}

/// A body bigger than the world does not leave the world and does not jump farther than its speed.
#[test]
fn огромное_тело_не_прыгает() {
    let s = Space::default();
    for size in [2500.0, 3500.0, 4100.0, 9000.0] {
        for (lo, hi) in [(5, 10), (0, 100), (90, 100)] {
            let g = BASE.with(Gene::Size, size).with(Gene::Speed, 60.0);
            let mut v = creature(100.0, 100.0, g);
            layered(&mut v, lo, hi);
            let (body_lo, body_hi) = band(&v);
            assert!(v.pheno.x_lo <= v.pheno.x_hi && body_lo <= body_hi);
            for _ in 0..30 {
                let (x, y) = (v.x, v.y);
                v.energy = v.pheno.max_energy;
                v.step(&Blind);
                assert!((v.x - x).hypot(v.y - y) <= v.pheno.speed + 1e-9, "прыжок дальше скорости");
                assert!(
                    (0.0..=s.width).contains(&v.x) && (0.0..=s.height).contains(&v.y),
                    "центр вне мира: ({}, {})",
                    v.x,
                    v.y
                );
            }
        }
    }
}

/// A child is born next to its parent (the layer is soft — it is not teleported into its
/// own), inside the world and not on a diagonal from the parent.
#[test]
fn ребёнок_рождается_у_родителя() {
    let (s, r) = (Space::default(), Rules::default());
    let mut parent = Creature::new(&s, &r, BASE, Some(3000.0), Some(3900.0), None, Rng::new(0));
    dividing(&mut parent, 0.3, 0.4);
    let (mut diagonal, mut outside) = (0, 0);
    for _ in 0..300 {
        parent.reproduction_wait = 0;
        parent.energy = parent.pheno.max_energy;
        let mut c = parent.maybe_divide(&s, &r).expect("сытый родитель не поделился");
        assert!(c.pheno.y_lo <= c.y && c.y <= c.pheno.y_hi, "ребёнок вне мира: y={}", c.y);
        assert!(c.pheno.x_lo <= c.x && c.x <= c.pheno.x_hi);
        let span = parent.pheno.size * 2.0;
        assert!(
            (c.x - parent.x).abs() <= span && (c.y - parent.y).abs() <= span,
            "ребёнок далеко от родителя"
        );
        let (lo, hi) = band(&c);
        outside += (c.y < lo || c.y > hi) as u32;
        diagonal += ((c.x - parent.x) == (c.y - parent.y)) as u32;
        let (x, y) = (c.x, c.y);
        c.step(&Blind);
        assert!((c.x - x).hypot(c.y - y) <= c.pheno.speed + 1e-9, "первый ход ребёнка — прыжок");
    }
    assert_eq!(diagonal, 0);
    assert!(outside > 0, "ни один ребёнок не родился вне своего слоя — тест ничего не проверил");
}

/// The layer is soft: a plant above the layer is visible — the creature goes and eats it.
#[test]
fn еда_над_слоем_съедается() {
    // no plants grow: a nearer sprout between patches would be eaten first
    let mut w = empty_world(Rules::default().with("plant_rate", 0.0).unwrap());
    w.spawn(BASE, 3000.0, 2100.0, Some(80.0));
    layered(&mut w.creatures[0], 50, 100);
    let v = &w.creatures[0];
    let (body_lo, _) = band(v);
    assert!(v.y >= body_lo, "существо должно стартовать в своём слое");
    let plant_y = v.pheno.layer((0.5, 1.0)).0 - 250.0;
    w.plants.push(Plant::at(3000.0, plant_y));
    let there = |w: &World| w.plants.iter().any(|p| p.x == 3000.0 && p.y == plant_y);
    for _ in 0..60 {
        w.step();
        if !there(&w) {
            break;
        }
    }
    assert!(!there(&w), "растение над слоем осталось несъеденным");
    assert!(w.creatures[0].y < body_lo, "съело, не выходя из слоя?");
}

/// Outside its layer and without food a creature returns home and then keeps to the layer.
#[test]
fn без_еды_возвращается_в_слой() {
    let mut v = creature(3000.0, 500.0, BASE);
    layered(&mut v, 50, 100);
    let (body_lo, body_hi) = band(&v);
    assert_eq!(v.y, 500.0, "заданная позиция не зажимается в слой");
    let mut home = None;
    for t in 0..400 {
        let (x, y) = (v.x, v.y);
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
        assert!((v.x - x).hypot(v.y - y) <= v.pheno.speed + 1e-9, "прыжок дальше скорости");
        let inside = body_lo <= v.y && v.y <= body_hi;
        match home {
            None if inside => home = Some(t),
            Some(_) => assert!(inside, "вернулось в слой и снова ушло без еды: y={}", v.y),
            None => {}
        }
    }
    let t = home.expect("за 400 тиков не вернулось в слой");
    let ideal = ((body_lo - 500.0) / v.pheno.speed).ceil() as usize;
    assert!(t < ideal + 5, "шло домой {t} тиков вместо ~{ideal}: не по прямой");
}

/// A layer narrower than the body: the creature lives on a line and does not stand like a post.
#[test]
fn схлопнутый_слой_проходим() {
    let mut v = creature(3000.0, 2000.0, BASE);
    layered(&mut v, 50, 50);
    let (body_lo, body_hi) = band(&v);
    assert_eq!(body_lo, body_hi);
    let x0 = v.x;
    for _ in 0..50 {
        // The fullness is below the rest threshold: what is checked here is precisely the movement along a
        // line.
        v.energy = v.pheno.max_energy * 0.8;
        v.step(&Blind);
        assert_eq!(v.y, body_lo);
    }
    assert!(v.x != x0, "существо на схлопнутом слое стоит столбом");
}

// ── strategies and behaviour genes ─────────────────────────────────────────

const LURKER: CreatureGenome = BASE.with(Gene::Strategy, 1.0);
/// The upkeep recovered from the difference of energies matches up to rounding.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

/// A creature's move: (distance covered, energy spent).
fn move_once(v: &mut Creature, senses: &impl life_core::senses::Senses) -> (f64, f64) {
    let (x, y, e) = (v.x, v.y, v.energy);
    v.step(senses);
    ((v.x - x).hypot(v.y - y), e - v.energy)
}

/// The speed term is paid for the step taken: standing costs only the body and eyes, a full step
/// the whole upkeep, a lurker's slow step (a third of its speed) about a ninth of the speed term.
#[test]
fn a_slow_step_costs_less() {
    let v = creature(1000.0, 1000.0, LURKER);
    let slow = v.pheno.speed * 0.33;
    let costs = [DIET_SIZE_COST[0], DIET_SPEED_COST[0]];
    let r = Rules::default();
    assert_eq!(v.pheno.still_upkeep, r.upkeep_diet(40.0, 0.0, v.pheno.vision, costs));
    for step in [0.0, slow, v.pheno.speed] {
        let expected = r.upkeep_diet(40.0, step, v.pheno.vision, costs);
        assert!(close(v.pheno.step_cost(step), expected), "a step of {step}");
    }
    assert!(close(v.pheno.step_cost(v.pheno.speed), v.pheno.upkeep));
    assert!(v.pheno.step_cost(slow) < v.pheno.upkeep);
}

/// With no food in sight the lurker wanders at its template's third of its speed and cheaper, the
/// standard one at full speed, a program wandering at half at half.
#[test]
fn a_lurker_without_food_wanders_slowly() {
    use life_core::creature::{Action, Program};
    let mut lurker = creature(1000.0, 1000.0, LURKER);
    let mut standard = creature(1000.0, 1000.0, BASE);
    let mut cruiser = creature(1000.0, 1000.0, BASE);
    cruiser.programs = [Program::STANDARD.tuned(Action::Wander, |b| b.args[0] = 50); 2].into();
    for _ in 0..20 {
        let (d, cost) = move_once(&mut lurker, &Blind);
        assert!((d - lurker.pheno.speed * 0.33).abs() < 1e-9, "затаившийся прошёл {d}");
        assert!(close(cost, lurker.pheno.step_cost(d)), "затаившийся потратил {cost}");
        let (d, cost) = move_once(&mut standard, &Blind);
        assert!((d - standard.pheno.speed).abs() < 1e-9, "стандартный прошёл {d}");
        assert!(close(cost, standard.pheno.upkeep), "стандартный потратил {cost}");
        let (d, cost) = move_once(&mut cruiser, &Blind);
        assert!((d - cruiser.pheno.speed * 0.5).abs() < 1e-9, "крейсер прошёл {d}");
        assert!(close(cost, cruiser.pheno.step_cost(d)) && cost < standard.pheno.upkeep);
    }
}

/// A lurker goes to food at full speed.
#[test]
fn затаившийся_к_еде_на_полной() {
    let mut v = creature(1000.0, 1000.0, LURKER);
    let (d, cost) = move_once(&mut v, &senses_from(|_, _, _| Some((1300.0, 1000.0))));
    assert!((d - v.pheno.speed).abs() < 1e-9 && close(cost, v.pheno.upkeep), "к еде: {d}, {cost}");
}

/// The strategy — the template of the founders' programs — never switches; both programs, the
/// juvenile and the adult, are inherited: a copy's exactly, a mutating child's with every number
/// drifted, and each mutates apart in the rule's share of the mutating children.
#[test]
fn a_child_inherits_its_parents_program_and_it_mutates_now_and_then() {
    use life_core::creature::{ADULT, JUVENILE, Program};
    let s = Space::default();
    let r = Rules::default().with("program_mutation", 0.2).unwrap();
    let g = BASE.with(Gene::Strategy, 1.0);
    let mut parent = Creature::new(&s, &r, g, Some(3000.0), Some(1000.0), None, Rng::new(7));
    dividing(&mut parent, 0.3, 0.4);
    assert_eq!(parent.programs, [Program::LURKER; 2], "a creature starts from its strategy's template");
    let n = 4000;
    let (mut mutated, mut drifted) = ([0; 2], [0; 2]);
    for _ in 0..n {
        parent.reproduction_wait = 0;
        parent.energy = parent.pheno.max_energy;
        let c = parent.maybe_divide(&s, &r).expect("сытый родитель не поделился");
        assert_eq!(c.genome[Gene::Strategy], 1.0, "the strategy never switches");
        let copy = c.genome == parent.genome;
        for stage in [JUVENILE, ADULT] {
            let p = c.programs[stage];
            if copy {
                assert_eq!(p, Program::LURKER, "a copy's program is the parent's");
            } else if p.changes == 0 {
                assert_eq!(p.shape(), Program::LURKER.shape(), "unmutated, only its numbers drifted");
            }
            mutated[stage] += (p.changes > 0) as u32;
            drifted[stage] += (!copy && p.changes == 0 && !p.same_blocks(&Program::LURKER)) as u32;
        }
    }
    assert_eq!(STRATEGY_SWITCH_CHANCE, 0.0);
    // half the children are copies; of the rest a fifth of each program mutates, at the base
    // mutability of 1, and nearly every other one drifted
    for stage in [JUVENILE, ADULT] {
        let rate = mutated[stage] as f64 / n as f64;
        assert!((rate - 0.1).abs() < 0.02, "program {stage} mutated in {rate:.3} of the children");
        let rate = drifted[stage] as f64 / n as f64;
        assert!((rate - 0.4).abs() < 0.03, "program {stage} drifted in {rate:.3} of the children");
    }
}

/// A mixed world: the starting mix is dealt without a draw, and the same seed is the same world.
#[test]
fn смешанный_мир_детерминирован() {
    let cfg = WorldConfig { seed: 5, strategies: vec![1.0, 1.0], ..Default::default() };
    let w = World::new(&cfg);
    let lurkers = w.creatures.iter().filter(|v| v.genome[Gene::Strategy] == 1.0).count();
    assert_eq!(lurkers, w.creatures.len() / 2, "смесь 50/50 раздана неровно");
    let run = || {
        let mut w = World::new(&cfg);
        for _ in 0..1000 {
            w.step();
        }
        w.stats()
    };
    assert_eq!(run(), run());
}

/// Behaviour evolving fast (every mutating child's program mutates): the world runs, programs
/// spread into many different ones, lineages gather mutations, and the same seed is the same
/// world, programs included.
#[test]
fn a_world_of_mutating_programs_runs_and_stays_deterministic() {
    let rules = Rules::default().with("program_mutation", 1.0).unwrap();
    let cfg = WorldConfig { seed: 11, strategies: vec![1.0, 1.0], rules, ..Default::default() };
    let run = || {
        let mut w = World::new(&cfg);
        for _ in 0..2000 {
            w.step();
        }
        w
    };
    let (a, b) = (run(), run());
    assert!(!a.creatures.is_empty() && a.counters.born > 0, "life goes on");
    let state =
        |w: &World| w.creatures.iter().map(|v| (v.id, v.programs.clone(), v.x.to_bits())).collect::<Vec<_>>();
    assert!(state(&a) == state(&b), "the same seed, the same world and programs");
    let mut distinct: Vec<Vec<[u64; 3]>> = a
        .creatures
        .iter()
        .flat_map(|v| (*v.programs).map(|p| p.blocks().iter().map(|b| b.code()).collect()))
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() > 5, "programs diversify: {} distinct", distinct.len());
    let changes = |stage: usize| a.creatures.iter().map(|v| v.programs[stage].changes).max().unwrap_or(0);
    assert!(changes(0) >= 2 && changes(1) >= 2, "lineages gather mutations in both tracks");
}

// ── the neighbour grid ──────────────────────────────────────────────────────

/// The grid must be a SUPERset of an honest brute-force search. Should it miss a neighbour, a
/// creature would stop noticing food under its nose while the population stays plausible, so
/// other tests would not catch it.
#[test]
fn сетка_совпадает_с_перебором() {
    let mut rng = Rng::new(42);
    for scale in [1.0, 3.5] {
        let s = Space::new(scale, Shape::Strip);
        for cell in [64.0, 256.0, 1000.0] {
            let mut pts: Vec<(f64, f64)> =
                (0..800).map(|_| (rng.uniform(0.0, s.width), rng.uniform(0.0, s.height))).collect();
            // points on the very edges and corners of the world
            pts.extend([(0.0, 0.0), (s.width, s.height), (0.0, s.height), (s.width, 0.0)]);
            let mut g = Grid::new(cell);
            g.rebuild(&s, pts.iter().copied());
            for _ in 0..300 {
                let (x, y) = (rng.uniform(-100.0, s.width + 100.0), rng.uniform(-100.0, s.height + 100.0));
                let r = rng.uniform(0.0, 1500.0);
                let mut found = vec![false; pts.len()];
                g.for_each_near(x, y, r, |i, px, py| {
                    assert_eq!((px, py), pts[i], "сетка вернула не те координаты");
                    found[i] = true;
                });
                for (i, &(px, py)) in pts.iter().enumerate() {
                    if (px - x).hypot(py - y) <= r {
                        assert!(
                            found[i],
                            "клетка {cell}: пропущена точка ({px}, {py}) в радиусе {r} от ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
}

// ── rules ──────────────────────────────────────────────────────────────────

#[test]
fn не_конечные_правила_отвергаются() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Rules::default().with("mutation_sigma", bad).is_err());
    }
    assert!(Rules::default().with("нет_такого", 1.0).is_err());
}

/// Values at which a rule loses meaning are rejected; the edges of what is allowed are not.
#[test]
fn бессмысленные_правила_отвергаются() {
    let r = Rules::default();
    for (key, bad) in [("cost_scale", -1.0), ("plant_energy", -5.0), ("mutation_sigma", -0.1)] {
        assert!(r.with(key, bad).is_err(), "{key}={bad} принято");
    }
    for (key, ok) in [("cost_scale", 0.0), ("plant_rate", 0.0)] {
        assert!(r.with(key, ok).is_ok(), "{key}={ok} отвергнуто");
    }
}

#[test]
fn расход_по_умолчанию_это_формула_конфига() {
    let r = Rules::default();
    for (size, speed, vision) in [(40.0_f64, 10.0_f64, 400.0_f64), (80.0, 25.0, 150.0), (13.0, 3.0, 900.0)] {
        let mass = (size / 40.0_f64).powf(SPEED_MASS_POWER);
        let want = SIZE_ENERGY_COEF * size.powf(SIZE_ENERGY_POWER)
            + SPEED_ENERGY_COEF * speed.powf(SPEED_ENERGY_POWER) * mass
            + SIGHT_ENERGY_COEF * vision.powf(SIGHT_ENERGY_POWER);
        assert_eq!(r.upkeep(size, speed, vision), want);
    }
}

/// An exponent changes the steepness, and the base stat costs the same.
#[test]
fn показатель_меняет_крутизну_а_не_базу() {
    let base = Rules::default();
    let steep = base.with("size_power", 3.5).unwrap();
    let flat = base.with("size_power", 1.5).unwrap();
    let b = base.upkeep(40.0, 10.0, 400.0);
    assert!((steep.upkeep(40.0, 10.0, 400.0) - b).abs() < 1e-12);
    assert!((flat.upkeep(40.0, 10.0, 400.0) - b).abs() < 1e-12);
    assert!(steep.upkeep(80.0, 10.0, 400.0) > base.upkeep(80.0, 10.0, 400.0));
    assert!(flat.upkeep(80.0, 10.0, 400.0) < base.upkeep(80.0, 10.0, 400.0));
}

#[test]
fn цена_статов_масштабирует_всё() {
    let base = Rules::default();
    let double = base.with("cost_scale", 2.0).unwrap();
    assert!((double.upkeep(55.0, 17.0, 300.0) - 2.0 * base.upkeep(55.0, 17.0, 300.0)).abs() < 1e-12);
}

// ── invariants and reproducibility ─────────────────────────────────────────

/// In the default world and in the player's, with every diet: bodies inside the world, tanks
/// within their store, every gene within its table, no eaten plant left, no id twice.
#[test]
fn инварианты_держатся_со_временем() {
    for cfg in [WorldConfig { seed: 1, ..Default::default() }, game_world(1)] {
        invariants_hold(&cfg, 600);
    }
}

fn invariants_hold(cfg: &WorldConfig, ticks: u64) {
    let mut w = World::new(cfg);
    for _ in 0..ticks {
        w.step();
        for v in &w.creatures {
            assert!(v.alive && v.energy > 0.0 && v.energy <= v.pheno.max_energy + 1e-9);
            assert!(v.pheno.x_lo <= v.x && v.x <= v.pheno.x_hi && v.pheno.y_lo <= v.y && v.y <= v.pheno.y_hi);
            let g = v.genome.values();
            for (spec, x) in GENES.iter().zip(g) {
                match spec.variants() {
                    // a choice gene is the number of an existing variant
                    Some(variants) => {
                        assert!(x.fract() == 0.0 && (*x as usize) < variants.len(), "ген {} = {x}", spec.key)
                    }
                    // a number gene never falls to zero, but one moved by points may sit on it
                    None => assert!(
                        *x >= 0.01 || matches!(spec.mutation, life_core::genome::Mutation::Shift { .. }),
                        "ген {} ниже 0.01: {g:?}",
                        spec.key
                    ),
                }
                assert!(!spec.is_percent() || (0.0..=100.0).contains(x), "ген-процент вне 0‒100: {g:?}");
            }
        }
        assert!(w.plants.iter().all(|p| p.alive()), "съеденное растение не выметено");
        let mut ids: Vec<u64> = w.creatures.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), w.creatures.len(), "номера существ повторяются");
    }
}

/// Energy is never made from nothing (the user's first rule): it enters only in plants and then
/// only passes along the chain, losing some. So each tick what the living hold (their tanks and the
/// bodies they grew — a body got for free at birth is nobody's) and what the corpses hold grows by
/// no more than the raw energy of the plant bites taken that tick. In the player's world with every
/// diet, so hunting, fresh meat, rot, bones and division all take part; a new mechanic that
/// makes energy breaks this at once.
#[test]
fn energy_is_never_made_from_nothing() {
    let held = |w: &World| {
        let living: f64 = w
            .creatures
            .iter()
            .map(|v| v.energy + (v.pheno.size - v.birth_size) * GROWTH_ENERGY_PER_SIZE)
            .sum();
        living + w.corpses.iter().map(|c| c.remaining).sum::<f64>()
    };
    let mut w = World::new(&game_world(1));
    let bite = w.rules.plant_energy * w.rules.plant_bite_yield / f64::from(life_core::plant::PORTIONS);
    for _ in 0..3000 {
        let (before, bites) = (held(&w), w.counters.plant_bites);
        w.step();
        let (after, eaten) = (held(&w), (w.counters.plant_bites - bites) as f64 * bite);
        assert!(
            after - before <= eaten + 1e-9 * before.max(1.0),
            "tick {}: the living and the corpses gained {:.6}, the plants gave {eaten:.6}",
            w.tick,
            after - before
        );
    }
    let c = w.counters;
    assert!(
        c.born > 0 && c.combat > 0 && c.meat_bites > c.rot_bites && c.rot_bites > 0 && c.bone_bites > 0,
        "the whole chain took part: {c:?}"
    );
}

#[test]
fn один_сид_один_мир() {
    let run = |seed| {
        let mut w = World::new(&WorldConfig { seed, ..Default::default() });
        for _ in 0..300 {
            w.step();
        }
        w.stats()
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6));
}

/// The counters are lossless bookkeeping: how many there were, plus born, minus eaten and
/// dead, equals how many there are. Should the world miss one death, the report would explain
/// the population by wrong causes.
#[test]
fn счётчики_сходятся_с_численностью() {
    let mut w = World::new(&WorldConfig { seed: 3, ..Default::default() });
    let (n0, plants0) = (w.creatures.len() as u64, w.plants.len() as u64);
    for _ in 0..3000 {
        w.step();
    }
    let c = w.counters;
    assert!(c.born > 0 && c.combat > 0 && c.starved > 0, "{c:?}");
    assert_eq!(w.creatures.len() as u64, n0 + c.born - c.starved - c.old_age - c.combat);
    assert_eq!(w.plants.len() as u64, plants0 + c.plants_grown - c.plants_eaten);
    assert_eq!(c.since(&c), Counters::default());
}

/// A big world is the same world, only bigger: the starting populations and the ceilings grow
/// with the area. A strip grows in width, the other shapes both ways.
#[test]
fn масштаб_растит_площадь() {
    let strip = World::new(&WorldConfig { scale: 10.0, shape: Shape::Strip, ..Default::default() });
    assert_eq!(strip.space.height, WORLD_HEIGHT);
    assert_eq!(strip.space.width, WORLD_WIDTH * 10.0);
    for shape in Shape::ALL {
        let w = World::new(&WorldConfig { scale: 10.0, shape, ..Default::default() });
        assert_eq!(w.creatures.len(), CREATURES_AT_START * 10, "{shape:?}");
        assert!(w.creatures.iter().all(|v| v.x <= w.space.width && v.y <= w.space.height), "{shape:?}");
    }
    let wide = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
    assert!(wide.space.height > WORLD_HEIGHT * 3.0, "3:2 растёт и вглубь: {:?}", wide.space);

    let mut e = World::new(&WorldConfig { scale: 2.0, n_creatures: Some(0), ..Default::default() });
    for _ in 0..2000 {
        e.step();
    }
    assert!((PLANT_MAX * 2 * 9 / 10..=PLANT_MAX * 2).contains(&e.plants.len()), "{}", e.plants.len());
}

/// A world narrower than the base is not built: in a narrow world the wandering strip turned
/// over, and the world crashed on the very first tick instead of giving a clear error.
#[test]
#[should_panic(expected = "масштаб мира")]
fn масштаб_меньше_базового_отвергается() {
    Space::new(0.01, Shape::Strip);
}

/// A gigantic scale is a clear error, not a crash on a memory allocation.
#[test]
#[should_panic(expected = "масштаб мира")]
fn масштаб_больше_предела_отвергается() {
    Space::new(1e7, Shape::Strip);
}

/// Founders get the diet mix exactly, dealt without draws and spread over them: not a block of
/// herbivores followed by the rest.
#[test]
fn диеты_основателей_раздаются_по_долям_вперемешку() {
    let diets = vec![55.0, 25.0, 10.0, 10.0];
    let w = World::new(&WorldConfig { seed: 4, diets, ..Default::default() });
    let diets: Vec<usize> = w.creatures.iter().map(|v| v.pheno.diet as usize).collect();
    let count = |k| diets.iter().filter(|&&d| d == k).count();
    assert_eq!([count(0), count(1), count(2), count(3)], [11, 5, 2, 2], "{diets:?}");
    assert!(diets[..10].iter().any(|&d| d != 0) && diets[10..].contains(&0), "spread: {diets:?}");
    let all_herbivores = World::new(&WorldConfig { seed: 4, diets: Vec::new(), ..Default::default() });
    assert!(all_herbivores.creatures.iter().all(|v| v.pheno.diet as usize == 0));
    // the diets draw nothing: the founders stand where they stood — meat-eaters bigger
    // (`MEAT_FOUNDER_SIZE`), so a body's margin off the edge may move them a little, and scavengers
    // held in the deep (`SCAVENGER_START_LAYER`)
    let height = w.space.height;
    for (a, b) in all_herbivores.creatures.iter().zip(&w.creatures) {
        use life_core::creature::Diet;
        if matches!(b.pheno.diet, Diet::Scavenger | Diet::Carnivore) {
            assert_eq!(b.pheno.size, a.pheno.size * life_core::config::MEAT_FOUNDER_SIZE, "starts bigger");
            assert!(
                (a.x - b.x).abs() < b.pheno.size,
                "dealing diets must not shift the world's random numbers"
            );
        }
        if b.pheno.diet == Diet::Scavenger {
            let layer = b.programs[life_core::creature::ADULT].home_layer();
            assert!(b.y > height * 0.5 && layer == (0.5, 1.0), "a scavenger starts in the deep: {}", b.y);
        } else if b.pheno.diet == Diet::Carnivore {
            continue;
        } else {
            assert_eq!((a.x, a.y), (b.x, b.y), "dealing diets must not shift the world's random numbers");
        }
    }
}

#[test]
fn стартовые_численности_известны_до_постройки_мира() {
    for cfg in [
        WorldConfig::default(),
        WorldConfig { scale: 3.0, ..Default::default() },
        WorldConfig { n_creatures: Some(7), ..Default::default() },
    ] {
        let w = World::new(&cfg);
        assert_eq!(w.creatures.len(), cfg.creatures_at_start());
    }
}

// ── performance ────────────────────────────────────────────────────────────

/// A guard against a collapse of speed: the grid keeps a tick almost linear in the population,
/// a brute-force search makes it quadratic. Two worlds of one density — ×2.5 with 1000
/// creatures and 1000 plants and ×10 with 4000/4000, both with crowds of 50 creatures — and
/// the ratio of their ticks: with the grid the big one is about four times dearer, with a
/// brute-force search about 16 times. The threshold 8 catches a breakage like «the grid stopped
/// working and everything became O(n²)», not the machine's speed: absolute milliseconds on a
/// slow CI wandered by more than a factor of two. The measurements alternate, the best of each
/// world is taken (noise only slows). Plants are topped up every tick so that the load does not melt.
/// Both worlds step on one thread: the pool's spread over however many cores are free while the
/// other tests run is no part of the algorithm's growth.
#[test]
fn тик_растёт_линейно_с_численностью() {
    fn world(n: usize) -> World {
        let mut w = World::new(&WorldConfig {
            seed: 9,
            scale: n as f64 / 400.0,
            n_creatures: Some(n),
            ..Default::default()
        });
        w.set_threads(life_core::par::Threads::One);
        let packs = n / 50;
        let cols = ((packs as f64 * w.space.width / w.space.height).sqrt().ceil() as usize).max(1);
        let rows = packs.div_ceil(cols);
        let (dx, dy) = ((w.space.width - 1000.0) / cols as f64, (w.space.height - 1000.0) / rows as f64);
        for (i, v) in w.creatures.iter_mut().enumerate() {
            let pack = i / 50;
            v.x = 500.0 + (pack % cols) as f64 * dx + (i % 10) as f64 * 8.0;
            v.y = 500.0 + (pack / cols) as f64 * dy + (i % 50 / 10) as f64 * 8.0;
        }
        w
    }
    fn ms_per_tick(w: &mut World, n: usize, rng: &mut Rng) -> f64 {
        let ticks = 20;
        let started = std::time::Instant::now();
        for _ in 0..ticks {
            for _ in w.plants.len()..n {
                let p = w.flora().plant(rng);
                w.plants.push(p);
            }
            w.step();
        }
        started.elapsed().as_secs_f64() * 1000.0 / ticks as f64
    }
    let (mut small, mut big) = (world(1000), world(4000));
    let mut rng = Rng::new(9);
    let (mut best_small, mut best_big) = (f64::INFINITY, f64::INFINITY);
    for _ in 0..4 {
        best_small = best_small.min(ms_per_tick(&mut small, 1000, &mut rng));
        best_big = best_big.min(ms_per_tick(&mut big, 4000, &mut rng));
    }
    let ratio = best_big / best_small;
    eprintln!("  [скорость] {best_small:.3} мс/тик при 1000/1000, {best_big:.3} при 4000/4000: ×{ratio:.2}");
    assert!(
        small.creatures.len() > 250 && big.creatures.len() > 1000,
        "нагрузка растаяла — замер бессмыслен"
    );
    assert!(
        ratio < 8.0,
        "вчетверо больше существ — тик дороже в {ratio:.1} раза: где-то перебор вместо сетки?"
    );
}

// ── the game: selection, following, rules on the fly ────────────────────────

#[test]
fn выбор_кликом_совпадает_с_перебором_в_живом_мире() {
    let mut w = World::new(&WorldConfig { seed: 5, ..Default::default() });
    for _ in 0..300 {
        w.step();
    }
    assert!(w.creatures.len() > 20);
    // Clicks on a grid of points: the answer is the nearest by the body's edge among those closer than
    // radius.
    let radius = 15.0;
    let mut hits = 0;
    for i in 0..60 {
        for j in 0..40 {
            let (x, y) = (i as f64 * 100.0 + 13.0, j as f64 * 100.0 + 7.0);
            let mut best: Option<(f64, u64)> = None;
            for v in &w.creatures {
                let d = ((v.x - x).powi(2) + (v.y - y).powi(2)).sqrt() - v.pheno.size / 2.0;
                if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, v.id));
                }
            }
            assert_eq!(w.pick(x, y, radius), best.map(|(_, id)| id));
            hits += best.is_some() as usize;
        }
    }
    // a few dozen creatures in a world of 2400 click points: some clicks hit
    assert!(hits >= 5, "клики хоть куда-то попали ({hits})");
}

#[test]
fn существо_находится_по_номеру_после_смертей_и_рождений() {
    let mut w = World::new(&WorldConfig { seed: 2, ..Default::default() });
    for _ in 0..600 {
        w.step();
        assert!(w.creatures.windows(2).all(|p| p[0].id < p[1].id), "существа по возрастанию id");
    }
    for v in &w.creatures {
        assert_eq!(w.creature(v.id).map(|f| f.id), Some(v.id));
    }
    assert!(w.creature(u64::MAX).is_none());
}

#[test]
fn новые_правила_пересчитывают_живых_как_новорождённых() {
    let mut w = World::new(&WorldConfig { seed: 4, ..Default::default() });
    for _ in 0..200 {
        w.step();
    }
    let rules = Rules::default().with("cost_scale", 3.0).and_then(|r| r.with("size_power", 2.0)).unwrap();
    w.set_rules(rules.clone());
    let space = w.space;
    for v in &w.creatures {
        let fresh = life_core::creature::Phenotype::at_size(&v.genome, &rules, &space, v.pheno.size);
        assert_eq!(v.pheno, fresh, "правила сохраняют фактический размер");
    }
    assert_eq!(w.rules, rules);
}

/// The food profile changes on the fly: what has grown stays in place, what is new grows
/// anew. The world is empty, so the plants are not eaten and their order is kept.
#[test]
fn профиль_еды_меняется_на_ходу() {
    let mut w = empty_world(Rules::default());
    for _ in 0..150 {
        w.step();
    }
    let before = w.plants.len();
    let third = w.space.width / 3.0;
    // a steepness of 30 across the width: right of a third of the axis — e^-10 of the food
    let rules = w
        .rules
        .with_text("plant_width_profile", "exp")
        .and_then(|r| r.with("plant_width_steepness", 30.0))
        .unwrap();
    w.set_rules(rules);
    for _ in 0..300 {
        w.step();
    }
    let (old, fresh) = w.plants.split_at(before);
    assert!(fresh.len() > 500, "выросло {}", fresh.len());
    let left = |ps: &[Plant]| ps.iter().filter(|p| p.x < third).count() as f64 / ps.len() as f64;
    assert!(left(fresh) > 0.99, "новые — у левого края: {:.3}", left(fresh));
    assert!(left(old) < 0.5, "старые остались равномерными: {:.3}", left(old));
}

#[test]
fn совпавшая_угроза_не_обездвиживает() {
    let mut v = creature(1000.0, 1000.0, BASE);
    let senses =
        senses_from(|_, _, _| None).with_threat(Threat { id: 999, x: v.x, y: v.y, gap: -50.0, half: 50.0 });
    v.health = v.max_health() * 0.1;
    let before = (v.x, v.y);
    v.step(&senses);
    assert!((v.x - before.0).hypot(v.y - before.1) > 0.0);
    assert!(v.x.is_finite() && v.y.is_finite());
}

/// A burst: in flight a creature goes `burst` times its speed for `BURST_TICKS` ticks in a row,
/// then is winded for `BURST_REST`; it pays for the steps as taken and for its muscles standing.
#[test]
fn a_burst_is_short_and_its_muscles_cost() {
    let mut v = creature(3000.0, 2000.0, BASE.with(Gene::Burst, 2.0));
    let plain = creature(3000.0, 2000.0, BASE);
    assert!(v.pheno.still_upkeep > plain.pheno.still_upkeep, "the muscles cost standing");
    assert_eq!(v.pheno.speed, plain.pheno.speed);
    // three times its size, close: it flees
    let threat = Threat { id: 99, x: 2900.0, y: 2000.0, gap: 40.0, half: 60.0 };
    let senses = senses_from(|_, _, _| None).with_threat(threat);
    let speed = v.pheno.speed;
    for tick in 0..BURST_TICKS + 5 {
        let (d, cost) = move_once(&mut v, &senses);
        let expected = if tick < BURST_TICKS { 2.0 * speed } else { speed };
        assert!((d - expected).abs() < 1e-9, "tick {tick}: went {d}, not {expected}");
        assert!(close(cost, v.pheno.step_cost(d)), "tick {tick}: paid {cost} for {d}");
    }
    assert_eq!(v.winded, BURST_REST - 5, "winded");
    // no burst without the gene, nor when wandering
    let mut p = plain.clone();
    let (d, _) = move_once(&mut p, &senses);
    assert!((d - speed).abs() < 1e-9);
    let mut w = creature(3000.0, 2000.0, BASE.with(Gene::Burst, 2.0));
    let (d, _) = move_once(&mut w, &Blind);
    assert!(d <= speed + 1e-9, "wandering: {d}");
}

/// The standard template with `blocks` put in just before its wandering.
fn standard_with(blocks: &[life_core::creature::Block]) -> life_core::creature::Program {
    use life_core::creature::{Action, Program};
    let mut all = Program::STANDARD.blocks().to_vec();
    let at = all.iter().position(|b| b.action == Action::Wander).unwrap();
    all.splice(at..at, blocks.iter().copied());
    Program::of(&all)
}

/// Torpor: a torpor block below 50% of the store, before wandering, so it sleeps only when nothing
/// else is to be done. Torpid it stands and pays `TORPOR_UPKEEP` of its standing upkeep; it wakes
/// the tick food comes into sight. The template never sleeps.
#[test]
fn torpor_saves_the_hungry_and_ends_at_food() {
    use life_core::creature::{Action, Block, Cond, Test};
    let torpor = standard_with(&[Block::when(Test::at(Cond::Fullness, 50).not(), Action::Torpor)]);
    let mut v = creature(3000.0, 1000.0, BASE);
    v.programs = [torpor; 2].into();
    v.energy = v.pheno.max_energy * 0.2;
    let (d, cost) = move_once(&mut v, &Blind);
    assert!(v.torpid && d == 0.0, "asleep: went {d}");
    assert!(close(cost, v.pheno.still_upkeep * TORPOR_UPKEEP), "paid {cost}");
    let (d, _) = move_once(&mut v, &senses_from(|_, _, _| Some((3300.0, 1000.0))));
    assert!(!v.torpid && (d - v.pheno.speed).abs() < 1e-9, "woke for food: went {d}");
    // above its share it wanders as usual, and the template never sleeps
    let mut fed = creature(3000.0, 1000.0, BASE);
    fed.programs = [torpor; 2].into();
    fed.energy = fed.pheno.max_energy * 0.6;
    move_once(&mut fed, &Blind);
    let mut base = creature(3000.0, 1000.0, BASE);
    base.energy = base.pheno.max_energy * 0.01;
    move_once(&mut base, &Blind);
    assert!(!fed.torpid && !base.torpid);
}

/// A torpid creature gets its breath back as a standing one does: «winded → torpor» sleeps
/// `BURST_REST` ticks and wakes, never for ever.
#[test]
fn a_sleep_ends_the_windedness() {
    use life_core::creature::{Action, Block, Cond, Test};
    let mut v = creature(3000.0, 1000.0, BASE);
    v.programs = [standard_with(&[Block::when(Test::is(Cond::Winded), Action::Torpor)]); 2].into();
    // neither full (its template would rest) nor hungry
    v.energy = v.pheno.max_energy * 0.6;
    v.winded = BURST_REST;
    for _ in 0..BURST_REST {
        v.step(&Blind);
        assert!(v.torpid, "asleep while winded: {} left", v.winded);
    }
    assert_eq!(v.winded, 0);
    v.step(&Blind);
    assert!(!v.torpid, "breath back: awake");
}

/// A rest is no torpor: of a rest block from 50% and a torpor block below 60%, the first in the
/// program decides. Resting it stands at its standing upkeep, awake.
#[test]
fn a_rest_is_no_torpor() {
    use life_core::creature::{Action, Block, Cond, Test};
    let rest = Block::when(Test::at(Cond::Fullness, 50), Action::Rest);
    let torpor = Block::when(Test::at(Cond::Fullness, 60).not(), Action::Torpor);
    let mut v = creature(3000.0, 1000.0, BASE);
    v.programs = [standard_with(&[rest, torpor]); 2].into();
    v.energy = v.pheno.max_energy * 0.55;
    let (d, cost) = move_once(&mut v, &Blind);
    assert_eq!(v.mind.activity, life_core::creature::Activity::Resting);
    assert!(!v.torpid && d == 0.0, "resting in place, awake");
    assert!(close(cost, v.pheno.still_upkeep), "paid {cost}");
    let mut w = creature(3000.0, 1000.0, BASE);
    w.programs = [standard_with(&[torpor, rest]); 2].into();
    w.energy = w.pheno.max_energy * 0.55;
    let (d, cost) = move_once(&mut w, &Blind);
    assert!(w.torpid && d == 0.0, "the torpor block first: asleep");
    assert!(close(cost, w.pheno.still_upkeep * TORPOR_UPKEEP), "paid {cost}");
}

/// A torpid creature eats nothing, not even the plant it lies on — no sleeping filter feeder at a
/// third of the upkeep; one standing awake in ambush eats it on the move, and without «есть на
/// ходу» it does not.
#[test]
fn a_torpid_one_eats_nothing() {
    use life_core::creature::{Action, Block, Program, Programs};
    let graze = Block::does(Action::Graze);
    for (blocks, eats) in [
        ([graze, Block::does(Action::Ambush)], true),
        ([graze, Block::does(Action::Torpor)], false),
        ([Block::does(Action::Ambush); 2], false),
    ] {
        let action = blocks[1].action;
        let rules = Rules::default().with("plant_rate", 0.0).unwrap();
        let mut w = World::new(&WorldConfig { seed: 3, n_creatures: Some(0), rules, ..Default::default() });
        w.spawn(BASE, 1000.0, 1000.0, Some(20.0));
        w.creatures[0].programs = Programs::both(Program::of(&blocks));
        w.plants.push(life_core::plant::Plant::at(1000.0, 1000.0));
        w.step();
        assert_eq!(w.creatures[0].torpid, action == Action::Torpor);
        assert_eq!(w.counters.plant_bites > 0, eats, "{action:?}");
    }
}
