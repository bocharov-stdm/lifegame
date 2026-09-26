//! Где растёт еда: профиль плотности по глубине и по ширине.
//!
//! Растение ставится по двум осям независимо: x — по профилю ширины, y — по
//! профилю глубины (плотность — произведение двух профилей). Профиль — функция
//! `f(t)` доли пути от ближнего края (поверхность, левый край) к дальнему: `t`
//! от 0 до 1 на всю длину оси. Профили — правила мира (`Rules::plant_depth`,
//! `plant_width`), их можно менять посреди партии; `Flora` — то, что из них
//! выводится (как фенотип из генома), и пересчитывается вместе с правилами.
//!
//! Распределение еды — свойство мира. Гены слоя под него не подстраиваются и
//! остаются вертикальным предпочтением в процентах глубины.
//!
//! На каждое растение тянется ровно два случайных числа при любом профиле:
//! сначала x, потом y. Экспонента по глубине и равномерность по ширине
//! считаются теми же выражениями, что и до профилей, — мир по умолчанию не
//! сдвинулся ни на бит.
//!
//! Capacity is `PLANT_MAX` (per area) places, *slots*, and a slot holds at most one plant
//! (`World::spawn_plants`): a seed that lands in an occupied slot does not sprout. So a full
//! world follows the profile exactly and a grazed surface cannot hand its room to the deep sea.
//!
//! Without patches (`plant_patches` = 0) the slots are cells of equal fertility — equal steps of
//! each axis's distribution function, narrow where food is rich and wide where it is poor — and a
//! seed is drawn by the profile as above.
//!
//! With patches the food grows in islands. The world is split into coarse *regions* of equal
//! fertility, about two patches each; every region holds the same number of slots, so region by
//! region the food still follows the profile. A region has one to three patches with their own
//! size, shape and weight: a heavier patch takes more of the region's slots, so its seeds fall
//! more often. Patch centres are drawn by the profile inside their region, from a stream keyed by
//! the world's seed (not the world's stream). Slots lie on a sunflower spiral inside the patch; a
//! seed draws three numbers — the slot, uniform over all of them, then a jitter inside the slot.

use std::f64::consts::{PI, TAU};

use crate::config::{
    GAME_PLATEAU, GAME_SLOPE_END, GAME_SLOPE_LEVEL, PATCH_STRETCH, PATCH_WEIGHT_MIN, PLANT_MAX, PLANT_RADIUS,
    PLANT_TOP_MARGIN_PCT, WORLD_HEIGHT, WORLD_WIDTH,
};
use crate::plant::Plant;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::space::Space;

/// Закон плотности вдоль оси.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Uniform,
    /// Прямая от ближнего края к дальнему: `1 − (1 − b)·t`.
    Linear,
    /// `e^(−k·t)`: быстро редеет, дальше длинный хвост.
    Exp,
    /// `ln(1 + k(1−t)) / ln(1 + k)`: долго держится, потом обрыв к дальнему краю.
    Log,
    /// `1 − a·cos(2π·n·t)`: n богатых полос, пики — в серединах полос.
    Waves,
    /// A rough real sea: full food down to `GAME_PLATEAU` of the axis, a straight slope to
    /// `GAME_SLOPE_LEVEL` at `GAME_SLOPE_END`, then a cosine fall to a dead bottom. No parameters.
    Game,
}

impl Profile {
    pub const ALL: [Profile; 6] =
        [Profile::Uniform, Profile::Linear, Profile::Exp, Profile::Log, Profile::Waves, Profile::Game];

    /// Имя для флага: `--rule plant_width_profile=waves`.
    pub fn key(self) -> &'static str {
        match self {
            Profile::Uniform => "uniform",
            Profile::Linear => "linear",
            Profile::Exp => "exp",
            Profile::Log => "log",
            Profile::Waves => "waves",
            Profile::Game => "game",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Profile::Uniform => "равномерно",
            Profile::Linear => "линейно",
            Profile::Exp => "экспонента",
            Profile::Log => "логарифм",
            Profile::Waves => "волны",
            Profile::Game => "игровое",
        }
    }

    /// Номер профиля в правилах — значение правила `plant_*_profile`.
    pub fn index(self) -> f64 {
        Profile::ALL.iter().position(|p| *p == self).expect("профиль есть в ALL") as f64
    }

    /// Профиль по значению правила (`with` пускает только целые номера).
    pub fn of(value: f64) -> Profile {
        Profile::ALL[(value.max(0.0) as usize).min(Profile::ALL.len() - 1)]
    }

    pub fn parse(s: &str) -> Option<Profile> {
        let s = s.trim();
        Profile::ALL.into_iter().find(|p| p.key() == s || p.label() == s)
    }
}

