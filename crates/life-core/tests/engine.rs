//! Тесты движка — перенос по смыслу `python/tests/test_simulation.py` (тег python-final).
//! Каждый тест на регрессию закрывает баг, который уже был в Python-коде.
//! Циклов без границы нет: все прогоны ограничены числом тиков.

use life_core::config::*;
use life_core::genome::vegetarian::{GENES, Gene};
use life_core::grid::Grid;
use life_core::plant::Plant;
use life_core::rng::Rng;
use life_core::senses::{Blind, vegetarian_senses};
use life_core::vegetarian::Vegetarian;
use life_core::{Counters, Genome, Rules, Shape, Space, VegetarianGenome, World, WorldConfig};

const BASE: VegetarianGenome = VegetarianGenome::BASE;

fn genom(size: f64) -> VegetarianGenome {
    BASE.with(Gene::Size, size)
}

/// Пустой мир: ни существ, ни растений.
fn empty_world(rules: Rules) -> World {
    let mut w = World::new(&WorldConfig { seed: 3, rules, n_vegetarians: Some(0), ..Default::default() });
    w.plants.clear();
    w
}

fn veg(x: f64, y: f64, g: VegetarianGenome) -> Vegetarian {
    Vegetarian::new(&Space::default(), &Rules::default(), g, Some(x), Some(y), None, Rng::new(0))
}

// ── регрессии ───────────────────────────────────────────────────────────────

/// Темп растений — ожидаемое число за тик, а не вероятность (было: ровно 1 за тик).
#[test]
fn темп_растений_это_ожидаемое_число() {
    let mut w = empty_world(Rules::default());
    let ticks = 500; // 2.5 * 500 = 1250 — ниже потолка 1500
    for _ in 0..ticks {
        w.step();
    }
    let rate = w.plants.len() as f64 / ticks as f64;
    assert!((rate - PLANT_SPAWN_CHANCE).abs() < 0.15, "прирост {rate:.3} растений/тик");
}

/// Без травоядных растения упираются в потолок, а не растут вечно.
#[test]
fn растения_упираются_в_потолок() {
    let mut w = empty_world(Rules::default());
    for _ in 0..1000 {
        w.step();
    }
    assert_eq!(w.plants.len(), PLANT_MAX);
}

// ── каннибализм ────────────────────────────────────────────────────────────

/// Мир с большим травоядным и соседом заданного размера на заданном расстоянии.
/// Тик 1 — не тик деления: считаем только поедание.
fn cannibal_world(on: bool, small: f64, dx: f64) -> World {
    let rules = Rules::default().with("cannibalism", if on { 1.0 } else { 0.0 }).unwrap();
    let mut w = empty_world(rules);
    w.tick = 1;
    w.spawn_vegetarian(genom(100.0), 3000.0, 2000.0, Some(100.0));
    w.spawn_vegetarian(genom(small), 3000.0 + dx, 2000.0, None);
    w
}

#[test]
fn каннибал_съедает_мелкого_рядом() {
    let mut w = cannibal_world(true, 30.0, 10.0);
    let prey_energy = w.vegetarians[1].energy;
    w.step();
    assert_eq!(w.vegetarians.len(), 1, "мелкий сородич в досягаемости съеден");
    assert_eq!(w.counters.vegetarians_cannibalized, 1);
    let big = &w.vegetarians[0];
    assert!(big.energy > 100.0 + prey_energy - 5.0, "энергия жертвы досталась едоку: {}", big.energy);
}

#[test]
fn каннибал_не_ест_крупного_дальнего_и_при_выключенном_правиле() {
    for (on, small, dx, why) in [
        (true, 50.0, 10.0, "всего вдвое мельче — при отношении 2.5 не еда"),
        (true, 30.0, 400.0, "далеко — каннибал не ищет, а ест того, кто рядом"),
        (false, 30.0, 10.0, "правило выключено"),
    ] {
        let mut w = cannibal_world(on, small, dx);
        w.step();
        assert_eq!(w.vegetarians.len(), 2, "{why}");
        assert_eq!(w.counters.vegetarians_cannibalized, 0, "{why}");
    }
}

