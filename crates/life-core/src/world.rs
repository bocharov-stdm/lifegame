//! Состояние симуляции и один логический тик.
//!
//! Порядок тика — как в Python-версии: растения → хищники → травоядные →
//! счётчик тиков → миграция. Травоядные видят уже сдвинутых хищников. Съеденный
//! в этом тике и умерший на своём ходу не действуют дальше. Дети копятся в
//! отдельном буфере и не ходят в тик рождения. Съеденные растения помечаются
//! и выметаются раз за тик.

use crate::config::*;
use crate::genome::Genom;
use crate::grid::Grid;
use crate::plant::Plant;
use crate::predator::{Predator, Prey};
use crate::rng::Rng;
use crate::rules::Rules;
use crate::space::Space;
use crate::vegetarian::Vegetarian;

/// С чего начинается мир. None у численностей — значение из конфига,
/// пересчитанное на площадь мира.
#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub seed: u64,
    pub scale: f64,
    pub rules: Rules,
    pub n_vegetarians: Option<usize>,
    pub n_predators: Option<usize>,
    pub predator_speed: f64,
    pub predator_vision: f64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig {
            seed: 1,
            scale: 1.0,
            rules: Rules::default(),
            n_vegetarians: None,
            n_predators: None,
            predator_speed: PREDATOR_BASE_SPEED,
            predator_vision: PREDATOR_BASE_VISION,
        }
    }
}

/// Сводка по популяции; `avg_genom` и `avg_energy` — None, если травоядных нет.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub tick: u64,
    pub plants: usize,
    pub vegetarians: usize,
    pub predators: usize,
    pub avg_genom: Option<[f64; 7]>,
    pub avg_energy: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct World {
    pub space: Space,
    pub rules: Rules,
    pub tick: u64,
    pub plants: Vec<Plant>,
    pub vegetarians: Vec<Vegetarian>,
    pub predators: Vec<Predator>,

    /// Такими приходят мигранты.
    pub predator_speed: f64,
    pub predator_vision: f64,
    /// Мир без охоты мигрантов не ждёт.
    pub hunting: bool,
    pub migrants: u64,

    next_id: u64,
    /// Поток мира: растения и мигранты. У каждого существа поток свой.
    rng: Rng,
    prey_grid: Grid,
    food_grid: Grid,
    hunter_grid: Grid,
}

impl World {
    pub fn new(cfg: &WorldConfig) -> Self {
        let space = Space::scaled(cfg.scale);
        let rules = cfg.rules.clone();
        let mut rng = Rng::keyed(cfg.seed, 0);
        let n_veg = cfg.n_vegetarians.unwrap_or_else(|| space.per_area(VEGETARIANS_AT_START));
        let n_pred = cfg.n_predators.unwrap_or_else(|| space.per_area(PREDATORS_AT_START));

        let mut w = World {
            space,
            rules,
            tick: 0,
            plants: Vec::new(),
            vegetarians: Vec::with_capacity(n_veg),
            predators: Vec::with_capacity(n_pred),
            predator_speed: cfg.predator_speed,
            predator_vision: cfg.predator_vision,
            hunting: n_pred > 0,
            migrants: 0,
            next_id: 1,
            rng: Rng::new(0),
            prey_grid: Grid::new(GRID_CELL),
            food_grid: Grid::new(GRID_CELL),
            hunter_grid: Grid::new(GRID_CELL),
        };
        for _ in 0..n_veg {
            let v = Vegetarian::base(&w.space, &w.rules, rng.fork());
            w.add_vegetarian(v);
        }
        for _ in 0..n_pred {
            let p = Predator::new(
                &w.space,
                &w.rules,
                None,
                None,
                None,
                cfg.predator_speed,
                cfg.predator_vision,
                rng.fork(),
            );
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
    pub fn spawn_vegetarian(&mut self, genom: Genom, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let v = Vegetarian::new(&self.space, &self.rules, genom, Some(x), Some(y), energy, rng);
        self.add_vegetarian(v);
        self.next_id - 1
    }

    /// Новый хищник со стартовыми скоростью и зрением в заданном месте.
    pub fn spawn_predator(&mut self, x: f64, y: f64, energy: Option<f64>) -> u64 {
        let rng = self.rng.fork();
        let p = Predator::new(
            &self.space,
            &self.rules,
            Some(x),
            Some(y),
            energy,
            self.predator_speed,
            self.predator_vision,
            rng,
        );
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
        for _ in 0..count {
            let p = Plant::random(&self.space, &mut self.rng);
            self.plants.push(p);
        }
    }

    /// Если хищников почти не осталось, раз в период с края мира приходит новый.
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
        let (x, y) = if self.rng.random() < 0.5 {
            (self.rng.choose2(d, w - d), self.rng.uniform(d, h - d)) // левый/правый край
        } else {
            (self.rng.uniform(d, w - d), self.rng.choose2(d, h - d)) // верхний/нижний
        };
        self.spawn_predator(x, y, None);
        self.migrants += 1;
    }

    fn update_predators(&mut self) {
        let divide = self.tick.is_multiple_of(DIVIDE_PERIOD);
        let World { space, rules, vegetarians, predators, prey_grid, .. } = self;
        prey_grid.rebuild(space, vegetarians.iter().map(|v| (v.x, v.y)));
        // Хищник видит и ловит по краю тела, поэтому к радиусу запроса к сетке
        // прибавляется половина самого крупного травоядного.
        let max_half = vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.half));

