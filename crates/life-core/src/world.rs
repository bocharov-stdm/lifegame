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
use crate::senses::{GridSenses, Herd, eat_plants};
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
}

impl World {
    pub fn new(cfg: &WorldConfig) -> Self {
        let space = cfg.space();
        let rules = cfg.rules.clone();
        let mut rng = Rng::keyed(cfg.seed, 0);
        let n_start = cfg.creatures_at_start();

        let mut w = World {
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
        };
        let variants = creature_strategy::VARIANTS.len();
        for i in 0..n_start {
            let k = variant_for(i, n_start, &cfg.strategies, variants);
            let genome = CreatureGenome::BASE.with(creature::Gene::Strategy, k as f64);
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
        crate::flock::update(&mut self.flocks, &mut self.creatures, &self.space, self.flock_seed, true);
        crate::flock::food_goals(&mut self.flocks, &mut self.creatures, self.tick);
        self.prey_grid.rebuild(&self.space, self.creatures.iter().map(|v| (v.x, v.y)));
        crate::social::prepare(&mut self.creatures, &self.prey_grid, self.tick);
        if self.rules.cannibals() {
            self.social_counts.interventions +=
                crate::social::prepare_aid(&mut self.creatures, &self.prey_grid, self.tick);
        }
        let World { space, rules, plants, creatures, herd, prey_grid, food_grid, counters, .. } = self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        // Сородичей видят такими, какими они были в начале фазы: исход не
        // зависит от порядка ходов. Пока съесть друг друга нельзя (каннибализм
        // выключен), бояться некого — снимок не строится, и мир бит в бит прежний.
        let herd = if rules.cannibals() {
            herd.rebuild(space, creatures, rules.cannibal_ratio);
            Some(&*herd)
        } else {
            None
        };

        let mut offspring = Vec::new();
        for v in creatures.iter_mut() {
            v.step(&GridSenses { food: food_grid, plants, herd });
            if !v.alive {
                if v.death == Some(crate::creature::Death::OldAge) {
                    counters.old_age += 1;
                } else {
                    counters.starved += 1;
                }
                continue; // умер от голода на этом ходу: не ест и не делится
            }
        }
        // Все решения видели одни растения; питание начинается после всех ходов.
        for v in creatures.iter_mut().filter(|v| v.alive) {
            // ест всё не дальше size от центра — уже с новой позиции
            let eaten = eat_plants(food_grid, plants, v.x, v.y, v.pheno.size);
            counters.plants_eaten += eaten as u64;
            v.feed(eaten, rules);
        }
        if rules.cannibals() {
            // все уже сходили, дети ещё в буфере — они в тик рождения не едят и не съедаются
            crate::combat::resolve(space, rules, creatures, prey_grid, counters, self.tick + 1);
        }
        for v in creatures.iter_mut().filter(|v| v.alive) {
            if let Some(child) = v.maybe_divide(space, rules) {
                offspring.push(child);
            }
        }
        creatures.retain(|v| v.alive);
        plants.retain(|p| p.alive); // выметаем съеденное
        counters.born += offspring.len() as u64;
        for child in offspring {
            self.add_creature(child);
        }
        if (self.tick + 1).is_multiple_of(60) {
            self.prey_grid.rebuild(&self.space, self.creatures.iter().map(|v| (v.x, v.y)));
            self.social_counts.splits += crate::flock::split(
                &mut self.creatures,
                &self.prey_grid,
                &mut self.split_watches,
                &mut self.next_flock,
                self.tick + 1,
            );
        }
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
            for f in self.flocks.values_mut() {
                self.social_counts.alarm_ends += f.alarmed as u64;
                f.alarmed = false;
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
