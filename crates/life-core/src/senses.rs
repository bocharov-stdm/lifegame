//! Чувства: что существо может узнать о мире.
//!
//! Существа не видят сетку соседей: они спрашивают «где ближайшее растение»,
//! «кто рядом может меня съесть» — через трейт. Мир отвечает по сеткам
//! (`GridSenses` ниже, строится на каждое существо), тесты — обычными
//! замыканиями (`senses_from`) или слепотой (`Blind`).
//!
//! Новое чувство — метод трейта, функция-запрос внизу и её сверка с перебором
//! в тесте этого модуля.
//!
//! Свой вид существа видят только по снимку на начало фазы (`Herd`). В своей
//! фазе они двигаются: копии координат в сетке устарели бы, а чтение живых
//! позиций сделало бы исход зависимым от порядка обхода. По снимку все видят
//! соседей там, где те стояли в начале тика (отставание — не больше шага), и
//! параллельный тик сможет решать за всех одновременно (CLAUDE.md,
//! «Neighbour search»).

use crate::config::GRID_CELL;
use crate::creature::{Creature, Kinship, Me};
use crate::grid::Grid;
use crate::plant::Plant;
use crate::space::Space;

/// Чувства существа.
pub trait Senses {
    fn visible_enemy(&self, _me: &Me, _id: u64) -> Option<Threat> {
        None
    }
    /// Ближайшее живое растение строго ближе √r2.
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)>;

    /// Ближайший чужак (не родня), который может меня съесть и до края тела
    /// которого меньше `within`, — по снимку стада на начало фазы.
    fn nearest_threat(&self, me: &Me, within: f64) -> Option<Threat>;
    /// Личная видимая добыча; прежняя цель имеет приоритет, пока допустима.
    fn prey(&self, _me: &Me, _previous: Option<u64>) -> Option<Prey> {
        None
    }
}

/// Оценка добычи только по лично видимому существу.
#[derive(Clone, Copy, Debug)]
pub struct Prey {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub score: f64,
}

impl Prey {
    fn of(s: &Seen, me: &Me) -> Self {
        let travel =
            ((s.x - me.x).hypot(s.y - me.y) - s.half - me.pheno.half).max(0.0) / me.pheno.speed.max(0.01);
        let hits = (s.health / (me.pheno.size * 0.05).min(s.max_health * 0.25).max(0.001)).ceil();
        Self {
            id: s.kinship.id,
            x: s.x,
            y: s.y,
            score: s.nutrition * me.pheno.meat_efficiency / (travel + hits).max(1.0),
        }
    }
}

/// Опасный чужак, каким его видит существо.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub id: u64,
    /// Центр его тела.
    pub x: f64,
    pub y: f64,
    /// Расстояние до края его тела; внутри тела — меньше нуля.
    pub gap: f64,
}

/// Мир глазами существа: растения по сетке тика, сородичи по снимку стада.
///
/// Запросы и методы чувств помечены `#[inline(always)]`: без этого компилятор
/// не встраивал поиск растения в ход существа, и мир ×100 шёл на 8%
/// медленнее, чем с прежними замыканиями (замер против версии до чувств).
pub(crate) struct GridSenses<'a> {
    pub food: &'a Grid,
    pub plants: &'a [Plant],
    /// Снимок стада. None — съесть друг друга нельзя (каннибализм выключен),
    /// и смотреть на сородичей незачем.
    pub herd: Option<&'a Herd>,
}

