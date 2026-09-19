//! Состояние симуляции и один логический тик.
//!
//! Порядок тика — как в Python-версии: растения → хищники → травоядные →
//! счётчик тиков → миграция. Травоядные видят уже сдвинутых хищников. Съеденный
//! в этом тике и умерший на своём ходу не действуют дальше. Дети копятся в
//! отдельном буфере и не ходят в тик рождения. Съеденные растения помечаются
//! и выметаются раз за тик. Каннибализм (правило) — отдельный проход после
//! хода всех травоядных, когда они уже стоят.

use crate::config::*;
use crate::flora::Flora;
use crate::genome::{PredatorGenome, VegetarianGenome, predator, variant_for, vegetarian};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::predator::Predator;
use crate::predator::strategy as predator_strategy;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::senses::{
    GridPredatorSenses, GridVegetarianSenses, eat_plants, prey_in_contact, smaller_prey_in_contact,
};
use crate::space::{Shape, Space};
use crate::vegetarian::Vegetarian;
use crate::vegetarian::strategy as vegetarian_strategy;

/// С чего начинается мир. None у травоядных — значение из конфига,
/// пересчитанное на площадь мира; у хищников — ни одного.
#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub seed: u64,
    pub scale: f64,
    /// Форма мира. По умолчанию 3:2: при x1 это базовый мир 6000x4000, тот же,
    /// что у полосы, а большой мир растёт в обе стороны, а не в ленту.
    pub shape: Shape,
    pub rules: Rules,
    pub n_vegetarians: Option<usize>,
    pub n_predators: Option<usize>,
    pub predator_speed: f64,
    pub predator_vision: f64,
    /// Стартовая смесь стратегий: доли вариантов по порядку `VARIANTS` своего
    /// вида (пустая — у всех первый). Раздаётся без жребия (`variant_for`).
    pub vegetarian_strategies: Vec<f64>,
    pub predator_strategies: Vec<f64>,
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig {
            seed: 1,
            scale: 1.0,
            shape: Shape::R3x2,
            rules: Rules::default(),
            n_vegetarians: None,
            n_predators: None,
            predator_speed: PREDATOR_BASE_SPEED,
            predator_vision: PREDATOR_BASE_VISION,
            vegetarian_strategies: Vec::new(),
            predator_strategies: Vec::new(),
        }
    }
}

impl WorldConfig {
    /// Размеры мира: масштаб задаёт площадь, форма — пропорции.
    pub fn space(&self) -> Space {
        Space::new(self.scale, self.shape)
    }

    /// Сколько травоядных будет на старте: заданное или из конфига на площадь мира.
    pub fn vegetarians_at_start(&self) -> usize {
        self.n_vegetarians.unwrap_or_else(|| self.space().per_area(VEGETARIANS_AT_START))
    }

    /// Сколько хищников будет на старте: заданное или ни одного — по
    /// умолчанию мир без хищников.
    pub fn predators_at_start(&self) -> usize {
        self.n_predators.unwrap_or(0)
    }

    /// Тот же мир, но с хищниками: `PREDATORS_PER_AREA` на базовый участок.
    /// Звать после масштаба и формы — число растёт с площадью.
    pub fn with_predators(mut self) -> Self {
        self.n_predators = Some(self.space().per_area(PREDATORS_PER_AREA));
        self
    }

    /// Геном стартовых хищников и мигрантов.
    pub fn predator_genome(&self) -> PredatorGenome {
        use crate::genome::predator::Gene;
        PredatorGenome::BASE.with(Gene::Speed, self.predator_speed).with(Gene::Vision, self.predator_vision)
    }
}

/// Сводка по популяции; `avg_genom` и `avg_energy` — None, если травоядных нет.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub tick: u64,
    pub plants: usize,
    pub vegetarians: usize,
    pub predators: usize,
    pub avg_genom: Option<[f64; vegetarian::N]>,
    pub avg_energy: Option<f64>,
}

/// Сколько всего выросло, родилось и умерло с начала мира. Численность говорит,
/// ЧТО стало, а разность двух снимков счётчиков — ОТЧЕГО: травоядных стало
/// меньше, потому что их съели или потому что им нечего есть. Стартовые
/// существа и мигранты рождениями не считаются (мигранты — `World::migrants`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub plants_grown: u64,
    pub plants_eaten: u64,
    pub vegetarians_born: u64,
    pub vegetarians_eaten: u64,
    pub vegetarians_starved: u64,
    /// Съедены сородичами (каннибализм).
    pub vegetarians_cannibalized: u64,
    pub predators_born: u64,
    pub predators_starved: u64,
}

