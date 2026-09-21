//! Тесты движка — перенос по смыслу `python/tests/test_simulation.py` (тег python-final).
//! Каждый тест на регрессию закрывает баг, который уже был в Python-коде.
//! Циклов без границы нет: все прогоны ограничены числом тиков.

use life_core::config::*;
use life_core::creature::Creature;
use life_core::creature::Kinship;
use life_core::genome::creature::{GENES, Gene};
use life_core::grid::Grid;
use life_core::plant::Plant;
use life_core::rng::Rng;
use life_core::senses::{Blind, Threat, senses_from};
use life_core::{Counters, CreatureGenome, Genome, Rules, Shape, Space, World, WorldConfig};

const BASE: CreatureGenome = CreatureGenome::BASE;

fn genom(size: f64) -> CreatureGenome {
    BASE.with(Gene::Size, size)
}

/// Пустой мир: ни существ, ни растений.
fn empty_world(rules: Rules) -> World {
    let mut w = World::new(&WorldConfig { seed: 3, rules, n_creatures: Some(0), ..Default::default() });
    w.plants.clear();
    w
}

fn creature(x: f64, y: f64, g: CreatureGenome) -> Creature {
    Creature::new(&Space::default(), &Rules::default(), g, Some(x), Some(y), None, Rng::new(0))
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

/// Без существ растения упираются в потолок, а не растут вечно.
#[test]
fn растения_упираются_в_потолок() {
    let mut w = empty_world(Rules::default());
    for _ in 0..1000 {
        w.step();
    }
    assert_eq!(w.plants.len(), PLANT_MAX);
}

// ── каннибализм ────────────────────────────────────────────────────────────

/// Мир с большим существом и соседом заданного размера на заданном расстоянии.
/// Тик 1 — не тик деления: считаем только поедание.
fn cannibal_world(on: bool, small: f64, dx: f64) -> World {
    let rules = Rules::default().with("cannibalism", if on { 1.0 } else { 0.0 }).unwrap();
    let mut w = empty_world(rules);
    w.tick = 1;
    w.spawn(genom(100.0), 3000.0, 2000.0, Some(100.0));
    w.spawn(genom(small), 3000.0 + dx, 2000.0, None);
    w
}

#[test]
fn каннибал_съедает_мелкого_рядом() {
    let mut w = cannibal_world(true, 30.0, 10.0);
    w.step();
    assert_eq!(w.creatures.len(), 2, "полное здоровье не теряется за один удар");
    assert!(w.creatures[1].health < w.creatures[1].max_health());
    assert_eq!(w.counters.combat, 0);
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
        assert_eq!(w.creatures.len(), 2, "{why}");
        assert_eq!(w.counters.cannibalized, 0, "{why}");
    }
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
    assert!(c.combat > 0, "за 3000 тиков при отношении 1.2 хоть кого-то съели: {c:?}");
    let n0 = CREATURES_AT_START as u64;
    assert_eq!(w.creatures.len() as u64, n0 + c.born - c.starved - c.cannibalized - c.old_age - c.combat);
}

// ── родство и бегство ──────────────────────────────────────────────────────

/// Ребёнок знает родителя; родня — родитель, дети и братья, но не внуки.
#[test]
fn родство_наследуется() {
    let mut w = empty_world(Rules::default());
    let id = w.spawn(BASE.with(Gene::ReproThreshold, 30.0), 3000.0, 2000.0, None);
    let (s, r) = (w.space, w.rules.clone());
    let parent = &mut w.creatures[0];
    assert_eq!(parent.parent, 0, "подсаженное — без родителя");
    let mut kids = Vec::new();
    for n in 0..2 {
        parent.reproduction_wait = 0;
        parent.energy = parent.pheno.max_energy;
        let mut kid = parent.maybe_divide(&s, &r).expect("сытый родитель не поделился");
        kid.id = 100 + n; // номер выдаёт мир
        kids.push(kid);
    }
    let parent = parent.kinship();
    let (a, b) = (kids[0].kinship(), kids[1].kinship());
    assert_eq!(a.parent, id, "ребёнок помнит родителя");
    assert!(parent.kin(a) && a.kin(parent), "родитель и ребёнок — родня");
    assert!(a.kin(b), "братья — родня");
    kids[0].nourish(10000.0, &r);
    kids[0].reproduction_wait = 0;
    kids[0].energy = kids[0].pheno.max_energy;
    let mut grandchild = kids[0].maybe_divide(&s, &r).expect("сытый ребёнок не поделился");
    grandchild.id = 200;
    let grandchild = grandchild.kinship();
    assert!(a.kin(grandchild), "ребёнок и внук — родня");
    assert!(!parent.kin(grandchild) && !b.kin(grandchild), "внук деду и дяде уже чужой");
    let strangers = (Kinship { id: 7, parent: 0 }, Kinship { id: 8, parent: 0 });
    assert!(!strangers.0.kin(strangers.1), "стартовые без родителя друг другу не братья");
}

