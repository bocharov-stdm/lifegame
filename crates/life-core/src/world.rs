//! Состояние симуляции и один логический тик.
//!
//! Порядок тика: растения → существа (снимок стада, ходы, каннибализм) →
//! счётчик тиков. Сородичей видят по снимку на начало фазы. Съеденный в этом
//! тике и умерший на своём ходу не действуют дальше. Дети копятся в отдельном
//! буфере и не ходят в тик рождения. Съеденные растения помечаются и
//! выметаются раз за тик. Каннибализм (правило) — отдельный проход после хода
//! всех существ, когда они уже стоят.
//!
//! Хищники были отдельным видом до тега `predators-final`: их заменили мутации
//! и каннибализм.

use crate::config::*;
use crate::creature::Creature;
use crate::creature::strategy as creature_strategy;
use crate::flora::Flora;
use crate::genome::{CreatureGenome, creature, variant_for};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::rng::Rng;
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
    pub meat_bites: u64,
    pub ranged_shots: u64,
    pub territorial_fights: u64,
    pub born: u64,
    pub starved: u64,
    pub old_age: u64,
    pub combat: u64,
    /// Съедены сородичами (каннибализм).
    pub cannibalized: u64,
}

impl Counters {
    /// Потоки за промежуток от `earlier` до `self`.
    pub fn since(&self, earlier: &Counters) -> Counters {
        Counters {
            plants_grown: self.plants_grown - earlier.plants_grown,
            plants_eaten: self.plants_eaten - earlier.plants_eaten,
            plant_bites: self.plant_bites - earlier.plant_bites,
            meat_bites: self.meat_bites - earlier.meat_bites,
            ranged_shots: self.ranged_shots - earlier.ranged_shots,
            territorial_fights: self.territorial_fights - earlier.territorial_fights,
            born: self.born - earlier.born,
            starved: self.starved - earlier.starved,
            old_age: self.old_age - earlier.old_age,
            combat: self.combat - earlier.combat,
            cannibalized: self.cannibalized - earlier.cannibalized,
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub corpses: Vec<crate::corpse::Corpse>,
    pub shots: Vec<crate::Shot>,
    pub territory: crate::territory::State,
    pub grace: crate::kin_grace::Grace,
    pub next_flock: u64,
    pub split_watches: Vec<crate::flock::SplitWatch>,
    pub social_counts: crate::social::Counters,
    pub flocks: std::collections::BTreeMap<u64, crate::flock::Flock>,
    flock_seed: u64,
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
            grace: Default::default(),
            next_flock: 1,
            split_watches: Vec::new(),
            social_counts: Default::default(),
            flora: Flora::new(&rules, &space),
            space,
            rules,
            tick: 0,
            plants: Vec::new(),
            creatures: Vec::with_capacity(n_start),
            counters: Counters::default(),
            flocks: Default::default(),
            flock_seed: cfg.seed,
            next_id: 1,
            rng: Rng::new(0),
            herd: Herd::new(),
            prey_grid: Grid::new(GRID_CELL),
            food_grid: Grid::new(GRID_CELL),
            corpse_grid: Grid::new(GRID_CELL),
            bitten_plants: Vec::new(),
        };
        let variants = creature_strategy::VARIANTS.len();
        for i in 0..n_start {
            let k = variant_for(i, n_start, &cfg.strategies, variants);
            // Независимые от потока мира жребии не меняют места рождения и растения.
            let mut founder = Rng::keyed(cfg.seed, 0x5A6C_5A6C_0000_0000 ^ i as u64);
            let pack = founder.random() < 0.5;
            let territory = founder.random();
            let shooter = founder.random() < 0.05;
            let mode = if !pack || territory < 0.5 {
                0.0
            } else if territory < 0.9 {
                1.0
            } else {
                2.0
            };
            let genome = CreatureGenome::BASE
                .with(creature::Gene::Strategy, k as f64)
                .with(creature::Gene::PackInstinct, f64::from(pack as u8))
                .with(creature::Gene::Territoriality, mode)
                .with(creature::Gene::Shooter, f64::from(shooter as u8));
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
    fn spawn_plants(&mut self) {
        let rate = self.rules.plant_rate * self.space.area_ratio();
        let mut count = rate as usize;
        if self.rng.random() < rate - count as f64 {
            count += 1;
        }
        let cap = self.space.per_area(PLANT_MAX);
        let count = count.min(cap.saturating_sub(self.plants.len()));
        self.counters.plants_grown += count as u64;
        for _ in 0..count {
            let mut p = self.flora.plant(&mut self.rng);
            p.born = self.tick.min(u32::MAX as u64) as u32;
            self.plants.push(p);
        }
    }

    fn update_creatures(&mut self) {
        let now = self.tick + 1;
        self.grace.prune(now);
        self.shots.retain(|s| now.saturating_sub(s.tick) <= 8);
        if self.rules.cannibals() {
            self.corpses.retain_mut(|c| c.decay(now));
        } else {
            self.corpses.clear();
        }
        crate::care::feed_children(&mut self.creatures, &self.rules);
        crate::flock::update(&mut self.flocks, &mut self.creatures, &self.space, self.flock_seed, true);
        crate::flock::food_goals(&mut self.flocks, &mut self.creatures, self.tick);
        self.prey_grid.rebuild(&self.space, self.creatures.iter().map(|v| (v.x, v.y)));
        crate::social::prepare(&mut self.creatures, &self.prey_grid, self.tick);
        let territorial_targets = if self.rules.cannibals() {
            self.territory.prepare_with_grace(
                &mut self.flocks,
                &mut self.creatures,
                &self.space,
                self.tick,
                &self.grace,
            )
        } else {
            self.territory.clear();
            for f in self.flocks.values_mut() {
                f.warned = 0;
            }
            vec![None; self.creatures.len()]
        };
        if self.rules.cannibals() {
            self.social_counts.interventions += crate::social::prepare_aid_with_grace(
                &mut self.creatures,
                &self.prey_grid,
                self.tick,
                &self.grace,
            );
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
            ..
        } = self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        corpse_grid.rebuild(space, corpses.iter().map(|c| (c.x, c.y)));
        bitten_plants.clear();
        bitten_plants.resize(plants.len(), false);
        // Сородичей видят такими, какими они были в начале фазы: исход не
        // зависит от порядка ходов. Пока съесть друг друга нельзя (каннибализм
        // выключен), бояться некого — снимок не строится, и мир бит в бит прежний.
        let herd = if rules.cannibals() {
            herd.rebuild_with_grace(space, creatures, rules.cannibal_ratio, &self.grace, now);
            Some(&*herd)
        } else {
            None
        };

        let mut offspring = Vec::new();
        for v in creatures.iter_mut() {
            v.step(&GridSenses {
                food: food_grid,
                plants,
                corpse_grid: rules.cannibals().then_some(&*corpse_grid),
                corpses,
                now,
                herd,
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
        let mut reserved_corpses = rules.cannibals().then(|| corpses.clone());
        for (i, v) in creatures.iter_mut().enumerate().filter(|(_, v)| v.alive) {
            let prefer_corpse = reserved_corpses.as_mut().is_some_and(|reserved| {
                let Some(j) = crate::corpse::contact(
                    corpse_grid,
                    reserved,
                    (v.x, v.y),
                    v.pheno.size,
                    max_corpse_half,
                    now,
                ) else {
                    return false;
                };
                let c = &reserved[j];
                let prefers = c.portion(rules.plant_energy)
                    * v.pheno.meat_efficiency
                    * crate::config::CORPSE_BITE_YIELD
                    > rules.plant_energy * rules.plant_bite_yield / f64::from(crate::plant::PORTIONS)
                        * v.pheno.plant_efficiency;
                if prefers {
                    reserved[j].bite(now, rules.plant_energy);
                }
                prefers
            });
            if !prefer_corpse {
                if let Some(finished) = bite_plant(food_grid, plants, bitten_plants, v.x, v.y, v.pheno.size) {
                    counters.plant_bites += 1;
                    counters.plants_eaten += finished as u64;
                    v.feed(1, rules);
                    fed[i] = true;
                } else if let Some(reserved) = reserved_corpses.as_mut()
                    && let Some(j) = crate::corpse::contact(
                        corpse_grid,
                        reserved,
                        (v.x, v.y),
                        v.pheno.size,
                        max_corpse_half,
                        now,
                    )
                {
                    // Растение досталось более раннему ID; этот едок теперь
                    // претендует на труп раньше следующих участников.
                    reserved[j].bite(now, rules.plant_energy);
                }
            }
        }
        if rules.cannibals() {
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
                if let Some(gain) = crate::corpse::bite(
                    corpse_grid,
                    corpses,
                    (v.x, v.y),
                    v.pheno.size,
                    max_corpse_half,
                    rules.plant_energy,
                    now,
                ) {
                    v.devour(gain * crate::config::CORPSE_BITE_YIELD, rules);
                    counters.meat_bites += 1;
                    fed[i] = true;
                }
            }
        }
        for v in creatures.iter_mut().filter(|v| v.alive) {
            if let Some(child) = v.maybe_divide(space, rules) {
                // У неизменной одиночной линии каждая метка принадлежит одному
                // существу. Родитель и ребёнок уже защищены родством; хранить
                // ещё 600-тиковую пару для каждого такого рождения незачем.
                let protect = v.pheno.pack_instinct
                    || v.pheno.pack_instinct != child.pheno.pack_instinct
                    || v.pheno.territoriality != child.pheno.territoriality
                    || v.pheno.strategy != child.pheno.strategy
                    || v.pheno.shooter != child.pheno.shooter;
                offspring.push((child, v.flock, protect));
            }
        }
        if rules.cannibals() {
            corpses.extend(
                creatures.iter().filter(|v| !v.alive).map(|v| crate::corpse::Corpse::from_creature(v, now)),
            );
        }
        creatures.retain(|v| v.alive);
        plants.retain(|p| p.alive); // выметаем съеденное
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
        }
        self.social_counts.departures += crate::flock::departures_with_transitions(
            &mut self.creatures,
            &mut self.next_flock,
            now,
            &mut transitions,
        );
        self.grace.register_transitions(&transitions, now);
        let prior_alarms: Vec<_> = self.flocks.iter().filter(|(_, f)| f.alarmed).map(|(&id, _)| id).collect();
        crate::flock::update(&mut self.flocks, &mut self.creatures, &self.space, self.flock_seed, false);
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
        if !rules.cannibals() {
            self.corpses.clear();
            self.shots.clear();
            self.territory.clear();
            for f in self.flocks.values_mut() {
                self.social_counts.alarm_ends += f.alarmed as u64;
                f.alarmed = false;
                f.warned = 0;
            }
        }
        for v in &mut self.creatures {
            v.apply_rules(&rules, &self.space);
        }
        // уже выросшие растения остаются на местах, новые — по новому профилю
        self.flora = Flora::new(&rules, &self.space);
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

#[cfg(test)]
mod trait_tests {
    use super::*;
    use crate::genome::creature::Gene;

    #[test]
    fn основатели_получают_воспроизводимые_режимы_без_смены_мест_рождения() {
        let cfg = WorldConfig { seed: 17, n_creatures: Some(400), ..Default::default() };
        let first = World::new(&cfg);
        let again = World::new(&cfg);
        let extended = World::new(&WorldConfig { n_creatures: Some(401), ..cfg });
        let mut pack = 0;
        let mut modes = [0usize; 3];
        let mut shooters = 0;
        for ((a, b), c) in first.creatures.iter().zip(&again.creatures).zip(&extended.creatures) {
            assert_eq!(a.genome, b.genome);
            assert_eq!((a.x, a.y), (b.x, b.y));
            // Дополнительный основатель не сдвигает независимые жребии.
            assert_eq!(a.pheno.pack_instinct, c.pheno.pack_instinct);
            assert_eq!(a.pheno.territoriality, c.pheno.territoriality);
            assert_eq!(a.pheno.shooter, c.pheno.shooter);
            assert_eq!((a.x, a.y), (c.x, c.y));
            pack += a.pheno.pack_instinct as usize;
            shooters += a.pheno.shooter as usize;
            if a.pheno.pack_instinct {
                modes[a.genome[Gene::Territoriality] as usize] += 1;
            } else {
                assert_eq!(a.genome[Gene::Territoriality], 0.0);
            }
        }
        assert!((160..=240).contains(&pack), "половина основателей стайные: {pack}");
        assert!(modes[0] > modes[1] && modes[1] > modes[2], "режимы 50/40/10: {modes:?}");
        assert!((8..=36).contains(&shooters), "пять процентов основателей стреляют: {shooters}");
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
