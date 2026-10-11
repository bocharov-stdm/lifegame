use super::*;

/// Diet gene values, by the variants' order.
const CARNIVORE: f64 = crate::creature::Diet::Carnivore as usize as f64;
const SCAVENGER: f64 = crate::creature::Diet::Scavenger as usize as f64;
use crate::rules::Rules;
use crate::world::{World, WorldConfig};

fn dist2(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (ax - bx, ay - by);
    dx * dx + dy * dy
}

fn min(v: impl Iterator<Item = f64>) -> Option<f64> {
    v.fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))))
}

/// A hunt block's terms with the template's ratio, a caution gene of old (50 = weight 1) and
/// foreign food allowed: the hunter is hungry.
fn careful(caution: f64) -> Hunting {
    Hunting {
        ratio: 1.5,
        caution: caution / 50.0,
        taste: Taste { foreign: true, ..Taste::OWN },
        range: f64::INFINITY,
    }
}

#[test]
fn один_укус_берёт_ближайшее_растение_и_разрешает_равенство_по_порядку() {
    let mut plants = vec![Plant::at(1010.0, 1000.0), Plant::at(990.0, 1000.0)];
    let mut grid = Grid::new(GRID_CELL);
    grid.rebuild(&Space::default(), plants.iter().map(|p| (p.x, p.y)));
    let mut eaten_today = vec![false; plants.len()];
    for _ in 0..4 {
        eaten_today.fill(false);
        assert_eq!(
            bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
            Some((false, 1010.0, 1000.0))
        );
    }
    assert_eq!((plants[0].portions, plants[1].portions), (1, 5));
    eaten_today.fill(false);
    assert_eq!(
        bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
        Some((true, 1010.0, 1000.0))
    );
    assert!(!plants[0].alive());
    assert_eq!(
        bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0),
        Some((false, 990.0, 1000.0))
    );
    assert_eq!(plants[1].portions, 4);
    assert_eq!(bite_plant(&grid, &mut plants, &mut eaten_today, 1000.0, 1000.0, 20.0), None);
}

#[test]
fn одно_растение_не_отдаёт_две_порции_за_тик() {
    let mut plants = [Plant::at(1000.0, 1000.0)];
    let mut grid = Grid::new(GRID_CELL);
    grid.rebuild(&Space::default(), plants.iter().map(|p| (p.x, p.y)));
    let mut bitten = [false];
    assert_eq!(bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0).map(|b| b.0), Some(false));
    assert_eq!(bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0), None);
    assert_eq!(plants[0].portions, 4);
    bitten.fill(false);
    assert_eq!(bite_plant(&grid, &mut plants, &mut bitten, 1000.0, 1000.0, 40.0).map(|b| b.0), Some(false));
    assert_eq!(plants[0].portions, 3);
}

#[test]
fn падаль_выбирается_по_порциям_и_дороге_но_не_в_тик_смерти() {
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    w.spawn(
        crate::CreatureGenome::BASE.with(crate::genome::creature::Gene::Diet, CARNIVORE),
        1000.0,
        1000.0,
        Some(40.0),
    );
    let v = &w.creatures[0];
    let me = Me {
        age: 0.0,
        winded: false,
        x: v.x,
        y: v.y,
        energy: v.energy,
        kinship: v.kinship(),
        health_share: 1.0,
        health: v.pheno.size,
        pheno: &v.pheno,
    };
    let mut near = Corpse::from_creature(v, 1);
    near.owner = 2;
    near.x = 1005.0;
    near.remaining = 5.0;
    let mut rich = Corpse::from_creature(v, 1);
    rich.owner = 3;
    rich.x = 1040.0;
    let corpses = [near, rich];
    let mut cgrid = Grid::new(GRID_CELL);
    cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
    let food = Grid::new(GRID_CELL);
    let mut view = GridSenses {
        food: &food,
        plants: &[],
        corpse_grid: &cgrid,
        corpses: &corpses,
        now: 1,
        herd: &Herd::new(),
    };
    assert!(view.best_corpse(&me, Taste::OWN).is_none());
    view.now = 2;
    assert_eq!(view.best_corpse(&me, Taste::OWN).unwrap().owner, 3);
}

