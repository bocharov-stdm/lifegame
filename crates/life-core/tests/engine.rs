//! Тесты движка — перенос по смыслу `python/tests/test_simulation.py` (тег python-final).
//! Каждый тест на регрессию закрывает баг, который уже был в Python-коде.
//! Циклов без границы нет: все прогоны ограничены числом тиков.

use life_core::config::*;
use life_core::genome::predator::Gene as PredatorGene;
use life_core::genome::vegetarian::{GENES, Gene};
use life_core::grid::Grid;
use life_core::plant::Plant;
use life_core::predator::{Predator, Prey};
use life_core::rng::Rng;
use life_core::senses::{Blind, predator_senses, vegetarian_senses};
use life_core::vegetarian::Vegetarian;
use life_core::{
    Counters, Creature, Genome, PredatorGenome, Rules, Shape, Space, VegetarianGenome, World, WorldConfig,
};

const BASE: VegetarianGenome = VegetarianGenome::BASE;

fn genom(size: f64) -> VegetarianGenome {
    BASE.with(Gene::Size, size)
}

/// Пустой мир: ни существ, ни растений.
fn empty_world(rules: Rules) -> World {
    let mut w = World::new(&WorldConfig {
        seed: 3,
        rules,
        n_vegetarians: Some(0),
        n_predators: Some(0),
        ..Default::default()
    });
    w.plants.clear();
    w
}

fn veg(x: f64, y: f64, g: VegetarianGenome) -> Vegetarian {
    Vegetarian::new(&Space::default(), &Rules::default(), g, Some(x), Some(y), None, Rng::new(0))
}