/// Съеденный сородич в этом же тике сам никого не ест: тройка «крупный →
/// средний → мелкий» по порядку номеров теряет только среднего.
#[test]
fn съеденный_сородич_не_ест() {
    let rules = Rules::default().with("cannibalism", 1.0).unwrap();
    let mut w = empty_world(rules);
    w.tick = 1;
    w.spawn_vegetarian(genom(250.0), 3000.0, 2000.0, Some(500.0));
    w.spawn_vegetarian(genom(90.0), 3150.0, 2000.0, None);
    w.spawn_vegetarian(genom(30.0), 3200.0, 2000.0, None);
    let ids: Vec<u64> = w.vegetarians.iter().map(|v| v.id).collect();
    w.step();
    let left: Vec<u64> = w.vegetarians.iter().map(|v| v.id).collect();
    assert_eq!(w.counters.vegetarians_cannibalized, 1, "одно поедание за тик на едока");
    assert_eq!(left, vec![ids[0], ids[2]], "съеден средний, мелкий цел");
}

/// Выключенный каннибализм — тот же мир бит в бит, что и без правила вовсе:
/// проход не тянет случайных чисел. Включённый — другой мир, и счётчики сходятся.
#[test]
fn каннибализм_выключен_бит_в_бит_и_счётчики_сходятся() {
    let run = |rules: Rules| {
        let mut w = World::new(&WorldConfig { seed: 3, rules, ..Default::default() });
        for _ in 0..3000 {
            w.step();
        }
        w
    };
    let off = Rules::default().with("cannibalism", 0.0).unwrap().with("cannibal_ratio", 1.5).unwrap();
    assert_eq!(run(off).stats(), run(Rules::default()).stats());

    let on = Rules::default().with("cannibalism", 1.0).unwrap().with("cannibal_ratio", 1.2).unwrap();
    let w = run(on);
    let c = w.counters;
    assert!(c.vegetarians_cannibalized > 0, "за 3000 тиков при отношении 1.2 хоть кого-то съели: {c:?}");
    let veg0 = VEGETARIANS_AT_START as u64;
    assert_eq!(
        w.vegetarians.len() as u64,
        veg0 + c.vegetarians_born - c.vegetarians_starved - c.vegetarians_cannibalized
    );
}

/// Умершее от голода на своём ходу травоядное не ест и не делится.
#[test]
fn умерший_от_голода_не_ест() {
    let mut w = empty_world(Rules::default());
    w.spawn_vegetarian(genom(40.0), 1000.0, 1000.0, None);
    let v = &mut w.vegetarians[0];
    v.energy = v.pheno.upkeep / 2.0; // этот ход — последний
    let (x, y) = (v.x, v.y);
    w.plants.push(Plant::at(x, y));
    w.plants.push(Plant::at(x + 5.0, y));
    w.step();
    assert!(w.vegetarians.is_empty());
    assert!(w.plants.iter().filter(|p| p.y == y).count() == 2, "труп съел растение");
}

/// После деления у родителя остаётся резерв (было: отдавал всё и умирал).
#[test]
fn родитель_сохраняет_резерв() {
    let (s, r) = (Space::default(), Rules::default());
    let (mut divided, mut blocked) = (0, 0);
    for share in [10.0, 30.0, 50.0, 70.0, 90.0] {
        let g = BASE.with(Gene::ReproThreshold, 30.0).with(Gene::ReproShare, share);
        let mut p = Vegetarian::new(&s, &r, g, Some(1000.0), Some(1000.0), Some(60.0), Rng::new(0));
        match p.maybe_divide(&s, &r) {
            Some(_) => {
                divided += 1;
                assert!(p.energy >= VEGETARIAN_REPRO_RESERVE, "доля {share}%: осталось {}", p.energy);
            }
            None => {
                blocked += 1;
                assert_eq!(p.energy, 60.0, "деления не было, а энергия ушла");
            }
        }
    }
    assert!(divided > 0 && blocked > 0, "одна из веток не проверена");
}