/// Of a fresh and a rotten corpse at equal distance a carnivore picks the fresh one, a
/// scavenger the rot (each on its own food), and a herbivore sees neither.
#[test]
fn падальщик_выбирает_гниль_мясоед_свежее() {
    use crate::genome::creature::Gene;
    let now = 1000;
    for (diet, want) in [(CARNIVORE, Some(1)), (SCAVENGER, Some(2)), (0.0, None), (1.0, Some(1))] {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        w.spawn(crate::CreatureGenome::BASE.with(Gene::Diet, diet), 1000.0, 1000.0, Some(40.0));
        let v = &w.creatures[0];
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            health_share: 1.0,
            health: v.pheno.size,
            pheno: &v.pheno,
        };
        let corpse = |owner: u64, x: f64, born: u64| {
            let mut c = Corpse::from_creature(v, born);
            (c.owner, c.x, c.bottom) = (owner, x, c.y0);
            (c.initial, c.remaining) = (100.0, 60.0);
            c
        };
        let corpses = [corpse(1, 1100.0, now - 10), corpse(2, 900.0, now - 700)];
        use crate::corpse::Stage;
        assert_eq!((corpses[0].stage(now), corpses[1].stage(now)), (Stage::Fresh, Stage::Rot));
        let mut cgrid = Grid::new(GRID_CELL);
        cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
        let food = Grid::new(GRID_CELL);
        let view = GridSenses {
            food: &food,
            plants: &[],
            corpse_grid: &cgrid,
            corpses: &corpses,
            now,
            herd: &Herd::new(),
        };
        assert_eq!(view.best_corpse(&me, Taste::OWN).map(|c| c.owner), want, "diet {diet}");
    }
}

/// Whether a creature of `diet`, taking foreign food or not, finds through the corpse grid at
/// tick 1000 a corpse that died at `born` lying `far` of its vision away on its level.
fn finds_corpse(diet: f64, foreign: bool, born: u64, far: f64) -> bool {
    use crate::genome::creature::Gene;
    let now = 1000;
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    w.spawn(crate::CreatureGenome::BASE.with(Gene::Diet, diet), 1000.0, 1000.0, Some(10.0));
    let v = &w.creatures[0];
    let me = Me {
        age: 0.0,
        winded: false,
        x: v.x,
        y: v.y,
        energy: v.energy,
        kinship: v.kinship(),
        health_share: 1.0,
        health: v.pheno.size,
        pheno: &v.pheno,
    };
    let mut c = Corpse::from_creature(v, born);
    (c.owner, c.x, c.bottom) = (1, 1000.0 + far * v.pheno.vision, c.y0);
    let corpses = [c];
    let mut cgrid = Grid::new(GRID_CELL);
    cgrid.rebuild(&w.space, corpses.iter().map(|c| (c.x, c.y)));
    let food = Grid::new(GRID_CELL);
    let view = GridSenses {
        food: &food,
        plants: &[],
        corpse_grid: &cgrid,
        corpses: &corpses,
        now,
        herd: &Herd::new(),
    };
    view.best_corpse(&me, Taste { foreign, ..Taste::OWN }).is_some()
}

/// The scavenger smells corpses from afar; the omnivore finds them by sight. On its own food it
/// leaves a fresh corpse to the hunters; with its program's «foreign food» setting it goes for
/// it too.
#[test]
fn падальщик_чует_издалека_и_сытым_не_берёт_свежее() {
    let now = 1000;
    for (diet, foreign, born, far, want) in [
        (SCAVENGER, false, now - 700, 1.8, true),
        (1.0, false, now - 700, 1.8, false),
        (SCAVENGER, false, now - 10, 0.5, false),
        (SCAVENGER, true, now - 10, 0.5, true),
    ] {
        assert_eq!(
            finds_corpse(diet, foreign, born, far),
            want,
            "diet {diet}, foreign {foreign}, born {born}"
        );
    }
}