/// Мир с крупным существом размера `big` и мелким (30) на `dx` правее;
/// `kin` задаёт им родство руками.
fn threat_world(cannibals: bool, big: f64, dx: f64, kin: impl Fn(&mut World)) -> World {
    let mut w = cannibal_world(cannibals, 30.0, dx);
    let v = &mut w.creatures[0];
    v.genome = genom(big);
    v.pheno = life_core::creature::Phenotype::of(&v.genome, &w.rules, &w.space);
    kin(&mut w);
    w
}

/// Тик мира: сдвиг мелкого по x и y и бежит ли он теперь.
fn small_step(mut w: World) -> (f64, f64, bool) {
    let (x0, y0) = (w.creatures[1].x, w.creatures[1].y);
    w.step();
    let v = &w.creatures[1];
    (v.x - x0, v.y - y0, v.fleeing())
}

#[test]
fn мелкий_бежит_от_крупного_чужака() {
    // до края тела крупного 150 - 50 = 100: ближе трети зрения (133)
    let w = threat_world(true, 100.0, 150.0, |_| {});
    let speed = w.creatures[1].pheno.speed;
    let (dx, dy, fleeing) = small_step(w);
    assert!(fleeing, "мелкий не испугался");
    assert!((dx - speed).abs() < 1e-9 && dy.abs() < 1e-9, "бежал не прочь: ({dx}, {dy})");
}

#[test]
fn не_бежит_от_родни_равного_и_далёкого() {
    let parent = |w: &mut World| w.creatures[1].parent = w.creatures[0].id;
    let child = |w: &mut World| w.creatures[0].parent = w.creatures[1].id;
    let brothers = |w: &mut World| {
        w.creatures[0].parent = 999;
        w.creatures[1].parent = 999;
    };
    let strangers = |w: &mut World| w.creatures[1].parent = 999; // у крупного родителя нет
    let cases: [(World, &str); 6] = [
        (threat_world(true, 100.0, 150.0, parent), "от родителя"),
        (threat_world(true, 100.0, 150.0, child), "от своего ребёнка"),
        (threat_world(true, 100.0, 150.0, brothers), "от брата"),
        (threat_world(true, 70.0, 150.0, |_| {}), "от того, кто крупнее всего в 2.3 раза"),
        (threat_world(true, 100.0, 250.0, |_| {}), "от того, до кого 200 — дальше трети зрения"),
        (threat_world(false, 100.0, 150.0, |_| {}), "когда есть сородичей нельзя"),
    ];
    for (w, why) in cases {
        let (_, _, fleeing) = small_step(w);
        assert!(!fleeing, "бежит {why}");
    }
    let (_, _, fleeing) = small_step(threat_world(true, 100.0, 150.0, strangers));
    assert!(fleeing, "от чужого с другим родителем не бежит");
}