impl Senses for GridSenses<'_> {
    fn visible_enemy(&self, me: &Me, id: u64) -> Option<Threat> {
        let herd = self.herd?;
        let i = herd.seen.binary_search_by_key(&id, |s| s.kinship.id).ok()?;
        let s = &herd.seen[i];
        let distance = (s.x - me.x).hypot(s.y - me.y);
        if me.kinship.kin(s.kinship) || me.flock == s.flock || distance > me.pheno.vision {
            return None;
        }
        Some(Threat { id, x: s.x, y: s.y, gap: distance - s.half })
    }
    fn prey(&self, me: &Me, previous: Option<u64>) -> Option<Prey> {
        let herd = self.herd?;
        let max_size = me.pheno.size / me.pheno.prey_ratio.max(herd.ratio);
        let mut best: Option<Prey> = None;
        herd.grid.for_each_near(me.x, me.y, me.pheno.vision, |j, _, _| {
            let s = &herd.seen[j];
            if (me.flock != 0 && me.flock == s.flock)
                || s.half * 2.0 > max_size
                || me.kinship.kin(s.kinship)
                || (s.x - me.x).hypot(s.y - me.y) > me.pheno.vision
            {
                return;
            }
            let p = Prey::of(s, me);
            if best.is_none_or(|b| {
                if b.id == previous.unwrap_or(0) {
                    false
                } else {
                    p.id == previous.unwrap_or(0) || p.score > b.score || (p.score == b.score && p.id < b.id)
                }
            }) {
                best = Some(p);
            }
        });
        best
    }

    #[inline(always)]
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        nearest_plant(self.food, self.plants, x, y, r2)
    }

    #[inline(always)]
    fn nearest_threat(&self, me: &Me, within: f64) -> Option<Threat> {
        nearest_threat(self.herd?, me.kinship, me.flock, me.x, me.y, me.pheno.size, within)
    }
}

/// Чувства для тестов: растение из замыкания `senses_from(|x, y, r2| ..)`,
/// угроза — заданная (`with_threat`; видна, если ближе запрошенного) или никакой.
pub struct FnSenses<F> {
    plant: F,
    threat: Option<Threat>,
}

/// Существу: ближайшее растение; угроз нет.
pub fn senses_from<F>(plant: F) -> FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    FnSenses { plant, threat: None }
}

impl<F> FnSenses<F> {
    /// Те же чувства, но рядом опасный чужак.
    pub fn with_threat(self, threat: Threat) -> Self {
        FnSenses { threat: Some(threat), ..self }
    }
}

impl<F> Senses for FnSenses<F>
where
    F: Fn(f64, f64, f64) -> Option<(f64, f64)>,
{
    fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
        (self.plant)(x, y, r2)
    }

    fn nearest_threat(&self, _: &Me, within: f64) -> Option<Threat> {
        self.threat.filter(|t| t.gap < within)
    }
}

/// Ничего не видит.
pub struct Blind;

impl Senses for Blind {
    fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        None
    }

    fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
        None
    }
}

// ── снимок стада ────────────────────────────────────────────────────────────

/// Существо в снимке стада: что о нём видно другим.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seen {
    x: f64,
    y: f64,
    /// Радиус тела: до чужака меряют расстояние до края тела, а не до центра.
    half: f64,
    /// Самое крупное тело, какое он может съесть.
    eats_up_to: f64,
    health: f64,
    max_health: f64,
    nutrition: f64,
    kinship: Kinship,
    flock: u64,
}

/// Снимок всех существ на начало фазы: мелкие нужны как добыча, крупные — как угрозы.
/// Запросы используют сетку; буферы живут между тиками.
#[derive(Clone, Debug)]
pub(crate) struct Herd {
    grid: Grid,
    seen: Vec<Seen>,
    /// Самое крупное тело, какое может съесть хоть кто-то. Кто крупнее, тому
    /// бояться некого, и в сетку он не смотрит.
    max_eats: f64,
    ratio: f64,
    /// Самый большой радиус тела в снимке: на него шире запрос.
    max_half: f64,
}

impl Herd {
    pub fn new() -> Self {
        Herd { grid: Grid::new(GRID_CELL), seen: Vec::new(), max_eats: 0.0, max_half: 0.0, ratio: 2.5 }
    }

