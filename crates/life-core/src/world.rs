//! Состояние симуляции и один логический тик.
//!
//! Порядок тика: растения → существа (снимок стада, ходы, еда, бой, трупы) →
//! счётчик тиков. Сородичей видят по снимку на начало фазы. Съеденный в этом
//! тике и умерший на своём ходу не действуют дальше. Дети копятся в отдельном
//! буфере и не ходят в тик рождения. Съеденные растения помечаются и
//! выметаются раз за тик. Бой — отдельный проход после хода всех существ, когда
//! они уже стоят; бои включены всегда.
//!
//! Хищники были отдельным видом до тега `predators-final`: их заменили мутации
//! и каннибализм.

use crate::config::*;
use crate::creature::strategy as creature_strategy;
use crate::creature::{Creature, Meal, Morsel};
use crate::flora::Flora;
use crate::genome::{CreatureGenome, creature, variant_for};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::rng::{Rng, mix};
use crate::rules::Rules;
use crate::senses::{GridSenses, Herd, bite_plant};
use crate::space::{Shape, Space};

/// С чего начинается мир. None у существ — значение из конфига,
/// пересчитанное на площадь мира.
#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub seed: u64,
    pub scale: f64,
    /// Форма мира. По умолчанию 3:2: при x1 это базовый мир 6000x4000, тот же,
    /// что у полосы, а большой мир растёт в обе стороны, а не в ленту.
    pub shape: Shape,
    pub rules: Rules,
    pub n_creatures: Option<usize>,
    /// Стартовая смесь стратегий: доли вариантов по порядку `VARIANTS`
    /// (пустая — у всех первый). Раздаётся без жребия (`variant_for`).
    pub strategies: Vec<f64>,
    /// Founders' diets: shares in the order of `DIET_VARIANTS` (empty — all herbivores).
    /// Dealt without a draw and spread over the founders (`spread_ranks`).
    pub diets: Vec<f64>,
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
        }
    }
}

impl WorldConfig {
    /// Размеры мира: масштаб задаёт площадь, форма — пропорции.
    pub fn space(&self) -> Space {
        Space::new(self.scale, self.shape)
    }

    /// Сколько существ будет на старте: заданное или из конфига на площадь мира.
    pub fn creatures_at_start(&self) -> usize {
        self.n_creatures.unwrap_or_else(|| self.space().per_area(CREATURES_AT_START))
    }
}

/// Сводка по популяции; `avg_genom` и `avg_energy` — None, если существ нет.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub tick: u64,
    pub plants: usize,
    pub creatures: usize,
    pub avg_genom: Option<[f64; creature::N]>,
    pub avg_energy: Option<f64>,
}

/// Сколько всего выросло, родилось и умерло с начала мира. Численность говорит,
/// ЧТО стало, а разность двух снимков счётчиков — ОТЧЕГО: существ стало
/// меньше, потому что их съели или потому что им нечего есть. Стартовые и
/// подсаженные существа рождениями не считаются.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub plants_grown: u64,
    pub plants_eaten: u64,
    pub plant_bites: u64,
    /// Bites of corpses; `rot_bites` of them were mostly rot (rot share at least ½).
    pub meat_bites: u64,
    pub rot_bites: u64,
    pub ranged_shots: u64,
    pub territorial_fights: u64,
    pub born: u64,
    pub starved: u64,
    pub old_age: u64,
    pub combat: u64,
    /// Съедены сородичами (каннибализм).
    pub cannibalized: u64,
    /// Corpses that appeared, and of those removed: how many had lain fully rotten on the bottom
    /// with meat left, how many had been eaten down to a skeleton, and their lifetimes summed.
    pub corpses: u64,
    pub corpses_gone: u64,
    pub corpses_bottom: u64,
    pub skeletons: u64,
    pub corpse_ticks: u64,
}