        let mut offspring = Vec::new();
        for pr in predators.iter_mut() {
            pr.step(space, |x, y, vision| {
                let mut best: Option<(f64, Prey)> = None;
                prey_grid.for_each_near(x, y, vision + max_half, |j, vx, vy| {
                    let v = &vegetarians[j];
                    if !v.alive {
                        return; // съеден другим хищником в этом же тике
                    }
                    let (dx, dy) = (x - vx, y - vy);
                    let d2 = dx * dx + dy * dy;
                    let reach = vision + v.half;
                    if d2 < reach * reach && best.is_none_or(|(b, _)| d2 < b) {
                        best = Some((d2, Prey { x: vx, y: vy, half: v.half }));
                    }
                });
                best.map(|(_, p)| p)
            });
            if !pr.alive {
                continue; // умер от голода на этом ходу: не охотится и не делится
            }

            if let Some(own) = pr.catch_reach() {
                let mut caught = None;
                prey_grid.for_each_near(pr.x, pr.y, own + max_half, |j, vx, vy| {
                    if caught.is_some() || !vegetarians[j].alive {
                        return;
                    }
                    let (dx, dy) = (pr.x - vx, pr.y - vy);
                    let reach = own + vegetarians[j].half;
                    if dx * dx + dy * dy < reach * reach {
                        caught = Some(j);
                    }
                });
                if let Some(j) = caught {
                    let v = &mut vegetarians[j];
                    let gain = v.energy;
                    v.alive = false;
                    v.energy = 0.0;
                    pr.eat(gain, space);
                }
            }

            if divide && let Some(child) = pr.maybe_divide(space, rules) {
                offspring.push(child); // ← в буфер, а не в список обхода
            }
        }
        predators.retain(|p| p.alive);
        for child in offspring {
            self.add_predator(child);
        }
    }

    fn update_vegetarians(&mut self) {
        let divide = self.tick.is_multiple_of(DIVIDE_PERIOD);
        let World { space, rules, plants, vegetarians, predators, food_grid, hunter_grid, .. } = self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        hunter_grid.rebuild(space, predators.iter().map(|p| (p.x, p.y)));

        let mut offspring = Vec::new();
        for v in vegetarians.iter_mut() {
            if !v.alive {
                continue; // съеден хищником в этом же тике
            }
            v.step(
                |x, y, r2| {
                    let mut best: Option<(f64, f64, f64)> = None;
                    hunter_grid.for_each_near(x, y, r2.sqrt(), |_, px, py| {
                        let (dx, dy) = (px - x, py - y);
                        let d2 = dx * dx + dy * dy;
                        if d2 < best.map_or(r2, |b| b.2) {
                            best = Some((px, py, d2));
                        }
                    });
                    best
                },
                |x, y, r2| {
                    let mut best: Option<(f64, f64, f64)> = None;
                    food_grid.for_each_near(x, y, r2.sqrt(), |j, px, py| {
                        if !plants[j].alive {
                            return; // съедено раньше в этом же тике
                        }
                        let (dx, dy) = (px - x, py - y);
                        let d2 = dx * dx + dy * dy;
                        if d2 < best.map_or(r2, |b| b.2) {
                            best = Some((px, py, d2));
                        }
                    });
                    best.map(|(px, py, _)| (px, py))
                },
            );
            if !v.alive {
                continue; // умер от голода на этом ходу: не ест и не делится
            }

            // ест всё не дальше size от центра — уже с новой позиции
            let (x, y, r2) = (v.x, v.y, v.size2);
            let mut eaten = 0;
            food_grid.for_each_near(x, y, v.size, |j, px, py| {
                let p = &mut plants[j];
                let (dx, dy) = (x - px, y - py);
                if p.alive && dx * dx + dy * dy <= r2 {
                    p.alive = false;
                    eaten += 1;
                }
            });
            v.feed(eaten, rules);

            if divide && let Some(child) = v.maybe_divide(space, rules) {
                offspring.push(child);
            }
        }
        vegetarians.retain(|v| v.alive);
        plants.retain(|p| p.alive); // выметаем съеденное
        for child in offspring {
            self.add_vegetarian(child);
        }
    }

    // ── статистика ──────────────────────────────────────────────────────────
    pub fn stats(&self) -> Stats {
        let n = self.vegetarians.len();
        let (avg_genom, avg_energy) = if n == 0 {
            (None, None)
        } else {
            let mut sum = [0.0; 7];
            let mut energy = 0.0;
            for v in &self.vegetarians {
                for (s, g) in sum.iter_mut().zip(v.genom.to_array()) {
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
}