/// Тело крупнее мира не выходит за мир и не прыгает дальше скорости.
#[test]
fn огромное_тело_не_прыгает() {
    let s = Space::default();
    for size in [2500.0, 3500.0, 4100.0, 9000.0] {
        for (lo, hi) in [(5.0, 10.0), (0.0, 100.0), (90.0, 100.0)] {
            let g =
                BASE.with(Gene::Size, size).with(Gene::Speed, 60.0).with(Gene::MinY, lo).with(Gene::MaxY, hi);
            let mut v = veg(100.0, 100.0, g);
            assert!(v.pheno.x_lo <= v.pheno.x_hi && v.pheno.body_lo <= v.pheno.body_hi);
            for _ in 0..30 {
                let (x, y) = (v.x, v.y);
                v.energy = v.pheno.max_energy;
                v.step(&Blind);
                assert!((v.x - x).hypot(v.y - y) <= v.pheno.speed + 1e-9, "прыжок дальше скорости");
                assert!(
                    (0.0..=s.width).contains(&v.x) && (0.0..=s.height).contains(&v.y),
                    "центр вне мира: ({}, {})",
                    v.x,
                    v.y
                );
            }
        }
    }
}

/// Ребёнок рождается у родителя (слой мягкий — не телепортируется в свой),
/// внутри мира и не на диагонали от родителя.
#[test]
fn ребёнок_рождается_у_родителя() {
    let (s, r) = (Space::default(), Rules::default());
    let g = BASE.with(Gene::ReproThreshold, 30.0);
    let mut parent = Vegetarian::new(&s, &r, g, Some(3000.0), Some(3900.0), None, Rng::new(0));
    let (mut diagonal, mut outside) = (0, 0);
    for _ in 0..300 {
        parent.energy = parent.pheno.max_energy;
        let mut c = parent.maybe_divide(&s, &r).expect("сытый родитель не поделился");
        assert!(c.pheno.y_lo <= c.y && c.y <= c.pheno.y_hi, "ребёнок вне мира: y={}", c.y);
        assert!(c.pheno.x_lo <= c.x && c.x <= c.pheno.x_hi);
        let span = parent.pheno.size * 2.0;
        assert!(
            (c.x - parent.x).abs() <= span && (c.y - parent.y).abs() <= span,
            "ребёнок далеко от родителя"
        );
        outside += (c.y < c.pheno.body_lo || c.y > c.pheno.body_hi) as u32;
        diagonal += ((c.x - parent.x) == (c.y - parent.y)) as u32;
        let (x, y) = (c.x, c.y);
        c.step(&Blind);
        assert!((c.x - x).hypot(c.y - y) <= c.pheno.speed + 1e-9, "первый ход ребёнка — прыжок");
    }
    assert_eq!(diagonal, 0);
    assert!(outside > 0, "ни один ребёнок не родился вне своего слоя — тест ничего не проверил");
}

/// Слой мягкий: растение над слоем видно — существо идёт и съедает его.
#[test]
fn еда_над_слоем_съедается() {
    let mut w = empty_world(Rules::default());
    let g = BASE.with(Gene::MinY, 50.0);
    w.spawn_vegetarian(g, 3000.0, 2100.0, Some(80.0));
    let v = &w.vegetarians[0];
    assert!(v.y >= v.pheno.body_lo, "существо должно стартовать в своём слое");
    let plant_y = v.pheno.layer_lo - 250.0;
    w.plants.push(Plant::at(3000.0, plant_y));
    let there = |w: &World| w.plants.iter().any(|p| p.x == 3000.0 && p.y == plant_y);
    for _ in 0..60 {
        w.step();
        if !there(&w) {
            break;
        }
    }
    assert!(!there(&w), "растение над слоем осталось несъеденным");
    assert!(w.vegetarians[0].y < w.vegetarians[0].pheno.body_lo, "съело, не выходя из слоя?");
}