/// Крутизна экспоненты — не больше этого. Дальше вся еда лежит в доле процента
/// оси, а `e^(−k)` на дальнем краю уходит под предел точности чисел.
pub const MAX_STEEPNESS: f64 = 100.0;
/// Волн — не больше этого: таблица распределения делит ось на `TABLE_BINS`
/// частей, и у более частых волн на каждую пришлось бы меньше 40 корзин.
pub const MAX_WAVES: f64 = 100.0;
/// Корзин в таблице распределения. При высоте мира 126 000 (3:2, x1000) это
/// ~30 px на корзину — мельче растения.
const TABLE_BINS: usize = 4096;

/// Профиль еды по одной оси — как он задан в правилах мира. Числа, как у
/// остальных правил: профиль — номер в `Profile::ALL`, остальное — параметры
/// профилей (каждый читает свои).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodAxis {
    pub profile: f64,
    /// Экспонента: крутизна k.
    pub steepness: f64,
    /// Линейный: сколько еды у дальнего края, % от ближнего.
    pub end: f64,
    /// Логарифм: изгиб k.
    pub bend: f64,
    /// Волны: сколько богатых полос.
    pub waves: f64,
    /// Волны: размах, % (100 — между полосами пусто).
    pub amplitude: f64,
}

/// Параметры оси по порядку — хвосты имён правил `plant_depth_*`, `plant_width_*`.
pub const AXIS_PARAMS: [&str; 6] = ["profile", "steepness", "end", "bend", "waves", "amplitude"];

/// Ось профиля.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Along {
    Depth,
    Width,
}

/// Правило профиля еды: `plant_depth_waves` → (глубина, "waves").
pub fn split_key(key: &str) -> Option<(Along, &str)> {
    let (along, param) = if let Some(p) = key.strip_prefix("plant_depth_") {
        (Along::Depth, p)
    } else {
        (Along::Width, key.strip_prefix("plant_width_")?)
    };
    AXIS_PARAMS.contains(&param).then_some((along, param))
}

impl FoodAxis {
    pub fn get(&self, param: &str) -> Option<f64> {
        Some(match param {
            "profile" => self.profile,
            "steepness" => self.steepness,
            "end" => self.end,
            "bend" => self.bend,
            "waves" => self.waves,
            "amplitude" => self.amplitude,
            _ => return None,
        })
    }

    pub fn slot(&mut self, param: &str) -> Option<&mut f64> {
        Some(match param {
            "profile" => &mut self.profile,
            "steepness" => &mut self.steepness,
            "end" => &mut self.end,
            "bend" => &mut self.bend,
            "waves" => &mut self.waves,
            "amplitude" => &mut self.amplitude,
            _ => return None,
        })
    }

    /// Пределы, за которыми параметр теряет смысл; Err — что нужно.
    pub fn check(param: &str, v: f64) -> Result<(), String> {
        let whole = v.fract() == 0.0;
        let (ok, need) = match param {
            "profile" => (
                whole && (0.0..Profile::ALL.len() as f64).contains(&v),
                format!(
                    "номер или имя профиля: {}",
                    Profile::ALL.map(|p| format!("{} ({})", p.key(), p.index())).join(", ")
                ),
            ),
            "steepness" => ((0.0..=MAX_STEEPNESS).contains(&v), format!("число от 0 до {MAX_STEEPNESS}")),
            "end" | "amplitude" => ((0.0..=100.0).contains(&v), "процент от 0 до 100".into()),
            "bend" => (v >= 0.0, "число не меньше 0".into()),
            "waves" => (whole && (1.0..=MAX_WAVES).contains(&v), format!("целое число от 1 до {MAX_WAVES}")),
            _ => (false, "известный параметр".into()),
        };
        if ok { Ok(()) } else { Err(need) }
    }

    pub fn kind(&self) -> Profile {
        Profile::of(self.profile)
    }

    /// Плотность в точке `t` (доля оси от ближнего края), от 0 до 1 — для
    /// предпросмотра. Выборка идёт по тем же формулам.
    pub fn density(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self.kind() {
            Profile::Uniform => 1.0,
            Profile::Linear => 1.0 - (1.0 - self.end / 100.0) * t,
            Profile::Exp => (-self.steepness * t).exp(),
            Profile::Log => {
                // при k → 0 логарифм переходит в прямую до нуля
                if self.bend < 1e-9 { 1.0 - t } else { (self.bend * (1.0 - t)).ln_1p() / self.bend.ln_1p() }
            }
            Profile::Waves => {
                let a = self.amplitude / 100.0;
                (1.0 - a * (TAU * self.waves * t).cos()) / (1.0 + a)
            }
            Profile::Game => {
                if t <= GAME_PLATEAU {
                    1.0
                } else if t <= GAME_SLOPE_END {
                    1.0 - (1.0 - GAME_SLOPE_LEVEL) * (t - GAME_PLATEAU) / (GAME_SLOPE_END - GAME_PLATEAU)
                } else {
                    GAME_SLOPE_LEVEL
                        * 0.5
                        * (1.0 + (PI * (t - GAME_SLOPE_END) / (1.0 - GAME_SLOPE_END)).cos())
                }
            }
        }
    }

