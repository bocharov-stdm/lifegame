use life_core::creature::Intent;
use life_core::flock;
use life_core::genome::creature::Gene;
use life_core::kin_grace::Grace;
use life_core::territory::{Area, State, steer};
use life_core::{CreatureGenome, Rules, World, WorldConfig};

fn world() -> World {
    World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("cannibalism", 1.0).unwrap(),
        ..Default::default()
    })
}

#[test]
fn три_режима_территории_дают_разный_момент_обороны() {
    for (mode, first) in [(0.0, false), (1.0, false), (2.0, true)] {
        let mut w = world();
        let defender = CreatureGenome::BASE.with(Gene::Territoriality, mode);
        let home = w.spawn(defender, 1000.0, 1000.0, Some(100.0));
        w.spawn(defender, 1020.0, 1000.0, Some(100.0));
        let stranger = w.spawn(CreatureGenome::BASE, 1010.0, 1000.0, Some(100.0));
        let tag = w.creatures[0].flock;
        w.creatures[1].flock = tag;
        flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        let mut territory = State::default();
        let targets = territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
        assert_eq!(targets[0] == Some(stranger), first, "режим {mode}");
        assert_eq!(w.flocks[&tag].territory_radius > 0.0, mode != 0.0);
        assert_eq!(territory.owner(1010.0, 1000.0).is_some(), mode != 0.0);
        let targets = territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 30);
        assert_eq!(targets[0] == Some(stranger), mode != 0.0, "режим {mode} после предупреждения");
        assert_eq!(w.creatures[0].id, home);
    }
}

#[test]
fn разделённые_стаи_не_считаются_вторженцами_пока_действует_защита() {
    let mut w = world();
    let defender = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    w.spawn(defender, 1000.0, 1000.0, Some(100.0));
    w.spawn(defender, 1020.0, 1000.0, Some(100.0));
    let stranger = w.spawn(CreatureGenome::BASE, 1010.0, 1000.0, Some(100.0));
    let tag = w.creatures[0].flock;
    let other = w.creatures[2].flock;
    w.creatures[1].flock = tag;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut grace = Grace::default();
    grace.register(tag, other, 0);
    let mut territory = State::default();
    for tick in [0, 300, 600] {
        let targets = territory.prepare_with_grace(&mut w.flocks, &mut w.creatures, &w.space, tick, &grace);
        assert_eq!(targets[0], None);
        assert_eq!(w.flocks[&tag].warned, 0);
    }
    let targets = territory.prepare_with_grace(&mut w.flocks, &mut w.creatures, &w.space, 601, &grace);
    assert_eq!(targets[0], Some(stranger));
    assert_eq!(w.flocks[&tag].warned, 1);
}

#[test]
fn граница_перекрытия_принадлежит_ближайшему_центру_с_устойчивой_ничьей() {
    let mut w = world();
    for x in [1000.0, 1000.0, 1100.0, 1100.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(100.0));
    }
    let left = w.creatures[0].flock;
    let right = w.creatures[2].flock;
    w.creatures[1].flock = left;
    w.creatures[3].flock = right;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    assert_eq!(territory.owner(1050.0, 1000.0).unwrap().flock, left);
    assert_eq!(territory.owner(1100.0, 1000.0).unwrap().flock, right);
}

#[test]
fn вторжение_предупреждает_тридцать_тиков_а_удар_действует_сразу() {
    let mut w = world();
    let shooter = CreatureGenome::BASE.with(Gene::Shooter, 1.0).with(Gene::FirePreference, 100.0);
    for (x, genome) in
        [(1000.0, shooter), (1020.0, shooter), (1010.0, CreatureGenome::BASE), (3000.0, CreatureGenome::BASE)]
    {
        w.spawn(genome, x, 1000.0, Some(100.0));
    }
    let home = w.creatures[0].flock;
    let outsider = w.creatures[2].flock;
    w.creatures[1].flock = home;
    w.creatures[3].flock = outsider;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0)[0], None);
    assert_eq!(w.flocks[&home].warned, 0);
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 29)[0], None);
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 30)[0], Some(w.creatures[2].id));
    assert_eq!(w.flocks[&home].warned, 1);
    w.creatures[2].x = 1400.0;
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 31);
    assert!(territory.encounters.is_empty());
    // Удар по своему даёт право ответить и по врагу сразу снаружи контура.
    w.creatures[2].x = 1140.0;
    territory.attacks.insert((home, w.creatures[2].id, 32));
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 32)[0], Some(w.creatures[2].id));
}