/// Вне своего слоя и без еды существо возвращается домой и дальше держится в слое.
#[test]
fn без_еды_возвращается_в_слой() {
    let g = BASE.with(Gene::MinY, 50.0);
    let mut v = veg(3000.0, 500.0, g);
    assert_eq!(v.y, 500.0, "заданная позиция не зажимается в слой");
    let mut home = None;
    for t in 0..400 {
        let (x, y) = (v.x, v.y);
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
        assert!((v.x - x).hypot(v.y - y) <= v.pheno.speed + 1e-9, "прыжок дальше скорости");
        let inside = v.pheno.body_lo <= v.y && v.y <= v.pheno.body_hi;
        match home {
            None if inside => home = Some(t),
            Some(_) => assert!(inside, "вернулось в слой и снова ушло без еды: y={}", v.y),
            None => {}
        }
    }
    let t = home.expect("за 400 тиков не вернулось в слой");
    let ideal = ((v.pheno.body_lo - 500.0) / v.pheno.speed).ceil() as usize;
    assert!(t < ideal + 5, "шло домой {t} тиков вместо ~{ideal}: не по прямой");
}

/// Слой уже тела: существо живёт на линии и не стоит столбом.
#[test]
fn схлопнутый_слой_проходим() {
    let g = BASE.with(Gene::MinY, 50.0).with(Gene::MaxY, 50.5);
    let mut v = veg(3000.0, 2000.0, g);
    assert_eq!(v.pheno.body_lo, v.pheno.body_hi);
    let x0 = v.x;
    for _ in 0..50 {
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
        assert_eq!(v.y, v.pheno.body_lo);
    }
    assert!(v.x != x0, "существо на схлопнутом слое стоит столбом");
}

// ── стратегии и гены поведения ─────────────────────────────────────────────

const LURKER: VegetarianGenome = BASE.with(Gene::Strategy, 1.0);
/// Расход, восстановленный из разницы энергий, совпадает с точностью до округления.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

/// Ход травоядного: (пройдено, потрачено).
fn veg_move(v: &mut Vegetarian, senses: &impl life_core::senses::VegetarianSenses) -> (f64, f64) {
    let (x, y, e) = (v.x, v.y, v.energy);
    v.step(senses);
    ((v.x - x).hypot(v.y - y), e - v.energy)
}

#[test]
fn медленный_ход_дешевле() {
    let v = veg(1000.0, 1000.0, BASE);
    assert!((v.pheno.slow_speed - v.pheno.speed * SLOW_PACE).abs() < 1e-12);
    assert!(v.pheno.slow_upkeep < v.pheno.upkeep);
    assert_eq!(v.pheno.slow_upkeep, Rules::default().upkeep(40.0, v.pheno.slow_speed, v.pheno.vision));
}

/// Затаившийся без еды бродит медленно и дёшево; стандартный — на полной.
#[test]
fn затаившийся_без_еды_бродит_медленно() {
    let mut lurker = veg(1000.0, 1000.0, LURKER);
    let mut standard = veg(1000.0, 1000.0, BASE);
    for _ in 0..20 {
        let (d, cost) = veg_move(&mut lurker, &Blind);
        assert!((d - lurker.pheno.slow_speed).abs() < 1e-9, "затаившийся прошёл {d}");
        assert!(close(cost, lurker.pheno.slow_upkeep), "затаившийся потратил {cost}");
        let (d, cost) = veg_move(&mut standard, &Blind);
        assert!((d - standard.pheno.speed).abs() < 1e-9, "стандартный прошёл {d}");
        assert!(close(cost, standard.pheno.upkeep), "стандартный потратил {cost}");
    }
}

/// К еде затаившийся идёт на полной скорости.
#[test]
fn затаившийся_к_еде_на_полной() {
    let mut v = veg(1000.0, 1000.0, LURKER);
    let (d, cost) = veg_move(&mut v, &vegetarian_senses(|_, _, _| Some((1300.0, 1000.0))));
    assert!((d - v.pheno.speed).abs() < 1e-9 && close(cost, v.pheno.upkeep), "к еде: {d}, {cost}");
}

