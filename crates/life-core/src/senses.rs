//! Чувства: что существо может узнать о мире.
//!
//! Существа не видят сетку соседей: они спрашивают «где ближайшее растение» —
//! через трейт. Мир отвечает по сеткам (`GridVegetarianSenses` ниже, строится
//! на каждое существо), тесты — обычными замыканиями (`vegetarian_senses`) или
//! слепотой (`Blind`).
//!
//! Новое чувство — метод трейта, функция-запрос внизу и её сверка с перебором
//! в тесте этого модуля. Чувств, смотрящих на свой же вид, не делать до
//! параллельного тика: травоядные двигаются в своей фазе, и копии координат в
//! сетке устарели бы (см. CLAUDE.md, «Neighbour search»).

use crate::grid::Grid;
use crate::plant::Plant;
use crate::vegetarian::Vegetarian;

/// Чувства травоядного.
pub trait VegetarianSenses {
    /// Ближайшее живое растение строго ближе √r2.
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)>;
}

/// Мир глазами травоядного: растения по сетке тика.
///
/// Запросы и методы чувств помечены `#[inline(always)]`: без этого компилятор
/// не встраивал поиск растения в ход травоядного, и мир ×100 шёл на 8%
/// медленнее, чем с прежними замыканиями (замер против версии до чувств).
pub(crate) struct GridVegetarianSenses<'a> {
    pub food: &'a Grid,
    pub plants: &'a [Plant],
}

impl VegetarianSenses for GridVegetarianSenses<'_> {
    #[inline(always)]
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        nearest_plant(self.food, self.plants, x, y, r2)
    }
}

/// Чувства из замыкания — для тестов: `vegetarian_senses(|x, y, r2| ..)`.
pub struct FnSenses<F>(pub F);

/// Травоядному: ближайшее растение.
pub fn vegetarian_senses<F>(plant: F) -> FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    FnSenses(plant)
}

impl<F> VegetarianSenses for FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        (self.0)(x, y, r2)
    }
}

/// Ничего не видит.
pub struct Blind;

impl VegetarianSenses for Blind {
    fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        None
    }
}

// ── запросы к сеткам ────────────────────────────────────────────────────────
// Вынесены из тика, чтобы тест мог сверить их с честным перебором в живом мире:
// ошибка в радиусе запроса не роняет ничего, а тихо меняет баланс — существа
// перестают замечать соседей под носом.

/// Каннибализм: первый живой сородич, кроме самого едока `me`, не крупнее
/// `max_size`, чьё тело касается круга радиуса `reach` вокруг (x, y).
pub(crate) fn smaller_prey_in_contact(
    grid: &Grid,
    vegetarians: &[Vegetarian],
    max_half: f64,
    me: usize,
    (x, y): (f64, f64),
    reach: f64,
    max_size: f64,
) -> Option<usize> {
    let mut caught = None;
    grid.for_each_near(x, y, reach + max_half, |j, vx, vy| {
        let v = &vegetarians[j];
        if caught.is_some() || j == me || !v.alive || v.pheno.size > max_size {
            return;
        }
        let (dx, dy) = (x - vx, y - vy);
        let r = reach + v.pheno.half;
        if dx * dx + dy * dy < r * r {
            caught = Some(j);
        }
    });
    caught
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
    /// дешёвым размером: там вырастают гиганты, и радиус запроса растёт с ними.
    #[test]
    fn запросы_к_сеткам_совпадают_с_перебором_в_живом_мире() {
        let giants = Rules::default().with("size_power", 1.0).unwrap();
        for (seed, rules) in [(1, Rules::default()), (4, giants)] {
            let mut w = World::new(&WorldConfig { seed, rules, ..Default::default() });
            let (mut prey, mut food) = (Grid::new(GRID_CELL), Grid::new(GRID_CELL));
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
                let max_half = vegs.iter().fold(0.0_f64, |m, v| m.max(v.pheno.half));

                for v in &w.vegetarians {
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
                for (i, v) in vegs.iter().enumerate() {
                    for ratio in [CANNIBAL_RATIO, 1.1] {
                        let max_size = v.pheno.size / ratio;
                        let fits = |j: usize, u: &Vegetarian| {
                            j != i
                                && u.alive
                                && u.pheno.size <= max_size
                                && dist2(v.x, v.y, u.x, u.y) < (v.pheno.size + u.pheno.half).powi(2)
                        };
                        let got = smaller_prey_in_contact(
                            &prey,
                            &vegs,
                            max_half,
                            i,
                            (v.x, v.y),
                            v.pheno.size,
                            max_size,
                        );
                        let any = vegs.iter().enumerate().any(|(j, u)| fits(j, u));
                        assert_eq!(got.is_some(), any, "сид {seed}, тик {tick}: каннибал");
                        if let Some(j) = got {
                            assert!(fits(j, &vegs[j]), "сид {seed}: съеден не тот сородич");
                        }
                    }
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