    /// Профиль словами: «экспонента, крутизна 8».
    pub fn describe(&self) -> String {
        let p = self.kind();
        match p {
            Profile::Uniform | Profile::Game => p.label().into(),
            Profile::Linear => format!("{}, у дальнего края {:.0}%", p.label(), self.end),
            Profile::Exp => format!("{}, крутизна {}", p.label(), self.steepness),
            Profile::Log => format!("{}, изгиб {}", p.label(), self.bend),
            Profile::Waves => format!("{}, {} × {:.0}%", p.label(), self.waves, self.amplitude),
        }
    }
}

/// Как ставить координату вдоль оси.
#[derive(Clone, Debug, PartialEq)]
enum Law {
    Uniform,
    /// Обратная функция распределения экспоненты — та же формула, что до
    /// профилей, поэтому мир по умолчанию совпадает бит в бит.
    Exp {
        lambda: f64,
        e_lo: f64,
        e_hi: f64,
    },
    /// Табулированная функция распределения: `cdf[i]` — доля растений до
    /// i-й границы корзин (от 0 до 1). Внутри корзины плотность постоянная.
    Table(Box<[f64]>),
}

#[derive(Clone, Debug, PartialEq)]
struct Axis {
    lo: f64,
    hi: f64,
    law: Law,
}

impl Axis {
    /// Ось длины `len`; растения ставятся в `[lo, hi]`, профиль — по доле всей длины.
    fn new(spec: &FoodAxis, len: f64, lo: f64, hi: f64) -> Axis {
        let law = match spec.kind() {
            Profile::Uniform => Law::Uniform,
            // крутизна почти ноль — равномерно (иначе 0/0 в формуле)
            Profile::Exp if spec.steepness < 1e-3 => Law::Uniform,
            Profile::Exp => {
                let lambda = spec.steepness / len;
                Law::Exp { lambda, e_lo: (-lambda * lo).exp(), e_hi: (-lambda * hi).exp() }
            }
            _ => table(spec, len, lo, hi).map_or(Law::Uniform, Law::Table),
        };
        Axis { lo, hi, law }
    }

    /// Одно случайное число — одна координата.
    #[inline]
    fn sample(&self, rng: &mut Rng) -> f64 {
        self.at(rng.random())
    }

    /// The coordinate with share `u` of the plants before it (0 to 1): the inverse of `cdf`.
    /// `Rng::uniform` is `lo + (hi − lo)·u`, so a uniform axis draws the same bits as before.
    #[inline]
    fn at(&self, u: f64) -> f64 {
        match &self.law {
            Law::Uniform => self.lo + (self.hi - self.lo) * u,
            Law::Exp { lambda, e_lo, e_hi } => -(e_lo - u * (e_lo - e_hi)).ln() / lambda,
            Law::Table(cdf) => {
                // первая граница, до которой больше u; корзина — перед ней.
                // Пустые корзины (плотность ноль) не выпадают: у них ширина по
                // cdf нулевая, и граница после них не больше u.
                let i = cdf.partition_point(|c| *c <= u).clamp(1, cdf.len() - 1) - 1;
                let within = (u - cdf[i]) / (cdf[i + 1] - cdf[i]);
                let s = (i as f64 + within.clamp(0.0, 1.0)) / (cdf.len() - 1) as f64;
                (self.lo + (self.hi - self.lo) * s).clamp(self.lo, self.hi)
            }
        }
    }

    /// Share of plants before `at` — the inverse of `sample`, from 0 to 1.
    #[inline]
    fn cdf(&self, at: f64) -> f64 {
        let at = at.clamp(self.lo, self.hi);
        let share = match &self.law {
            Law::Uniform => (at - self.lo) / (self.hi - self.lo),
            Law::Exp { lambda, e_lo, e_hi } => (e_lo - (-lambda * at).exp()) / (e_lo - e_hi),
            Law::Table(cdf) => {
                let pos = (at - self.lo) / (self.hi - self.lo) * (cdf.len() - 1) as f64;
                let i = (pos as usize).min(cdf.len() - 2);
                cdf[i] + (cdf[i + 1] - cdf[i]) * (pos - i as f64)
            }
        };
        share.clamp(0.0, 1.0)
    }

    /// Which of `n` equal shares of the axis `at` falls into.
    #[inline]
    fn slot(&self, at: f64, n: usize) -> usize {
        ((self.cdf(at) * n as f64) as usize).min(n - 1)
    }
}