/// Each diet smells corpses as far as its `DIET_SMELL` edge says, and no farther, through the
/// corpse grid itself: the scavenger at 3× its vision, the carnivore at 1.5×, the omnivore at
/// 1.2×. Hungry, so any corpse will do: even the herbivore goes for fresh meat (10%, not its own
/// food) — only with its program's «foreign food», and never for rot.
#[test]
fn each_diet_smells_as_far_as_its_edge() {
    use crate::config::DIET_SMELL;
    for diet in [1.0, SCAVENGER, CARNIVORE] {
        let smell = DIET_SMELL[diet as usize];
        assert!(finds_corpse(diet, true, 990, smell - 0.05), "diet {diet} smells at {smell}× vision");
        assert!(!finds_corpse(diet, true, 990, smell + 0.05), "diet {diet} smells no farther than {smell}×");
    }
    assert_eq!(DIET_SMELL, [1.0, 1.2, 3.0, 1.5], "the ranges the user calibrated");
    assert!(finds_corpse(0.0, true, 990, 0.5), "a hungry herbivore takes fresh meat");
    assert!(!finds_corpse(0.0, false, 990, 0.5), "not its own food");
    assert!(!finds_corpse(0.0, true, 300, 0.5), "a herbivore does not go for rot");
}

/// Every query of the tick is checked against a brute-force search of all creatures — on the
/// real positions, sizes and kinship of a live world, not on invented points. It catches a
/// wrong query radius, a forgotten half of a body, missed kin and a broken grid. The second
/// world has a cheap size: giants grow there, and the query radius grows with them.
#[test]
fn запросы_к_сеткам_совпадают_с_перебором_в_живом_мире() {
    // Food rich enough for bodies past 100. At 120 the biggest reached ~86 once the carnivore
    // and the scavenger smelled farther (2026-09-27); at 180 it is ~190.
    let giants = Rules::default().with("size_power", 1.0).unwrap().with("plant_energy", 180.0).unwrap();
    for (seed, rules) in [(1, Rules::default()), (4, giants)] {
        // the former founders' mix: hunters from the start, for the prey queries to check
        let diets = vec![55.0, 25.0, 10.0, 10.0];
        let mut w = World::new(&WorldConfig { seed, rules, diets, ..Default::default() });
        let mut food = Grid::new(GRID_CELL);
        let mut snapshot = Herd::new();
        let (mut checked, mut threats, mut spared, mut hunters) = (0, 0, 0, 0);
        let (mut hunts, mut guarded, mut seen_prey, mut richer) = (0, 0, 0, 0);
        for tick in 0..1500 {
            w.step();
            if tick % 50 != 0 {
                continue;
            }
            // some plants were "eaten this tick": the queries must skip them
            let mut plants = w.plants.clone();
            plants.iter_mut().step_by(5).for_each(|p| p.portions = 0);
            food.rebuild(&w.space, plants.iter().map(|p| (p.x, p.y)));

            for v in &w.creatures {
                let got = nearest_plant(&food, &plants, v.x, v.y, v.pheno.vision2)
                    .map(|(px, py)| dist2(px, py, v.x, v.y));
                let want = min(plants
                    .iter()
                    .filter(|p| p.alive())
                    .map(|p| dist2(p.x, p.y, v.x, v.y))
                    .filter(|&d2| d2 < v.pheno.vision2));
                assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение");
                // the same query limited to a disc beside it
                let inside = |x: f64, y: f64| dist2(x, y, v.x + 150.0, v.y) <= 200.0 * 200.0;
                let got = nearest_plant_where(&food, &plants, v.x, v.y, v.pheno.vision2, inside)
                    .map(|(px, py)| dist2(px, py, v.x, v.y));
                let want = min(plants
                    .iter()
                    .filter(|p| p.alive() && inside(p.x, p.y))
                    .map(|p| dist2(p.x, p.y, v.x, v.y))
                    .filter(|&d2| d2 < v.pheno.vision2));
                assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение в круге");
                // the most profitable plant, for some within a band of depth
                let me = v.me();
                let reach = if v.id % 2 == 0 { w.space.height * 0.05 } else { f64::INFINITY };
                let taste = Taste { foreign: false, reach, layer: v.pheno.layer(v.mind.stance.layer) };
                let got = best_plant(&food, &plants, &me, taste);
                let want = plants
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        p.alive() && dist2(p.x, p.y, v.x, v.y) < v.pheno.vision2 && taste.admits(p.y)
                    })
                    .map(|(i, p)| (i, p, plant_worth(&me, p.x, p.y, p.portions), dist2(p.x, p.y, v.x, v.y)))
                    .max_by(|a, b| a.2.total_cmp(&b.2).then(b.3.total_cmp(&a.3)).then(b.0.cmp(&a.0)))
                    .map(|(_, p, ..)| (p.x, p.y));
                assert_eq!(got, want, "seed {seed}, tick {tick}: the most profitable plant");
                let nearest =
                    nearest_plant_where(&food, &plants, v.x, v.y, v.pheno.vision2, |_, y| taste.admits(y));
                richer += (got != nearest) as usize;

                let expected = plants
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.alive())
                    .map(|(i, p)| (i, dist2(p.x, p.y, v.x, v.y)))
                    .filter(|(_, d2)| *d2 <= v.pheno.size * v.pheno.size)
                    .min_by(|(ia, da), (ib, db)| da.total_cmp(db).then(ia.cmp(ib)))
                    .map(|(i, _)| i);
                let mut bitten = plants.clone();
                let mut eaten_today = vec![false; plants.len()];
                let last =
                    bite_plant(&food, &mut bitten, &mut eaten_today, v.x, v.y, v.pheno.size).map(|b| b.0);
                assert_eq!(last, expected.map(|i| plants[i].portions == 1));
                for (i, (before, after)) in plants.iter().zip(&bitten).enumerate() {
                    assert_eq!(after.portions, before.portions - u8::from(expected == Some(i)));
                    assert_eq!(
                        after.alive(),
                        before.alive() && !(expected == Some(i) && before.portions == 1)
                    );
                }
                checked += 1;
            }
            // threats, from a snapshot of the live world as at the start of the phase; each
            // attacker's own program (its hunt blocks) decides whom it can threaten
            snapshot.rebuild(&w.space, &w.creatures);
            let share = crate::config::FLEE_SIGHT_SHARE;
            let lookers = w.creatures.iter().flat_map(|v| [(v, v.pheno.vision), (v, v.pheno.vision * share)]);
            for (v, within) in lookers {
                let size = v.pheno.size;
                let got = nearest_threats(&snapshot, v.kinship(), v.x, v.y, size, within);
                let can_eat_me = |u: &&Creature| {
                    u.pheno.hunts()
                        && u.menace().hunt().is_some_and(|ratio| size <= u.pheno.size / ratio)
                        && dist2(u.x, u.y, v.x, v.y) < (within + u.pheno.half).powi(2)
                };
                let gap = |u: &Creature| dist2(u.x, u.y, v.x, v.y).sqrt() - u.pheno.half;
                let stranger = |u: &&Creature| !v.kinship().kin(u.kinship());
                let want = min(w.creatures.iter().filter(can_eat_me).filter(stranger).map(gap));
                assert_eq!(got.0.map(|t| t.gap), want, "сид {seed}, тик {tick}: угроза");
                let hunting = |u: &&Creature| u.mind.attack.is_some();
                let want =
                    min(w.creatures.iter().filter(can_eat_me).filter(stranger).filter(hunting).map(gap));
                assert_eq!(got.1.map(|t| t.gap), want, "seed {seed}, tick {tick}: a hunter");
                threats += got.0.is_some() as usize;
                hunters += got.1.is_some() as usize;
                // kin that would otherwise be a threat: without it the kinship check is empty
                spared += w
                    .creatures
                    .iter()
                    .filter(can_eat_me)
                    .filter(|u| u.id != v.id && v.kinship().kin(u.kinship()))
                    .count();
            }
            // prey: the same valuation over every creature instead of the grid and the buffers
            let view = GridSenses {
                food: &food,
                plants: &plants,
                corpse_grid: &Grid::new(GRID_CELL),
                corpses: &[],
                now: tick,
                herd: &snapshot,
            };
            for v in &w.creatures {
                let me = Me {
                    age: 0.0,
                    winded: false,
                    x: v.x,
                    y: v.y,
                    energy: v.energy,
                    kinship: v.kinship(),
                    pheno: &v.pheno,
                    health_share: v.health / v.max_health(),
                    health: v.health,
                };
                // terms that differ from creature to creature: ratio, foreign food and reach
                let reach = if v.id % 4 == 1 { w.space.height * 0.1 } else { f64::INFINITY };
                let hunt = Hunting {
                    ratio: 1.2 + (v.id % 3) as f64 * 0.3,
                    caution: 1.0,
                    taste: Taste { foreign: v.id % 2 == 0, reach, layer: v.pheno.layer(v.mind.stance.layer) },
                    range: v.pheno.vision * if v.id % 5 == 2 { 0.5 } else { 1.0 },
                };
                let sees = |u: &Creature| (u.x - v.x).hypot(u.y - v.y) <= hunt.range && u.id != v.id;
                // prey in sight: the nearest it could eat, by the gap to its body's edge
                let within = v.pheno.vision * 0.6;
                let got = nearest_prey(&snapshot, &me, hunt.ratio, hunt.taste, within).map(|t| t.gap);
                let want = min(w
                    .creatures
                    .iter()
                    .filter(|u| {
                        v.pheno.hunts_now(hunt.taste.foreign)
                            && u.id != v.id
                            && u.pheno.size <= v.pheno.size / hunt.ratio
                            && !v.kinship().kin(u.kinship())
                            && hunt.taste.admits(u.y)
                    })
                    .map(|u| (u.x - v.x).hypot(u.y - v.y) - u.pheno.half)
                    .filter(|&gap| gap < within));
                assert_eq!(got, want, "seed {seed}, tick {tick}: prey in sight");
                seen_prey += got.is_some() as usize;
                let strike = |u: &Creature| u.pheno.strike_on(v.pheno.size);
                let mut candidates: Vec<usize> = (0..w.creatures.len())
                    .filter(|&j| {
                        let u = &w.creatures[j];
                        v.pheno.hunts_now(hunt.taste.foreign)
                            && sees(u)
                            && u.pheno.size <= v.pheno.size / hunt.ratio
                            && !v.kinship().kin(u.kinship())
                            && hunt.taste.admits(u.y)
                    })
                    .collect();
                // a full buffer keeps the nearest candidates, ties by id
                let d2 = |j: usize| (w.creatures[j].x - v.x).powi(2) + (w.creatures[j].y - v.y).powi(2);
                candidates.sort_by(|&a, &b| {
                    d2(a).total_cmp(&d2(b)).then(w.creatures[a].id.cmp(&w.creatures[b].id))
                });
                candidates.truncate(SEEN_PREY);
                let want = candidates
                    .iter()
                    .map(|&j| {
                        let u = &w.creatures[j];
                        let mut allies = 0.0;
                        if let Some(p) = w.creatures.iter().find(|p| p.id == u.parent)
                            && p.menace().defends
                            && p.kinship().kin(u.kinship())
                            && !v.kinship().kin(p.kinship())
                            && (p.x - v.x).hypot(p.y - v.y) <= v.pheno.vision
                        {
                            allies += strike(p);
                        }
                        guarded += (allies > 0.0) as usize;
                        Prey::of(&snapshot.seen[j], &me, allies, hunt.caution)
                    })
                    .fold(None, |b: Option<Prey>, p| {
                        if b.is_none_or(|b| p.score > b.score || (p.score == b.score && p.id < b.id)) {
                            Some(p)
                        } else {
                            b
                        }
                    });
                let got = view.prey(&me, None, None, hunt);
                // Sums in another order may differ in the last bits: compare scores, not ties.
                let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12);
                match (got, want) {
                    (Some(g), Some(e)) => assert!(
                        close(g.score, e.score),
                        "seed {seed}, tick {tick}: prey {} ({}) vs {} ({})",
                        g.id,
                        g.score,
                        e.id,
                        e.score
                    ),
                    (g, e) => {
                        assert_eq!(g.map(|p| p.id), e.map(|p| p.id), "seed {seed}, tick {tick}: prey")
                    }
                }
                hunts += got.is_some_and(|p| p.score > 0.0) as usize;
            }
        }
        assert!(checked > 1000, "сид {seed}: проверено всего {checked} запросов — мир вымер?");
        assert!(threats > 100, "сид {seed}: угроз нашлось всего {threats}");
        assert!(spared > 10, "сид {seed}: родни среди угроз всего {spared}");
        assert!(hunters > 10, "seed {seed}: only {hunters} threats were hunting");
        assert!(hunts > 100, "seed {seed}: only {hunts} worthwhile hunts");
        assert!(guarded > 10, "seed {seed}: only {guarded} candidates covered by a parent");
        assert!(seen_prey > 100, "seed {seed}: prey in sight only {seen_prey} times");
        assert!(richer > 10, "seed {seed}: the most profitable plant was the nearest but {richer} times");
        if seed == 4 {
            let biggest = w.creatures.iter().fold(0.0_f64, |m, v| m.max(v.pheno.size));
            assert!(biggest > 100.0, "гиганты не выросли ({biggest:.0}) — вторая часть теста бессмысленна");
        }
    }
}