/// Испуг длится FLEE_TICKS тиков и тогда, когда угроза пропала из виду, —
/// даже мимо видимой еды; потом существо снова идёт к еде. Затаившийся бежит
/// на полной скорости.
#[test]
fn бежит_ещё_после_пропажи_угрозы() {
    let food = |_: f64, _: f64, _: f64| Some((900.0, 1000.0));
    let threat = Threat { id: 999, x: 960.0, y: 1000.0, gap: 20.0 };
    for g in [BASE, LURKER] {
        let mut v = creature(1000.0, 1000.0, g);
        v.health = v.max_health() * 0.1;
        let x0 = v.x;
        let (d, _) = move_once(&mut v, &senses_from(food).with_threat(threat));
        assert!(v.x > x0 && close(d, v.pheno.speed), "не побежал прочь на полной скорости");
        for t in 0..FLEE_TICKS {
            let x = v.x;
            move_once(&mut v, &senses_from(food));
            assert!(v.x > x, "бросил бежать на тике {t}");
        }
        assert!(!v.fleeing());
        let x = v.x;
        move_once(&mut v, &senses_from(food));
        assert!(v.x < x, "испуг прошёл, а к еде не идёт");
    }
    // угроза в виду, но дальше порога — к еде
    let mut v = creature(1000.0, 1000.0, BASE);
    let far = Threat { gap: v.pheno.flee + 1.0, ..threat };
    move_once(&mut v, &senses_from(food).with_threat(far));
    assert!(v.x < 1000.0 && !v.fleeing(), "испугался далёкого");
}

#[test]
fn каннибал_не_ест_родню() {
    let as_parent = |w: &mut World| w.creatures[1].parent = w.creatures[0].id;
    let as_brother = |w: &mut World| {
        w.creatures[0].parent = 999;
        w.creatures[1].parent = 999;
    };
    for (w, why) in [
        (threat_world(true, 100.0, 10.0, as_parent), "своего ребёнка"),
        (threat_world(true, 100.0, 10.0, as_brother), "брата"),
    ] {
        let mut w = w;
        w.step();
        assert_eq!(w.creatures.len(), 2, "каннибал съел {why}");
        assert_eq!(w.counters.cannibalized, 0);
    }
}

/// Мир с бегством детерминирован: сородичей видят по снимку на начало фазы.
#[test]
fn мир_с_бегством_детерминирован() {
    let rules = Rules::default().with("cannibalism", 1.0).unwrap().with("cannibal_ratio", 1.5).unwrap();
    let run = || {
        let mut w = World::new(&WorldConfig { seed: 9, rules: rules.clone(), ..Default::default() });
        let mut fled = 0;
        for _ in 0..2000 {
            w.step();
            fled += w.creatures.iter().filter(|v| v.fleeing()).count();
        }
        (w.stats(), w.counters, fled)
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b);
    assert!(a.2 > 0, "за 2000 тиков никто ни разу не испугался");
}

/// Умершее от голода на своём ходу существо не ест и не делится.
#[test]
fn умерший_от_голода_не_ест() {
    let mut w = empty_world(Rules::default());
    w.spawn(genom(40.0), 1000.0, 1000.0, None);
    let v = &mut w.creatures[0];
    v.energy = v.pheno.upkeep / 2.0; // этот ход — последний
    let (x, y) = (v.x, v.y);
    w.plants.push(Plant::at(x, y));
    w.plants.push(Plant::at(x + 5.0, y));
    w.step();
    assert!(w.creatures.is_empty());
    assert!(w.plants.iter().filter(|p| p.y == y).count() == 2, "труп съел растение");
}

