//! Simultaneous melee strikes and weak shots. The world makes corpses after the fight.
use crate::config::SHOT_RANGE_SIZES;
use crate::creature::{Creature, Death};
use crate::grid::Grid;
use crate::{Counters, Rules, Space};

/// A short trace of a shot actually fired, for the window and the chronicle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub tick: u64,
}

#[derive(Clone, Copy)]
struct Hit {
    attacker: usize,
    victim: usize,
    damage: f64,
    cost: f64,
    ranged: bool,
}

pub(crate) struct CombatPolicy<'a> {
    /// What each creature eats this tick (missing entries: not eating). Whether it fights for it
    /// is its program's setting (`Stance::rival`).
    pub feeding: &'a [Feeding],
}

/// What a creature eats this tick, for fights at food: plants (any, side by side), or one corpse
/// (its owner's id).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Feeding {
    #[default]
    Nothing,
    Plants,
    Corpse(u64),
}

/// A defence (its program chose to fight back) and a parent defending its child
/// (`Action::DefendChild`) strike a body of any size. A hunt and a rival at food stay limited by
/// the size ratio of the block that chose them. Being struck strikes back only if its program
/// chooses it (`Action::FightBack`).
fn defending(v: &Creature, enemy: u64) -> bool {
    v.mind.stance.defending == Some(enemy)
        || (v.mind.attack == Some(enemy) && v.mind.activity == crate::creature::Activity::Alarm)
}

/// Whether a strike on a body of `size` keeps within the ratio its program allows (0: none).
fn small_enough(v: &Creature, size: f64, ratio: f64) -> bool {
    ratio > 0.0 && size <= v.pheno.size / ratio
}

#[cfg(test)]
pub(crate) fn resolve(
    space: &Space,
    rules: &Rules,
    creatures: &mut [Creature],
    grid: &mut Grid,
    counters: &mut Counters,
    tick: u64,
) -> Vec<Shot> {
    resolve_with(space, rules, creatures, grid, counters, tick, CombatPolicy { feeding: &[] })
}