#[test]
fn взрослая_особь_уходит_по_одной_а_детёныш_остаётся() {
    let mut w = world();
    for i in 0..51 {
        w.spawn(CreatureGenome::BASE, 1000.0 + i as f64, 1000.0, Some(100.0));
    }
    let tag = w.creatures[0].flock;
    for v in &mut w.creatures {
        v.flock = tag;
    }
    w.creatures[0].genome = w.creatures[0].genome.with(Gene::Size, 80.0);
    assert!(!w.creatures[0].adult());
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let original_next = w.next_flock;
    let tick = (1..100)
        .find(|&t| {
            let roll = (life_core::rng::mix(tag ^ life_core::rng::mix(t)) >> 11) as f64 / (1u64 << 53) as f64;
            roll < 0.06
        })
        .unwrap();
    assert_eq!(flock::departures(&mut w.creatures, &mut w.next_flock, tick), 1);
    assert_eq!(w.creatures[0].flock, tag);
    assert_eq!(w.creatures[50].flock, original_next);
    assert_eq!(flock::departures(&mut w.creatures, &mut w.next_flock, tick + 1), 0);
}

#[test]
fn пришелец_выходит_из_зоны_и_не_застревает_у_края_мира() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 5950.0, 2000.0, Some(100.0));
    let v = &mut w.creatures[0];
    v.mind.social.territory_avoid = Some(Area { flock: 99, x: 5800.0, y: 2000.0, radius: 320.0 });
    let intent = Intent { tx: 5800.0, ty: 2000.0, slow: false, attack: None };
    let moved = steer(v, intent);
    assert!((moved.tx - 5800.0).hypot(moved.ty - 2000.0) > 150.0);
    assert!(moved.tx <= v.pheno.x_hi && moved.ty <= v.pheno.y_hi);
}

#[test]
fn мирный_режим_сразу_очищает_территориальную_память() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    w.territory.encounters.insert((1, 2), 7);
    w.territory.attacks.insert((1, 2, 7));
    w.creatures[0].mind.social.territory_avoid = Some(Area { flock: 2, x: 1000.0, y: 1000.0, radius: 120.0 });
    w.set_rules(Rules::default());
    assert!(w.territory.encounters.is_empty() && w.territory.attacks.is_empty());
    assert!(w.creatures[0].mind.social.territory_avoid.is_none());
}

#[test]
fn после_очистки_сетка_не_оставляет_ложного_владельца() {
    let mut w = world();
    for x in [1000.0, 1020.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(100.0));
    }
    let flock = w.creatures[0].flock;
    w.creatures[1].flock = flock;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    assert!(territory.owner(1010.0, 1000.0).is_some());
    territory.clear();
    assert!(territory.owner(1010.0, 1000.0).is_none());
}

#[test]
fn обычный_взрослый_защищает_территорию_вблизи() {
    let mut w = world();
    for x in [1000.0, 1020.0, 1080.0] {
        w.spawn(CreatureGenome::BASE, x, 1000.0, Some(100.0));
    }
    let flock = w.creatures[0].flock;
    w.creatures[1].flock = flock;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    let targets = territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 30);
    assert_eq!(targets[0], Some(w.creatures[2].id));
    let defended = steer(&mut w.creatures[0], Intent { tx: 900.0, ty: 1000.0, slow: true, attack: None });
    assert_eq!(defended.attack, Some(w.creatures[2].id));
    assert_eq!((defended.tx, defended.ty), (1080.0, 1000.0));
}

#[test]
fn стрелок_без_резерва_сближается_для_ближней_защиты() {
    let mut w = world();
    let shooter = CreatureGenome::BASE
        .with(Gene::Shooter, 1.0)
        .with(Gene::FirePreference, 100.0)
        .with(Gene::FireReserve, 80.0);
    w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
    w.spawn(shooter, 1020.0, 1000.0, Some(100.0));
    w.spawn(CreatureGenome::BASE, 1080.0, 1000.0, Some(100.0));
    let flock = w.creatures[0].flock;
    w.creatures[1].flock = flock;
    w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.6;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 30);
    let defended = steer(&mut w.creatures[0], Intent { tx: 900.0, ty: 1000.0, slow: false, attack: None });
    assert_eq!(defended.attack, Some(w.creatures[2].id));
    assert_eq!((defended.tx, defended.ty), (1080.0, 1000.0));
}

#[test]
fn из_угла_мира_пришелец_выходит_без_двухточечного_цикла() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 5980.0, 20.0, Some(100.0));
    let v = &mut w.creatures[0];
    let area = Area { flock: 99, x: 5900.0, y: 100.0, radius: 120.0 };
    let safe = area.radius + v.pheno.half + 4.0;
    for _ in 0..40 {
        v.mind.social.territory_avoid = Some(area);
        let next = steer(v, Intent { tx: area.x, ty: area.y, slow: false, attack: None });
        let dx = next.tx - v.x;
        let dy = next.ty - v.y;
        let d = dx.hypot(dy);
        if d > 0.0 {
            let step = d.min(v.pheno.speed);
            v.x += dx / d * step;
            v.y += dy / d * step;
        }
        if (v.x - area.x).hypot(v.y - area.y) >= safe {
            return;
        }
    }
    panic!("пришелец застрял внутри территории в углу мира");
}