/// Стратегия наследуется и изредка мутирует в другую.
#[test]
fn стратегия_мутирует_изредка() {
    let (s, r) = (Space::default(), Rules::default());
    let g = BASE.with(Gene::ReproThreshold, 30.0);
    let mut parent = Vegetarian::new(&s, &r, g, Some(3000.0), Some(1000.0), None, Rng::new(7));
    let n = 2000;
    let mut switched = 0;
    for _ in 0..n {
        parent.energy = parent.pheno.max_energy;
        let c = parent.maybe_divide(&s, &r).expect("сытый родитель не поделился");
        switched += (c.genome[Gene::Strategy] != 0.0) as u32;
    }
    let rate = switched as f64 / n as f64;
    assert!((rate - STRATEGY_SWITCH_CHANCE).abs() < 0.01, "стратегию сменили {rate:.3} детей");
}

/// Смешанный мир: стартовая смесь раздаётся без жребия, и тот же сид — тот же мир.
#[test]
fn смешанный_мир_детерминирован() {
    let cfg = WorldConfig { seed: 5, vegetarian_strategies: vec![1.0, 1.0], ..Default::default() };
    let w = World::new(&cfg);
    let lurkers = w.vegetarians.iter().filter(|v| v.genome[Gene::Strategy] == 1.0).count();
    assert_eq!(lurkers, w.vegetarians.len() / 2, "смесь 50/50 раздана неровно");
    let run = || {
        let mut w = World::new(&cfg);
        for _ in 0..1000 {
            w.step();
        }
        w.stats()
    };
    assert_eq!(run(), run());
}

// ── сетка соседей ───────────────────────────────────────────────────────────