/// Функция распределения по корзинам (плотность — в серединах корзин). None,
/// если плотность нигде не больше нуля или не число.
fn table(spec: &FoodAxis, len: f64, lo: f64, hi: f64) -> Option<Box<[f64]>> {
    let mut cdf = Vec::with_capacity(TABLE_BINS + 1);
    let mut total = 0.0;
    cdf.push(0.0);
    for i in 0..TABLE_BINS {
        let at = lo + (hi - lo) * (i as f64 + 0.5) / TABLE_BINS as f64;
        total += spec.density(at / len).max(0.0);
        cdf.push(total);
    }
    if !(total > 0.0 && total.is_finite()) {
        return None;
    }
    for c in &mut cdf {
        *c /= total;
    }
    Some(cdf.into_boxed_slice())
}

/// Где растёт еда в этом мире: оси с готовыми законами и места для растений.
#[derive(Clone, Debug, PartialEq)]
pub struct Flora {
    x: Axis,
    y: Axis,
    /// Cells of equal fertility across and down — the slots when there are no patches: `nx * ny`
    /// is at least the world's plant cap.
    nx: usize,
    ny: usize,
    /// Patches in slot order; empty — plants are scattered into cells.
    patches: Vec<Patch>,
    slots: usize,
}

/// A patch: an ellipse squashed against the edges of the plant zone (its reach from the centre
/// differs by side), holding `slots` consecutive slots from `first`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Patch {
    pub x: f64,
    pub y: f64,
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
    pub slots: usize,
    first: usize,
    /// The spiral's turn, so patches do not all start their spiral eastwards.
    turn: f64,
    /// How far a plant strays from its slot, across and down: about one slot's width.
    jitter: f64,
}

/// The patch layout's stream: keyed by the world's seed, apart from the world's own stream.
const PATCH_STREAM: u64 = 0xF10A_0000_0000_0000;
/// The sunflower's turn between slots: slots fill the disc evenly at any count.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

impl Flora {
    pub fn new(rules: &Rules, space: &Space, seed: u64) -> Flora {
        // Мёртвая зона у поверхности — свойство поверхности, а не профиля.
        // Считается как 5 * 4000 / 100 — ровно 200, как было до профилей.
        let margin = PLANT_TOP_MARGIN_PCT * space.height / 100.0;
        let r = PLANT_RADIUS;
        // rows by the world's proportions: with uniform food the cells are near squares
        let cap = space.per_area(PLANT_MAX);
        let ny = rows(cap, space);
        let x = Axis::new(&rules.plant_width, space.width, r, space.width - r);
        let y = Axis::new(&rules.plant_depth, space.height, r + margin, space.height - r);
        let patches = layout(&x, &y, rules, space, seed, cap);
        let nx = cap.div_ceil(ny);
        let slots = if patches.is_empty() { nx * ny } else { cap };
        Flora { x, y, nx, ny, patches, slots }
    }

    /// How many places for plants the world has (one plant each).
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// The patches; empty when plants are scattered.
    pub fn patches(&self) -> &[Patch] {
        &self.patches
    }

    /// The cell a plant at (x, y) occupies when there are no patches.
    #[inline]
    fn cell(&self, x: f64, y: f64) -> usize {
        self.x.slot(x, self.nx) + self.nx * self.y.slot(y, self.ny)
    }

    /// Новое растение и его место. Без зарослей — два случайных числа, x и потом y; в зарослях —
    /// три: место, затем разброс внутри места поперёк и вглубь.
    #[inline]
    pub fn plant(&self, rng: &mut Rng) -> Plant {
        if self.patches.is_empty() {
            let x = self.x.sample(rng);
            let y = self.y.sample(rng);
            return Plant::in_slot(x, y, self.cell(x, y));
        }
        let slot = ((rng.random() * self.slots as f64) as usize).min(self.slots - 1);
        let (jx, jy) = (rng.random(), rng.random());
        let (x, y) = self.slot_at(slot, jx, jy);
        Plant::in_slot(x, y, slot)
    }

    /// Where a plant in `slot` grows; `jx`, `jy` from 0 to 1 place it inside the slot.
    #[inline]
    fn slot_at(&self, slot: usize, jx: f64, jy: f64) -> (f64, f64) {
        let p = &self.patches[self.patches.partition_point(|p| p.first <= slot) - 1];
        let k = slot - p.first;
        let rho = ((k as f64 + 0.5) / p.slots as f64).sqrt();
        let (sin, cos) = (p.turn + k as f64 * GOLDEN_ANGLE).sin_cos();
        let dx = rho * cos * if cos < 0.0 { p.left } else { p.right };
        let dy = rho * sin * if sin < 0.0 { p.up } else { p.down };
        let x = p.x + dx + p.jitter * (jx - 0.5);
        let y = p.y + dy + p.jitter * (jy - 0.5);
        (x.clamp(self.x.lo, self.x.hi), y.clamp(self.y.lo, self.y.hi))
    }
}

/// Rows of a grid of `n` cells by the world's proportions.
fn rows(n: usize, space: &Space) -> usize {
    ((n as f64 * space.height / space.width).sqrt().round() as usize).clamp(1, n)
}

