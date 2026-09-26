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
//! Capacity: the world is split into `PLANT_MAX` (per area) cells of equal
//! fertility — equal steps of each axis's distribution function, so a cell is
//! narrow where food is rich and wide where it is poor. A cell holds at most
//! one plant (`World::spawn_plants`), so a full world follows the profile
//! exactly and a grazed surface cannot hand its room to the deep sea.

use std::f64::consts::TAU;

use crate::config::{PLANT_MAX, PLANT_RADIUS, PLANT_TOP_MARGIN_PCT};
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
}

impl Profile {
    pub const ALL: [Profile; 5] =
        [Profile::Uniform, Profile::Linear, Profile::Exp, Profile::Log, Profile::Waves];

    /// Имя для флага: `--rule plant_width_profile=waves`.
    pub fn key(self) -> &'static str {
        match self {
            Profile::Uniform => "uniform",
            Profile::Linear => "linear",
            Profile::Exp => "exp",
            Profile::Log => "log",
            Profile::Waves => "waves",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Profile::Uniform => "равномерно",
            Profile::Linear => "линейно",
            Profile::Exp => "экспонента",
            Profile::Log => "логарифм",
            Profile::Waves => "волны",
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
        }
    }

    /// Профиль словами: «экспонента, крутизна 8».
    pub fn describe(&self) -> String {
        let p = self.kind();
        match p {
            Profile::Uniform => p.label().into(),
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
        match &self.law {
            Law::Uniform => rng.uniform(self.lo, self.hi),
            Law::Exp { lambda, e_lo, e_hi } => {
                let u = rng.random();
                -(e_lo - u * (e_lo - e_hi)).ln() / lambda
            }
            Law::Table(cdf) => {
                let u = rng.random();
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

/// Где растёт еда в этом мире: оси с готовыми законами.
#[derive(Clone, Debug, PartialEq)]
pub struct Flora {
    x: Axis,
    y: Axis,
    /// Cells of equal fertility across and down: `nx * ny` is at least the
    /// world's plant cap.
    nx: usize,
    ny: usize,
}

impl Flora {
    pub fn new(rules: &Rules, space: &Space) -> Flora {
        // Мёртвая зона у поверхности — свойство поверхности, а не профиля.
        // Считается как 5 * 4000 / 100 — ровно 200, как было до профилей.
        let margin = PLANT_TOP_MARGIN_PCT * space.height / 100.0;
        let r = PLANT_RADIUS;
        // rows by the world's proportions: with uniform food the cells are near squares
        let cap = space.per_area(PLANT_MAX);
        let ny = ((cap as f64 * space.height / space.width).sqrt().round() as usize).clamp(1, cap);
        Flora {
            x: Axis::new(&rules.plant_width, space.width, r, space.width - r),
            y: Axis::new(&rules.plant_depth, space.height, r + margin, space.height - r),
            nx: cap.div_ceil(ny),
            ny,
        }
    }

    /// How many cells of equal fertility the world has (one plant each).
    pub fn cells(&self) -> usize {
        self.nx * self.ny
    }

    /// The cell a plant at (x, y) occupies.
    #[inline]
    pub fn cell(&self, x: f64, y: f64) -> usize {
        self.x.slot(x, self.nx) + self.nx * self.y.slot(y, self.ny)
    }

    /// Новое растение: два случайных числа, x и потом y.
    #[inline]
    pub fn plant(&self, rng: &mut Rng) -> Plant {
        let x = self.x.sample(rng);
        let y = self.y.sample(rng);
        Plant::at(x, y)
    }
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

/// «по глубине — экспонента, крутизна 8; по ширине — равномерно».
pub fn describe(rules: &Rules) -> String {
    format!("по глубине — {}; по ширине — {}", rules.plant_depth.describe(), rules.plant_width.describe())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PLANT_DEPTH_DECAY;
    use crate::space::Shape;

    fn rules(pairs: &[(&str, f64)]) -> Rules {
        pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
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

    /// Мир по умолчанию сажает растения ровно как до профилей: та же формула,
    /// те же случайные числа, те же биты.
    #[test]
    fn по_умолчанию_как_до_профилей_бит_в_бит() {
        let space = Space::default();
        let flora = Flora::new(&Rules::default(), &space);
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

    /// Любой профиль тянет ровно два числа: смена профиля на ходу не сдвигает
    /// поток мира сильнее, чем меняет сами растения.
    #[test]
    fn два_числа_на_растение_при_любом_профиле() {
        let space = Space::new(10.0, Shape::Square);
        for r in every_profile() {
            let flora = Flora::new(&r, &space);
            let (mut a, mut b) = (Rng::new(5), Rng::new(5));
            for _ in 0..100 {
                flora.plant(&mut a);
                b.random();
                b.random();
            }
            assert_eq!(a.next_u64(), b.next_u64(), "{r:?}");
        }
    }

    #[test]
    fn растения_в_границах_при_любых_профилях() {
        for shape in Shape::ALL {
            let space = Space::new(7.0, shape);
            let margin = PLANT_TOP_MARGIN_PCT * space.height / 100.0;
            for r in every_profile() {
                let flora = Flora::new(&r, &space);
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
        let flora = Flora::new(r, &space);
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
        assert!((density(&r, 0.5, 0.05) - (-8.0 * 0.05_f64).exp()).abs() < 1e-12);
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
            "по глубине — экспонента, крутизна 8; по ширине — равномерно"
        );
    }

    /// The distribution function undoes sampling: a plant drawn with `u` sits at share `u`.
    #[test]
    fn cdf_inverts_sampling_for_every_profile() {
        let space = Space::new(7.0, Shape::Square);
        for r in every_profile() {
            let flora = Flora::new(&r, &space);
            let (mut a, mut b) = (Rng::new(17), Rng::new(17));
            for _ in 0..500 {
                let (ux, uy) = (a.random(), a.random());
                let p = flora.plant(&mut b);
                assert!((flora.x.cdf(p.x) - ux).abs() < 1e-6, "x: {ux} {r:?}");
                assert!((flora.y.cdf(p.y) - uy).abs() < 1e-6, "y: {uy} {r:?}");
            }
        }
    }

    /// Every cell is equally fertile: seeds spread evenly over cells, whatever the profile.
    #[test]
    fn seeds_fall_evenly_into_cells() {
        let space = Space::default();
        for r in [
            Rules::default(),
            rules(&[("plant_depth_profile", Profile::Waves.index()), ("plant_depth_amplitude", 100.0)]),
            rules(&[("plant_width_profile", Profile::Linear.index()), ("plant_width_end", 0.0)]),
        ] {
            let flora = Flora::new(&r, &space);
            assert!(flora.cells() >= PLANT_MAX && flora.cells() < PLANT_MAX + flora.ny);
            let mut hits = vec![0u32; flora.cells()];
            let mut rng = Rng::new(21);
            for _ in 0..100 * flora.cells() {
                let p = flora.plant(&mut rng);
                hits[flora.cell(p.x, p.y)] += 1;
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
}