impl Counters {
    /// Потоки за промежуток от `earlier` до `self`.
    pub fn since(&self, earlier: &Counters) -> Counters {
        Counters {
            plants_grown: self.plants_grown - earlier.plants_grown,
            plants_eaten: self.plants_eaten - earlier.plants_eaten,
            vegetarians_born: self.vegetarians_born - earlier.vegetarians_born,
            vegetarians_eaten: self.vegetarians_eaten - earlier.vegetarians_eaten,
            vegetarians_starved: self.vegetarians_starved - earlier.vegetarians_starved,
            vegetarians_cannibalized: self.vegetarians_cannibalized - earlier.vegetarians_cannibalized,
            predators_born: self.predators_born - earlier.predators_born,
            predators_starved: self.predators_starved - earlier.predators_starved,
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub space: Space,
    pub rules: Rules,
    pub tick: u64,
    pub plants: Vec<Plant>,
    pub vegetarians: Vec<Vegetarian>,
    pub predators: Vec<Predator>,

    /// Такими приходят мигранты и подсаженные хищники.
    pub predator_start: PredatorGenome,
    /// Мир без охоты мигрантов не ждёт.
    pub hunting: bool,
    pub migrants: u64,
    pub counters: Counters,

    next_id: u64,
    /// Где растёт еда — выведено из правил и размеров мира, пересчитывается
    /// вместе с правилами (`set_rules`).
    flora: Flora,
    /// Поток мира: растения и мигранты. У каждого существа поток свой.
    rng: Rng,
    prey_grid: Grid,
    food_grid: Grid,
    hunter_grid: Grid,
}

impl World {
    pub fn new(cfg: &WorldConfig) -> Self {
        let space = cfg.space();
        let rules = cfg.rules.clone();
        let mut rng = Rng::keyed(cfg.seed, 0);
        let (n_veg, n_pred) = (cfg.vegetarians_at_start(), cfg.predators_at_start());

        let mut w = World {
            flora: Flora::new(&rules, &space),
            space,
            rules,
            tick: 0,
            plants: Vec::new(),
            vegetarians: Vec::with_capacity(n_veg),
            predators: Vec::with_capacity(n_pred),
            predator_start: cfg.predator_genome(),
            hunting: n_pred > 0,
            migrants: 0,
            counters: Counters::default(),
            next_id: 1,
            rng: Rng::new(0),
            prey_grid: Grid::new(GRID_CELL),
            food_grid: Grid::new(GRID_CELL),
            hunter_grid: Grid::new(GRID_CELL),
        };
        let variants = vegetarian_strategy::VARIANTS.len();
        for i in 0..n_veg {
            let k = variant_for(i, n_veg, &cfg.vegetarian_strategies, variants);
            let genome = VegetarianGenome::BASE.with(vegetarian::Gene::Strategy, k as f64);
            let v = Vegetarian::new(&w.space, &w.rules, genome, None, None, None, rng.fork());
            w.add_vegetarian(v);
        }
        let variants = predator_strategy::VARIANTS.len();
        for i in 0..n_pred {
            let k = variant_for(i, n_pred, &cfg.predator_strategies, variants);
            let genome = w.predator_start.with(predator::Gene::Strategy, k as f64);
            let p = Predator::new(&w.space, &w.rules, genome, None, None, None, rng.fork());
            w.add_predator(p);
        }
        w.rng = rng;
        w
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_vegetarian(&mut self, mut v: Vegetarian) {
        v.id = self.take_id();
        self.vegetarians.push(v);
    }

    pub fn add_predator(&mut self, mut p: Predator) {
        p.id = self.take_id();
        self.predators.push(p);
    }

    /// Новое травоядное с заданным геномом в заданном месте (для тестов и игры).
    pub fn spawn_vegetarian(&mut self, genome: VegetarianGenome, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let v = Vegetarian::new(&self.space, &self.rules, genome, Some(x), Some(y), energy, rng);
        self.add_vegetarian(v);
        self.next_id - 1
    }

    /// Новый хищник со стартовыми скоростью и зрением в заданном месте.
    pub fn spawn_predator(&mut self, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let p = Predator::new(&self.space, &self.rules, self.predator_start, Some(x), Some(y), energy, rng);
        self.add_predator(p);
        self.next_id - 1
    }

    // ── один логический тик ─────────────────────────────────────────────────
    pub fn step(&mut self) {
        self.spawn_plants();
        self.update_predators();
        self.update_vegetarians();
        self.tick += 1;
        self.migrate_predators(); // после счёта: на тике 0 мигрантов нет
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

    /// Если хищников почти не осталось, раз в период с краёв мира приходят новые:
    /// по одному на базовую площадь. Иначе в мире x100 пороги выросли бы, а приток
    /// остался прежним — на единицу площади в сто раз слабее.
    pub fn migrate_predators(&mut self) {
        let period = self.rules.predator_migration as u64;
        if period == 0
            || !self.hunting
            || !self.tick.is_multiple_of(period)
            || self.predators.len() >= self.space.per_area(PREDATOR_MIGRATION_MIN)
            || self.vegetarians.len() < self.space.per_area(PREDATOR_MIGRATION_PREY)
        {
            return;
        }
        let d = PREDATOR_DIAM;
        let (w, h) = (self.space.width, self.space.height);
        for _ in 0..self.space.per_area(1) {
            let (x, y) = if self.rng.random() < 0.5 {
                (self.rng.choose2(d, w - d), self.rng.uniform(d, h - d)) // левый/правый край
            } else {
                (self.rng.uniform(d, w - d), self.rng.choose2(d, h - d)) // верхний/нижний
            };
            self.spawn_predator(x, y, None);
            self.migrants += 1;
        }
    }

    fn update_predators(&mut self) {
        let divide = self.tick.is_multiple_of(DIVIDE_PERIOD);
        let World { space, rules, vegetarians, predators, prey_grid, counters, .. } = self;
        prey_grid.rebuild(space, vegetarians.iter().map(|v| (v.x, v.y)));
        // Хищник видит и ловит по краю тела, поэтому к радиусу запроса к сетке
        // прибавляется половина самого крупного травоядного.
        let max_half = vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.pheno.half));

        let mut offspring = Vec::new();
        for pr in predators.iter_mut() {
            pr.step(space, &GridPredatorSenses { prey: prey_grid, vegetarians, max_half });
            if !pr.alive {
                counters.predators_starved += 1;
                continue; // умер от голода на этом ходу: не охотится и не делится
            }

            if let Some(own) = pr.catch_reach()
                && let Some(j) = prey_in_contact(prey_grid, vegetarians, max_half, pr.x, pr.y, own)
            {
                let v = &mut vegetarians[j];
                let gain = v.energy;
                v.alive = false;
                v.energy = 0.0;
                counters.vegetarians_eaten += 1;
                pr.eat(gain, space);
            }

            if divide && let Some(child) = pr.maybe_divide(space, rules) {
                offspring.push(child); // ← в буфер, а не в список обхода
            }
        }
        predators.retain(|p| p.alive);
        counters.predators_born += offspring.len() as u64;
        for child in offspring {
            self.add_predator(child);
        }
    }

    fn update_vegetarians(&mut self) {
        let divide = self.tick.is_multiple_of(DIVIDE_PERIOD);
        let World {
            space,
            rules,
            plants,
            vegetarians,
            predators,
            prey_grid,
            food_grid,
            hunter_grid,
            counters,
            ..
        } = self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        hunter_grid.rebuild(space, predators.iter().map(|p| (p.x, p.y)));

        let mut offspring = Vec::new();
        for v in vegetarians.iter_mut() {
            if !v.alive {
                continue; // съеден хищником в этом же тике
            }
            v.step(&GridVegetarianSenses { hunters: hunter_grid, food: food_grid, plants });
            if !v.alive {
                counters.vegetarians_starved += 1;
                continue; // умер от голода на этом ходу: не ест и не делится
            }

            // ест всё не дальше size от центра — уже с новой позиции
            let eaten = eat_plants(food_grid, plants, v.x, v.y, v.pheno.size);
            counters.plants_eaten += eaten as u64;
            v.feed(eaten, rules);

            if divide && let Some(child) = v.maybe_divide(space, rules) {
                offspring.push(child);
            }
        }
        if rules.cannibals() {
            // все уже сходили, дети ещё в буфере — они в тик рождения не едят и не съедаются
            Self::cannibalism(space, rules, vegetarians, prey_grid, counters);
        }
        vegetarians.retain(|v| v.alive);
        plants.retain(|p| p.alive); // выметаем съеденное
        counters.vegetarians_born += offspring.len() as u64;
        for child in offspring {
            self.add_vegetarian(child);
        }
    }

    /// Каннибализм: травоядные по порядку номеров съедают по одному сородичу,
    /// который мельче в `cannibal_ratio` раз и касается телом радиуса поедания
    /// (радиус — размер, как у растений). Это физика, а не чувство: никто не
    /// ищет сородичей, едят тех, кто уже рядом. Травоядные в этом проходе стоят,
    /// поэтому копии координат в сетке верны. Случайных чисел нет.
    fn cannibalism(
        space: &Space,
        rules: &Rules,
        vegetarians: &mut [Vegetarian],
        grid: &mut Grid,
        counters: &mut Counters,
    ) {
        grid.rebuild(space, vegetarians.iter().map(|v| (v.x, v.y)));
        let max_half = vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.pheno.half));
        for i in 0..vegetarians.len() {
            let v = &vegetarians[i];
            if !v.alive {
                continue; // съеден раньше в этом проходе или умер от голода
            }
            let max_size = v.pheno.size / rules.cannibal_ratio;
            let Some(j) =
                smaller_prey_in_contact(grid, vegetarians, max_half, i, (v.x, v.y), v.pheno.size, max_size)
            else {
                continue;
            };
            let prey = &mut vegetarians[j];
            let gain = prey.energy;
            prey.alive = false;
            prey.energy = 0.0;
            counters.vegetarians_cannibalized += 1;
            vegetarians[i].devour(gain);
        }
    }