impl Counters {
    /// Потоки за промежуток от `earlier` до `self`.
    pub fn since(&self, earlier: &Counters) -> Counters {
        Counters {
            plants_grown: self.plants_grown - earlier.plants_grown,
            plants_eaten: self.plants_eaten - earlier.plants_eaten,
            plant_bites: self.plant_bites - earlier.plant_bites,
            meat_bites: self.meat_bites - earlier.meat_bites,
            rot_bites: self.rot_bites - earlier.rot_bites,
            ranged_shots: self.ranged_shots - earlier.ranged_shots,
            territorial_fights: self.territorial_fights - earlier.territorial_fights,
            born: self.born - earlier.born,
            starved: self.starved - earlier.starved,
            old_age: self.old_age - earlier.old_age,
            combat: self.combat - earlier.combat,
            cannibalized: self.cannibalized - earlier.cannibalized,
            corpses: self.corpses - earlier.corpses,
            corpses_gone: self.corpses_gone - earlier.corpses_gone,
            corpses_bottom: self.corpses_bottom - earlier.corpses_bottom,
            skeletons: self.skeletons - earlier.skeletons,
            corpse_ticks: self.corpse_ticks - earlier.corpse_ticks,
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub corpses: Vec<crate::corpse::Corpse>,
    pub shots: Vec<crate::Shot>,
    pub territory: crate::territory::State,
    pub battles: crate::battle::Battles,
    pub grace: crate::kin_grace::Grace,
    pub next_flock: u64,
    pub split_watches: Vec<crate::flock::SplitWatch>,
    pub social_counts: crate::social::Counters,
    pub flocks: std::collections::BTreeMap<u64, crate::flock::Flock>,
    /// The world's seed: keys the flock and patch streams.
    seed: u64,
    pub space: Space,
    pub rules: Rules,
    pub tick: u64,
    pub plants: Vec<Plant>,
    pub creatures: Vec<Creature>,
    pub counters: Counters,

    next_id: u64,
    /// Где растёт еда — выведено из правил и размеров мира, пересчитывается
    /// вместе с правилами (`set_rules`).
    flora: Flora,
    /// Occupied fertility cells, kept in step with `plants`.
    plant_cells: Occupancy,
    /// Поток мира: растения и подсадка. У каждого существа поток свой.
    rng: Rng,
    /// Снимок стада на начало фазы существ: по нему видят сородичей.
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
            territory: Default::default(),
            battles: Default::default(),
            grace: Default::default(),
            next_flock: 1,
            split_watches: Vec::new(),
            social_counts: Default::default(),
            flora: Flora::new(&rules, &space, cfg.seed),
            plant_cells: Occupancy::stale(),
            space,
            rules,
            tick: 0,
            plants: Vec::new(),
            creatures: Vec::with_capacity(n_start),
            counters: Counters::default(),
            flocks: Default::default(),
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
        let kinds = creature::FLOCK_KIND_VARIANTS.len();
        // Flock kinds are dealt in turn among the flocking founders: any first part of them
        // has every kind in equal shares.
        let mut flocking = 0;
        let diet_ranks = crate::genome::spread_ranks(n_start);
        for (i, &diet_rank) in diet_ranks.iter().enumerate() {
            let k = variant_for(i, n_start, &cfg.strategies, variants);
            let diet = variant_for(diet_rank, n_start, &cfg.diets, creature::DIET_VARIANTS.len());
            // Независимые от потока мира жребии не меняют места рождения и растения.
            let mut founder = Rng::keyed(cfg.seed, 0x5A6C_5A6C_0000_0000 ^ i as u64);
            // Every other founder is flocking: a draw left 3 to 15 of 20 calm founders flocking,
            // and the flocks of a world were decided by that lottery. The draw stays, so the
            // founders' other draws are the same.
            let _ = founder.random();
            let pack = i % 2 == 0;
            let territory = founder.random();
            let shooter = founder.random() < 0.05;
            // Loners carry territoriality too: it acts only through a flock's circle, so for them
            // it is neutral variation that a flock descending from them inherits.
            let mode = if territory < 0.5 {
                0.0
            } else if territory < 0.9 {
                1.0
            } else {
                2.0
            };
            let kind = if pack {
                flocking += 1;
                (flocking - 1) % kinds
            } else {
                0
            };
            // A quarter of the founders is not held by its layer: by a hash, not a draw, and
            // independent of the kind.
            let free = mix(cfg.seed ^ mix(0x1A7E_0000_0000_0000 ^ i as u64)).is_multiple_of(4);
            let genome = CreatureGenome::BASE
                .with(creature::Gene::Strategy, k as f64)
                .with(creature::Gene::PackInstinct, f64::from(pack as u8))
                .with(creature::Gene::Territoriality, mode)
                .with(creature::Gene::Shooter, f64::from(shooter as u8))
                .with(creature::Gene::FlockKind, kind as f64)
                .with(creature::Gene::LayerBound, f64::from(free as u8))
                .with(creature::Gene::Diet, diet as f64);
            // Scavengers start held in the deep, where rot will settle.
            let genome = if diet == crate::creature::Diet::Scavenger as usize {
                let (top, bottom) = crate::config::SCAVENGER_START_LAYER;
                genome
                    .with(creature::Gene::MinY, top)
                    .with(creature::Gene::MaxY, bottom)
                    .with(creature::Gene::LayerBound, 0.0)
            } else {
                genome
            };
            let v = Creature::new(&w.space, &w.rules, genome, None, None, None, rng.fork());
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
        if v.flock == 0 {
            v.flock = self.next_flock;
            self.next_flock += 1;
        } else {
            self.next_flock = self.next_flock.max(v.flock + 1);
        }
        self.creatures.push(v);
    }

    /// Новое существо с заданным геномом в заданном месте (для тестов и игры).
    pub fn spawn(&mut self, genome: CreatureGenome, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let v = Creature::new(&self.space, &self.rules, genome, Some(x), Some(y), energy, rng);
        self.add_creature(v);
        self.next_id - 1
    }

    // ── один логический тик ─────────────────────────────────────────────────
    pub fn step(&mut self) {
        self.spawn_plants();
        self.update_creatures();
        self.tick += 1;
    }

    /// Растений за тик — ожидаемое число (не вероятность): целую часть спауним
    /// всегда, дробную — с соответствующим шансом. Выше потолка не растём.
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
        self.grace.prune(now);
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
        crate::flock::food_goals(&mut self.flocks, &self.creatures, self.tick);
        self.social_counts.relocations +=
            crate::flock::update(&mut self.flocks, &mut self.creatures, &self.space, self.seed, true);
        let outcome =
            self.battles.update(&mut self.flocks, &self.creatures, &self.space, self.tick, &self.grace);
        self.social_counts.battles += outcome.started;
        self.social_counts.battle_retreats += outcome.retreats;
        self.prey_grid.rebuild(&self.space, self.creatures.iter().map(|v| (v.x, v.y)));
        crate::social::prepare(&mut self.creatures, &self.prey_grid, self.tick);
        let territorial_targets = self.territory.prepare_full(
            &mut self.flocks,
            &mut self.creatures,
            &self.space,
            self.tick,
            &self.grace,
            &self.prey_grid,
        );
        self.social_counts.interventions +=
            crate::social::prepare_aid_with_grace(&mut self.creatures, self.tick, &self.grace);
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
        // Сородичей видят такими, какими они были в начале фазы: исход не
        // зависит от порядка ходов.
        herd.rebuild_with_grace(space, creatures, &self.grace, now);

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
                if v.death == Some(crate::creature::Death::OldAge) {
                    counters.old_age += 1;
                } else {
                    counters.starved += 1;
                }
                continue; // умер от голода на этом ходу: не ест и не делится
            }
        }
        // Один укус на существо и не больше одной порции с растения за тик.
        let mut fed = vec![false; creatures.len()];
        let max_corpse_half = corpses.iter().fold(0.0_f64, |m, c| m.max(c.size * 0.5));
        // Растения идут до боя, трупы — после. На копии заранее распределяем
        // порции трупов по ID: тот, кому остатка уже не хватит, может взять
        // растение сейчас, не получая второй порции в этом тике.
        let mut reserved = corpses.clone();
        // Hunger is judged once, before the meal: the same in both feeding phases.
        let hungry: Vec<bool> = creatures.iter().map(|v| v.pheno.hungry(v.energy)).collect();
        let plant_bite = rules.plant_energy * rules.plant_bite_yield / f64::from(crate::plant::PORTIONS);
        for (i, v) in creatures.iter_mut().enumerate().filter(|(_, v)| v.alive) {
            // Only what the diet digests is eaten at all: a meat-eater does not take a plant from
            // a herbivore for nothing, a herbivore does not touch a corpse. Sated, only its own.
            let eats = |c: &crate::corpse::Corpse| v.pheno.corpse_efficiency(c.rot(now), hungry[i]) > 0.0;
            let corpse = crate::corpse::contact(
                corpse_grid,
                &reserved,
                (v.x, v.y),
                v.pheno.size,
                max_corpse_half,
                now,
                eats,
            );
            let prefer_corpse = corpse.is_some_and(|j| {
                let c = &reserved[j];
                c.portion(rules.plant_energy) * v.pheno.corpse_efficiency(c.rot(now), hungry[i])
                    > plant_bite * v.pheno.plant_efficiency
            });
            let plant = if prefer_corpse || !v.pheno.eats_plants() {
                None
            } else {
                bite_plant(food_grid, plants, bitten_plants, v.x, v.y, v.pheno.size)
            };
            if let Some((finished, px, py)) = plant {
                counters.plant_bites += 1;
                counters.plants_eaten += finished as u64;
                v.feed(1, rules);
                v.meal = Some(Meal { tick: now, x: px, y: py, food: Morsel::Plant });
                fed[i] = true;
            } else if let Some(j) = corpse {
                // Мясо выгоднее, или растение досталось более раннему ID: этот едок
                // претендует на труп раньше следующих участников.
                reserved[j].bite(now, rules.plant_energy);
            }
        }
        // Все уже сходили; новорождённых ещё нет. Удары одновременны.
        let result = crate::combat::resolve_with_grace(
            space,
            rules,
            creatures,
            prey_grid,
            counters,
            now,
            crate::combat::CombatPolicy { territorial_targets: &territorial_targets, grace: &self.grace },
        );
        counters.ranged_shots += result.shots.len() as u64;
        counters.territorial_fights += result.territorial_attacks;
        shots.extend(result.shots);
        for (flock, enemy) in result.attacked_flocks {
            self.territory.attacks.insert((flock, enemy, now));
        }
        // Трупы прошлого тика делятся между выжившими по порядку ID.
        for (i, v) in creatures.iter_mut().enumerate() {
            if !v.alive || fed[i] {
                continue;
            }
            let eats = |c: &crate::corpse::Corpse| v.pheno.corpse_efficiency(c.rot(now), hungry[i]) > 0.0;
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
                v.devour(bite.amount * v.pheno.corpse_efficiency(bite.rot, true), rules);
                v.meal =
                    Some(Meal { tick: now, x: bite.x, y: bite.y, food: Morsel::Corpse { rot: bite.rot } });
                counters.meat_bites += 1;
                counters.rot_bites += (bite.rot >= 0.5) as u64;
                fed[i] = true;
            }
        }
        for v in creatures.iter_mut().filter(|v| v.alive) {
            if let Some(child) = v.maybe_divide(space, rules) {
                // У неизменной одиночной линии каждая метка принадлежит одному
                // существу. Родитель и ребёнок уже защищены родством; хранить
                // ещё 600-тиковую пару для каждого такого рождения незачем.
                let protect = v.pheno.pack_instinct || !v.same_mode(&child);
                offspring.push((child, v.flock, protect));
            }
        }
        let before = corpses.len();
        corpses.extend(
            creatures.iter().filter(|v| !v.alive).map(|v| crate::corpse::Corpse::from_creature(v, now)),
        );
        counters.corpses += (corpses.len() - before) as u64;
        creatures.retain(|v| v.alive);
        plants.retain(|p| {
            if !p.alive() {
                plant_cells.free(p.slot());
            }
            p.alive()
        }); // выметаем съеденное
        counters.born += offspring.len() as u64;
        let mut transitions = Vec::new();
        for (child, former_flock, protect) in offspring {
            let separate = child.flock == 0;
            self.add_creature(child);
            if separate && protect {
                transitions.push((former_flock, self.next_flock - 1));
            }
        }
        if (self.tick + 1).is_multiple_of(60) {
            self.prey_grid.rebuild(&self.space, self.creatures.iter().map(|v| (v.x, v.y)));
            self.social_counts.splits += crate::flock::split_with_transitions(
                &mut self.creatures,
                &self.prey_grid,
                &mut self.split_watches,
                &mut self.next_flock,
                self.tick + 1,
                &mut transitions,
            );
            self.social_counts.strays +=
                crate::flock::stragglers(&mut self.creatures, &mut self.next_flock, now, &mut transitions);
        }
        self.social_counts.departures += crate::flock::departures_with_transitions(
            &mut self.creatures,
            &mut self.next_flock,
            now,
            &mut transitions,
        );
        self.grace.register_transitions(&transitions, now);
        let prior_alarms: Vec<_> = self.flocks.iter().filter(|(_, f)| f.alarmed).map(|(&id, _)| id).collect();
        crate::flock::update(&mut self.flocks, &mut self.creatures, &self.space, self.seed, false);
        self.social_counts.alarm_ends +=
            prior_alarms.iter().filter(|id| !self.flocks.contains_key(id)).count() as u64;
        let alarmed: std::collections::BTreeSet<_> = self
            .creatures
            .iter()
            .filter(|v| v.mind.social.activity == crate::social::Activity::Alarm)
            .map(|v| v.flock)
            .collect();
        for (id, f) in &mut self.flocks {
            let active = f.members >= 2 && alarmed.contains(id);
            self.social_counts.alarms += (active && !f.alarmed) as u64;
            self.social_counts.alarm_ends += (!active && f.alarmed) as u64;
            f.alarmed = active;
        }
    }

    // ── статистика ──────────────────────────────────────────────────────────
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

    /// Новые правила посреди партии (лаборатория на ходу). Живые существа
    /// пересчитывают всё, что вычислили из правил при рождении, — иначе новая
    /// цена действовала бы только на новорождённых, и игрок двигал бы ползунок,
    /// не видя последствий.
    pub fn set_rules(&mut self, rules: Rules) {
        for v in &mut self.creatures {
            v.apply_rules(&rules, &self.space);
        }
        // уже выросшие растения остаются на местах, новые — по новому профилю
        let flora = Flora::new(&rules, &self.space, self.seed);
        if flora != self.flora {
            // the slots moved: old plants hold none, and new ones fill in as they are eaten
            self.plants.iter_mut().for_each(Plant::lose_slot);
            self.flora = flora;
            self.plant_cells.invalidate();
        }
        self.rules = rules;
    }

    /// Где растёт еда в этом мире.
    pub fn flora(&self) -> &Flora {
        &self.flora
    }

    // ── выбор существа (для игры) ───────────────────────────────────────────

    /// Номер ближайшего к точке существа, до края тела которого не дальше
    /// `radius`. Мелкое существо находится, даже если промахнуться на `radius`;
    /// крупное — если кликнуть в любое место его тела. Вызывается по клику,
    /// поэтому простой перебор: сетки мира строятся внутри тика и к этому
    /// моменту уже устарели.
    pub fn pick(&self, x: f64, y: f64, radius: f64) -> Option<u64> {
        let dist = |v: &Creature| ((v.x - x).powi(2) + (v.y - y).powi(2)).sqrt() - v.pheno.half;
        self.creatures
            .iter()
            .map(|v| (dist(v), v.id))
            .filter(|(d, _)| *d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// Существо по id. Номера выдаются по возрастанию, новые встают в конец,
    /// а умершие удаляются с сохранением порядка — поэтому список отсортирован
    /// по id и поиск двоичный: следить за существом можно и среди миллиона.
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
    use crate::genome::creature::Gene;

    #[test]
    fn founders_deal_flock_kinds_evenly_and_free_a_quarter_of_layers() {
        let cfg = WorldConfig { seed: 17, n_creatures: Some(400), ..Default::default() };
        let (first, again) = (World::new(&cfg), World::new(&cfg));
        let mut kinds = [0usize; 4];
        let mut free = 0;
        for (a, b) in first.creatures.iter().zip(&again.creatures) {
            assert_eq!(a.genome, b.genome);
            if a.pheno.pack_instinct {
                kinds[a.pheno.flock_kind as usize] += 1;
            }
            free += !a.pheno.layer_bound as usize;
        }
        let (lo, hi) = (kinds.iter().min().unwrap(), kinds.iter().max().unwrap());
        assert!(hi - lo <= 1, "kinds are dealt evenly: {kinds:?}");
        assert!((70..=130).contains(&free), "a quarter of the founders is free of its layer: {free}");
    }

    #[test]
    fn a_child_of_another_flock_kind_or_layer_switch_is_of_another_mode() {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let base = CreatureGenome::BASE.with(Gene::PackInstinct, 1.0);
        for genome in [base, base.with(Gene::FlockKind, 2.0), base.with(Gene::LayerBound, 1.0)] {
            w.spawn(genome, 1000.0, 1000.0, None);
        }
        let v = &w.creatures;
        assert!(v[0].same_mode(&v[0].clone()));
        assert!(!v[0].same_mode(&v[1]), "another flock kind");
        assert!(!v[0].same_mode(&v[2]), "another layer switch");
    }

    #[test]
    fn founders_get_reproducible_modes_without_moving_their_birthplaces() {
        let cfg = WorldConfig { seed: 17, n_creatures: Some(400), ..Default::default() };
        let first = World::new(&cfg);
        let again = World::new(&cfg);
        let extended = World::new(&WorldConfig { n_creatures: Some(401), ..cfg });
        let mut modes = [0usize; 3];
        let mut shooters = 0;
        for (i, ((a, b), c)) in
            first.creatures.iter().zip(&again.creatures).zip(&extended.creatures).enumerate()
        {
            assert_eq!(a.genome, b.genome);
            assert_eq!((a.x, a.y), (b.x, b.y));
            // An extra founder does not shift the independent draws.
            assert_eq!(a.pheno.pack_instinct, c.pheno.pack_instinct);
            assert_eq!(a.pheno.territoriality, c.pheno.territoriality);
            assert_eq!(a.pheno.shooter, c.pheno.shooter);
            assert_eq!((a.x, a.y), (c.x, c.y));
            assert_eq!(a.pheno.pack_instinct, i % 2 == 0, "every other founder is flocking");
            shooters += a.pheno.shooter as usize;
            // loners carry territoriality too, as neutral variation
            modes[a.genome[Gene::Territoriality] as usize] += 1;
        }
        assert!(modes[0] > modes[1] && modes[1] > modes[2], "modes 50/40/10: {modes:?}");
        assert!((8..=36).contains(&shooters), "five percent of the founders shoot: {shooters}");
    }

    #[test]
    fn неизменная_одиночная_линия_не_создаёт_лишнюю_защиту_между_метками() {
        let rules = Rules::default().with("plant_rate", 0.0).unwrap().with("mutation_sigma", 0.0).unwrap();
        let mut world = World::new(&WorldConfig { n_creatures: Some(0), rules, ..Default::default() });
        let genome = CreatureGenome::BASE
            .with(Gene::PackInstinct, 0.0)
            .with(Gene::Territoriality, 0.0)
            .with(Gene::Mutability, 0.0);
        let parent_id = world.spawn(genome, 1000.0, 1000.0, Some(100.0));
        world.creatures[0].reproduction_wait = 0;
        world.step();
        assert_eq!(world.counters.born, 1);
        assert_eq!(world.creatures[1].parent, parent_id);
        assert_ne!(world.creatures[0].flock, world.creatures[1].flock);
        assert_eq!(world.grace.entries().count(), 0);
    }

    #[test]
    fn взрослый_после_ухода_из_переполненной_стаи_защищён_от_бывших_своих() {
        let rules = Rules::default().with("plant_rate", 0.0).unwrap();
        let mut world = World::new(&WorldConfig { n_creatures: Some(0), rules, ..Default::default() });
        for i in 0..55 {
            world.spawn(CreatureGenome::BASE, 1000.0 + i as f64, 1000.0, Some(100.0));
        }
        let former = world.creatures[0].flock;
        for v in &mut world.creatures {
            v.flock = former;
            v.reproduction_wait = 10_000;
        }
        world.step();
        assert_eq!(world.social_counts.departures, 1);
        let departed = world.creatures.iter().find(|v| v.flock != former).unwrap();
        assert_eq!(world.grace.entries().count(), 1);
        assert!(world.grace.contains(former, departed.flock, 601));
        assert!(!world.grace.contains(former, departed.flock, 602));
    }
}