/// Сетка обязана быть НАДмножеством честного перебора. Пропусти она соседа —
/// существо перестанет замечать еду под носом, а численность останется
/// правдоподобной, так что другие тесты этого не поймают.
#[test]
fn сетка_совпадает_с_перебором() {
    let mut rng = Rng::new(42);
    for scale in [1.0, 3.5] {
        let s = Space::scaled(scale);
        for cell in [64.0, 256.0, 1000.0] {
            let mut pts: Vec<(f64, f64)> =
                (0..800).map(|_| (rng.uniform(0.0, s.width), rng.uniform(0.0, s.height))).collect();
            // точки на самых краях и углах мира
            pts.extend([(0.0, 0.0), (s.width, s.height), (0.0, s.height), (s.width, 0.0)]);
            let mut g = Grid::new(cell);
            g.rebuild(&s, pts.iter().copied());
            for _ in 0..300 {
                let (x, y) = (rng.uniform(-100.0, s.width + 100.0), rng.uniform(-100.0, s.height + 100.0));
                let r = rng.uniform(0.0, 1500.0);
                let mut found = vec![false; pts.len()];
                g.for_each_near(x, y, r, |i, px, py| {
                    assert_eq!((px, py), pts[i], "сетка вернула не те координаты");
                    found[i] = true;
                });
                for (i, &(px, py)) in pts.iter().enumerate() {
                    if (px - x).hypot(py - y) <= r {
                        assert!(
                            found[i],
                            "клетка {cell}: пропущена точка ({px}, {py}) в радиусе {r} от ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
}

// ── правила ────────────────────────────────────────────────────────────────

#[test]
fn не_конечные_правила_отвергаются() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Rules::default().with("mutation_sigma", bad).is_err());
    }
    assert!(Rules::default().with("нет_такого", 1.0).is_err());
}

/// Значения, при которых правило теряет смысл, отвергаются; края допустимого — нет.
#[test]
fn бессмысленные_правила_отвергаются() {
    let r = Rules::default();
    for (key, bad) in [
        ("cost_scale", -1.0),
        ("plant_energy", -5.0),
        ("mutation_sigma", -0.1),
        ("cannibalism", 0.5),
        ("cannibal_ratio", 1.0),
    ] {
        assert!(r.with(key, bad).is_err(), "{key}={bad} принято");
    }
    for (key, ok) in
        [("cost_scale", 0.0), ("plant_rate", 0.0), ("cannibalism", 1.0), ("cannibal_ratio", 1.01)]
    {
        assert!(r.with(key, ok).is_ok(), "{key}={ok} отвергнуто");
    }
}

#[test]
fn расход_по_умолчанию_это_формула_конфига() {
    let r = Rules::default();
    for (size, speed, vision) in [(40.0_f64, 10.0_f64, 400.0_f64), (80.0, 25.0, 150.0), (13.0, 3.0, 900.0)] {
        let mass = (size / 40.0_f64).powf(SPEED_MASS_POWER);
        let want = SIZE_ENERGY_COEF * size.powf(SIZE_ENERGY_POWER)
            + SPEED_ENERGY_COEF * speed.powf(SPEED_ENERGY_POWER) * mass
            + SIGHT_ENERGY_COEF * vision.powf(SIGHT_ENERGY_POWER);
        assert_eq!(r.upkeep(size, speed, vision), want);
    }
}

/// Показатель меняет крутизну, а базовый стат стоит столько же.
#[test]
fn показатель_меняет_крутизну_а_не_базу() {
    let base = Rules::default();
    let steep = base.with("size_power", 3.5).unwrap();
    let flat = base.with("size_power", 1.5).unwrap();
    let b = base.upkeep(40.0, 10.0, 400.0);
    assert!((steep.upkeep(40.0, 10.0, 400.0) - b).abs() < 1e-12);
    assert!((flat.upkeep(40.0, 10.0, 400.0) - b).abs() < 1e-12);
    assert!(steep.upkeep(80.0, 10.0, 400.0) > base.upkeep(80.0, 10.0, 400.0));
    assert!(flat.upkeep(80.0, 10.0, 400.0) < base.upkeep(80.0, 10.0, 400.0));
}

#[test]
fn цена_статов_масштабирует_всё() {
    let base = Rules::default();
    let double = base.with("cost_scale", 2.0).unwrap();
    assert!((double.upkeep(55.0, 17.0, 300.0) - 2.0 * base.upkeep(55.0, 17.0, 300.0)).abs() < 1e-12);
}

// ── инварианты и воспроизводимость ─────────────────────────────────────────

#[test]
fn инварианты_держатся_со_временем() {
    let mut w = World::new(&WorldConfig { seed: 1, ..Default::default() });
    for _ in 0..600 {
        w.step();
        for v in &w.vegetarians {
            assert!(v.alive && v.energy > 0.0 && v.energy <= v.pheno.max_energy + 1e-9);
            assert!(v.pheno.x_lo <= v.x && v.x <= v.pheno.x_hi && v.pheno.y_lo <= v.y && v.y <= v.pheno.y_hi);
            let g = v.genome.values();
            for (spec, x) in GENES.iter().zip(g) {
                match spec.variants() {
                    // ген-выбор — номер существующего варианта
                    Some(variants) => {
                        assert!(x.fract() == 0.0 && (*x as usize) < variants.len(), "ген {} = {x}", spec.key)
                    }
                    None => assert!(*x >= 0.01, "ген {} ниже 0.01: {g:?}", spec.key),
                }
                assert!(!spec.is_percent() || (0.0..=100.0).contains(x), "ген-процент вне 0‒100: {g:?}");
            }
        }
        assert!(w.plants.iter().all(|p| p.alive), "съеденное растение не выметено");
        let mut ids: Vec<u64> = w.vegetarians.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), w.vegetarians.len(), "номера существ повторяются");
    }
}

#[test]
fn один_сид_один_мир() {
    let run = |seed| {
        let mut w = World::new(&WorldConfig { seed, ..Default::default() });
        for _ in 0..300 {
            w.step();
        }
        w.stats()
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6));
}