/// A world with a hunter (size 80) at (1000, 1000) holding `energy_share` of its store; `setup`
/// adds the rest. Returns the hunter's best prey by its valuation on the terms of `hunt`.
fn best_prey(hunt: Hunting, energy_share: f64, setup: impl Fn(&mut World)) -> Option<Prey> {
    use crate::genome::creature::Gene;
    let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
    let hunter = crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, CARNIVORE);
    w.spawn(hunter, 1000.0, 1000.0, None);
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * energy_share;
    setup(&mut w);
    let mut herd = Herd::new();
    herd.rebuild(&w.space, &w.creatures);
    let food = Grid::new(GRID_CELL);
    let view = GridSenses {
        food: &food,
        plants: &[],
        corpse_grid: &Grid::new(GRID_CELL),
        corpses: &[],
        now: 1,
        herd: &herd,
    };
    let v = &w.creatures[0];
    let me = Me {
        age: 0.0,
        winded: false,
        x: v.x,
        y: v.y,
        energy: v.energy,
        kinship: v.kinship(),
        pheno: &v.pheno,
        health_share: 1.0,
        health: v.health,
    };
    view.prey(&me, None, None, hunt)
}

/// The reach setting holds the hunt too: a hunter keeping to the upper tenth of the depth with
/// a reach of 5% leaves prey far below alone, and takes it when it is within reach.
#[test]
fn a_hunter_leaves_prey_past_its_layers_reach() {
    use crate::genome::creature::Gene;
    let hunt = |prey_y: f64| {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let hunter = crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, CARNIVORE);
        w.spawn(hunter, 1000.0, 300.0, None);
        w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.3;
        w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 20.0), 1000.0, prey_y, None);
        w.creatures[1].birth_size = 10.0;
        let mut herd = Herd::new();
        herd.rebuild(&w.space, &w.creatures);
        let food = Grid::new(GRID_CELL);
        let view = GridSenses {
            food: &food,
            plants: &[],
            corpse_grid: &Grid::new(GRID_CELL),
            corpses: &[],
            now: 1,
            herd: &herd,
        };
        let v = &w.creatures[0];
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.health,
        };
        // its layer the upper tenth of the depth
        let taste = Taste { foreign: true, reach: w.space.height * 0.05, layer: (0.0, w.space.height * 0.1) };
        view.prey(&me, None, None, Hunting { taste, ..careful(50.0) }).map(|p| p.id)
    };
    // the layer ends at 400, the reach at 600; the hunter sees 400 around it
    assert!(hunt(550.0).is_some(), "within reach");
    assert!(hunt(680.0).is_none(), "past its reach, though in sight");
}

