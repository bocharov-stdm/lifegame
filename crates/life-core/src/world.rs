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

impl WorldConfig {
    /// Сколько травоядных будет на старте: заданное или из конфига на площадь мира.
    pub fn vegetarians_at_start(&self) -> usize {
        self.n_vegetarians.unwrap_or_else(|| Space::scaled(self.scale).per_area(VEGETARIANS_AT_START))
    }

    pub fn predators_at_start(&self) -> usize {
        self.n_predators.unwrap_or_else(|| Space::scaled(self.scale).per_area(PREDATORS_AT_START))
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

    /// Такими приходят мигранты.
    pub predator_speed: f64,
    pub predator_vision: f64,
    /// Мир без охоты мигрантов не ждёт.
    pub hunting: bool,
    pub migrants: u64,
    pub counters: Counters,

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
        let (n_veg, n_pred) = (cfg.vegetarians_at_start(), cfg.predators_at_start());

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
            counters: Counters::default(),
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
        self.counters.plants_grown += count as u64;
        for _ in 0..count {
            let p = Plant::random(&self.space, &mut self.rng);
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
        let max_half = vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.half));

        let mut offspring = Vec::new();
        for pr in predators.iter_mut() {
            pr.step(space, |x, y, vision| nearest_prey(prey_grid, vegetarians, max_half, x, y, vision));
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
        let World { space, rules, plants, vegetarians, predators, food_grid, hunter_grid, counters, .. } =
            self;
        food_grid.rebuild(space, plants.iter().map(|p| (p.x, p.y)));
        hunter_grid.rebuild(space, predators.iter().map(|p| (p.x, p.y)));

        let mut offspring = Vec::new();
        for v in vegetarians.iter_mut() {
            if !v.alive {
                continue; // съеден хищником в этом же тике
            }
            v.step(
                |x, y, r2| nearest_predator(hunter_grid, x, y, r2),
                |x, y, r2| nearest_plant(food_grid, plants, x, y, r2),
            );
            if !v.alive {
                counters.vegetarians_starved += 1;
                continue; // умер от голода на этом ходу: не ест и не делится
            }

            // ест всё не дальше size от центра — уже с новой позиции
            let eaten = eat_plants(food_grid, plants, v.x, v.y, v.size);
            counters.plants_eaten += eaten as u64;
            v.feed(eaten, rules);

            if divide && let Some(child) = v.maybe_divide(space, rules) {
                offspring.push(child);
            }
        }
        vegetarians.retain(|v| v.alive);
        plants.retain(|p| p.alive); // выметаем съеденное
        counters.vegetarians_born += offspring.len() as u64;
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

// ── запросы к сеткам ────────────────────────────────────────────────────────
// Вынесены из тика, чтобы тест мог сверить их с честным перебором в живом мире:
// ошибка в радиусе запроса не роняет ничего, а тихо меняет баланс — существа
// перестают замечать соседей под носом.

/// Ближайшая по центрам живая добыча, которую видно по краю тела:
/// d < vision + half. `max_half` — половина самого крупного травоядного.
pub(crate) fn nearest_prey(
    grid: &Grid,
    vegetarians: &[Vegetarian],
    max_half: f64,
    x: f64,
    y: f64,
    vision: f64,
) -> Option<Prey> {
    let mut best: Option<(f64, Prey)> = None;
    grid.for_each_near(x, y, vision + max_half, |j, vx, vy| {
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
}

/// Первая живая добыча, чьё тело касается круга радиуса `own` вокруг (x, y).
pub(crate) fn prey_in_contact(
    grid: &Grid,
    vegetarians: &[Vegetarian],
    max_half: f64,
    x: f64,
    y: f64,
    own: f64,
) -> Option<usize> {
    let mut caught = None;
    grid.for_each_near(x, y, own + max_half, |j, vx, vy| {
        if caught.is_some() || !vegetarians[j].alive {
            return;
        }
        let (dx, dy) = (x - vx, y - vy);
        let reach = own + vegetarians[j].half;
        if dx * dx + dy * dy < reach * reach {
            caught = Some(j);
        }
    });
    caught
}

/// Ближайший хищник строго ближе √r2: (x, y, квадрат расстояния).
pub(crate) fn nearest_predator(grid: &Grid, x: f64, y: f64, r2: f64) -> Option<(f64, f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    grid.for_each_near(x, y, r2.sqrt(), |_, px, py| {
        let (dx, dy) = (px - x, py - y);
        let d2 = dx * dx + dy * dy;
        if d2 < best.map_or(r2, |b| b.2) {
            best = Some((px, py, d2));
        }
    });
    best
}

/// Ближайшее живое растение строго ближе √r2.
pub(crate) fn nearest_plant(grid: &Grid, plants: &[Plant], x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    grid.for_each_near(x, y, r2.sqrt(), |j, px, py| {
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
}

/// Съесть все живые растения не дальше `size` от (x, y); сколько съедено.
pub(crate) fn eat_plants(grid: &Grid, plants: &mut [Plant], x: f64, y: f64, size: f64) -> usize {
    let r2 = size * size;
    let mut eaten = 0;
    grid.for_each_near(x, y, size, |j, px, py| {
        let p = &mut plants[j];
        let (dx, dy) = (x - px, y - py);
        if p.alive && dx * dx + dy * dy <= r2 {
            p.alive = false;
            eaten += 1;
        }
    });
    eaten
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist2(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
        let (dx, dy) = (ax - bx, ay - by);
        dx * dx + dy * dy
    }

    fn min(v: impl Iterator<Item = f64>) -> Option<f64> {
        v.fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))))
    }

    /// Каждый запрос тика сверяется с перебором всех существ — на настоящих
    /// позициях и размерах живого мира, а не на выдуманных точках. Ловит неверный
    /// радиус запроса, забытую половину тела и сломанную сетку. Второй мир — с
    /// дешёвым размером: там вырастают гиганты, и радиус хищника растёт с ними.
    #[test]
    fn запросы_к_сеткам_совпадают_с_перебором_в_живом_мире() {
        let giants = Rules::default().with("size_power", 1.0).unwrap();
        for (seed, rules) in [(1, Rules::default()), (4, giants)] {
            let mut w = World::new(&WorldConfig { seed, rules, ..Default::default() });
            let (mut prey, mut food, mut hunters) =
                (Grid::new(GRID_CELL), Grid::new(GRID_CELL), Grid::new(GRID_CELL));
            let mut checked = 0;
            for tick in 0..1500 {
                w.step();
                if tick % 50 != 0 {
                    continue;
                }
                // часть существ и растений «съедена в этом тике» — их запросы обязаны пропускать
                let mut vegs = w.vegetarians.clone();
                vegs.iter_mut().step_by(7).for_each(|v| v.alive = false);
                let mut plants = w.plants.clone();
                plants.iter_mut().step_by(5).for_each(|p| p.alive = false);
                prey.rebuild(&w.space, vegs.iter().map(|v| (v.x, v.y)));
                food.rebuild(&w.space, plants.iter().map(|p| (p.x, p.y)));
                hunters.rebuild(&w.space, w.predators.iter().map(|p| (p.x, p.y)));
                let max_half = vegs.iter().fold(0.0_f64, |m, v| m.max(v.half));
                let alive_vegs = || vegs.iter().filter(|v| v.alive);

                for p in &w.predators {
                    for vision in [p.vision, 2000.0] {
                        let got = nearest_prey(&prey, &vegs, max_half, p.x, p.y, vision)
                            .map(|q| dist2(p.x, p.y, q.x, q.y));
                        let want = min(alive_vegs()
                            .map(|v| (dist2(p.x, p.y, v.x, v.y), v.half))
                            .filter(|&(d2, half)| d2 < (vision + half) * (vision + half))
                            .map(|(d2, _)| d2));
                        assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшая добыча");
                    }
                    for own in [PREDATOR_DIAM / 2.0, 300.0] {
                        let touches =
                            |v: &Vegetarian| dist2(p.x, p.y, v.x, v.y) < (own + v.half) * (own + v.half);
                        let got = prey_in_contact(&prey, &vegs, max_half, p.x, p.y, own);
                        assert_eq!(got.is_some(), alive_vegs().any(touches), "сид {seed}: поимка");
                        if let Some(j) = got {
                            assert!(vegs[j].alive && touches(&vegs[j]), "сид {seed}: пойман не тот");
                        }
                    }
                    checked += 1;
                }
                for v in &w.vegetarians {
                    let got = nearest_predator(&hunters, v.x, v.y, v.vision2).map(|(_, _, d2)| d2);
                    let want = min(w
                        .predators
                        .iter()
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайший хищник");

                    let got = nearest_plant(&food, &plants, v.x, v.y, v.vision2)
                        .map(|(px, py)| dist2(px, py, v.x, v.y));
                    let want = min(plants
                        .iter()
                        .filter(|p| p.alive)
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение");

                    let mut eaten_by_grid = plants.clone();
                    let n = eat_plants(&food, &mut eaten_by_grid, v.x, v.y, v.size);
                    let want: Vec<bool> =
                        plants.iter().map(|p| p.alive && dist2(p.x, p.y, v.x, v.y) <= v.size2).collect();
                    let got: Vec<bool> =
                        plants.iter().zip(&eaten_by_grid).map(|(a, b)| a.alive && !b.alive).collect();
                    assert_eq!(got, want, "сид {seed}, тик {tick}: съедено не то");
                    assert_eq!(n, want.iter().filter(|&&e| e).count());
                    checked += 1;
                }
            }
            assert!(checked > 1000, "сид {seed}: проверено всего {checked} запросов — мир вымер?");
            if seed == 4 {
                let biggest = w.vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.size));
                assert!(
                    biggest > 100.0,
                    "гиганты не выросли ({biggest:.0}) — вторая часть теста бессмысленна"
                );
            }
        }
    }
}
