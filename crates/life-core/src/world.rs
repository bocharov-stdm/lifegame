//! The simulation's state and one logical tick.
//!
//! The order of a tick: plants → creatures (the herd snapshot, moves, eating, fighting, corpses)
//! → the tick counter. Relatives are seen by the snapshot at the start of the phase. One eaten
//! in this tick and one that died on its own move act no further. Children pile up in a separate
//! buffer and do not move in the tick of their birth. Eaten plants are marked and swept out once
//! a tick. A fight is a separate pass after all the creatures have moved, when they already
//! stand; fights are always on.
//!
//! Predators were a species of their own until the tag `predators-final`; the meat diets and
//! corpses took their place.

use crate::config::*;
use crate::creature::strategy as creature_strategy;
use crate::creature::{Creature, Diet, Food, Meal, Morsel};
use crate::flora::Flora;
use crate::genome::{CreatureGenome, creature, variant_for};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::rng::{Rng, mix};
use crate::rules::Rules;
use crate::senses::{GridSenses, Herd, bite_plant};
use crate::space::{Shape, Space};

/// What the world begins with. None for creatures — the value from the config, recomputed for
/// the world's area.
#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub seed: u64,
    pub scale: f64,
    /// The world's shape. By default 3:2: at x1 it is the base world 6000x4000, the same as the
    /// strip's, and a big world grows both ways, not into a ribbon.
    pub shape: Shape,
    pub rules: Rules,
    pub n_creatures: Option<usize>,
    /// The starting mix of strategies: the shares of the variants in the order of `VARIANTS` (an
    /// empty one — the first for all). Dealt without a draw (`variant_for`).
    pub strategies: Vec<f64>,
    /// Founders' diets: shares in the order of `DIET_VARIANTS` (empty — all herbivores).
    /// Dealt without a draw and spread over the founders (`spread_ranks`).
    pub diets: Vec<f64>,
    /// How many times bigger the meat-eating founders start (`config::MEAT_FOUNDER_SIZE`).
    pub meat_founder_size: f64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig {
            seed: 1,
            scale: 1.0,
            shape: Shape::R3x2,
            rules: Rules::default(),
            n_creatures: None,
            strategies: Vec::new(),
            diets: DIET_START_MIX.to_vec(),
            meat_founder_size: MEAT_FOUNDER_SIZE,
        }
    }
}

impl WorldConfig {
    /// The world's dimensions: the scale sets the area, the shape the proportions.
    pub fn space(&self) -> Space {
        Space::new(self.scale, self.shape)
    }

    /// How many creatures there will be at the start: the given number or the config's for the world's area.
    pub fn creatures_at_start(&self) -> usize {
        self.n_creatures.unwrap_or_else(|| self.space().per_area(CREATURES_AT_START))
    }
}

/// A summary of the population; `avg_genom` and `avg_energy` are None if there are no creatures.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub tick: u64,
    pub plants: usize,
    pub creatures: usize,
    pub avg_genom: Option<[f64; creature::N]>,
    pub avg_energy: Option<f64>,
}

/// How many have grown, been born and died in all since the world began. The population says
/// WHAT it became, and the difference of two counter snapshots says WHY: there are fewer
/// creatures because they were eaten or because they have nothing to eat. Starting and planted
/// creatures do not count as births.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub plants_grown: u64,
    pub plants_eaten: u64,
    pub plant_bites: u64,
    /// Bites of corpses; `rot_bites` and `bone_bites` of them were of rot and of bones.
    pub meat_bites: u64,
    pub rot_bites: u64,
    pub bone_bites: u64,
    pub ranged_shots: u64,
    pub born: u64,
    pub starved: u64,
    pub old_age: u64,
    pub combat: u64,
    /// Corpses that appeared, and of those removed: how many had lain on the bottom with flesh
    /// left, how many had come to bones (eaten or rotted away), and their lifetimes summed.
    pub corpses: u64,
    pub corpses_gone: u64,
    pub corpses_bottom: u64,
    pub skeletons: u64,
    pub corpse_ticks: u64,
    /// The same flows split by diet, and who fights and kills whom.
    pub by_diet: DietCounters,
}