/// Счётчики — бухгалтерия без потерь: сколько было, плюс родилось, минус
/// съедено и умерло, равно тому, сколько есть. Пропусти мир одну смерть —
/// отчёт стал бы объяснять численность неверными причинами.
#[test]
fn счётчики_сходятся_с_численностью() {
    let rules = Rules::default().with("cannibalism", 1.0).unwrap().with("cannibal_ratio", 1.5).unwrap();
    let mut w = World::new(&WorldConfig { seed: 3, rules, ..Default::default() });
    let (veg0, plants0) = (w.vegetarians.len() as u64, w.plants.len() as u64);
    for _ in 0..3000 {
        w.step();
    }
    let c = w.counters;
    assert!(c.vegetarians_born > 0 && c.vegetarians_cannibalized > 0 && c.vegetarians_starved > 0, "{c:?}");
    assert_eq!(
        w.vegetarians.len() as u64,
        veg0 + c.vegetarians_born - c.vegetarians_starved - c.vegetarians_cannibalized
    );
    assert_eq!(w.plants.len() as u64, plants0 + c.plants_grown - c.plants_eaten);
    assert_eq!(c.since(&c), Counters::default());
}

/// Большой мир — тот же мир, только больше: стартовые популяции и потолки
/// растут с площадью. Полоса растёт вширь, остальные формы — в обе стороны.
#[test]
fn масштаб_растит_площадь() {
    let strip = World::new(&WorldConfig { scale: 10.0, shape: Shape::Strip, ..Default::default() });
    assert_eq!(strip.space.height, WORLD_HEIGHT);
    assert_eq!(strip.space.width, WORLD_WIDTH * 10.0);
    for shape in Shape::ALL {
        let w = World::new(&WorldConfig { scale: 10.0, shape, ..Default::default() });
        assert_eq!(w.vegetarians.len(), VEGETARIANS_AT_START * 10, "{shape:?}");
        assert!(w.vegetarians.iter().all(|v| v.x <= w.space.width && v.y <= w.space.height), "{shape:?}");
    }
    let wide = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
    assert!(wide.space.height > WORLD_HEIGHT * 3.0, "3:2 растёт и вглубь: {:?}", wide.space);

    let mut e = World::new(&WorldConfig { scale: 2.0, n_vegetarians: Some(0), ..Default::default() });
    for _ in 0..2000 {
        e.step();
    }
    assert_eq!(e.plants.len(), PLANT_MAX * 2);
}

/// Мир уже базового не строится: в узком мире полоса блуждания переворачивалась,
/// и мир падал на первом же тике вместо внятной ошибки.
#[test]
#[should_panic(expected = "масштаб мира")]
fn масштаб_меньше_базового_отвергается() {
    Space::scaled(0.01);
}

/// Гигантский масштаб — внятная ошибка, а не падение на выделении памяти.
#[test]
#[should_panic(expected = "масштаб мира")]
fn масштаб_больше_предела_отвергается() {
    Space::scaled(1e7);
}

#[test]
fn стартовые_численности_известны_до_постройки_мира() {
    for cfg in [
        WorldConfig::default(),
        WorldConfig { scale: 3.0, ..Default::default() },
        WorldConfig { n_vegetarians: Some(7), ..Default::default() },
    ] {
        let w = World::new(&cfg);
        assert_eq!(w.vegetarians.len(), cfg.vegetarians_at_start());
    }
}

// ── производительность ─────────────────────────────────────────────────────