/// A small creature (size 20, grown from 10: its body is meat) at `x`.
fn small(w: &mut World, x: f64) -> u64 {
    use crate::genome::creature::Gene;
    let id = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 20.0), x, 1000.0, None);
    w.creatures.last_mut().unwrap().birth_size = 10.0;
    id
}

/// A parent of `size` at `x` and its growing child (half its adult size) of `child` at
/// `child_x`: the parent still knows it and covers it.
fn guarded(w: &mut World, x: f64, size: f64, child_x: f64, child: f64) -> u64 {
    use crate::genome::creature::Gene;
    let parent = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, size), x, 1000.0, None);
    w.creatures.last_mut().unwrap().mind.knew_until = 1.0;
    let id = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, child), child_x, 1000.0, None);
    let v = w.creatures.last_mut().unwrap();
    (v.parent, v.birth_size) = (parent, child / 2.0);
    v.genome = v.genome.with(Gene::Size, child * 2.0);
    id
}

#[test]
fn a_careful_hunter_leaves_a_guarded_prey_for_a_lone_one() {
    let (near, far) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
    let setup = |w: &mut World| {
        near.set(guarded(w, 1100.0, 120.0, 1060.0, 20.0)); // closer, but its parent is beside it
        far.set(small(w, 925.0)); // a loner a little farther away
    };
    let careless = best_prey(careful(0.0), 0.3, setup).unwrap();
    let careful = best_prey(careful(50.0), 0.3, setup).unwrap();
    assert_eq!(careless.id, near.get(), "a careless hunter takes the nearest");
    assert_eq!(careful.id, far.get(), "a careful hunter took the guarded one");
    assert!(careful.score > 0.0);
}