/// Flows by diet: rows and columns in `Diet` order (herbivore, omnivore, scavenger, carnivore).
/// Bookkeeping only: nothing in the world reads it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DietCounters {
    /// Children born, by the child's diet.
    pub born: [u64; 4],
    /// Deaths by the dead one's diet and cause, in `Death` order: starved, old age, combat.
    pub deaths: [[u64; 3]; 4],
    /// Melee strikes and shots, by the striker's diet (row) on the target's diet (column).
    pub strikes: [[u64; 4]; 4],
    /// Combat deaths by the killer's diet (row) and the victim's (column). Of several strikers in
    /// the fatal tick the killer is the one who dealt the most damage.
    pub kills: [[u64; 4]; 4],
}

impl DietCounters {
    pub fn since(&self, earlier: &DietCounters) -> DietCounters {
        let sub = |a: &[u64; 4], b: &[u64; 4]| std::array::from_fn(|k| a[k] - b[k]);
        DietCounters {
            born: sub(&self.born, &earlier.born),
            deaths: std::array::from_fn(|d| {
                std::array::from_fn(|k| self.deaths[d][k] - earlier.deaths[d][k])
            }),
            strikes: std::array::from_fn(|d| sub(&self.strikes[d], &earlier.strikes[d])),
            kills: std::array::from_fn(|d| sub(&self.kills[d], &earlier.kills[d])),
        }
    }
}

