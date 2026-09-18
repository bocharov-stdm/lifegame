//! Чувства: что существо может узнать о мире.
//!
//! Существа не видят сетку соседей: они спрашивают «где ближайший хищник»,
//! «где ближайшее растение», «кого я вижу из добычи» — через эти трейты. Мир
//! отвечает по сеткам (виды `Grid…Senses` ниже, строятся на каждое существо),
//! тесты — обычными замыканиями (`vegetarian_senses`, `predator_senses`) или
//! слепотой (`Blind`). Запросы ленивые: пока травоядное бежит, растения оно не
//! ищет вовсе.
//!
//! Новое чувство — метод трейта, функция-запрос внизу и её сверка с перебором
//! в тесте этого модуля. Чувств, смотрящих на свой же вид, не делать до
//! параллельного тика: травоядные двигаются в своей фазе, и копии координат в
//! сетке устарели бы (см. CLAUDE.md, «Neighbour search»).

use crate::grid::Grid;
use crate::plant::Plant;
use crate::vegetarian::Vegetarian;

/// Что хищнику нужно знать о добыче: где она и какого размера.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prey {
    pub x: f64,
    pub y: f64,
    pub half: f64,
}

/// Чувства травоядного.
pub trait VegetarianSenses {
    /// Ближайший хищник строго ближе √r2: (x, y, квадрат расстояния).
    fn nearest_predator(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64, f64)>;
    /// Ближайшее живое растение строго ближе √r2.
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)>;
}

/// Чувства хищника.
pub trait PredatorSenses {
    /// Ближайшая по центрам живая добыча, которую видно по краю тела:
    /// d < vision + half.
    fn nearest_prey(&self, x: f64, y: f64, vision: f64) -> Option<Prey>;
}

/// Мир глазами травоядного: хищники и растения по сеткам тика.
///
/// Запросы и методы чувств помечены `#[inline(always)]`: без этого компилятор
/// не встраивал поиск растения в ход травоядного, и мир ×100 шёл на 8%
/// медленнее, чем с прежними замыканиями (замер против версии до чувств).
pub(crate) struct GridVegetarianSenses<'a> {
    pub hunters: &'a Grid,
    pub food: &'a Grid,
    pub plants: &'a [Plant],
}

impl VegetarianSenses for GridVegetarianSenses<'_> {
    #[inline(always)]
    fn nearest_predator(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64, f64)> {
        nearest_predator(self.hunters, x, y, r2)
    }

    #[inline(always)]
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        nearest_plant(self.food, self.plants, x, y, r2)
    }
}

/// Мир глазами хищника. `max_half` — половина самого крупного травоядного.
pub(crate) struct GridPredatorSenses<'a> {
    pub prey: &'a Grid,
    pub vegetarians: &'a [Vegetarian],
    pub max_half: f64,
}

impl PredatorSenses for GridPredatorSenses<'_> {
    #[inline(always)]
    fn nearest_prey(&self, x: f64, y: f64, vision: f64) -> Option<Prey> {
        nearest_prey(self.prey, self.vegetarians, self.max_half, x, y, vision)
    }
}

/// Чувства из замыканий — для тестов: `vegetarian_senses(|..| .., |..| ..)`.
pub struct FnSenses<A, B>(pub A, pub B);

/// Травоядному: (ближайший хищник, ближайшее растение).
pub fn vegetarian_senses<P, F>(predator: P, plant: F) -> FnSenses<P, F>
where
    P: Fn(f64, f64, f64) -> Option<(f64, f64, f64)>,
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    FnSenses(predator, plant)
}

/// Хищнику: ближайшая добыча.
pub fn predator_senses<P>(prey: P) -> FnSenses<P, ()>
where
    P: Fn(f64, f64, f64) -> Option<Prey>,
{
    FnSenses(prey, ())
}

impl<P, F> VegetarianSenses for FnSenses<P, F>
where
    P: Fn(f64, f64, f64) -> Option<(f64, f64, f64)>,
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    fn nearest_predator(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64, f64)> {
        (self.0)(x, y, r2)
    }

    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        (self.1)(x, y, r2)
    }
}

impl<P> PredatorSenses for FnSenses<P, ()>
where
    P: Fn(f64, f64, f64) -> Option<Prey>,
{
    fn nearest_prey(&self, x: f64, y: f64, vision: f64) -> Option<Prey> {
        (self.0)(x, y, vision)
    }
}

/// Никого не видит: ни хищников, ни растений, ни добычи.
pub struct Blind;