#[test]
fn a_full_hunter_gains_nothing_and_a_hungry_one_something() {
    let setup = |w: &mut World| {
        small(w, 1060.0);
    };
    assert_eq!(best_prey(careful(50.0), 1.0, setup).unwrap().score, 0.0);
    assert!(best_prey(careful(50.0), 0.3, setup).unwrap().score > 0.0);
}

#[test]
fn a_hunt_that_looks_deadly_is_worth_nothing_unless_careless() {
    // Prey as big as the hunter, that fights back, with a bigger parent beside it.
    let setup = |w: &mut World| {
        guarded(w, 1150.0, 120.0, 1080.0, 80.0);
    };
    // a hunt block that takes prey as big as the hunter
    assert_eq!(best_prey(Hunting { ratio: 1.0, ..careful(50.0) }, 0.3, setup).unwrap().score, 0.0);
    assert!(best_prey(Hunting { ratio: 1.0, ..careful(0.0) }, 0.3, setup).unwrap().score > 0.0);
}

#[test]
fn a_parent_in_sight_covers_its_growing_child_only_while_it_knows_it() {
    use crate::genome::creature::Gene;
    let with_parent = |care: f64, defends: bool| {
        move |w: &mut World| {
            let parent = w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 60.0), 1100.0, 1000.0, None);
            // the old `care` gene was half of how long it knows its children
            let p = w.creatures.last_mut().unwrap();
            p.mind.knew_until = (care / 50.0).min(1.0);
            if !defends {
                use crate::creature::{Action, Block, Program, Programs};
                p.programs = Programs::both(Program::of(&[Block::does(Action::Wander)]));
            }
            small(w, 1060.0);
            let child = w.creatures.last_mut().unwrap();
            child.parent = parent;
            child.genome = child.genome.with(Gene::Size, 40.0); // half grown
        }
    };
    let alone = best_prey(careful(50.0), 0.3, |w: &mut World| {
        small(w, 1060.0);
    })
    .unwrap();
    let covered = best_prey(careful(50.0), 0.3, with_parent(50.0, true)).unwrap();
    let forgotten = best_prey(careful(50.0), 0.3, with_parent(10.0, true)).unwrap();
    let careless = best_prey(careful(50.0), 0.3, with_parent(50.0, false)).unwrap();
    // the hunter is the parent's growing child too: the parent strikes neither of its own
    let brother = best_prey(careful(50.0), 0.3, |w: &mut World| {
        with_parent(50.0, true)(w);
        let parent = w.creatures[1].id;
        let hunter = &mut w.creatures[0];
        hunter.parent = parent;
        hunter.genome = hunter.genome.with(Gene::Size, hunter.pheno.size * 2.0);
    })
    .unwrap();
    assert!(covered.score < alone.score, "a parent in sight did not count");
    assert_eq!(forgotten.score, alone.score, "a parent that forgot its child still covers it");
    assert_eq!(careless.score, alone.score, "a parent whose program does not defend covers it");
    assert_eq!(brother.score, alone.score, "a parent covers its child from its other child");
}