/// The strikes and shots of a tick, all at once; returns the shots made.
pub(crate) fn resolve_with(
    space: &Space,
    rules: &Rules,
    creatures: &mut [Creature],
    grid: &mut Grid,
    counters: &mut Counters,
    tick: u64,
    policy: CombatPolicy<'_>,
) -> Vec<Shot> {
    let CombatPolicy { feeding } = policy;
    let feeds = |i: usize| feeding.get(i).copied().unwrap_or_default();
    grid.rebuild(space, creatures.iter().map(|v| (v.x, v.y)));
    let max_half = creatures.iter().filter(|v| v.alive).fold(0.0_f64, |m, v| m.max(v.pheno.half));
    let mut hits = Vec::new();
    for (i, v) in creatures.iter().enumerate() {
        let cost = v.pheno.strike_cost();
        if !v.alive || v.fleeing() {
            continue;
        }
        // At its food, if its program says so, it strikes a smaller stranger eating the same food.
        let rival = v.mind.stance.rival > 0.0 && feeds(i) != Feeding::Nothing;
        // with no target, no defence and no rival there is nobody it would strike: no looking
        let aims = v.mind.attack.is_some() || v.mind.stance.defending.is_some() || rival;
        if aims && v.energy > cost {
            let mut target = None;
            grid.for_each_near(v.x, v.y, v.pheno.half + max_half, |j, _, _| {
                let u = &creatures[j];
                if !u.alive || v.kinship().kin(u.kinship()) {
                    return;
                }
                if (v.x - u.x).hypot(v.y - u.y) > v.pheno.half + u.pheno.half {
                    return;
                }
                // Only a chosen target, a defence or a rival at its food: a creature does not bite
                // whoever it bumps into.
                let selected = v.mind.attack == Some(u.id);
                let defense = defending(v, u.id);
                let rival_here = rival && feeds(j) == feeds(i);
                if !(selected || defense || rival_here) {
                    return;
                }
                let ratio = if selected { v.mind.stance.strike_ratio } else { v.mind.stance.rival };
                if !defense && !small_enough(v, u.pheno.size, ratio) {
                    return;
                }
                if target.is_none_or(|k: usize| {
                    (u.id != v.mind.attack.unwrap_or(0), u.id)
                        < (creatures[k].id != v.mind.attack.unwrap_or(0), creatures[k].id)
                }) {
                    target = Some(j);
                }
            });
            if let Some(j) = target {
                hits.push(Hit {
                    attacker: i,
                    victim: j,
                    damage: v.pheno.strike_on(creatures[j].pheno.size),
                    cost,
                    ranged: false,
                });
                continue; // in contact the melee strike takes priority
            }
        }
        // it shoots only when its program says so this tick (`Action::Shoot`)
        let Some(shoot) = v.mind.stance.shoot else { continue };
        if v.mind.last_shot != 0 && tick.saturating_sub(v.mind.last_shot) < rules.shot_period as u64 {
            continue;
        }
        let shot_cost = v.pheno.size * rules.shot_energy_share;
        if v.energy <= shot_cost || v.energy - shot_cost < v.pheno.max_energy * shoot.keep {
            continue;
        }
        // Whom it chose (a hunt, a fight back, its child's enemy); being struck alone is no target —
        // striking back is its program's choice.
        let Some(desired) = v.mind.attack else { continue };
        let range = v.pheno.vision.min(v.pheno.size * SHOT_RANGE_SIZES);
        grid.for_each_near(v.x, v.y, range, |j, _, _| {
            let u = &creatures[j];
            if !u.alive || u.id != desired || v.kinship().kin(u.kinship()) {
                return;
            }
            let dist = (v.x - u.x).hypot(v.y - u.y);
            let contact = v.pheno.half + u.pheno.half;
            let ready_range = contact + (range - contact).max(0.0) * shoot.from;
            if dist <= contact || dist > range || dist > ready_range {
                return;
            }
            if !defending(v, u.id)
                && (v.fleeing() || !small_enough(v, u.pheno.size, v.mind.stance.strike_ratio))
            {
                return;
            }
            // a shot weakens with old age like the melee strike (`Phenotype::strike`)
            hits.push(Hit {
                attacker: i,
                victim: j,
                damage: (v.pheno.size * rules.shot_damage_share * v.pheno.strike_bonus * v.pheno.vigour)
                    .min(u.max_health() * 0.25),
                cost: shot_cost,
                ranged: true,
            });
        });
    }
    let mut shots = Vec::new();
    let mut damage = vec![0.0; creatures.len()];
    // who dealt each victim the most this tick: the killer if it dies
    let mut killer: Vec<Option<(f64, usize)>> = vec![None; creatures.len()];
    for hit in hits {
        let Hit { attacker: i, victim: j, damage: d, cost, ranged } = hit;
        let signal =
            crate::creature::Sighting { enemy: creatures[i].id, x: creatures[i].x, y: creatures[i].y, tick };
        if creatures[j].mind.hit.is_none_or(|h| h.tick != tick || signal.enemy < h.enemy) {
            creatures[j].mind.hit = Some(signal);
            creatures[j].mind.alarm = Some(signal);
        }
        creatures[i].energy -= cost;
        if ranged {
            creatures[i].mind.last_shot = tick;
            shots.push(Shot {
                from: (creatures[i].x, creatures[i].y),
                to: (creatures[j].x, creatures[j].y),
                tick,
            });
        }
        creatures[i].peaceful_ticks = 0;
        creatures[j].peaceful_ticks = 0;
        damage[j] += d;
        let diets = (creatures[i].pheno.diet as usize, creatures[j].pheno.diet as usize);
        counters.by_diet.strikes[diets.0][diets.1] += 1;
        if killer[j].is_none_or(|(most, _)| d > most) {
            killer[j] = Some((d, diets.0));
        }
    }
    for ((v, d), killer) in creatures.iter_mut().zip(damage).zip(killer) {
        if !v.alive {
            continue;
        }
        v.health = (v.health - d).max(0.0);
        if v.health == 0.0 {
            v.alive = false;
            v.death = Some(Death::Combat);
            counters.combat += 1;
            let diet = v.pheno.diet as usize;
            counters.by_diet.deaths[diet][Death::Combat as usize] += 1;
            if let Some((_, by)) = killer {
                counters.by_diet.kills[by][diet] += 1;
            }
        }
    }
    shots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature::{Activity, Diet, Sighting, strategy::Shoot};
    use crate::genome::creature::Gene;
    use crate::{CreatureGenome, World, WorldConfig};
    fn world() -> World {
        World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() })
    }
    /// `v` chose `target` as its hunt block does: prey 1.5 times smaller (the template's ratio).
    fn aim(v: &mut crate::creature::Creature, target: u64) {
        v.mind.attack = Some(target);
        v.mind.stance.strike_ratio = 1.5;
    }
    fn hit(w: &mut World) -> Vec<Shot> {
        let tick = w.tick + 1;
        resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            tick,
        )
    }
    fn at(w: &mut World, tick: u64) -> Vec<Shot> {
        resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            tick,
        )
    }
    /// A base creature at (x, 1000) whose program shoots from its whole range, keeping nothing.
    fn shooter(w: &mut World, x: f64, energy: f64) -> u64 {
        let id = w.spawn(CreatureGenome::BASE, x, 1000.0, Some(energy));
        w.creatures.last_mut().unwrap().mind.stance.shoot = Some(Shoot { from: 1.0, keep: 0.0 });
        id
    }

    #[test]
    fn удары_одновременны_и_погибший_не_получает_добычу() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(80.0));
        }
        let ids = [w.creatures[0].id, w.creatures[1].id];
        for (i, v) in w.creatures.iter_mut().enumerate() {
            v.health = 1.0;
            aim(v, ids[1 - i]);
            v.mind.activity = Activity::Alarm;
        }
        hit(&mut w);
        assert!(w.creatures.iter().all(|v| !v.alive && v.health == 0.0 && v.energy == 78.0));
        assert_eq!(w.counters.combat, 2);
    }
    /// Every strike is counted by the striker's and the target's diet; a death goes to whoever
    /// dealt the most damage in the fatal tick.
    #[test]
    fn strikes_and_kills_are_counted_by_diet() {
        let mut w = world();
        let diet = |d: Diet| CreatureGenome::BASE.with(Gene::Diet, d as usize as f64);
        w.spawn(diet(Diet::Carnivore).with(Gene::Size, 100.0), 1000.0, 1000.0, Some(200.0));
        w.spawn(diet(Diet::Omnivore).with(Gene::Size, 40.0), 1000.0, 1000.0, Some(80.0));
        let prey = w.spawn(diet(Diet::Herbivore).with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        w.creatures[2].health = 1.0;
        for v in &mut w.creatures[..2] {
            aim(v, prey);
            v.mind.activity = Activity::Alarm;
        }
        hit(&mut w);
        assert!(!w.creatures[2].alive);
        let (h, o, c) = (Diet::Herbivore as usize, Diet::Omnivore as usize, Diet::Carnivore as usize);
        let by = w.counters.by_diet;
        assert_eq!((by.strikes[c][h], by.strikes[o][h]), (1, 1));
        assert_eq!((by.kills[c][h], by.kills[o][h]), (1, 0), "the harder striker killed it");
        assert_eq!(by.deaths[h][Death::Combat as usize], 1);
        assert_eq!(by.strikes.iter().flatten().sum::<u64>(), 2);
    }
    #[test]
    fn a_hungry_creature_does_not_bite_whoever_it_bumps_into() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(20.0));
        let small = w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        hit(&mut w);
        assert_eq!(w.creatures[1].health, w.creatures[1].max_health(), "struck without choosing a target");
        aim(&mut w.creatures[0], small);
        hit(&mut w);
        assert!(w.creatures[1].health < w.creatures[1].max_health(), "did not strike its chosen prey");
    }

    #[test]
    fn full_health_takes_several_strikes() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(200.0));
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        aim(&mut w.creatures[0], prey);
        // 3.3 times bigger, it strikes for 5 · 3.33^1.25 ≈ 22.5: the hardy herbivore (health 45)
        // takes two strikes
        let strikes = (w.creatures[1].max_health() / w.creatures[0].pheno.strike_on(30.0)).ceil() as usize;
        assert_eq!(strikes, 2);
        for _ in 0..strikes - 1 {
            hit(&mut w);
            assert!(w.creatures[1].alive);
        }
        hit(&mut w);
        assert!(!w.creatures[1].alive);
        assert_eq!(w.counters.combat, 1);
    }
    /// Equal bodies trade weak strikes; a bigger one strikes disproportionately harder: three times
    /// bigger a carnivore kills a herbivore in two blows, seven times bigger in one. A smaller one
    /// strikes in proportion to its size, no weaker.
    #[test]
    fn a_bigger_body_strikes_disproportionately_harder() {
        // the numbers of `MELEE_SIZE_POWER`'s comment, at the carnivore's former strike ×1.5 and
        // the herbivore's and the omnivore's former health ×1.5 and ×1
        let strikes_to_kill = |attacker: CreatureGenome, target: CreatureGenome| {
            let mut w = world();
            let rules = w.rules.with("carnivore_strike", 1.5).unwrap();
            w.set_rules(rules.with("herbivore_health", 1.5).unwrap().with("omnivore_health", 1.0).unwrap());
            w.spawn(attacker, 1000.0, 1000.0, Some(500.0));
            let prey = w.spawn(target, 1000.0, 1000.0, Some(50.0));
            aim(&mut w.creatures[0], prey);
            w.creatures[0].mind.activity = Activity::Alarm; // any size
            for n in 1..=40 {
                hit(&mut w);
                if !w.creatures[1].alive {
                    return n;
                }
            }
            panic!("still alive after 40 strikes");
        };
        let carnivore = |size| CreatureGenome::BASE.with(Gene::Diet, 3.0).with(Gene::Size, size);
        let herbivore = |size| CreatureGenome::BASE.with(Gene::Size, size);
        let omnivore = |size| CreatureGenome::BASE.with(Gene::Diet, 1.0).with(Gene::Size, size);
        assert_eq!(strikes_to_kill(herbivore(40.0), herbivore(40.0)), 30, "equals: 2 a strike, health 60");
        assert_eq!(strikes_to_kill(carnivore(120.0), herbivore(40.0)), 2, "3× carnivore");
        assert_eq!(strikes_to_kill(herbivore(120.0), omnivore(40.0)), 2, "3× herbivore on an omnivore");
        assert_eq!(strikes_to_kill(herbivore(120.0), herbivore(40.0)), 3, "the hardy herbivore");
        assert_eq!(strikes_to_kill(carnivore(80.0), herbivore(40.0)), 5, "2× carnivore");
        assert_eq!(strikes_to_kill(carnivore(280.0), herbivore(40.0)), 1, "7× carnivore");
        // the small one's strike is its own size's share, as before
        let mut w = world();
        w.spawn(herbivore(40.0), 1000.0, 1000.0, Some(500.0));
        let big = w.spawn(herbivore(120.0), 1000.0, 1000.0, Some(50.0));
        aim(&mut w.creatures[0], big);
        w.creatures[0].mind.activity = Activity::Alarm;
        hit(&mut w);
        assert_eq!(w.creatures[1].max_health() - w.creatures[1].health, 2.0);
        // with the power 0 a bigger body strikes in proportion to its size alone
        let mut w = world();
        w.set_rules(w.rules.with("melee_size_power", 0.0).unwrap());
        w.spawn(carnivore(120.0), 1000.0, 1000.0, Some(500.0));
        let prey = w.spawn(herbivore(40.0), 1000.0, 1000.0, Some(50.0));
        aim(&mut w.creatures[0], prey);
        hit(&mut w);
        let strike = 120.0 * crate::config::MELEE_DAMAGE_SHARE * crate::config::DIET_STRIKE[3];
        assert!((w.creatures[1].max_health() - w.creatures[1].health - strike).abs() < 1e-9);
    }

    /// Hungry at its food, a creature strikes a smaller stranger eating the same food beside it —
    /// plants side by side or the same corpse — and nobody else: not one eating other food, not its
    /// young child, not one too big for it, and not when sated.
    #[test]
    fn rivals_fight_only_over_the_same_food() {
        let struck = |feeding: [Feeding; 2], rivals: [bool; 2], small: f64, child: bool| {
            let mut w = world();
            w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(200.0));
            w.spawn(CreatureGenome::BASE.with(Gene::Size, small), 1030.0, 1000.0, Some(40.0));
            if child {
                w.creatures[1].parent = w.creatures[0].id;
                w.creatures[1].genome = w.creatures[1].genome.with(Gene::Size, 200.0); // still growing
                w.creatures[0].mind.stance.spare = 1.0; // the template spares it until grown
            }
            // the template's rival setting: strangers 1.5 times smaller
            for (v, rival) in w.creatures.iter_mut().zip(rivals) {
                v.mind.stance.rival = if rival { 1.5 } else { 0.0 };
            }
            resolve_with(
                &w.space,
                &w.rules,
                &mut w.creatures,
                &mut Grid::new(crate::config::GRID_CELL),
                &mut w.counters,
                1,
                CombatPolicy { feeding: &feeding },
            );
            (
                w.creatures[1].health < w.creatures[1].max_health(),
                w.creatures[0].health < w.creatures[0].max_health(),
            )
        };
        let plants = [Feeding::Plants; 2];
        assert_eq!(struck(plants, [true, true], 30.0, false), (true, false), "grazing side by side");
        let corpse = [Feeding::Corpse(7); 2];
        assert_eq!(struck(corpse, [true, false], 30.0, false), (true, false), "at one corpse");
        let two = [Feeding::Corpse(7), Feeding::Corpse(8)];
        assert_eq!(struck(two, [true, true], 30.0, false), (false, false), "two corpses");
        let other = [Feeding::Plants, Feeding::Corpse(7)];
        assert_eq!(struck(other, [true, true], 30.0, false), (false, false), "other food");
        let idle = [Feeding::Plants, Feeding::Nothing];
        assert_eq!(struck(idle, [true, true], 30.0, false), (false, false), "not eating");
        assert_eq!(struck(plants, [false, true], 30.0, false), (false, false), "no rival setting");
        assert_eq!(struck(plants, [true, true], 70.0, false), (false, false), "not 1.5 times smaller");
        assert_eq!(struck(plants, [true, true], 30.0, true), (false, false), "its young child");
    }

    #[test]
    fn a_death_passes_no_energy_at_once() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(100.0));
        }
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        w.creatures[2].health = 5.0;
        aim(&mut w.creatures[0], prey);
        aim(&mut w.creatures[1], prey);
        hit(&mut w);
        assert_eq!(w.creatures[0].energy, 95.0);
        assert_eq!(w.creatures[1].energy, 95.0);
        assert!(!w.creatures[2].alive);
        assert!(w.creatures[2].energy > 0.0);
    }

    #[test]
    fn выстрел_слабее_удара_тратит_энергию_и_имеет_паузу() {
        let mut w = world();
        shooter(&mut w, 1000.0, 100.0);
        let target = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(40.0));
        aim(&mut w.creatures[0], target);
        let mut total = 0;
        for tick in 1..=6 {
            let shots = at(&mut w, tick);
            total += shots.len();
            assert_eq!(shots.len(), usize::from(tick == 1 || tick == 6));
        }
        assert_eq!(total, 2);
        assert!((w.creatures[0].energy - 98.4).abs() < 1e-9);
        assert!((w.creatures[1].max_health() - 0.8 - w.creatures[1].health).abs() < 1e-9, "two shots of 0.4");
    }

    #[test]
    fn контакт_выбирает_ближний_удар_и_не_создаёт_следа() {
        let mut w = world();
        shooter(&mut w, 1000.0, 100.0);
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1020.0, 1000.0, Some(40.0));
        aim(&mut w.creatures[0], prey);
        assert!(at(&mut w, 1).is_empty());
        assert_eq!(w.creatures[0].energy, 98.0);
        // a strike of 2, times (40 / 15) ** `MELEE_SIZE_POWER` for the bigger body
        let damage = 2.0 * (40.0_f64 / 15.0).powf(crate::config::MELEE_SIZE_POWER);
        assert_eq!(w.creatures[1].health, w.creatures[1].max_health() - damage);
    }

    #[test]
    fn one_creature_strikes_at_most_once_a_tick() {
        let mut w = world();
        shooter(&mut w, 1000.0, 80.0);
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1020.0, 1000.0, Some(30.0));
        let distant = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        // Hunting the distant one, it grazes beside the near one and its program drives rivals off:
        // the rival in contact takes its one strike, and it does not shoot as well.
        aim(&mut w.creatures[0], distant);
        w.creatures[0].mind.stance.rival = 1.5;
        let shots = resolve_with(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            CombatPolicy { feeding: &[Feeding::Plants; 3] },
        );
        assert!(shots.is_empty());
        let damage = 2.0 * (40.0_f64 / 15.0).powf(crate::config::MELEE_SIZE_POWER);
        assert_eq!(w.creatures[1].health, w.creatures[1].max_health() - damage);
        assert_eq!(w.creatures[2].health, w.creatures[2].max_health());
        assert_eq!(w.creatures[0].energy, 78.0);
    }

    /// Covering its child a parent shoots an enemy of any size, but never its own growing child.
    #[test]
    fn a_cover_may_shoot_a_bigger_stranger_but_not_a_growing_child() {
        let mut w = world();
        shooter(&mut w, 1000.0, 100.0);
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1120.0, 1000.0, Some(100.0));
        aim(&mut w.creatures[0], enemy);
        assert!(at(&mut w, 1).is_empty(), "too big to hunt");
        w.creatures[0].mind.stance.defending = Some(enemy);
        assert_eq!(at(&mut w, 2).len(), 1);
        let parent = w.creatures[0].id;
        w.creatures[1].parent = parent;
        w.creatures[1].genome = w.creatures[1].genome.with(Gene::Size, 200.0); // still growing
        w.creatures[0].mind.stance.spare = 1.0; // the template spares it until grown
        assert!(at(&mut w, 7).is_empty());
    }

    #[test]
    fn одновременные_выстрелы_могут_убить_обоих() {
        let mut w = world();
        for x in [1000.0, 1100.0] {
            shooter(&mut w, x, 100.0);
        }
        let ids = [w.creatures[0].id, w.creatures[1].id];
        for (i, v) in w.creatures.iter_mut().enumerate() {
            v.health = 0.2;
            aim(v, ids[1 - i]);
            v.mind.activity = Activity::Alarm; // fighting back: any size
        }
        assert_eq!(at(&mut w, 1).len(), 2);
        assert!(w.creatures.iter().all(|v| !v.alive));
        assert_eq!(w.counters.combat, 2);
    }

    #[test]
    fn трое_стрелков_совместно_останавливают_крупного_хищника() {
        let mut w = world();
        for x in [1000.0, 1020.0, 1040.0] {
            shooter(&mut w, x, 100.0);
        }
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(100.0));
        for v in &mut w.creatures[..3] {
            aim(v, enemy);
            v.mind.activity = Activity::Alarm;
        }
        w.creatures[3].health = 2.0;
        assert_eq!(at(&mut w, 1).len(), 3);
        assert!((w.creatures[3].health - 0.8).abs() < 1e-9);
        assert_eq!(at(&mut w, 6).len(), 3);
        assert!(!w.creatures[3].alive);
        assert!(w.creatures[..3].iter().all(|v| v.alive && v.health == v.max_health()));
    }

    #[test]
    fn a_cover_shoots_the_childs_enemy() {
        let mut w = world();
        shooter(&mut w, 1000.0, 100.0);
        let aid_enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1120.0, 1000.0, Some(100.0));
        aim(&mut w.creatures[0], aid_enemy);
        w.creatures[0].mind.stance.defending = Some(aid_enemy);
        let shots = at(&mut w, 1);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].to, (1100.0, 1000.0));
    }

    #[test]
    fn беглец_не_бьёт_свежего_нападавшего_без_намерения_защищаться() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1010.0, 1000.0, Some(100.0));
        w.creatures[0].mind.flight = true;
        w.creatures[0].mind.hit = Some(Sighting { enemy, x: 1010.0, y: 1000.0, tick: 1 });
        w.creatures[0].mind.attack = Some(enemy);
        assert!(at(&mut w, 2).is_empty());
        assert_eq!(w.creatures[0].energy, 100.0);
        assert_eq!(w.creatures[1].health, w.creatures[1].max_health());
    }

    /// Of several attackers in a tick the victim remembers the lowest number: a fight back or a
    /// parent's cover goes for it.
    #[test]
    fn the_victim_remembers_one_of_its_attackers() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        }
        let victim = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1000.0, 1000.0, Some(30.0));
        let attackers = [w.creatures[0].id, w.creatures[1].id];
        for v in &mut w.creatures[..2] {
            aim(v, victim);
        }
        at(&mut w, 1);
        let hit = w.creatures[2].mind.hit.expect("struck");
        assert_eq!((hit.enemy, hit.tick), (attackers[0].min(attackers[1]), 1));
        assert_eq!(w.creatures[2].mind.alarm, Some(hit));
    }

    #[test]
    fn выстрел_не_расходует_энергию_ниже_личного_резерва() {
        let mut w = world();
        // it shoots from its whole range, keeping half its tank
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(50.5));
        w.creatures[0].mind.stance.shoot = Some(Shoot { from: 1.0, keep: 0.5 });
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        aim(&mut w.creatures[0], prey);
        assert!(at(&mut w, 1).is_empty());
        assert_eq!(w.creatures[0].energy, 50.5);
        w.creatures[0].energy = 51.0;
        assert_eq!(at(&mut w, 2).len(), 1);
        assert!((w.creatures[0].energy - 50.2).abs() < 1e-9);
    }

    #[test]
    fn новые_цена_и_сила_выстрела_действуют_на_живых_без_изменения_генов() {
        let mut w = world();
        shooter(&mut w, 1000.0, 100.0);
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        let genes = w.creatures.iter().map(|v| v.genome).collect::<Vec<_>>();
        let rules = w
            .rules
            .with("shot_damage_share", 0.05)
            .unwrap()
            .with("shot_energy_share", 0.01)
            .unwrap()
            .with("shot_period", 2.0)
            .unwrap();
        w.set_rules(rules);
        aim(&mut w.creatures[0], prey);
        assert_eq!(at(&mut w, 1).len(), 1);
        assert!((w.creatures[0].energy - 99.6).abs() < 1e-9);
        assert!((w.creatures[1].max_health() - 2.0 - w.creatures[1].health).abs() < 1e-9);
        assert!(at(&mut w, 2).is_empty());
        assert_eq!(w.creatures.iter().map(|v| v.genome).collect::<Vec<_>>(), genes);
    }
}