    /// Снимок существ, как они стоят сейчас. В начале фазы все живы: умерших
    /// выметают в конце прошлой. Смотрят в снимок те же, кто в нём: дети
    /// рождаются после ходов. `ratio` — во сколько раз жертва мельче едока
    /// (правило каннибализма).
    pub fn rebuild(&mut self, space: &Space, creatures: &[Creature], ratio: f64) {
        debug_assert!(creatures.iter().all(|v| v.alive), "в снимке стада мёртвые");
        self.ratio = ratio;
        self.seen.clear();
        self.seen.extend(creatures.iter().map(|v| Seen {
            x: v.x,
            y: v.y,
            half: v.pheno.half,
            eats_up_to: v.pheno.size / ratio.max(v.pheno.prey_ratio),
            health: v.health,
            max_health: v.max_health(),
            nutrition: v.energy + (v.pheno.size - v.birth_size).max(0.0) * crate::config::ENERGY_PER_SIZE,
            kinship: v.kinship(),
            flock: v.flock,
        }));
        self.grid.rebuild(space, self.seen.iter().map(|s| (s.x, s.y)));
        let (mut max_eats, mut max_half) = (0.0_f64, 0.0_f64);
        for s in &self.seen {
            max_eats = max_eats.max(s.eats_up_to);
            max_half = max_half.max(s.half);
        }
        (self.max_eats, self.max_half) = (max_eats, max_half);
    }
}

// ── запросы к сеткам ────────────────────────────────────────────────────────
// Вынесены из тика, чтобы тест мог сверить их с честным перебором в живом мире:
// ошибка в радиусе запроса не роняет ничего, а тихо меняет баланс — существа
// перестают замечать соседей под носом.

/// Ближайший по краю тела чужак (не родня `who`) из снимка, который может
/// съесть тело размера `size` и до края тела которого меньше `within`.
#[inline(always)]
pub(crate) fn nearest_threat(
    herd: &Herd,
    who: Kinship,
    flock: u64,
    x: f64,
    y: f64,
    size: f64,
    within: f64,
) -> Option<Threat> {
    if size > herd.max_eats {
        return None; // такое тело не может съесть никто в мире
    }
    let mut best: Option<Threat> = None;
    herd.grid.for_each_near(x, y, within + herd.max_half, |j, sx, sy| {
        let s = &herd.seen[j];
        if size > s.eats_up_to || who.kin(s.kinship) || (flock != 0 && flock == s.flock) {
            return;
        }
        let (dx, dy) = (sx - x, sy - y);
        let d2 = dx * dx + dy * dy;
        let reach = within + s.half;
        if d2 >= reach * reach {
            return;
        }
        let gap = d2.sqrt() - s.half;
        if best.is_none_or(|b| gap < b.gap) {
            best = Some(Threat { id: s.kinship.id, x: sx, y: sy, gap });
        }
    });
    best
}