/// Patches over regions of equal fertility, in slot order; empty when the rules ask for none.
fn layout(x: &Axis, y: &Axis, rules: &Rules, space: &Space, seed: u64, cap: usize) -> Vec<Patch> {
    let wanted = (rules.plant_patches * space.area_ratio()).round() as usize;
    if wanted == 0 {
        return Vec::new();
    }
    // about two patches a region, and every region holds at least one slot
    let ry = rows(wanted.div_ceil(2).min(cap), space);
    let rx = wanted.div_ceil(2).min(cap).div_ceil(ry);
    let regions = (rx * ry).min(cap);
    let mut rng = Rng::keyed(seed, PATCH_STREAM);
    let mut patches = Vec::with_capacity(2 * regions);
    let mut first = 0;
    for r in 0..regions {
        let (i, j) = ((r % rx) as f64, (r / rx) as f64);
        // every region its own equal share of the slots
        let own = (r + 1) * cap / regions - r * cap / regions;
        let n = (1 + (rng.random() * 3.0) as usize).min(3).min(own);
        let mut drawn = [(0.0, Patch::default()); 3];
        for (weight, p) in drawn.iter_mut().take(n) {
            // the centre by the profile inside the region
            p.x = x.at((i + rng.random()) / rx as f64);
            p.y = y.at((j + rng.random()) / ry as f64);
            let size = rules.plant_patch_size * rng.uniform(0.5, 1.5);
            let stretch = PATCH_STRETCH.powf(rng.uniform(-1.0, 1.0)).sqrt();
            let (wide, tall) = (size * stretch, size / stretch);
            (p.left, p.right) = (wide.min(p.x - x.lo), wide.min(x.hi - p.x));
            (p.up, p.down) = (tall.min(p.y - y.lo), tall.min(y.hi - p.y));
            p.turn = rng.uniform(0.0, TAU);
            *weight = rng.uniform(PATCH_WEIGHT_MIN, 1.0);
            p.jitter = (PI * wide * tall).sqrt();
        }
        // the region's slots by weight, at least one each
        let total: f64 = drawn[..n].iter().map(|(w, _)| w).sum();
        let (mut before, mut sum) = (0, 0.0);
        for (q, (weight, p)) in drawn[..n].iter_mut().enumerate() {
            sum += *weight;
            let upto =
                if q + 1 == n { own } else { q + 1 + ((own - n) as f64 * sum / total).round() as usize };
            p.first = first + before;
            p.slots = upto - before;
            p.jitter /= (p.slots as f64).sqrt();
            before = upto;
            patches.push(*p);
        }
        first += own;
    }
    debug_assert_eq!(first, cap);
    patches
}

/// Плотность еды в точке (доли ширины и глубины) по правилам, от 0 до 1 — для
/// предпросмотра: окну не нужно строить таблицы. Мёртвая зона у поверхности
/// тоже видна.
pub fn density(rules: &Rules, tx: f64, ty: f64) -> f64 {
    if ty < PLANT_TOP_MARGIN_PCT / 100.0 {
        return 0.0;
    }
    rules.plant_width.density(tx) * rules.plant_depth.density(ty)
}