/// A full candidate buffer keeps the nearest prey: a crowd in the grid rows above the hunter
/// (scanned first) does not hide the prey next to it.
#[test]
fn a_crowd_higher_up_does_not_hide_the_nearest_prey() {
    let near = std::cell::Cell::new(0);
    let best = best_prey(careful(50.0), 0.3, |w: &mut World| {
        // a row of small loners in the grid row above the hunter's, 300..360 away
        let small_genome = crate::CreatureGenome::BASE.with(crate::genome::creature::Gene::Size, 20.0);
        for i in 0..SEEN_PREY {
            w.spawn(small_genome, 810.0 + 6.0 * i as f64, 700.0, None);
        }
        near.set(small(w, 1030.0));
    })
    .unwrap();
    assert_eq!(best.id, near.get(), "the nearest prey was never looked at");
}

/// Whom a stranger fears: anyone whose program hunts bodies of its size (the threat), and of
/// them the ones hunting somebody now (the hunter). A carnivore whose program has no hunt block
/// threatens no one. The herbivore digests a little fresh meat (10%): before its first move its
/// hunt block makes it feared like any hunter; a diet with no fresh meat eats no one.
#[test]
fn a_threat_is_whoever_could_hunt_it_a_hunter_whoever_does() {
    use crate::creature::{Action, Block, Program};
    use crate::genome::creature::Gene;
    for (diet, hunts, hunting, threat, hunter) in [
        (SCAVENGER, true, false, true, false),
        (CARNIVORE, true, true, true, true),
        (CARNIVORE, false, true, false, false),
        (0.0, true, true, true, true),
        (-1.0, true, true, false, false),
    ] {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        // -1: a herbivore that digests no fresh meat
        let diet = if diet < 0.0 {
            w.set_rules(w.rules.with("herbivore_meat", 0.0).unwrap());
            0.0
        } else {
            diet
        };
        w.spawn(
            crate::CreatureGenome::BASE.with(Gene::Size, 80.0).with(Gene::Diet, diet),
            1000.0,
            1000.0,
            None,
        );
        if !hunts {
            w.creatures[0].programs = [Program::of(&[Block::does(Action::Wander)]); 2].into();
        }
        // 60 from the edge of the big body
        w.spawn(crate::CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, None);
        if hunting {
            w.creatures[0].mind.attack = Some(999);
        }
        let mut herd = Herd::new();
        herd.rebuild(&w.space, &w.creatures);
        let v = &w.creatures[1];
        let (any, hunts_now) = nearest_threats(&herd, v.kinship(), v.x, v.y, v.pheno.size, 100.0);
        let case = format!("diet {diet}, hunt block {hunts}, hunting {hunting}");
        assert_eq!((any.is_some(), hunts_now.is_some()), (threat, hunter), "{case}");
        let (none, _) = nearest_threats(&herd, v.kinship(), v.x, v.y, v.pheno.size, 50.0);
        assert!(none.is_none(), "{case}: farther than asked");
    }
}