/// Страж от обвала скорости: мир x10 на фиксированной нагрузке 4000 травоядных
/// и 4000 растений. На 400 существах (как было в Python-версии) Rust
/// и полным перебором успевал бы, а здесь перебор — десятки миллионов пар за тик.
/// Порог с большим запасом: тест ловит поломку вроде «сетка перестала работать
/// и всё стало O(n²)», а не шум машины CI: с сеткой ~2 мс, без неё ~80 мс,
/// порог 20. Растения подсыпаются каждый тик,
/// чтобы нагрузка не таяла.
#[test]
fn тик_укладывается_в_бюджет_на_фиксированной_нагрузке() {
    let mut w =
        World::new(&WorldConfig { seed: 9, scale: 10.0, n_vegetarians: Some(4000), ..Default::default() });
    let mut rng = Rng::new(9);
    let ticks = 100;
    let started = std::time::Instant::now();
    for _ in 0..ticks {
        while w.plants.len() < 4000 {
            let p = w.flora().plant(&mut rng);
            w.plants.push(p);
        }
        w.step();
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
    eprintln!("  [скорость] {ms:.3} мс/тик при 4000/4000");
    assert!(w.vegetarians.len() > 1000, "нагрузка растаяла — замер бессмыслен");
    assert!(ms < 20.0, "тик {ms:.2} мс при 4000/4000: где-то перебор вместо сетки?");
}

// ── игра: выбор, слежение, правила на ходу ──────────────────────────────────

#[test]
fn выбор_кликом_совпадает_с_перебором_в_живом_мире() {
    let mut w = World::new(&WorldConfig { seed: 5, ..Default::default() });
    for _ in 0..300 {
        w.step();
    }
    assert!(w.vegetarians.len() > 20);
    // Клики по сетке точек: ответ — ближайший по краю тела среди тех, кто ближе radius.
    let radius = 15.0;
    let mut hits = 0;
    for i in 0..60 {
        for j in 0..40 {
            let (x, y) = (i as f64 * 100.0 + 13.0, j as f64 * 100.0 + 7.0);
            let mut best: Option<(f64, u64)> = None;
            for v in &w.vegetarians {
                let d = ((v.x - x).powi(2) + (v.y - y).powi(2)).sqrt() - v.pheno.size / 2.0;
                if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, v.id));
                }
            }
            assert_eq!(w.pick(x, y, radius), best.map(|(_, id)| id));
            hits += best.is_some() as usize;
        }
    }
    assert!(hits > 10, "клики хоть куда-то попали ({hits})");
}

#[test]
fn существо_находится_по_номеру_после_смертей_и_рождений() {
    let mut w = World::new(&WorldConfig { seed: 2, ..Default::default() });
    for _ in 0..600 {
        w.step();
        assert!(w.vegetarians.windows(2).all(|p| p[0].id < p[1].id), "травоядные по возрастанию id");
    }
    for v in &w.vegetarians {
        assert_eq!(w.vegetarian(v.id).map(|f| f.id), Some(v.id));
    }
    assert!(w.vegetarian(u64::MAX).is_none());
}

#[test]
fn новые_правила_пересчитывают_живых_как_новорождённых() {
    let mut w = World::new(&WorldConfig { seed: 4, ..Default::default() });
    for _ in 0..200 {
        w.step();
    }
    let rules = Rules::default().with("cost_scale", 3.0).and_then(|r| r.with("size_power", 2.0)).unwrap();
    w.set_rules(rules.clone());
    let space = w.space;
    for v in &w.vegetarians {
        let fresh = Vegetarian::new(&space, &rules, v.genome, Some(v.x), Some(v.y), None, Rng::new(0));
        assert_eq!(v.pheno, fresh.pheno, "фенотип живого — как у новорождённого с тем же геномом");
    }
    assert_eq!(w.rules, rules);
}

/// Профиль еды меняется на ходу: выросшее остаётся на местах, новое растёт
/// по-новому. Мир пустой, поэтому растения не едят и порядок их сохраняется.
#[test]
fn профиль_еды_меняется_на_ходу() {
    let mut w = empty_world(Rules::default());
    for _ in 0..150 {
        w.step();
    }
    let before = w.plants.len();
    let third = w.space.width / 3.0;
    // крутизна 30 по ширине: правее трети оси — e^-10 еды
    let rules = w
        .rules
        .with_text("plant_width_profile", "exp")
        .and_then(|r| r.with("plant_width_steepness", 30.0))
        .unwrap();
    w.set_rules(rules);
    for _ in 0..300 {
        w.step();
    }
    let (old, fresh) = w.plants.split_at(before);
    assert!(fresh.len() > 500, "выросло {}", fresh.len());
    let left = |ps: &[Plant]| ps.iter().filter(|p| p.x < third).count() as f64 / ps.len() as f64;
    assert!(left(fresh) > 0.99, "новые — у левого края: {:.3}", left(fresh));
    assert!(left(old) < 0.5, "старые остались равномерными: {:.3}", left(old));
}