impl Counters {
    /// The flows over the interval from `earlier` to `self`.
    pub fn since(&self, earlier: &Counters) -> Counters {
        Counters {
            plants_grown: self.plants_grown - earlier.plants_grown,
            plants_eaten: self.plants_eaten - earlier.plants_eaten,
            plant_bites: self.plant_bites - earlier.plant_bites,
            meat_bites: self.meat_bites - earlier.meat_bites,
            rot_bites: self.rot_bites - earlier.rot_bites,
            bone_bites: self.bone_bites - earlier.bone_bites,
            ranged_shots: self.ranged_shots - earlier.ranged_shots,
            born: self.born - earlier.born,
            starved: self.starved - earlier.starved,
            old_age: self.old_age - earlier.old_age,
            combat: self.combat - earlier.combat,
            corpses: self.corpses - earlier.corpses,
            corpses_gone: self.corpses_gone - earlier.corpses_gone,
            corpses_bottom: self.corpses_bottom - earlier.corpses_bottom,
            skeletons: self.skeletons - earlier.skeletons,
            corpse_ticks: self.corpse_ticks - earlier.corpse_ticks,
            by_diet: self.by_diet.since(&earlier.by_diet),
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub corpses: Vec<crate::corpse::Corpse>,
    pub shots: Vec<crate::Shot>,
    /// The world's seed: keys the patch streams.
    seed: u64,
    pub space: Space,
    pub rules: Rules,
    pub tick: u64,
    pub plants: Vec<Plant>,
    pub creatures: Vec<Creature>,
    pub counters: Counters,

    next_id: u64,
    /// Where the food grows — derived from the rules and the world's dimensions, recomputed with
    /// the rules (`set_rules`).
    flora: Flora,
    /// Occupied fertility cells, kept in step with `plants`.
    plant_cells: Occupancy,
    /// The world's stream: plants and planting. Every creature has a stream of its own.
    rng: Rng,
    /// The herd's snapshot at the start of the creatures' phase: relatives are seen by it.
    herd: Herd,
    prey_grid: Grid,
    food_grid: Grid,
    corpse_grid: Grid,
    bitten_plants: Vec<bool>,
}

impl World {
    pub fn new(cfg: &WorldConfig) -> Self {
        let space = cfg.space();
        let rules = cfg.rules.clone();
        let mut rng = Rng::keyed(cfg.seed, 0);
        let n_start = cfg.creatures_at_start();

        let mut w = World {
            corpses: Vec::new(),
            shots: Vec::new(),
            flora: Flora::new(&rules, &space, cfg.seed),
            plant_cells: Occupancy::stale(),
            space,
            rules,
            tick: 0,
            plants: Vec::new(),
            creatures: Vec::with_capacity(n_start),
            counters: Counters::default(),
            seed: cfg.seed,
            next_id: 1,
            rng: Rng::new(0),
            herd: Herd::new(),
            prey_grid: Grid::new(GRID_CELL),
            food_grid: Grid::new(GRID_CELL),
            corpse_grid: Grid::new(GRID_CELL),
            bitten_plants: Vec::new(),
        };
        let variants = creature_strategy::VARIANTS.len();
        let diet_ranks = crate::genome::spread_ranks(n_start);
        for (i, &diet_rank) in diet_ranks.iter().enumerate() {
            let k = variant_for(i, n_start, &cfg.strategies, variants);
            let diet = variant_for(diet_rank, n_start, &cfg.diets, creature::DIET_VARIANTS.len());
            // Draws independent of the world's stream do not change the places of birth and the plants.
            let mut founder = Rng::keyed(cfg.seed, 0x5A6C_5A6C_0000_0000 ^ i as u64);
            // The first two draws decided a founder's flock and territoriality until flocks went
            // (tag `flocks-final`); they stay, so the shooters are the same founders.
            let _ = founder.random();
            let _ = founder.random();
            let shooter = founder.random() < 0.05;
            // A quarter of the founders is not held by its layer: by a hash, not a draw.
            let free = mix(cfg.seed ^ mix(0x1A7E_0000_0000_0000 ^ i as u64)).is_multiple_of(4);
            let genome = CreatureGenome::BASE
                .with(creature::Gene::Strategy, k as f64)
                .with(creature::Gene::Diet, diet as f64);
            // Scavengers start held in the deep, where rot will settle, and cold-blooded.
            let scavenger = diet == crate::creature::Diet::Scavenger as usize;
            let genome = if scavenger {
                genome.with(creature::Gene::ColdBlood, crate::config::SCAVENGER_START_COLD)
            } else {
                genome
            };
            let layer = match (scavenger, free) {
                (true, _) => crate::config::SCAVENGER_START_LAYER,
                (false, true) => (0, 100),
                (false, false) => crate::creature::Program::STANDARD.home_layer_pct(),
            };
            let strategy = crate::creature::Strategy::from_gene(k as f64);
            let programs =
                crate::creature::Programs::both(crate::creature::Program::founder(strategy, layer, shooter));
            // Meat-eating founders start bigger, so the first herbivores' children are their prey.
            let genome = if matches!(Diet::ALL[diet], Diet::Scavenger | Diet::Carnivore) {
                genome.with(creature::Gene::Size, genome[creature::Gene::Size] * cfg.meat_founder_size)
            } else {
                genome
            };
            let v = Creature::founder(&w.space, &w.rules, genome, programs, rng.fork());
            w.add_creature(v);
        }
        w.rng = rng;
        w
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_creature(&mut self, mut v: Creature) {
        v.id = self.take_id();
        self.creatures.push(v);
    }

    /// A new creature with a given genome at a given place (for tests and the game).
    pub fn spawn(&mut self, genome: CreatureGenome, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let v = Creature::new(&self.space, &self.rules, genome, Some(x), Some(y), energy, rng);
        self.add_creature(v);
        self.next_id - 1
    }

    // ── one logical tick ────────────────────────────────────────────────────
    pub fn step(&mut self) {
        self.spawn_plants();
        self.update_creatures();
        self.tick += 1;
    }

    /// Plants per tick are an expected number (not a probability): the whole part is always
    /// spawned, the fractional part with the corresponding chance. Above the ceiling we do not grow.
    /// A seed that lands in an occupied slot (`flora.rs`) does not sprout, so
    /// growth slows as the neighbourhood fills up.
    fn spawn_plants(&mut self) {
        let rate = self.rules.plant_rate * self.space.area_ratio();
        let mut count = rate as usize;
        if self.rng.random() < rate - count as f64 {
            count += 1;
        }
        let cap = self.space.per_area(PLANT_MAX);
        let count = count.min(cap.saturating_sub(self.plants.len()));
        self.plant_cells.sync(&self.flora, &self.plants);
        for _ in 0..count {
            let mut p = self.flora.plant(&mut self.rng);
            if !self.plant_cells.take(p.slot()) {
                continue;
            }
            p.born = self.tick.min(u32::MAX as u64) as u32;
            self.plants.push(p);
            self.counters.plants_grown += 1;
        }
    }

    fn update_creatures(&mut self) {
        let now = self.tick + 1;
        self.shots.retain(|s| now.saturating_sub(s.tick) <= 8);
        let counters = &mut self.counters;
        self.corpses.retain_mut(|c| {
            let keep = c.decay(now);
            if !keep {
                counters.corpses_gone += 1;
                counters.corpses_bottom += c.settled as u64;
                counters.skeletons += c.skeleton.is_some() as u64;
                counters.corpse_ticks += now - c.born;
            }
            keep
        });
        // old age weakens before anyone looks: the snapshot and the moves see the aged bodies
        for v in &mut self.creatures {
            v.grow_old(&self.rules);
            // its step counts it on to this tick
            v.mind.tick = self.tick;
        }
        let World {
            space,
            rules,
            plants,
            corpses,
            creatures,
            herd,
            prey_grid,
            food_grid,
            corpse_grid,
            bitten_plants,
            counters,
            shots,
            plant_cells,
            ..
        } = self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        corpse_grid.rebuild(space, corpses.iter().map(|c| (c.x, c.y)));
        bitten_plants.clear();
        bitten_plants.resize(plants.len(), false);
        // Relatives are seen as they were at the start of the phase: the outcome does not depend on
        // the order of moves.
        herd.rebuild(space, creatures);

        let mut offspring = Vec::new();
        for v in creatures.iter_mut() {
            v.step(&GridSenses {
                food: food_grid,
                plants,
                corpse_grid: Some(&*corpse_grid),
                corpses,
                now,
                herd: Some(&*herd),
            });
            if !v.alive {
                let death = v.death.unwrap_or(crate::creature::Death::Starved);
                if death == crate::creature::Death::OldAge {
                    counters.old_age += 1;
                } else {
                    counters.starved += 1;
                }
                counters.by_diet.deaths[v.pheno.diet as usize][death as usize] += 1;
                continue; // starved to death on this move: neither eats nor divides
            }
        }
        // One bite per creature and no more than one portion from a plant per tick.
        let mut fed = vec![false; creatures.len()];
        let max_corpse_half = corpses.iter().fold(0.0_f64, |m, c| m.max(c.size * 0.5));
        // Plants go before the fight, corpses after. On a copy we distribute the corpses' portions by
        // ID in advance: one whose remainder would no longer suffice may take a plant now, without
        // getting a second portion in this tick. Only the corpses somebody claims
        // are copied (a few a tick); the rest are read as they lie.
        let mut claimed: Vec<(usize, crate::corpse::Corpse)> = Vec::new();
        let lying: &[crate::corpse::Corpse] = corpses;
        fn shadow<'a>(
            claimed: &'a [(usize, crate::corpse::Corpse)],
            corpses: &'a [crate::corpse::Corpse],
            j: usize,
        ) -> &'a crate::corpse::Corpse {
            claimed.iter().find(|(k, _)| *k == j).map_or(&corpses[j], |(_, c)| c)
        }
        // Whether it eats another niche's food was its program's setting for this tick
        // (`Stance::foreign`), the same in both feeding phases; what it eats on contact too
        // (`Stance::eats`: what its block went for, and on the move).
        let foreign: Vec<bool> = creatures.iter().map(|v| v.mind.stance.foreign).collect();
        let takes = |v: &Creature, food| v.mind.stance.eats(food, v.energy / v.pheno.max_energy);
        // What each one eats this tick: rivals fight only over the same food.
        let mut feeding = vec![crate::combat::Feeding::Nothing; creatures.len()];
        let plant_bite = rules.plant_energy * rules.plant_bite_yield / f64::from(crate::plant::PORTIONS);
        // A torpid creature eats nothing, not even what touches it (`Creature::torpid`).
        for (i, v) in creatures.iter_mut().enumerate().filter(|(_, v)| v.alive && !v.torpid) {
            // Only what the diet digests is eaten at all: a meat-eater does not take a plant from
            // a herbivore for nothing, a herbivore does not touch a corpse. Sated, only its own.
            let eats = |c: &crate::corpse::Corpse| v.pheno.corpse_efficiency(c.stage(now), foreign[i]) > 0.0;
            let corpse = if takes(v, Food::Corpse) {
                crate::corpse::contact_by(
                    corpse_grid,
                    |j| shadow(&claimed, lying, j),
                    (v.x, v.y),
                    v.pheno.size,
                    max_corpse_half,
                    now,
                    eats,
                )
            } else {
                None
            };
            let prefer_corpse = corpse.is_some_and(|j| {
                let c = shadow(&claimed, lying, j);
                c.portion(rules.plant_energy) * v.pheno.corpse_efficiency(c.stage(now), foreign[i])
                    > plant_bite * v.pheno.plant_efficiency
            });
            let plant = if prefer_corpse || !v.pheno.eats_plants() || !takes(v, Food::Plant) {
                None
            } else {
                bite_plant(food_grid, plants, bitten_plants, v.x, v.y, v.pheno.size)
            };
            if let Some((finished, px, py)) = plant {
                counters.plant_bites += 1;
                counters.plants_eaten += finished as u64;
                v.feed(rules);
                v.meal = Some(Meal { tick: now, x: px, y: py, food: Morsel::Plant });
                fed[i] = true;
                feeding[i] = crate::combat::Feeding::Plants;
            } else if let Some(j) = corpse {
                // The meat pays better, or the plant went to an earlier ID: this eater claims the corpse
                // before the following participants.
                let at = claimed.iter().position(|(k, _)| *k == j).unwrap_or_else(|| {
                    claimed.push((j, lying[j].clone()));
                    claimed.len() - 1
                });
                let c = &mut claimed[at].1;
                feeding[i] = crate::combat::Feeding::Corpse(c.owner);
                c.bite(now, rules.plant_energy);
            }
        }
        // Everyone has already moved; there are no newborns yet. The strikes are simultaneous.
        let result = crate::combat::resolve_with(
            space,
            rules,
            creatures,
            prey_grid,
            counters,
            now,
            crate::combat::CombatPolicy { feeding: &feeding },
        );
        counters.ranged_shots += result.len() as u64;
        shots.extend(result);
        // The previous tick's corpses are shared among the survivors in the order of ID.
        for (i, v) in creatures.iter_mut().enumerate() {
            if !v.alive || fed[i] || v.torpid || !takes(v, Food::Corpse) {
                continue;
            }
            let eats = |c: &crate::corpse::Corpse| v.pheno.corpse_efficiency(c.stage(now), foreign[i]) > 0.0;
            if let Some(bite) = crate::corpse::bite(
                corpse_grid,
                corpses,
                (v.x, v.y),
                v.pheno.size,
                max_corpse_half,
                rules.plant_energy,
                now,
                eats,
            ) {
                v.nourish(bite.amount * v.pheno.corpse_efficiency(bite.stage, true), rules);
                v.meal = Some(Meal {
                    tick: now,
                    x: bite.x,
                    y: bite.y,
                    food: Morsel::Corpse { stage: bite.stage },
                });
                counters.meat_bites += 1;
                counters.rot_bites += (bite.stage == crate::corpse::Stage::Rot) as u64;
                counters.bone_bites += (bite.stage == crate::corpse::Stage::Bones) as u64;
                fed[i] = true;
            }
        }
        for v in creatures.iter_mut().filter(|v| v.alive) {
            if let Some(child) = v.maybe_divide(space, rules) {
                offspring.push(child);
            }
        }
        let before = corpses.len();
        // One that never grew and died with an empty tank leaves no meat, so no corpse: it would
        // only count as a corpse gone the next tick and pull the corpses' mean lifetime down.
        corpses.extend(
            creatures.iter().filter(|v| !v.alive && crate::corpse::meat(v) > 0.0).map(|v| {
                crate::corpse::Corpse::from_creature_in(v, now, crate::corpse::CorpseClock::of(rules))
            }),
        );
        counters.corpses += (corpses.len() - before) as u64;
        creatures.retain(|v| v.alive);
        plants.retain(|p| {
            if !p.alive() {
                plant_cells.free(p.slot());
            }
            p.alive()
        }); // sweep out what has been eaten
        counters.born += offspring.len() as u64;
        for child in &offspring {
            counters.by_diet.born[child.pheno.diet as usize] += 1;
        }
        for child in offspring {
            self.add_creature(child);
        }
    }

    // ── statistics ──────────────────────────────────────────────────────────
    pub fn stats(&self) -> Stats {
        let n = self.creatures.len();
        let (avg_genom, avg_energy) = if n == 0 {
            (None, None)
        } else {
            let mut sum = [0.0; creature::N];
            let mut energy = 0.0;
            for v in &self.creatures {
                for (s, g) in sum.iter_mut().zip(v.genome.to_values()) {
                    *s += g;
                }
                energy += v.energy;
            }
            (Some(sum.map(|s| s / n as f64)), Some(energy / n as f64))
        };
        Stats { tick: self.tick, plants: self.plants.len(), creatures: n, avg_genom, avg_energy }
    }

    /// New rules in the middle of a game (the lab on the fly). Living creatures recompute
    /// everything they computed from the rules at birth — otherwise the new price would act only on
    /// newborns, and the player would move a slider without seeing the consequences.
    pub fn set_rules(&mut self, rules: Rules) {
        for v in &mut self.creatures {
            v.apply_rules(&rules, &self.space);
        }
        // plants already grown stay in place, new ones follow the new profile
        let flora = Flora::new(&rules, &self.space, self.seed);
        if flora != self.flora {
            // the slots moved: old plants hold none, and new ones fill in as they are eaten
            self.plants.iter_mut().for_each(Plant::lose_slot);
            self.flora = flora;
            self.plant_cells.invalidate();
        }
        self.rules = rules;
    }

    /// Where the food grows in this world.
    pub fn flora(&self) -> &Flora {
        &self.flora
    }

    // ── selecting a creature (for the game) ─────────────────────────────────

    /// The number of the creature nearest to a point, no farther than `radius` from the edge of
    /// its body. A small creature is found even if one misses by `radius`; a big one — if one
    /// clicks anywhere on its body. Called on a click, so a plain scan: the world's grids are built
    /// inside a tick and are already stale by then.
    pub fn pick(&self, x: f64, y: f64, radius: f64) -> Option<u64> {
        let dist = |v: &Creature| ((v.x - x).powi(2) + (v.y - y).powi(2)).sqrt() - v.pheno.half;
        self.creatures
            .iter()
            .map(|v| (dist(v), v.id))
            .filter(|(d, _)| *d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// A creature by id. Numbers are issued in ascending order, new ones go to the end, and the
    /// dead are removed keeping the order — so the list is sorted by id and the search is binary:
    /// a creature can be followed even among a million.
    pub fn creature(&self, id: u64) -> Option<&Creature> {
        self.creatures.binary_search_by_key(&id, |v| v.id).ok().map(|i| &self.creatures[i])
    }
}

/// Occupied slots (`flora.rs`) as a bitset, updated per birth and per eaten
/// plant rather than rebuilt from every plant each tick. Anything that edits
/// `World::plants` from outside changes their count (tests, the app), and a
/// count that does not match makes the next birth phase rebuild the set. So
/// does a new slot layout.
#[derive(Clone, Debug)]
struct Occupancy {
    bits: Vec<u64>,
    /// How many plants the bits describe; `None` — out of date.
    plants: Option<usize>,
}

impl Occupancy {
    fn stale() -> Occupancy {
        Occupancy { bits: Vec::new(), plants: None }
    }

    fn sync(&mut self, flora: &Flora, plants: &[Plant]) {
        if self.plants == Some(plants.len()) {
            return;
        }
        self.bits.clear();
        self.bits.resize(flora.slots().div_ceil(64), 0);
        // plants put in by hand may share a slot; one out of range holds none
        for c in plants.iter().filter_map(Plant::slot).filter(|c| *c < flora.slots()) {
            self.bits[c / 64] |= 1 << (c % 64);
        }
        self.plants = Some(plants.len());
    }

    /// Takes a free slot for a new plant; false if it is occupied.
    fn take(&mut self, slot: Option<usize>) -> bool {
        if let Some(c) = slot {
            let (word, bit) = (c / 64, 1 << (c % 64));
            if self.bits[word] & bit != 0 {
                return false;
            }
            self.bits[word] |= bit;
        }
        self.plants = self.plants.map(|n| n + 1);
        true
    }

    fn free(&mut self, slot: Option<usize>) {
        if let Some(n) = self.plants {
            if let Some(c) = slot.filter(|c| *c / 64 < self.bits.len()) {
                self.bits[c / 64] &= !(1 << (c % 64));
            }
            self.plants = Some(n - 1);
        }
    }

    fn invalidate(&mut self) {
        self.plants = None;
    }
}

#[cfg(test)]
mod occupancy_tests {
    use super::*;

    /// Updating per birth and per eaten plant gives the same slots as a rebuild
    /// from all plants, in a world where creatures eat — also after the layout
    /// changes mid-game, when the old plants hold no slot.
    #[test]
    fn incremental_slots_match_a_rebuild() {
        let mut w = World::new(&WorldConfig { seed: 2, ..Default::default() });
        for round in 0..6 {
            if round == 3 {
                let grown = w.plants.clone();
                w.set_rules(w.rules.with("plant_patch_size", 300.0).unwrap());
                assert!(w.plants.iter().all(|p| p.slot().is_none()), "the old slots are gone");
                assert!(
                    grown.iter().zip(&w.plants).all(|(a, b)| (a.x, a.y) == (b.x, b.y)),
                    "plants stay put"
                );
                let same = w.flora.clone();
                w.set_rules(w.rules.with("cost_scale", 2.0).unwrap());
                assert_eq!(w.flora, same, "rules that do not touch food keep the layout");
            }
            for _ in 0..100 {
                w.step();
            }
            assert_eq!(w.plant_cells.plants, Some(w.plants.len()), "tick {}", w.tick);
            let mut fresh = Occupancy::stale();
            fresh.sync(&w.flora, &w.plants);
            assert_eq!(w.plant_cells.bits, fresh.bits, "tick {}", w.tick);
        }
        assert!(w.plants.iter().any(|p| p.slot().is_some()), "new plants take the new slots");
        assert!(w.counters.plants_grown > w.plants.len() as u64, "nothing was eaten");
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;
    use crate::creature::ADULT;

    /// The diet split adds up to the totals: every birth and death is counted once, under its
    /// diet; combat deaths never outnumber the kills plus the deaths nobody struck hardest.
    #[test]
    fn diet_counters_add_up_to_the_totals() {
        let cfg = WorldConfig { seed: 3, diets: vec![25.0; 4], ..Default::default() };
        let mut w = World::new(&cfg);
        for _ in 0..1500 {
            w.step();
        }
        let (c, by) = (w.counters, w.counters.by_diet);
        let deaths = |cause: usize| by.deaths.iter().map(|d| d[cause]).sum::<u64>();
        assert_eq!(by.born.iter().sum::<u64>(), c.born);
        assert_eq!((deaths(0), deaths(1), deaths(2)), (c.starved, c.old_age, c.combat));
        assert_eq!(by.kills.iter().flatten().sum::<u64>(), c.combat, "every combat death has a killer");
        assert!(c.combat > 0 && by.strikes.iter().flatten().sum::<u64>() >= c.combat);
    }

    #[test]
    fn founders_free_a_quarter_of_layers() {
        let cfg = WorldConfig { seed: 17, n_creatures: Some(400), ..Default::default() };
        let (first, again) = (World::new(&cfg), World::new(&cfg));
        let mut free = 0;
        for (a, b) in first.creatures.iter().zip(&again.creatures) {
            assert_eq!(a.genome, b.genome);
            free += (a.programs[ADULT].home_layer() == (0.0, 1.0)) as usize;
        }
        assert!((70..=130).contains(&free), "a quarter of the founders is free of its layer: {free}");
    }

    #[test]
    fn founders_get_reproducible_modes_without_moving_their_birthplaces() {
        let cfg = WorldConfig { seed: 17, n_creatures: Some(400), ..Default::default() };
        let first = World::new(&cfg);
        let again = World::new(&cfg);
        let extended = World::new(&WorldConfig { n_creatures: Some(401), ..cfg });
        let mut shooters = 0;
        for ((a, b), c) in first.creatures.iter().zip(&again.creatures).zip(&extended.creatures) {
            assert_eq!(a.genome, b.genome);
            assert_eq!((a.x, a.y), (b.x, b.y));
            // An extra founder does not shift the independent draws.
            assert_eq!(a.programs[ADULT].shoots(), c.programs[ADULT].shoots());
            assert_eq!((a.x, a.y), (c.x, c.y));
            shooters += a.programs[ADULT].shoots() as usize;
        }
        assert!((8..=36).contains(&shooters), "five percent of the founders shoot: {shooters}");
    }
}