/// Каннибализм: первый живой чужой (не родня едоку `who`) сородич не крупнее
/// `max_size`, чьё тело касается круга радиуса `reach` вокруг (x, y).
#[cfg(test)]
pub(crate) fn smaller_prey_in_contact(
    grid: &Grid,
    creatures: &[Creature],
    max_half: f64,
    who: Kinship,
    (x, y): (f64, f64),
    reach: f64,
    max_size: f64,
) -> Option<usize> {
    let mut caught = None;
    grid.for_each_near(x, y, reach + max_half, |j, vx, vy| {
        let v = &creatures[j];
        if caught.is_some() || v.pheno.size > max_size || !v.alive || who.kin(v.kinship()) {
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
    /// позициях, размерах и родстве живого мира, а не на выдуманных точках.
    /// Ловит неверный радиус запроса, забытую половину тела, пропущенную родню
    /// и сломанную сетку. Второй мир — с дешёвым размером: там вырастают
    /// гиганты, и радиус запроса растёт с ними.
    #[test]
    fn запросы_к_сеткам_совпадают_с_перебором_в_живом_мире() {
        let giants = Rules::default().with("size_power", 1.0).unwrap();
        for (seed, rules) in [(1, Rules::default()), (4, giants)] {
            let mut w = World::new(&WorldConfig { seed, rules, ..Default::default() });
            let (mut prey, mut food) = (Grid::new(GRID_CELL), Grid::new(GRID_CELL));
            let mut snapshot = Herd::new();
            let (mut checked, mut threats, mut spared) = (0, 0, 0);
            for tick in 0..1500 {
                w.step();
                if tick % 50 != 0 {
                    continue;
                }
                // часть существ и растений «съедена в этом тике» — их запросы обязаны пропускать
                let mut herd = w.creatures.clone();
                herd.iter_mut().step_by(7).for_each(|v| v.alive = false);
                let mut plants = w.plants.clone();
                plants.iter_mut().step_by(5).for_each(|p| p.alive = false);
                prey.rebuild(&w.space, herd.iter().map(|v| (v.x, v.y)));
                food.rebuild(&w.space, plants.iter().map(|p| (p.x, p.y)));
                let max_half = herd.iter().fold(0.0_f64, |m, v| m.max(v.pheno.half));

                for v in &w.creatures {
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
                for v in &herd {
                    for ratio in [CANNIBAL_RATIO, 1.1] {
                        let max_size = v.pheno.size / ratio;
                        let fits = |u: &Creature| {
                            !v.kinship().kin(u.kinship())
                                && u.alive
                                && u.pheno.size <= max_size
                                && dist2(v.x, v.y, u.x, u.y) < (v.pheno.size + u.pheno.half).powi(2)
                        };
                        let got = smaller_prey_in_contact(
                            &prey,
                            &herd,
                            max_half,
                            v.kinship(),
                            (v.x, v.y),
                            v.pheno.size,
                            max_size,
                        );
                        let any = herd.iter().any(fits);
                        assert_eq!(got.is_some(), any, "сид {seed}, тик {tick}: каннибал");
                        if let Some(j) = got {
                            assert!(fits(&herd[j]), "сид {seed}: съеден не тот сородич");
                        }
                    }
                }

                // угрозы — по снимку живого мира, как в начале фазы
                for ratio in [CANNIBAL_RATIO, 1.1] {
                    snapshot.rebuild(&w.space, &w.creatures, ratio);
                    let lookers = w.creatures.iter().flat_map(|v| [(v, v.pheno.vision), (v, v.pheno.flee)]);
                    for (v, within) in lookers {
                        let size = v.pheno.size;
                        let got = nearest_threat(&snapshot, v.kinship(), v.flock, v.x, v.y, size, within);
                        let can_eat_me = |u: &&Creature| {
                            size <= u.pheno.size / ratio.max(u.pheno.prey_ratio)
                                && dist2(u.x, u.y, v.x, v.y) < (within + u.pheno.half).powi(2)
                        };
                        let gap = |u: &Creature| dist2(u.x, u.y, v.x, v.y).sqrt() - u.pheno.half;
                        let want = min(w
                            .creatures
                            .iter()
                            .filter(can_eat_me)
                            .filter(|u| !v.kinship().kin(u.kinship()) && v.flock != u.flock)
                            .map(gap));
                        assert_eq!(got.map(|t| t.gap), want, "сид {seed}, тик {tick}: угроза");
                        threats += got.is_some() as usize;
                        // родня, которая иначе была бы угрозой: без неё проверка родства пуста
                        spared += w
                            .creatures
                            .iter()
                            .filter(can_eat_me)
                            .filter(|u| u.id != v.id && v.kinship().kin(u.kinship()))
                            .count();
                    }
                }
            }
            assert!(checked > 1000, "сид {seed}: проверено всего {checked} запросов — мир вымер?");
            assert!(threats > 100, "сид {seed}: угроз нашлось всего {threats}");
            assert!(spared > 10, "сид {seed}: родни среди угроз всего {spared}");
            if seed == 4 {
                let biggest = w.creatures.iter().fold(0.0_f64, |m, v| m.max(v.pheno.size));
                assert!(
                    biggest > 100.0,
                    "гиганты не выросли ({biggest:.0}) — вторая часть теста бессмысленна"
                );
            }
        }
    }
}