fn predator(x: f64, y: f64, energy: Option<f64>) -> Predator {
    Predator::new(
        &Space::default(),
        &Rules::default(),
        PredatorGenome::BASE,
        Some(x),
        Some(y),
        energy,
        Rng::new(0),
    )
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

/// Съеденный хищником в этом же тике не ест и не размножается.
#[test]
fn съеденный_не_действует() {
    let mut w = empty_world(Rules::default());
    w.spawn_vegetarian(genom(40.0), 1000.0, 1000.0, Some(1000.0));
    let (x, y) = (w.vegetarians[0].x, w.vegetarians[0].y);
    w.vegetarians[0].alive = false; // «съеден» на этом тике
    w.plants.push(Plant::at(x, y)); // прямо под ним
    w.step(); // tick 0 → ветка размножения
    assert!(w.plants.iter().any(|p| p.x == x && p.y == y), "мёртвое травоядное съело растение");
    assert!(w.vegetarians.is_empty(), "мёртвое травоядное оставило потомство");
}

/// Бегство продолжается, когда хищник пропал из виду (было: стоял столбом).
#[test]
fn бегство_продолжается_без_хищника() {
    let mut v = veg(1000.0, 1000.0, genom(40.0));
    let px = 1000.0 + v.pheno.vision / 4.0;
    let d2 = (px - 1000.0) * (px - 1000.0);
    v.step(&vegetarian_senses(|_, _, _| Some((px, 1000.0, d2)), |_, _, _| None));
    assert!(v.mind.flee_ticks > 0, "испуг не сработал — тест бессмыслен");

    let (x0, y0) = (v.x, v.y);
    for _ in 0..10 {
        v.energy = v.pheno.max_energy;
        v.step(&Blind);
    }
    let travelled = (v.x - x0).hypot(v.y - y0);
    assert!(travelled > 9.0 * v.pheno.speed, "убежал всего на {travelled:.1} px за 10 тиков");
    assert!(v.x < x0, "убегает не в ту сторону");
}

/// Новорождённый хищник не ходит в тике рождения: энергия у него нетронутая.
#[test]
fn новорождённый_хищник_не_ходит() {
    let rules = Rules::default().with("predator_divide_chance", 1.0).unwrap();
    let mut w = empty_world(rules);
    w.spawn_predator(3000.0, 2000.0, Some(PREDATOR_MAX_ENERGY));
    w.step();
    assert_eq!(w.predators.len(), 2, "хищник не поделился — тест бессмыслен");
    let child = &w.predators[1];
    assert_eq!(
        child.energy,
        child.pheno.max_energy * PREDATOR_CHILD_ENERGY,
        "ребёнок потратил энергию — его обработали в тике рождения"
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

/// Умерший от голода хищник никого не съедает.
#[test]
fn умерший_от_голода_хищник_не_охотится() {
    let mut w = empty_world(Rules::default());
    w.spawn_vegetarian(genom(40.0), 1000.0, 1000.0, None);
    w.spawn_predator(1010.0, 1000.0, None);
    w.predators[0].energy = w.predators[0].pheno.upkeep / 2.0;
    w.step();
    assert!(w.predators.is_empty(), "умерший хищник остался в мире");
    assert_eq!(w.vegetarians.len(), 1, "умерший от голода хищник съел добычу");
}

/// Цель блуждания хищника достижима (было: у стены, хищник стоял и умирал).
#[test]
fn цель_хищника_достижима() {
    let s = Space::default();
    let d = PREDATOR_DIAM;
    for (x, y) in [(d, d), (s.width - d, d), (d, s.height - d), (s.width - d, s.height - d), (3000.0, 2000.0)]
    {
        let mut p = predator(x, y, None);
        for _ in 0..200 {
            p.choose_new_target(&s);
            let (tx, ty) = (p.mind.tx, p.mind.ty);
            assert!(d <= tx && tx <= s.width - d, "цель x={tx} от ({x}, {y})");
            assert!(d <= ty && ty <= s.height - d, "цель y={ty} от ({x}, {y})");
        }
    }
    let mut p = predator(d, d, None);
    p.energy = 1e9; // голод ни при чём
    p.pheno.max_energy = 1e9;
    let mut stood = 0;
    for _ in 0..2000 {
        let before = (p.x, p.y);
        p.step(&s, &Blind);
        stood += ((p.x, p.y) == before) as u32;
    }
    assert_eq!(stood, 0, "хищник без добычи стоял {stood} тиков");
}

/// Хищник не гонится за травоядным, которого в этом тике уже съели.
#[test]
fn хищник_не_гонится_за_трупом() {
    let mut w = empty_world(Rules::default());
    w.spawn_predator(1000.0, 1000.0, None);
    w.spawn_vegetarian(genom(40.0), 1100.0, 1000.0, None);
    w.spawn_vegetarian(genom(40.0), 1000.0, 1300.0, None);
    w.vegetarians[0].alive = false;
    w.step();
    let p = &w.predators[0];
    assert_eq!(p.x, 1000.0, "хищник свернул к трупу");
    assert!(p.y > 1000.0, "хищник не пошёл к живой добыче");
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

// ── охота ──────────────────────────────────────────────────────────────────

/// Центр далеко, но тела соприкасаются — крупного ловят, мелкого нет.
#[test]
fn крупного_ловят_при_касании() {
    for (size, pos, caught) in [(40.0, (1070.0, 2000.0), false), (120.0, (1000.0, 2070.0), true)] {
        let mut w = World::new(&WorldConfig {
            n_vegetarians: Some(0),
            n_predators: Some(0),
            predator_speed: 0.0,
            ..Default::default()
        });
        w.plants.clear();
        w.spawn_predator(1000.0, 2000.0, None);
        w.spawn_vegetarian(genom(size), pos.0, pos.1, None);
        w.step();
        assert_eq!(w.vegetarians.is_empty(), caught, "размер {size}");
    }
}

/// Крупное тело видно издалека: хищник идёт прямо к нему.
#[test]
fn крупного_видно_дальше() {
    for (size, seen) in [(40.0, false), (120.0, true)] {
        let mut w = World::new(&WorldConfig {
            n_vegetarians: Some(0),
            n_predators: Some(0),
            predator_speed: 1.0,
            ..Default::default()
        });
        w.plants.clear();
        w.spawn_predator(1000.0, 2000.0, None);
        w.spawn_vegetarian(genom(size), 1540.0, 2000.0, None);
        w.step();
        let p = &w.predators[0];
        assert_eq!((p.x, p.y) == (1001.0, 2000.0), seen, "размер {size}: хищник в ({}, {})", p.x, p.y);
    }
}

#[test]
fn рывок_вблизи_стоит_энергии() {
    let s = Space::default();
    let mut far = predator(1000.0, 2000.0, Some(50.0));
    let mut near = predator(1000.0, 2000.0, Some(50.0));
    far.step(&s, &predator_senses(|_, _, _| Some(Prey { x: 1400.0, y: 2000.0, half: 20.0 })));
    near.step(&s, &predator_senses(|_, _, _| Some(Prey { x: 1150.0, y: 2000.0, half: 20.0 })));
    assert!((far.x - 1000.0 - far.pheno.speed).abs() < 1e-9);
    assert!((near.x - 1000.0 - near.pheno.speed * PREDATOR_SPRINT_MULT).abs() < 1e-9);
    assert!((far.energy - (50.0 - far.pheno.upkeep)).abs() < 1e-12);
    assert!((near.energy - (50.0 - near.pheno.upkeep - PREDATOR_SPRINT_COST)).abs() < 1e-12);
}

#[test]
fn рывок_не_проскакивает_добычу() {
    let mut p = predator(1000.0, 2000.0, None);
    p.step(&Space::default(), &predator_senses(|_, _, _| Some(Prey { x: 1010.0, y: 2000.0, half: 20.0 })));
    assert!((p.x - 1010.0).abs() < 1e-9);
}

#[test]
fn мигрант_приходит_когда_хищников_нет() {
    let mut w = World::new(&WorldConfig { n_vegetarians: Some(40), ..Default::default() });
    w.predators.clear();
    w.tick = PREDATOR_MIGRATION_PERIOD as u64;
    w.migrate_predators();
    assert_eq!((w.predators.len(), w.migrants), (1, 1));
    let p = &w.predators[0];
    assert_eq!((p.pheno.speed, p.pheno.vision), (PREDATOR_BASE_SPEED, PREDATOR_BASE_VISION));
    let d = PREDATOR_DIAM;
    let edge = p.x == d || p.y == d || p.x == WORLD_WIDTH - d || p.y == WORLD_HEIGHT - d;
    assert!(edge, "мигрант не у края: ({}, {})", p.x, p.y);
}

#[test]
fn мигранта_нет_когда_не_положено() {
    let period = PREDATOR_MIGRATION_PERIOD as u64;
    let world = |n_pred, n_veg, migration: f64| {
        World::new(&WorldConfig {
            n_predators: Some(n_pred),
            n_vegetarians: Some(n_veg),
            rules: Rules::default().with("predator_migration", migration).unwrap(),
            ..Default::default()
        })
    };
    let cases = [
        ("не тот тик", world(6, 40, PREDATOR_MIGRATION_PERIOD), period + 1),
        ("миграция выключена", world(6, 40, 0.0), period),
        ("мир без охоты", world(0, 40, PREDATOR_MIGRATION_PERIOD), period),
        ("нечего есть", world(6, PREDATOR_MIGRATION_PREY - 1, PREDATOR_MIGRATION_PERIOD), period),
    ];
    for (name, mut w, tick) in cases {
        w.predators.clear();
        w.tick = tick;
        // Жребий не тянется без мигранта: следующий спаун одинаков в копиях.
        let mut twin = w.clone();
        w.migrate_predators();
        assert!(w.predators.is_empty(), "{name}");
        w.spawn_predator(100.0, 100.0, None);
        twin.spawn_predator(100.0, 100.0, None);
        assert_eq!(w.predators[0].mind, twin.predators[0].mind, "{name}: жребий тянется без мигранта");
    }
}

/// Приток на единицу площади тот же, что в базовом мире: в мире x10 приходят десятеро.
#[test]
fn мигрантов_больше_в_большом_мире() {
    let mut w = World::new(&WorldConfig {
        scale: 10.0,
        n_vegetarians: Some(PREDATOR_MIGRATION_PREY * 10),
        ..Default::default()
    });
    w.predators.clear();
    w.tick = PREDATOR_MIGRATION_PERIOD as u64;
    w.migrate_predators();
    assert_eq!((w.predators.len(), w.migrants), (10, 10));
}

#[test]
fn мигранта_нет_пока_хищников_хватает() {
    let mut w = World::new(&WorldConfig { n_vegetarians: Some(40), ..Default::default() });
    w.predators.truncate(PREDATOR_MIGRATION_MIN);
    w.tick = PREDATOR_MIGRATION_PERIOD as u64;
    w.migrate_predators();
    assert_eq!(w.predators.len(), PREDATOR_MIGRATION_MIN);
}

// ── стратегии и гены поведения ─────────────────────────────────────────────

/// Хищник с заданным геномом.
fn predator_with(genome: PredatorGenome, energy: f64) -> Predator {
    let (s, r) = (Space::default(), Rules::default());
    Predator::new(&s, &r, genome, Some(1000.0), Some(2000.0), Some(energy), Rng::new(0))
}

const LURKER: VegetarianGenome = BASE.with(Gene::Strategy, 1.0);
const AMBUSHER: PredatorGenome = PredatorGenome::BASE.with(PredatorGene::Strategy, 1.0);

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

/// Ход хищника: (сдвиг по x, сдвиг по y, потрачено).
fn pred_move(p: &mut Predator, senses: &impl life_core::senses::PredatorSenses) -> (f64, f64, f64) {
    let (x, y, e) = (p.x, p.y, p.energy);
    p.step(&Space::default(), senses);
    (p.x - x, p.y - y, e - p.energy)
}

#[test]
fn медленный_ход_дешевле() {
    let v = veg(1000.0, 1000.0, BASE);
    assert!((v.pheno.slow_speed - v.pheno.speed * SLOW_PACE).abs() < 1e-12);
    assert!(v.pheno.slow_upkeep < v.pheno.upkeep);
    assert_eq!(v.pheno.slow_upkeep, Rules::default().upkeep(40.0, v.pheno.slow_speed, v.pheno.vision));
    let p = predator(1000.0, 2000.0, None);
    assert!(p.pheno.slow_upkeep < p.pheno.upkeep);
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

/// К еде и от хищника затаившийся идёт на полной скорости.
#[test]
fn затаившийся_к_еде_и_от_хищника_на_полной() {
    let mut v = veg(1000.0, 1000.0, LURKER);
    let (d, cost) = veg_move(&mut v, &vegetarian_senses(|_, _, _| None, |_, _, _| Some((1300.0, 1000.0))));
    assert!((d - v.pheno.speed).abs() < 1e-9 && close(cost, v.pheno.upkeep), "к еде: {d}, {cost}");

    let mut v = veg(1000.0, 1000.0, LURKER);
    let near = |_: f64, _: f64, _: f64| Some((1050.0, 1000.0, 2500.0));
    let (d, cost) = veg_move(&mut v, &vegetarian_senses(near, |_, _, _| None));
    assert!((d - v.pheno.speed).abs() < 1e-9 && close(cost, v.pheno.upkeep), "от хищника: {d}, {cost}");
    assert!(v.x < 1000.0, "бежит не от хищника");
}

/// Засадник бродит медленно и дёшево.
#[test]
fn засадник_бродит_медленно() {
    let mut p = predator_with(AMBUSHER, 30.0);
    for _ in 0..20 {
        let (dx, dy, cost) = pred_move(&mut p, &Blind);
        assert!((dx.hypot(dy) - p.pheno.slow_speed).abs() < 1e-9, "прошёл {}", dx.hypot(dy));
        assert!(close(cost, p.pheno.slow_upkeep), "потратил {cost}");
    }
}

/// Дальнюю добычу засадник не преследует, на близкую бросается рывком; сытый
/// не бросается вовсе.
#[test]
fn засадник_бросается_только_на_близкую() {
    let far = predator_senses(|_, _, _| Some(Prey { x: 1300.0, y: 2000.0, half: 20.0 }));
    let near = predator_senses(|_, _, _| Some(Prey { x: 1150.0, y: 2000.0, half: 20.0 }));

    let mut p = predator_with(AMBUSHER, 30.0);
    let (dx, dy, cost) = pred_move(&mut p, &far);
    assert!(
        (dx.hypot(dy) - p.pheno.slow_speed).abs() < 1e-9 && close(cost, p.pheno.slow_upkeep),
        "за дальней погнался"
    );

    let mut p = predator_with(AMBUSHER, 30.0);
    let (dx, dy, cost) = pred_move(&mut p, &near);
    assert!((dx - p.pheno.speed * PREDATOR_SPRINT_MULT).abs() < 1e-9 && dy.abs() < 1e-9, "рывка нет: {dx}");
    assert!(close(cost, p.pheno.upkeep + PREDATOR_SPRINT_COST), "потратил {cost}");

    let mut p = predator_with(AMBUSHER, PREDATOR_MAX_ENERGY);
    let (dx, dy, cost) = pred_move(&mut p, &near);
    assert!(
        (dx.hypot(dy) - p.pheno.slow_speed).abs() < 1e-9 && close(cost, p.pheno.slow_upkeep),
        "сытый бросился"
    );

    // стандартный за дальней гонится на обычной скорости
    let mut p = predator_with(PredatorGenome::BASE, 30.0);
    let (dx, _, _) = pred_move(&mut p, &far);
    assert!((dx - p.pheno.speed).abs() < 1e-9);
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
    let cfg = WorldConfig {
        seed: 5,
        vegetarian_strategies: vec![1.0, 1.0],
        predator_strategies: vec![1.0, 1.0],
        ..Default::default()
    };
    let w = World::new(&cfg);
    let lurkers = w.vegetarians.iter().filter(|v| v.genome[Gene::Strategy] == 1.0).count();
    assert_eq!(lurkers, w.vegetarians.len() / 2, "смесь 50/50 раздана неровно");
    let ambushers = w.predators.iter().filter(|p| p.genome[PredatorGene::Strategy] == 1.0).count();
    assert_eq!(ambushers, w.predators.len() / 2);
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
        ("predator_divide_chance", 1.5),
        ("predator_max_energy", 0.0),
        ("predator_migration", 0.5),
        ("predator_migration", -500.0),
    ] {
        assert!(r.with(key, bad).is_err(), "{key}={bad} принято");
    }
    for (key, ok) in [
        ("cost_scale", 0.0),
        ("plant_rate", 0.0),
        ("predator_divide_chance", 1.0),
        ("predator_migration", 0.0),
        ("predator_migration", 250.0),
    ] {
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
        let s = w.space;
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
        for p in &w.predators {
            assert!(p.alive && p.energy > 0.0 && p.energy <= p.pheno.max_energy + 1e-9);
            assert!(PREDATOR_DIAM <= p.x && p.x <= s.width - PREDATOR_DIAM);
            assert!(PREDATOR_DIAM <= p.y && p.y <= s.height - PREDATOR_DIAM);
        }
        assert!(w.plants.iter().all(|p| p.alive), "съеденное растение не выметено");
        let mut ids: Vec<u64> =
            w.vegetarians.iter().map(|v| v.id).chain(w.predators.iter().map(|p| p.id)).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), w.vegetarians.len() + w.predators.len(), "номера существ повторяются");
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

/// Счётчики — бухгалтерия без потерь: сколько было, плюс родилось и пришло,
/// минус съедено и умерло, равно тому, сколько есть. Пропусти мир одну смерть —
/// отчёт стал бы объяснять численность неверными причинами.
#[test]
fn счётчики_сходятся_с_численностью() {
    let mut w = World::new(&WorldConfig { seed: 3, ..Default::default() });
    let (veg0, pred0, plants0) =
        (w.vegetarians.len() as u64, w.predators.len() as u64, w.plants.len() as u64);
    for _ in 0..3000 {
        w.step();
    }
    let c = w.counters;
    assert!(c.vegetarians_born > 0 && c.vegetarians_eaten > 0 && c.plants_eaten > 0, "{c:?}");
    assert_eq!(
        w.vegetarians.len() as u64,
        veg0 + c.vegetarians_born - c.vegetarians_eaten - c.vegetarians_starved
    );
    assert_eq!(w.predators.len() as u64, pred0 + c.predators_born + w.migrants - c.predators_starved);
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
        assert_eq!(w.predators.len(), PREDATORS_AT_START * 10, "{shape:?}");
        assert!(w.vegetarians.iter().all(|v| v.x <= w.space.width && v.y <= w.space.height), "{shape:?}");
    }
    let wide = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
    assert!(wide.space.height > WORLD_HEIGHT * 3.0, "3:2 растёт и вглубь: {:?}", wide.space);

    let mut e = World::new(&WorldConfig {
        scale: 2.0,
        n_vegetarians: Some(0),
        n_predators: Some(0),
        ..Default::default()
    });
    for _ in 0..2000 {
        e.step();
    }
    assert_eq!(e.plants.len(), PLANT_MAX * 2);
}

/// Мир уже базового не строится: при ширине 60 полоса хищника переворачивалась,
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
        WorldConfig { n_vegetarians: Some(7), n_predators: Some(0), ..Default::default() },
    ] {
        let w = World::new(&cfg);
        assert_eq!(
            (w.vegetarians.len(), w.predators.len()),
            (cfg.vegetarians_at_start(), cfg.predators_at_start())
        );
    }
}

// ── производительность ─────────────────────────────────────────────────────

/// Страж от обвала скорости: мир x10 на фиксированной нагрузке 4000 травоядных,
/// 4000 растений, 100 хищников. На 400 существах (как было в Python-версии) Rust
/// и полным перебором успевал бы, а здесь перебор — десятки миллионов пар за тик.
/// Порог с большим запасом: тест ловит поломку вроде «сетка перестала работать
/// и всё стало O(n²)», а не шум машины CI: с сеткой ~2 мс, без неё ~80 мс,
/// порог 20. Растения подсыпаются каждый тик,
/// чтобы нагрузка не таяла.
#[test]
fn тик_укладывается_в_бюджет_на_фиксированной_нагрузке() {
    let mut w = World::new(&WorldConfig {
        seed: 9,
        scale: 10.0,
        n_vegetarians: Some(4000),
        n_predators: Some(100),
        ..Default::default()
    });
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
    eprintln!("  [скорость] {ms:.3} мс/тик при 4000/4000/100");
    assert!(w.vegetarians.len() > 1000, "нагрузка растаяла — замер бессмыслен");
    assert!(ms < 20.0, "тик {ms:.2} мс при 4000/4000/100: где-то перебор вместо сетки?");
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
            let mut best: Option<(f64, Creature)> = None;
            let mut consider = |d: f64, c: Creature| {
                if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, c));
                }
            };
            for v in &w.vegetarians {
                consider(
                    ((v.x - x).powi(2) + (v.y - y).powi(2)).sqrt() - v.pheno.size / 2.0,
                    Creature::Vegetarian(v.id),
                );
            }
            for p in &w.predators {
                consider(
                    ((p.x - x).powi(2) + (p.y - y).powi(2)).sqrt() - Predator::DIAM / 2.0,
                    Creature::Predator(p.id),
                );
            }
            assert_eq!(w.pick(x, y, radius), best.map(|(_, c)| c));
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
        assert!(w.predators.windows(2).all(|p| p[0].id < p[1].id), "хищники по возрастанию id");
    }
    for v in &w.vegetarians {
        assert_eq!(w.vegetarian(v.id).map(|f| f.id), Some(v.id));
    }
    for p in &w.predators {
        assert_eq!(w.predator(p.id).map(|f| f.id), Some(p.id));
    }
    assert!(w.vegetarian(u64::MAX).is_none());
}

#[test]
fn новые_правила_пересчитывают_живых_как_новорождённых() {
    let mut w = World::new(&WorldConfig { seed: 4, ..Default::default() });
    for _ in 0..200 {
        w.step();
    }
    let rules = Rules::default()
        .with("cost_scale", 3.0)
        .and_then(|r| r.with("size_power", 2.0))
        .and_then(|r| r.with("predator_max_energy", 40.0))
        .unwrap();
    w.set_rules(rules.clone());
    let space = w.space;
    for v in &w.vegetarians {
        let fresh = Vegetarian::new(&space, &rules, v.genome, Some(v.x), Some(v.y), None, Rng::new(0));
        assert_eq!(v.pheno, fresh.pheno, "фенотип живого — как у новорождённого с тем же геномом");
    }
    for p in &w.predators {
        let fresh = Predator::new(&space, &rules, p.genome, Some(p.x), Some(p.y), None, Rng::new(0));
        assert_eq!(p.pheno, fresh.pheno, "фенотип живого — как у новорождённого с тем же геномом");
        assert!(p.energy <= p.pheno.max_energy);
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