#[test]
fn сторона_обхода_не_меняется_из_за_смены_личной_цели() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 950.0, 1000.0, Some(100.0));
    let v = &mut w.creatures[0];
    let area = Area { flock: 99, x: 1100.0, y: 1000.0, radius: 120.0 };
    v.mind.social.territory_avoid = Some(area);
    let first = steer(v, Intent { tx: 1300.0, ty: 1010.0, slow: false, attack: None });
    let side = v.mind.social.territory_side;
    assert!(side.is_some());
    let next = steer(v, Intent { tx: 1300.0, ty: 990.0, slow: false, attack: None });
    assert_eq!(v.mind.social.territory_side, side);
    assert_eq!((first.tx, first.ty), (next.tx, next.ty));
}

#[test]
fn перекрытые_зоны_у_края_не_меняют_курс_выхода_при_смене_владельца() {
    let mut w = world();
    for x in [100.0, 100.0, 250.0, 250.0, 175.0] {
        w.spawn(CreatureGenome::BASE, x, 40.0, Some(100.0));
    }
    let left = w.creatures[0].flock;
    let right = w.creatures[2].flock;
    w.creatures[1].flock = left;
    w.creatures[3].flock = right;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let mut territory = State::default();
    let traveler = 4;
    let bottom = w.creatures[traveler].pheno.y_lo;
    assert_eq!(w.creatures[traveler].y, bottom);
    w.creatures[traveler].mind.social.territory_escape = Some((0.0, -1.0));
    let mut crossed_owner = false;
    for tick in 0..80 {
        territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, tick);
        let v = &mut w.creatures[traveler];
        let owner = territory.owner(v.x, v.y);
        crossed_owner |= owner.is_some_and(|a| a.flock == right);
        let safe = [left, right].into_iter().all(|tag| {
            let f = &w.flocks[&tag];
            (v.x - f.goal.x).hypot(v.y - f.goal.y) >= f.territory_radius + v.pheno.half + 4.0
        });
        if safe {
            assert!(crossed_owner, "выход должен пройти через вторую область");
            return;
        }
        let next = steer(v, Intent { tx: 175.0, ty: 40.0, slow: false, attack: None });
        let (dx, dy) = (next.tx - v.x, next.ty - v.y);
        let distance = dx.hypot(dy);
        if distance > 0.0 {
            let step = distance.min(v.pheno.speed);
            v.x = (v.x + dx / distance * step).clamp(v.pheno.x_lo, v.pheno.x_hi);
            v.y = (v.y + dy / distance * step).clamp(v.pheno.y_lo, v.pheno.y_hi);
        }
    }
    panic!("пришелец не вышел из перекрытых областей за 80 шагов");
}

#[test]
fn касательная_на_границе_мира_меняет_сторону_вместо_остановки() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 150.0, 40.0, Some(100.0));
    let v = &mut w.creatures[0];
    assert_eq!(v.y, v.pheno.y_lo);
    let area = Area { flock: 91, x: 300.0, y: v.pheno.y_lo, radius: 120.0 };
    assert_eq!(life_core::rng::mix(v.id ^ area.flock) & 1, 0);
    v.mind.social.territory_avoid = Some(area);
    for _ in 0..15 {
        let next = steer(v, Intent { tx: 450.0, ty: v.pheno.y_lo, slow: false, attack: None });
        let (dx, dy) = (next.tx - v.x, next.ty - v.y);
        let distance = dx.hypot(dy);
        assert!(distance > 1e-9, "касательная в край мира обнулила шаг");
        let step = distance.min(v.pheno.speed);
        v.x += dx / distance * step;
        v.y += dy / distance * step;
        assert!((v.x - area.x).hypot(v.y - area.y) >= area.radius + v.pheno.half + 4.0 - 1e-9);
    }
    assert_eq!(v.mind.social.territory_side, Some((area.flock, -1)));
}

#[test]
fn подтвержденное_нападение_вызывает_залп_и_оставляет_труп_для_следующего_тика() {
    let mut w = World::new(&WorldConfig {
        n_creatures: Some(0),
        rules: Rules::default().with("cannibalism", 1.0).unwrap().with("plant_rate", 0.0).unwrap(),
        ..Default::default()
    });
    let shooter = CreatureGenome::BASE
        .with(Gene::Shooter, 1.0)
        .with(Gene::FirePreference, 100.0)
        .with(Gene::FireReserve, 0.0);
    for x in [1000.0, 1020.0, 1040.0] {
        w.spawn(shooter, x, 1000.0, Some(100.0));
    }
    let flock = w.creatures[0].flock;
    for v in &mut w.creatures[..3] {
        v.flock = flock;
    }
    let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(120.0));
    w.creatures[3].health = 0.8;
    w.territory.attacks.insert((flock, enemy, 0));
    w.step();
    assert_eq!(w.counters.ranged_shots, 3);
    assert_eq!(w.counters.territorial_fights, 3);
    assert_eq!(w.counters.combat, 1);
    assert_eq!(w.corpses.len(), 1);
    assert_eq!(w.counters.meat_bites, 0);
    let corpse = &w.corpses[0];
    let v = &mut w.creatures[0];
    v.x = corpse.x;
    v.y = corpse.y;
    v.energy = 20.0;
    w.step();
    assert!(w.counters.meat_bites > 0);
    assert!(w.corpses[0].remaining < w.corpses[0].initial);
}