    // ── статистика ──────────────────────────────────────────────────────────
    pub fn stats(&self) -> Stats {
        let n = self.vegetarians.len();
        let (avg_genom, avg_energy) = if n == 0 {
            (None, None)
        } else {
            let mut sum = [0.0; vegetarian::N];
            let mut energy = 0.0;
            for v in &self.vegetarians {
                for (s, g) in sum.iter_mut().zip(v.genome.to_values()) {
                    *s += g;
                }
                energy += v.energy;
            }
            (Some(sum.map(|s| s / n as f64)), Some(energy / n as f64))
        };
        Stats {
            tick: self.tick,
            plants: self.plants.len(),
            vegetarians: n,
            predators: self.predators.len(),
            avg_genom,
            avg_energy,
        }
    }

    /// Новые правила посреди партии (лаборатория на ходу). Живые существа
    /// пересчитывают всё, что вычислили из правил при рождении, — иначе новая
    /// цена действовала бы только на новорождённых, и игрок двигал бы ползунок,
    /// не видя последствий.
    pub fn set_rules(&mut self, rules: Rules) {
        for v in &mut self.vegetarians {
            v.apply_rules(&rules, &self.space);
        }
        for p in &mut self.predators {
            p.apply_rules(&rules);
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

    /// Ближайшее к точке травоядное или хищник, до края тела которого не дальше
    /// `radius`. Мелкое существо находится, даже если промахнуться на `radius`;
    /// крупное — если кликнуть в любое место его тела. Вызывается по клику,
    /// поэтому простой перебор: сетки мира строятся внутри тика и к этому
    /// моменту уже устарели.
    pub fn pick(&self, x: f64, y: f64, radius: f64) -> Option<Creature> {
        let dist = |cx: f64, cy: f64, half: f64| ((cx - x).powi(2) + (cy - y).powi(2)).sqrt() - half;
        let vegs =
            self.vegetarians.iter().map(|v| (dist(v.x, v.y, v.pheno.half), Creature::Vegetarian(v.id)));
        let half = Predator::DIAM / 2.0;
        let preds = self.predators.iter().map(|p| (dist(p.x, p.y, half), Creature::Predator(p.id)));
        vegs.chain(preds).filter(|(d, _)| *d <= radius).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, c)| c)
    }

    /// Травоядное по id. Номера выдаются по возрастанию, новые встают в конец,
    /// а умершие удаляются с сохранением порядка — поэтому список отсортирован
    /// по id и поиск двоичный: следить за существом можно и среди миллиона.
    pub fn vegetarian(&self, id: u64) -> Option<&Vegetarian> {
        self.vegetarians.binary_search_by_key(&id, |v| v.id).ok().map(|i| &self.vegetarians[i])
    }

    pub fn predator(&self, id: u64) -> Option<&Predator> {
        self.predators.binary_search_by_key(&id, |p| p.id).ok().map(|i| &self.predators[i])
    }
}

/// Выбранное существо: вид и постоянный номер.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Creature {
    Vegetarian(u64),
    Predator(u64),
}