/// После деления у родителя остаётся резерв (было: отдавал всё и умирал).
#[test]
fn родитель_сохраняет_резерв() {
    let (s, r) = (Space::default(), Rules::default());
    let (mut divided, mut blocked) = (0, 0);
    for share in [10.0, 30.0, 50.0, 70.0, 90.0] {
        let g = BASE.with(Gene::ReproThreshold, 30.0).with(Gene::ReproShare, share);
        let mut p = Creature::new(&s, &r, g, Some(1000.0), Some(1000.0), Some(60.0), Rng::new(0));
        p.reproduction_wait = 0;
        match p.maybe_divide(&s, &r) {
            Some(_) => {
                divided += 1;
                assert!(p.energy >= REPRO_RESERVE, "доля {share}%: осталось {}", p.energy);
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
            let mut v = creature(100.0, 100.0, g);
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
    let mut parent = Creature::new(&s, &r, g, Some(3000.0), Some(3900.0), None, Rng::new(0));
    let (mut diagonal, mut outside) = (0, 0);
    for _ in 0..300 {
        parent.reproduction_wait = 0;
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
    w.spawn(g, 3000.0, 2100.0, Some(80.0));
    let v = &w.creatures[0];
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
    assert!(w.creatures[0].y < w.creatures[0].pheno.body_lo, "съело, не выходя из слоя?");
}

/// Вне своего слоя и без еды существо возвращается домой и дальше держится в слое.
#[test]
fn без_еды_возвращается_в_слой() {
    let g = BASE.with(Gene::MinY, 50.0);
    let mut v = creature(3000.0, 500.0, g);
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
    let mut v = creature(3000.0, 2000.0, g);
    assert_eq!(v.pheno.body_lo, v.pheno.body_hi);
    let x0 = v.x;
    for _ in 0..50 {
        // Сытость ниже порога отдыха: здесь проверяется именно движение по линии.
        v.energy = v.pheno.max_energy * 0.8;
        v.step(&Blind);
        assert_eq!(v.y, v.pheno.body_lo);
    }
    assert!(v.x != x0, "существо на схлопнутом слое стоит столбом");
}

// ── стратегии и гены поведения ─────────────────────────────────────────────

const LURKER: CreatureGenome = BASE.with(Gene::Strategy, 1.0);
/// Расход, восстановленный из разницы энергий, совпадает с точностью до округления.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

/// Ход существа: (пройдено, потрачено).
fn move_once(v: &mut Creature, senses: &impl life_core::senses::Senses) -> (f64, f64) {
    let (x, y, e) = (v.x, v.y, v.energy);
    v.step(senses);
    ((v.x - x).hypot(v.y - y), e - v.energy)
}

#[test]
fn медленный_ход_дешевле() {
    let v = creature(1000.0, 1000.0, BASE);
    assert!((v.pheno.slow_speed - v.pheno.speed * SLOW_PACE).abs() < 1e-12);
    assert!(v.pheno.slow_upkeep < v.pheno.upkeep);
    assert_eq!(v.pheno.slow_upkeep, Rules::default().upkeep(40.0, v.pheno.slow_speed, v.pheno.vision));
}

/// Затаившийся без еды бродит медленно и дёшево; стандартный — на полной.
#[test]
fn затаившийся_без_еды_бродит_медленно() {
    let mut lurker = creature(1000.0, 1000.0, LURKER);
    let mut standard = creature(1000.0, 1000.0, BASE);
    for _ in 0..20 {
        let (d, cost) = move_once(&mut lurker, &Blind);
        assert!((d - lurker.pheno.slow_speed).abs() < 1e-9, "затаившийся прошёл {d}");
        assert!(close(cost, lurker.pheno.slow_upkeep), "затаившийся потратил {cost}");
        let (d, cost) = move_once(&mut standard, &Blind);
        assert!((d - standard.pheno.speed).abs() < 1e-9, "стандартный прошёл {d}");
        assert!(close(cost, standard.pheno.upkeep), "стандартный потратил {cost}");
    }
}

/// К еде затаившийся идёт на полной скорости.
#[test]
fn затаившийся_к_еде_на_полной() {
    let mut v = creature(1000.0, 1000.0, LURKER);
    let (d, cost) = move_once(&mut v, &senses_from(|_, _, _| Some((1300.0, 1000.0))));
    assert!((d - v.pheno.speed).abs() < 1e-9 && close(cost, v.pheno.upkeep), "к еде: {d}, {cost}");
}

/// Стратегия наследуется и изредка мутирует в другую.
#[test]
fn стратегия_мутирует_изредка() {
    let (s, r) = (Space::default(), Rules::default());
    let g = BASE.with(Gene::ReproThreshold, 30.0);
    let mut parent = Creature::new(&s, &r, g, Some(3000.0), Some(1000.0), None, Rng::new(7));
    let n = 2000;
    let mut switched = 0;
    for _ in 0..n {
        parent.reproduction_wait = 0;
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
    let cfg = WorldConfig { seed: 5, strategies: vec![1.0, 1.0], ..Default::default() };
    let w = World::new(&cfg);
    let lurkers = w.creatures.iter().filter(|v| v.genome[Gene::Strategy] == 1.0).count();
    assert_eq!(lurkers, w.creatures.len() / 2, "смесь 50/50 раздана неровно");
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
        for v in &w.creatures {
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
        let mut ids: Vec<u64> = w.creatures.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), w.creatures.len(), "номера существ повторяются");
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
    let (n0, plants0) = (w.creatures.len() as u64, w.plants.len() as u64);
    for _ in 0..3000 {
        w.step();
    }
    let c = w.counters;
    assert!(c.born > 0 && c.combat > 0 && c.starved > 0, "{c:?}");
    assert_eq!(w.creatures.len() as u64, n0 + c.born - c.starved - c.cannibalized - c.old_age - c.combat);
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
        assert_eq!(w.creatures.len(), CREATURES_AT_START * 10, "{shape:?}");
        assert!(w.creatures.iter().all(|v| v.x <= w.space.width && v.y <= w.space.height), "{shape:?}");
    }
    let wide = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
    assert!(wide.space.height > WORLD_HEIGHT * 3.0, "3:2 растёт и вглубь: {:?}", wide.space);

    let mut e = World::new(&WorldConfig { scale: 2.0, n_creatures: Some(0), ..Default::default() });
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
        WorldConfig { n_creatures: Some(7), ..Default::default() },
    ] {
        let w = World::new(&cfg);
        assert_eq!(w.creatures.len(), cfg.creatures_at_start());
    }
}

// ── производительность ─────────────────────────────────────────────────────

/// Страж от обвала скорости: мир x10 на фиксированной нагрузке 4000 существ
/// и 4000 растений. На 400 существах (как было в Python-версии) Rust
/// и полным перебором успевал бы, а здесь перебор — десятки миллионов пар за тик.
/// Порог с большим запасом: тест ловит поломку вроде «сетка перестала работать
/// и всё стало O(n²)», а не шум машины CI: с сеткой ~2 мс, без неё ~80 мс,
/// порог 20. Растения подсыпаются каждый тик,
/// чтобы нагрузка не таяла.
#[test]
fn тик_укладывается_в_бюджет_на_фиксированной_нагрузке() {
    let mut w =
        World::new(&WorldConfig { seed: 9, scale: 10.0, n_creatures: Some(4000), ..Default::default() });
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
    assert!(w.creatures.len() > 1000, "нагрузка растаяла — замер бессмыслен");
    assert!(ms < 20.0, "тик {ms:.2} мс при 4000/4000: где-то перебор вместо сетки?");
}

// ── игра: выбор, слежение, правила на ходу ──────────────────────────────────

#[test]
fn выбор_кликом_совпадает_с_перебором_в_живом_мире() {
    let mut w = World::new(&WorldConfig { seed: 5, ..Default::default() });
    for _ in 0..300 {
        w.step();
    }
    assert!(w.creatures.len() > 20);
    // Клики по сетке точек: ответ — ближайший по краю тела среди тех, кто ближе radius.
    let radius = 15.0;
    let mut hits = 0;
    for i in 0..60 {
        for j in 0..40 {
            let (x, y) = (i as f64 * 100.0 + 13.0, j as f64 * 100.0 + 7.0);
            let mut best: Option<(f64, u64)> = None;
            for v in &w.creatures {
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
        assert!(w.creatures.windows(2).all(|p| p[0].id < p[1].id), "существа по возрастанию id");
    }
    for v in &w.creatures {
        assert_eq!(w.creature(v.id).map(|f| f.id), Some(v.id));
    }
    assert!(w.creature(u64::MAX).is_none());
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
    for v in &w.creatures {
        let fresh = life_core::creature::Phenotype::at_size(&v.genome, &rules, &space, v.pheno.size);
        assert_eq!(v.pheno, fresh, "правила сохраняют фактический размер");
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

#[test]
fn выключение_каннибализма_сбрасывает_испуг() {
    let mut w = threat_world(true, 100.0, 150.0, |_| {});
    w.step();
    assert!(w.creatures[1].fleeing());
    w.set_rules(Rules::default());
    assert!(w.creatures.iter().all(|v| !v.fleeing()));
}

#[test]
fn совпавшая_угроза_не_обездвиживает() {
    let mut v = creature(1000.0, 1000.0, BASE);
    let senses = senses_from(|_, _, _| None).with_threat(Threat { id: 999, x: v.x, y: v.y, gap: -50.0 });
    v.health = v.max_health() * 0.1;
    let before = (v.x, v.y);
    v.step(&senses);
    assert!((v.x - before.0).hypot(v.y - before.1) > 0.0);
    assert!(v.x.is_finite() && v.y.is_finite());
}