impl VegetarianSenses for Blind {
    fn nearest_predator(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64, f64)> {
        None
    }

    fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        None
    }
}

impl PredatorSenses for Blind {
    fn nearest_prey(&self, _: f64, _: f64, _: f64) -> Option<Prey> {
        None
    }
}

// ── запросы к сеткам ────────────────────────────────────────────────────────
// Вынесены из тика, чтобы тест мог сверить их с честным перебором в живом мире:
// ошибка в радиусе запроса не роняет ничего, а тихо меняет баланс — существа
// перестают замечать соседей под носом.

/// Ближайшая по центрам живая добыча, которую видно по краю тела:
/// d < vision + half. `max_half` — половина самого крупного травоядного.
#[inline(always)]
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
        let reach = vision + v.pheno.half;
        if d2 < reach * reach && best.is_none_or(|(b, _)| d2 < b) {
            best = Some((d2, Prey { x: vx, y: vy, half: v.pheno.half }));
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
        let reach = own + vegetarians[j].pheno.half;
        if dx * dx + dy * dy < reach * reach {
            caught = Some(j);
        }
    });
    caught
}

/// Ближайший хищник строго ближе √r2: (x, y, квадрат расстояния).
#[inline(always)]
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
#[inline(always)]
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
    use crate::config::*;
    use crate::rules::Rules;
    use crate::world::{World, WorldConfig};

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
                let max_half = vegs.iter().fold(0.0_f64, |m, v| m.max(v.pheno.half));
                let alive_vegs = || vegs.iter().filter(|v| v.alive);

                for p in &w.predators {
                    for vision in [p.pheno.vision, 2000.0] {
                        let got = nearest_prey(&prey, &vegs, max_half, p.x, p.y, vision)
                            .map(|q| dist2(p.x, p.y, q.x, q.y));
                        let want = min(alive_vegs()
                            .map(|v| (dist2(p.x, p.y, v.x, v.y), v.pheno.half))
                            .filter(|&(d2, half)| d2 < (vision + half) * (vision + half))
                            .map(|(d2, _)| d2));
                        assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшая добыча");
                    }
                    for own in [PREDATOR_DIAM / 2.0, 300.0] {
                        let touches = |v: &Vegetarian| {
                            dist2(p.x, p.y, v.x, v.y) < (own + v.pheno.half) * (own + v.pheno.half)
                        };
                        let got = prey_in_contact(&prey, &vegs, max_half, p.x, p.y, own);
                        assert_eq!(got.is_some(), alive_vegs().any(touches), "сид {seed}: поимка");
                        if let Some(j) = got {
                            assert!(vegs[j].alive && touches(&vegs[j]), "сид {seed}: пойман не тот");
                        }
                    }
                    checked += 1;
                }
                for v in &w.vegetarians {
                    let got = nearest_predator(&hunters, v.x, v.y, v.pheno.vision2).map(|(_, _, d2)| d2);
                    let want = min(w
                        .predators
                        .iter()
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.pheno.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайший хищник");

                    let got = nearest_plant(&food, &plants, v.x, v.y, v.pheno.vision2)
                        .map(|(px, py)| dist2(px, py, v.x, v.y));
                    let want = min(plants
                        .iter()
                        .filter(|p| p.alive)
                        .map(|p| dist2(p.x, p.y, v.x, v.y))
                        .filter(|&d2| d2 < v.pheno.vision2));
                    assert_eq!(got, want, "сид {seed}, тик {tick}: ближайшее растение");

                    let mut eaten_by_grid = plants.clone();
                    let n = eat_plants(&food, &mut eaten_by_grid, v.x, v.y, v.pheno.size);
                    let want: Vec<bool> = plants
                        .iter()
                        .map(|p| p.alive && dist2(p.x, p.y, v.x, v.y) <= v.pheno.size2)
                        .collect();
                    let got: Vec<bool> =
                        plants.iter().zip(&eaten_by_grid).map(|(a, b)| a.alive && !b.alive).collect();
                    assert_eq!(got, want, "сид {seed}, тик {tick}: съедено не то");
                    assert_eq!(n, want.iter().filter(|&&e| e).count());
                    checked += 1;
                }
            }
            assert!(checked > 1000, "сид {seed}: проверено всего {checked} запросов — мир вымер?");
            if seed == 4 {
                let biggest = w.vegetarians.iter().fold(0.0_f64, |m, v| m.max(v.pheno.size));
                assert!(
                    biggest > 100.0,
                    "гиганты не выросли ({biggest:.0}) — вторая часть теста бессмысленна"
                );
            }
        }
    }
}