/// «по глубине — игровое; по ширине — равномерно; заросли — 24 на участок 6000×4000, радиус ~200».
pub fn describe(rules: &Rules) -> String {
    let patches = if rules.plant_patches == 0.0 {
        "россыпью".to_string()
    } else {
        format!(
            "заросли — {} на участок {WORLD_WIDTH}×{WORLD_HEIGHT}, радиус ~{}",
            rules.plant_patches, rules.plant_patch_size
        )
    };
    format!(
        "по глубине — {}; по ширине — {}; {patches}",
        rules.plant_depth.describe(),
        rules.plant_width.describe()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PLANT_DEPTH_DECAY;
    use crate::space::Shape;

    fn rules(pairs: &[(&str, f64)]) -> Rules {
        pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
    }

    /// The same rules with plants scattered by the profile, no patches.
    fn scattered(r: &Rules) -> Rules {
        r.with("plant_patches", 0.0).unwrap()
    }

    /// Все профили на крайних параметрах, по обеим осям.
    fn every_profile() -> Vec<Rules> {
        let mut out = Vec::new();
        for p in Profile::ALL {
            for axis in ["depth", "width"] {
                let key = |param: &str| format!("plant_{axis}_{param}");
                for (param, values) in [
                    ("steepness", &[0.0, 1e-4, 0.5, 8.0, MAX_STEEPNESS][..]),
                    ("end", &[0.0, 10.0, 100.0][..]),
                    ("bend", &[0.0, 1e-12, 20.0, 1e9][..]),
                    ("waves", &[1.0, 3.0, MAX_WAVES][..]),
                    ("amplitude", &[0.0, 80.0, 100.0][..]),
                ] {
                    for &v in values {
                        out.push(rules(&[(&key("profile"), p.index()), (&key(param), v)]));
                    }
                }
            }
        }
        out
    }

    /// The exponent without patches — the default before the «игровое» profile — plants exactly
    /// as before profiles: the same formula, the same random numbers, the same bits.
    #[test]
    fn экспонента_россыпью_как_до_профилей_бит_в_бит() {
        let space = Space::default();
        let exp = scattered(&rules(&[("plant_depth_profile", Profile::Exp.index())]));
        let flora = Flora::new(&exp, &space, 1);
        let (mut a, mut b) = (Rng::new(11), Rng::new(11));
        for _ in 0..10_000 {
            // прежний Plant::random
            let lambda = PLANT_DEPTH_DECAY / space.height;
            let e_top = (-lambda * (PLANT_RADIUS + 200.0)).exp();
            let e_bottom = (-lambda * (space.height - PLANT_RADIUS)).exp();
            let x = a.uniform(PLANT_RADIUS, space.width - PLANT_RADIUS);
            let u = a.random();
            let y = -(e_top - u * (e_top - e_bottom)).ln() / lambda;

            let p = flora.plant(&mut b);
            assert_eq!((p.x.to_bits(), p.y.to_bits()), (x.to_bits(), y.to_bits()));
        }
    }

    /// Любой профиль тянет ровно два числа без зарослей и три в зарослях: смена
    /// профиля на ходу не сдвигает поток мира сильнее, чем меняет сами растения.
    #[test]
    fn два_числа_на_растение_россыпью_и_три_в_зарослях() {
        let space = Space::new(10.0, Shape::Square);
        for r in every_profile() {
            for (r, draws) in [(scattered(&r), 2), (r, 3)] {
                let flora = Flora::new(&r, &space, 1);
                let (mut a, mut b) = (Rng::new(5), Rng::new(5));
                for _ in 0..100 {
                    flora.plant(&mut a);
                    for _ in 0..draws {
                        b.random();
                    }
                }
                assert_eq!(a.next_u64(), b.next_u64(), "{r:?}");
            }
        }
    }

    #[test]
    fn растения_в_границах_при_любых_профилях() {
        for shape in Shape::ALL {
            let space = Space::new(7.0, shape);
            let margin = PLANT_TOP_MARGIN_PCT * space.height / 100.0;
            for r in every_profile().into_iter().flat_map(|r| [scattered(&r), r]) {
                let flora = Flora::new(&r, &space, 3);
                let mut rng = Rng::new(3);
                for _ in 0..2000 {
                    let p = flora.plant(&mut rng);
                    let eps = 1e-9 * space.width.max(space.height);
                    assert!(
                        p.x >= PLANT_RADIUS - eps && p.x <= space.width - PLANT_RADIUS + eps,
                        "x={} {r:?}",
                        p.x
                    );
                    assert!(
                        p.y >= PLANT_RADIUS + margin - eps && p.y <= space.height - PLANT_RADIUS + eps,
                        "y={} {r:?}",
                        p.y
                    );
                }
            }
        }
    }

    /// Доля выборки по полосам совпадает с интегралом плотности.
    fn check_shape(axis: &str, r: &Rules) {
        let space = Space::new(1.0, Shape::R3x2);
        let flora = Flora::new(&scattered(r), &space, 1);
        let spec = if axis == "depth" { r.plant_depth } else { r.plant_width };
        let (len, lo) = if axis == "depth" {
            (space.height, PLANT_RADIUS + PLANT_TOP_MARGIN_PCT * space.height / 100.0)
        } else {
            (space.width, PLANT_RADIUS)
        };
        let hi = len - PLANT_RADIUS;
        const BANDS: usize = 20;
        const N: usize = 200_000;
        let mut got = [0usize; BANDS];
        let mut rng = Rng::new(17);
        for _ in 0..N {
            let p = flora.plant(&mut rng);
            let v = if axis == "depth" { p.y } else { p.x };
            got[(((v - lo) / (hi - lo) * BANDS as f64) as usize).min(BANDS - 1)] += 1;
        }
        // интеграл плотности по полосе — мелкой суммой
        let fine = 200;
        let mass: Vec<f64> = (0..BANDS)
            .map(|b| {
                (0..fine)
                    .map(|i| {
                        let at = lo + (hi - lo) * (b as f64 + (i as f64 + 0.5) / fine as f64) / BANDS as f64;
                        spec.density(at / len)
                    })
                    .sum::<f64>()
            })
            .collect();
        let total: f64 = mass.iter().sum();
        for b in 0..BANDS {
            let expected = mass[b] / total;
            let share = got[b] as f64 / N as f64;
            assert!(
                (share - expected).abs() < 0.006,
                "{axis} {}: полоса {b} — {share:.4}, ожидалось {expected:.4}",
                spec.describe()
            );
        }
    }

    #[test]
    fn выборка_повторяет_плотность() {
        for axis in ["depth", "width"] {
            let key = |param: &str| format!("plant_{axis}_{param}");
            for pairs in [
                vec![(key("profile"), Profile::Uniform.index())],
                vec![(key("profile"), Profile::Linear.index()), (key("end"), 10.0)],
                vec![(key("profile"), Profile::Exp.index()), (key("steepness"), 3.0)],
                vec![(key("profile"), Profile::Log.index()), (key("bend"), 20.0)],
                vec![
                    (key("profile"), Profile::Waves.index()),
                    (key("waves"), 3.0),
                    (key("amplitude"), 100.0),
                ],
                vec![(key("profile"), Profile::Game.index())],
            ] {
                let pairs: Vec<(&str, f64)> = pairs.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                check_shape(axis, &rules(&pairs));
            }
        }
    }

    /// Предпросмотр: на мёртвой зоне ноль, у богатого края — единица.
    #[test]
    fn плотность_для_предпросмотра_от_нуля_до_единицы() {
        let r = Rules::default();
        assert_eq!(density(&r, 0.5, 0.01), 0.0);
        assert_eq!(density(&r, 0.5, 0.05), 1.0);
        let exp = rules(&[("plant_depth_profile", Profile::Exp.index())]);
        assert!((density(&exp, 0.5, 0.05) - (-8.0 * 0.05_f64).exp()).abs() < 1e-12);
        for r in every_profile() {
            for i in 0..=50 {
                let d = density(&r, i as f64 / 50.0, 0.05 + 0.95 * i as f64 / 50.0);
                assert!((0.0..=1.0 + 1e-12).contains(&d), "{d} {r:?}");
            }
        }
    }

    #[test]
    fn профиль_словами() {
        assert_eq!(
            describe(&Rules::default()),
            "по глубине — игровое; по ширине — равномерно; заросли — 24 на участок 6000×4000, радиус ~200"
        );
        assert_eq!(
            describe(&scattered(&rules(&[("plant_depth_profile", Profile::Exp.index())]))),
            "по глубине — экспонента, крутизна 8; по ширине — равномерно; россыпью"
        );
    }

    /// The distribution function undoes sampling: a plant drawn with `u` sits at share `u`.
    #[test]
    fn cdf_inverts_sampling_for_every_profile() {
        let space = Space::new(7.0, Shape::Square);
        for r in every_profile() {
            let flora = Flora::new(&scattered(&r), &space, 1);
            let (mut a, mut b) = (Rng::new(17), Rng::new(17));
            for _ in 0..500 {
                let (ux, uy) = (a.random(), a.random());
                let p = flora.plant(&mut b);
                assert!((flora.x.cdf(p.x) - ux).abs() < 1e-6, "x: {ux} {r:?}");
                assert!((flora.y.cdf(p.y) - uy).abs() < 1e-6, "y: {uy} {r:?}");
            }
        }
    }

    /// Every slot is equally fertile: seeds spread evenly over slots, whatever the profile, in
    /// cells and in patches.
    #[test]
    fn seeds_fall_evenly_into_slots() {
        let space = Space::default();
        for r in [
            Rules::default(),
            rules(&[("plant_depth_profile", Profile::Waves.index()), ("plant_depth_amplitude", 100.0)]),
            rules(&[("plant_width_profile", Profile::Linear.index()), ("plant_width_end", 0.0)]),
        ]
        .into_iter()
        .flat_map(|r| [scattered(&r), r])
        {
            let flora = Flora::new(&r, &space, 1);
            if flora.patches.is_empty() {
                assert!(flora.slots() >= PLANT_MAX && flora.slots() < PLANT_MAX + flora.ny);
            } else {
                assert_eq!(flora.slots(), PLANT_MAX);
            }
            let mut hits = vec![0u32; flora.slots()];
            let mut rng = Rng::new(21);
            for _ in 0..100 * flora.slots() {
                let p = flora.plant(&mut rng);
                hits[p.slot().expect("a seed has a slot")] += 1;
            }
            // Poisson(100): 6 sigma either way
            assert!(
                hits.iter().all(|&n| (40..=160).contains(&n)),
                "{:?}..{:?} {r:?}",
                hits.iter().min(),
                hits.iter().max()
            );
        }
    }

    #[test]
    fn игровой_профиль_сытый_верх_и_мёртвое_дно() {
        let game = Rules::default().plant_depth;
        assert_eq!(game.kind(), Profile::Game);
        for t in [0.0, 0.1, GAME_PLATEAU] {
            assert_eq!(game.density(t), 1.0, "flat to {GAME_PLATEAU}: {t}");
        }
        let middle = (GAME_PLATEAU + GAME_SLOPE_END) / 2.0;
        assert!((game.density(middle) - (1.0 + GAME_SLOPE_LEVEL) / 2.0).abs() < 1e-12, "a straight slope");
        assert!((game.density(GAME_SLOPE_END) - GAME_SLOPE_LEVEL).abs() < 1e-12);
        assert!(game.density(1.0).abs() < 1e-12, "the bottom is dead");
        // continuous and never rising with depth
        let mut last = 1.0;
        for i in 0..=10_000 {
            let d = game.density(i as f64 / 10_000.0);
            assert!(d <= last + 1e-12 && last - d < 2e-4, "{i}: {last} -> {d}");
            last = d;
        }
        assert_eq!(game.describe(), "игровое");
        assert_eq!(Profile::parse("game"), Some(Profile::Game));
    }

    /// Patches differ in size, shape and weight; every region holds the same number of slots;
    /// the layout depends on the seed only.
    #[test]
    fn заросли_разные_области_равные() {
        let space = Space::default();
        let r = Rules::default();
        let flora = Flora::new(&r, &space, 7);
        assert_eq!(flora, Flora::new(&r, &space, 7), "the same seed, the same layout");
        assert_ne!(flora.patches, Flora::new(&r, &space, 8).patches, "another seed, other patches");
        let n = flora.patches.len();
        assert!((16..=36).contains(&n), "about {} patches: {n}", r.plant_patches);
        let slots: Vec<usize> = flora.patches.iter().map(|p| p.slots).collect();
        let (lo, hi) = (*slots.iter().min().unwrap(), *slots.iter().max().unwrap());
        assert!(lo >= 1 && hi > 2 * lo, "patches differ in weight: {slots:?}");
        let widths: Vec<f64> = flora.patches.iter().map(|p| p.left + p.right).collect();
        let (w_lo, w_hi) = widths.iter().fold((f64::MAX, 0.0_f64), |(a, b), w| (a.min(*w), b.max(*w)));
        assert!(w_hi > 2.0 * w_lo, "patches differ in size: {widths:?}");
        // slots run on without gaps, patch after patch
        let mut next = 0;
        for p in &flora.patches {
            assert_eq!(p.first, next);
            next += p.slots;
            // squashed against the plant zone, never past it
            assert!(p.x - p.left >= flora.x.lo - 1e-9 && p.x + p.right <= flora.x.hi + 1e-9);
            assert!(p.y - p.up >= flora.y.lo - 1e-9 && p.y + p.down <= flora.y.hi + 1e-9);
        }
        assert_eq!(next, flora.slots());
    }

    /// Patches do not break the profile: averaged over layouts, bands of depth and of width
    /// get about the profile's share of the plants.
    #[test]
    fn заросли_не_ломают_профиль() {
        let space = Space::default();
        let top = (PLANT_RADIUS + PLANT_TOP_MARGIN_PCT * space.height / 100.0) / space.height;
        const BANDS: usize = 5;
        // the profile's mass over a share of its axis
        let mass = |spec: &FoodAxis, from: f64, to: f64| -> f64 {
            (0..1000).map(|i| spec.density(from + (to - from) * (i as f64 + 0.5) / 1000.0)).sum()
        };
        for r in [
            Rules::default(),
            rules(&[("plant_depth_profile", Profile::Exp.index())]),
            rules(&[("plant_width_profile", Profile::Linear.index()), ("plant_width_end", 0.0)]),
        ] {
            let mut down = [0.0; BANDS];
            let mut across = [0.0; BANDS];
            let (seeds, each) = (30, 10_000);
            for seed in 0..seeds {
                let flora = Flora::new(&r, &space, seed);
                let mut rng = Rng::new(seed);
                for _ in 0..each {
                    let p = flora.plant(&mut rng);
                    let t = (p.y / space.height - top) / (1.0 - top);
                    down[((t * BANDS as f64) as usize).min(BANDS - 1)] += 1.0;
                    across[((p.x / space.width * BANDS as f64) as usize).min(BANDS - 1)] += 1.0;
                }
            }
            let step = (1.0 - top) / BANDS as f64;
            let depth: Vec<f64> = (0..BANDS)
                .map(|b| mass(&r.plant_depth, top + step * b as f64, top + step * (b + 1) as f64))
                .collect();
            let width: Vec<f64> = (0..BANDS)
                .map(|b| mass(&r.plant_width, b as f64 / BANDS as f64, (b + 1) as f64 / BANDS as f64))
                .collect();
            let total = (seeds * each) as f64;
            for (name, got, want) in [("depth", down, depth), ("width", across, width)] {
                let sum: f64 = want.iter().sum();
                for b in 0..BANDS {
                    let (share, expected) = (got[b] / total, want[b] / sum);
                    assert!(
                        (share - expected).abs() < 0.03,
                        "{name} {}: band {b} — {share:.3}, the profile gives {expected:.3}",
                        describe(&r)
                    );
                }
            }
        }
    }
}
