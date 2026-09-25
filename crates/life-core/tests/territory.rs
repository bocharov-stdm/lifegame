use life_core::creature::Intent;
use life_core::flock;
use life_core::flock::Circle;
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
        assert!(w.flocks[&tag].circle.is_some());
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
    // Moderate circles are placed apart; overlap them on purpose to test ownership.
    w.flocks.get_mut(&left).unwrap().circle = Some(Circle { x: 1000.0, y: 1000.0, radius: 120.0 });
    w.flocks.get_mut(&right).unwrap().circle = Some(Circle { x: 1100.0, y: 1000.0, radius: 120.0 });
    let mut territory = State::default();
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    assert_eq!(territory.owner(1050.0, 1000.0).unwrap().flock, left);
    assert_eq!(territory.owner(1100.0, 1000.0).unwrap().flock, right);
}

#[test]
fn intrusion_warns_after_thirty_ticks_a_strike_at_once() {
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
    // A pair's own circle is as wide as it sees; a smaller one keeps the distances short.
    w.flocks.get_mut(&home).unwrap().circle = Some(Circle { x: 1010.0, y: 1000.0, radius: 120.0 });
    let mut territory = State::default();
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0)[0], None);
    assert_eq!(w.flocks[&home].warned, 0);
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 29)[0], None);
    assert_eq!(territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 30)[0], Some(w.creatures[2].id));
    assert_eq!(w.flocks[&home].warned, 1);
    w.creatures[2].x = 1400.0;
    territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 31);
    assert!(territory.encounters.is_empty());
    // A strike on a member lets the flock answer at once, even just outside the border.
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
fn overlapping_areas_at_the_edge_keep_one_course_out() {
    let mut w = world();
    for x in [100.0, 100.0, 250.0, 250.0, 175.0] {
        w.spawn(CreatureGenome::BASE, x, 40.0, Some(100.0));
    }
    let left = w.creatures[0].flock;
    let right = w.creatures[2].flock;
    w.creatures[1].flock = left;
    w.creatures[3].flock = right;
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    // Two overlapping areas at the edge of the world, as circles drawn by hand.
    let bottom = w.creatures[0].pheno.y_lo;
    w.flocks.get_mut(&left).unwrap().circle = Some(Circle { x: 100.0, y: bottom, radius: 120.0 });
    w.flocks.get_mut(&right).unwrap().circle = Some(Circle { x: 250.0, y: bottom, radius: 120.0 });
    let mut territory = State::default();
    let traveler = 4;
    let bottom = w.creatures[traveler].pheno.y_lo;
    assert_eq!(w.creatures[traveler].y, bottom);
    // its way out points into the wall of the world
    w.creatures[traveler].mind.social.territory_escape = Some((0.0, -1.0));
    let mut last: Option<(f64, f64)> = None;
    for tick in 0..80 {
        territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, tick);
        let v = &mut w.creatures[traveler];
        let safe = [left, right].into_iter().all(|tag| {
            let c = w.flocks[&tag].circle.unwrap();
            (v.x - c.x).hypot(v.y - c.y) >= c.radius + v.pheno.half + 4.0
        });
        if safe {
            return;
        }
        let next = steer(v, Intent { tx: 175.0, ty: 40.0, slow: false, attack: None });
        let (dx, dy) = (next.tx - v.x, next.ty - v.y);
        let distance = dx.hypot(dy);
        assert!(distance > 0.0, "tick {tick}: stuck at the wall");
        if let Some((px, py)) = last {
            assert!(px * dx + py * dy > 0.0, "tick {tick}: the course out turned back");
        }
        last = Some((dx, dy));
        let step = distance.min(v.pheno.speed);
        v.x = (v.x + dx / distance * step).clamp(v.pheno.x_lo, v.pheno.x_hi);
        v.y = (v.y + dy / distance * step).clamp(v.pheno.y_lo, v.pheno.y_hi);
    }
    panic!("did not leave the overlapping areas in 80 steps");
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

/// Two members of a flock of `genome` at (x, y) and (x + 10, y); returns the tag.
fn pair(w: &mut World, genome: CreatureGenome, x: f64, y: f64) -> u64 {
    w.spawn(genome, x, y, Some(100.0));
    w.spawn(genome, x + 10.0, y, Some(100.0));
    let n = w.creatures.len();
    let tag = w.creatures[n - 2].flock;
    w.creatures[n - 1].flock = tag;
    tag
}

#[test]
fn a_moderate_border_holds_only_in_sight_of_a_member_a_hard_one_always() {
    for (mode, unseen_respected) in [(1.0, false), (2.0, true)] {
        let mut w = world();
        let tag = pair(&mut w, CreatureGenome::BASE.with(Gene::Territoriality, mode), 2400.0, 1000.0);
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
        // the border is within the loner's sight, the members are not
        w.flocks.get_mut(&tag).unwrap().circle = Some(Circle { x: 1900.0, y: 1000.0, radius: 600.0 });
        let mut territory = State::default();
        territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
        let avoided = |w: &World| w.creatures[2].mind.social.territory_avoid.map(|a| a.flock);
        assert_eq!(avoided(&w) == Some(tag), unseen_respected, "mode {mode}, members out of sight");
        // a member comes into sight
        w.creatures[0].x = 1350.0;
        territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 1);
        assert_eq!(avoided(&w), Some(tag), "mode {mode}, a member in sight");
    }
}

#[test]
fn a_starving_creature_crosses_a_border() {
    let mut w = world();
    w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
    let v = &mut w.creatures[0];
    let area = Area { flock: 99, x: 1300.0, y: 1000.0, radius: 200.0 };
    let into = Intent { tx: 1300.0, ty: 1000.0, slow: false, attack: None };
    v.mind.social.territory_avoid = Some(area);
    let fed = steer(v, into);
    assert!((fed.tx - into.tx).hypot(fed.ty - into.ty) > 1.0, "a fed creature walks around");
    v.energy = v.pheno.max_energy * 0.2;
    v.mind.social.territory_side = None;
    let starving = steer(v, into);
    assert_eq!((starving.tx, starving.ty), (into.tx, into.ty), "hunger outweighs the border");
}

#[test]
fn fighters_of_a_battle_strike_other_flocks_of_it_but_not_kin_juveniles_or_the_calm() {
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let a = pair(&mut w, hard, 1000.0, 1000.0);
    let b = pair(&mut w, hard, 1150.0, 1000.0);
    let none = pair(&mut w, CreatureGenome::BASE.with(Gene::Territoriality, 0.0), 1000.0, 1150.0);
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    for (tag, x, y) in [(a, 1005.0, 1000.0), (b, 1155.0, 1000.0), (none, 1005.0, 1150.0)] {
        w.flocks.get_mut(&tag).unwrap().circle = Some(Circle { x, y, radius: 60.0 });
    }
    for tag in [a, b, none] {
        w.flocks.get_mut(&tag).unwrap().battle = Some(7);
    }
    let mut territory = State::default();
    let targets = territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 0);
    let flock_of = |id: u64| w.creatures.iter().find(|v| v.id == id).unwrap().flock;
    // an adult of a hard flock strikes the nearest adult of another flock of the battle
    let t0 = targets[0].expect("a fighter has a target");
    assert!(flock_of(t0) == b || flock_of(t0) == none);
    // a flock without territoriality only strikes back
    assert_eq!(targets[4], None);
    assert_eq!(targets[5], None);
    // not in the same battle: no target
    w.flocks.get_mut(&b).unwrap().battle = Some(8);
    w.flocks.get_mut(&none).unwrap().battle = None;
    let targets = territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, 1);
    assert_eq!(targets[0], None);
}

#[test]
fn a_battle_gathers_touching_flocks_lets_the_beaten_go_and_ends_in_time() {
    use life_core::battle::{BATTLE_TICKS, Battles, CALM_TICKS};
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let mut tags = Vec::new();
    // a row of three touching circles and a fourth far away
    for x in [1000.0, 1400.0, 1800.0, 4500.0] {
        let tag = pair(&mut w, hard, x, 1000.0);
        w.spawn(hard, x + 20.0, 1000.0, Some(100.0));
        w.spawn(hard, x + 30.0, 1000.0, Some(100.0));
        let n = w.creatures.len();
        w.creatures[n - 2].flock = tag;
        w.creatures[n - 1].flock = tag;
        tags.push(tag);
    }
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    for (&tag, x) in tags.iter().zip([1000.0, 1400.0, 1800.0, 4500.0]) {
        w.flocks.get_mut(&tag).unwrap().circle = Some(Circle { x, y: 1000.0, radius: 200.0 });
    }
    w.flocks.get_mut(&tags[1]).unwrap().cornered = true;
    let mut battles = Battles::default();
    let grace = Grace::default();
    let out = battles.update(&mut w.flocks, &w.creatures, &w.space, 0, &grace);
    assert_eq!(out.started, 1);
    let battle = battles.active[0].clone();
    assert_eq!(battle.sides.keys().copied().collect::<Vec<_>>(), tags[..3].to_vec(), "all touching flocks");
    assert!(tags[..3].iter().all(|t| w.flocks[t].battle == Some(battle.id)));
    assert_eq!(w.flocks[&tags[3]].battle, None);
    // the first flock loses three of its four adults: it leaves and moves away
    let first = tags[0];
    let mut lost = 0;
    for v in w.creatures.iter_mut().filter(|v| v.flock == first).take(3) {
        v.alive = false;
        lost += 1;
    }
    assert_eq!(lost, 3);
    w.flocks.get_mut(&tags[1]).unwrap().cornered = false;
    let out = battles.update(&mut w.flocks, &w.creatures, &w.space, 1, &grace);
    assert_eq!(out.retreats, 1);
    assert_eq!(w.flocks[&first].battle, None);
    assert_eq!(w.flocks[&first].calm, CALM_TICKS);
    assert!(w.flocks[&first].moving_to.is_some(), "the beaten flock moves away");
    assert_eq!(battles.active[0].sides.len(), 2);
    // the others fight until the time runs out
    battles.update(&mut w.flocks, &w.creatures, &w.space, BATTLE_TICKS, &grace);
    assert!(battles.active.is_empty());
    assert!(tags[1..3].iter().all(|t| w.flocks[t].battle.is_none() && w.flocks[t].calm == CALM_TICKS));
}

#[test]
fn a_battle_skips_a_flock_under_grace() {
    use life_core::battle::Battles;
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let a = pair(&mut w, hard, 1000.0, 1000.0);
    let b = pair(&mut w, hard, 1400.0, 1000.0);
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    w.flocks.get_mut(&a).unwrap().circle = Some(Circle { x: 1000.0, y: 1000.0, radius: 200.0 });
    w.flocks.get_mut(&b).unwrap().circle = Some(Circle { x: 1400.0, y: 1000.0, radius: 200.0 });
    w.flocks.get_mut(&a).unwrap().cornered = true;
    let mut grace = Grace::default();
    grace.register(a, b, 0);
    let mut battles = Battles::default();
    let out = battles.update(&mut w.flocks, &w.creatures, &w.space, 10, &grace);
    assert_eq!(out.started, 0);
    assert!(w.flocks[&a].calm > 0, "with nobody to fight, the flock moves next time");
}

// ── Battles, retreats, parental cover and moving out of an empty circle ─────────────────────────
// Regressions for the review of add7f96–8e94b91: each of these failed before its fix.

/// A flock of `n` adults of `genome` in a row from (x, y); returns the tag.
fn flock_at(w: &mut World, genome: CreatureGenome, n: usize, x: f64, y: f64) -> u64 {
    let first = w.creatures.len();
    for i in 0..n {
        w.spawn(genome, x + 10.0 * i as f64, y, Some(100.0));
    }
    let tag = w.creatures[first].flock;
    for v in &mut w.creatures[first..] {
        v.flock = tag;
        v.reproduction_wait = 1_000_000;
    }
    tag
}

/// Circles of radius 200 centred on the given x at y = 1000.
fn circles_at(w: &mut World, row: &[(u64, f64)]) {
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    for &(tag, x) in row {
        w.flocks.get_mut(&tag).unwrap().circle = Some(Circle { x, y: 1000.0, radius: 200.0 });
    }
}

#[test]
fn a_beaten_flock_left_as_a_young_family_still_moves_away() {
    use life_core::battle::Battles;
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let winner = flock_at(&mut w, hard, 4, 1000.0, 1000.0);
    let beaten = flock_at(&mut w, hard, 5, 1400.0, 1000.0);
    circles_at(&mut w, &[(winner, 1000.0), (beaten, 1400.0)]);
    w.flocks.get_mut(&winner).unwrap().cornered = true;
    let mut battles = Battles::default();
    let grace = Grace::default();
    assert_eq!(battles.update(&mut w.flocks, &w.creatures, &w.space, 0, &grace).started, 1);
    // the beaten flock loses three of its five adults: the two left are a young family
    for v in w.creatures.iter_mut().filter(|v| v.flock == beaten).take(3) {
        v.alive = false;
    }
    w.creatures.retain(|v| v.alive);
    w.flocks.get_mut(&winner).unwrap().cornered = false;
    assert_eq!(battles.update(&mut w.flocks, &w.creatures, &w.space, 1, &grace).retreats, 1);
    assert!(w.flocks[&beaten].moving_to.is_some(), "a retreat is planned");
    for _ in 0..600 {
        flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
        for v in &mut w.creatures {
            // the members keep with their circles, and the winners see food at home
            let c = v.circle.unwrap();
            (v.x, v.y) = (c.x, c.y);
            v.mind.social.personal_food = Some((v.x, v.y));
        }
    }
    let (home, away) = (w.flocks[&winner].circle.unwrap(), w.flocks[&beaten].circle.unwrap());
    let gap = (away.x - home.x).hypot(away.y - home.y) - away.radius - home.radius;
    assert!(gap > 200.0, "the beaten family stayed at the winners' border: gap {gap:.0}");
}

#[test]
fn a_battle_ends_when_the_remaining_sides_may_not_strike_each_other() {
    use life_core::battle::Battles;
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let a = flock_at(&mut w, hard, 4, 1000.0, 1000.0);
    let b = flock_at(&mut w, hard, 4, 1400.0, 1000.0);
    let c = flock_at(&mut w, hard, 4, 1800.0, 1000.0);
    circles_at(&mut w, &[(a, 1000.0), (b, 1400.0), (c, 1800.0)]);
    // a and c split from one family a moment ago; b is cornered between them
    let mut grace = Grace::default();
    grace.register(a, c, 0);
    w.flocks.get_mut(&b).unwrap().cornered = true;
    let mut battles = Battles::default();
    battles.update(&mut w.flocks, &w.creatures, &w.space, 0, &grace);
    assert_eq!(battles.active[0].sides.len(), 3, "the battle gathers both neighbours of b");
    // b loses three of its four adults and leaves
    for v in w.creatures.iter_mut().filter(|v| v.flock == b).take(3) {
        v.alive = false;
    }
    w.creatures.retain(|v| v.alive);
    w.flocks.get_mut(&b).unwrap().cornered = false;
    assert_eq!(battles.update(&mut w.flocks, &w.creatures, &w.space, 1, &grace).retreats, 1);
    battles.update(&mut w.flocks, &w.creatures, &w.space, 2, &grace);
    // a and c may not strike each other, yet both stay held: no move, however hungry or squeezed
    assert!(battles.active.is_empty(), "a and c still hold a battle they cannot fight");
    assert!([a, c].iter().all(|t| w.flocks[t].battle.is_none()));
}

#[test]
fn a_flock_without_adults_is_not_held_in_a_battle() {
    use life_core::battle::Battles;
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let cornered = flock_at(&mut w, hard, 4, 1000.0, 1000.0);
    // three growing children: fighters strike adults only, and they brought none to lose
    let young = flock_at(&mut w, hard, 3, 1400.0, 1000.0);
    for v in w.creatures.iter_mut().filter(|v| v.flock == young) {
        v.genome = v.genome.with(Gene::Size, 200.0);
    }
    assert!(w.creatures.iter().filter(|v| v.flock == young).all(|v| !v.adult()));
    circles_at(&mut w, &[(cornered, 1000.0), (young, 1400.0)]);
    w.flocks.get_mut(&cornered).unwrap().cornered = true;
    let mut battles = Battles::default();
    let grace = Grace::default();
    for tick in 0..2 {
        battles.update(&mut w.flocks, &w.creatures, &w.space, tick, &grace);
        assert_eq!(w.flocks[&young].battle, None, "tick {tick}: held in a battle nobody can fight");
    }
}

#[test]
fn a_parent_defends_only_a_child_it_still_knows() {
    use life_core::social::{Alarm, prepare_aid};
    let mut w = world();
    // care 20%: a parent knows its child while the child is below 40% of its adult size
    let parent = w.spawn(CreatureGenome::BASE.with(Gene::Care, 20.0), 1000.0, 1000.0, Some(100.0));
    w.spawn(CreatureGenome::BASE, 1030.0, 1000.0, Some(60.0));
    let child = &mut w.creatures[1];
    child.parent = parent;
    child.genome = child.genome.with(Gene::Size, 60.0); // at 40 of 60: growing, two thirds grown
    let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1080.0, 1000.0, Some(100.0));
    w.creatures[1].mind.social.observed_alarm = Some(Alarm { enemy, x: 1080.0, y: 1000.0, tick: 1 });
    assert!(!w.creatures[1].adult());
    assert!(!w.creatures[0].kinship().kin(w.creatures[1].kinship()), "the parent no longer knows it");
    prepare_aid(&mut w.creatures, 1);
    assert!(w.creatures[0].mind.social.aid.is_none(), "defends a child it does not know");
}

#[test]
fn a_settled_flock_whose_members_see_food_only_outside_moves() {
    let mut w = world();
    let tag = flock_at(&mut w, CreatureGenome::BASE, 4, 3000.0, 1500.0);
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    let home = w.flocks[&tag].circle.unwrap();
    let mut moves = 0;
    for _ in 0..400 {
        for v in &mut w.creatures {
            // fed and at home, but the only plant any member sees lies far outside the circle
            let c = v.circle.unwrap();
            (v.x, v.y) = (c.x, c.y);
            v.mind.social.personal_food = Some((c.x + c.radius + 300.0, c.y));
        }
        moves += flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, true);
    }
    // BEHAVIOR.md: a settled circle moves when no member sees a plant inside it for 300 ticks
    assert!(moves >= 1, "no plant inside for 400 ticks, yet the circle stays at {home:?}");
}

#[test]
fn a_circle_pressed_against_the_wall_is_walked_around_without_zigzags() {
    let mut w = world();
    let hard = CreatureGenome::BASE.with(Gene::Territoriality, 2.0);
    let tag = pair(&mut w, hard, 1000.0, 400.0);
    w.spawn(CreatureGenome::BASE, 1500.0, 40.0, Some(100.0));
    flock::update(&mut w.flocks, &mut w.creatures, &w.space, 1, false);
    // the circle touches the top of the world: the corridor along the wall is closed
    w.flocks.get_mut(&tag).unwrap().circle = Some(Circle { x: 1000.0, y: 300.0, radius: 300.0 });
    let traveler = 2;
    let top = w.creatures[traveler].pheno.y_lo;
    w.creatures[traveler].y = top;
    let mut territory = State::default();
    let (mut last, mut turns) = (None::<(f64, f64)>, 0);
    for tick in 0..200 {
        territory.prepare(&mut w.flocks, &mut w.creatures, &w.space, tick);
        let v = &mut w.creatures[traveler];
        // it wants to get along the wall past the circle
        let next = steer(v, Intent { tx: 400.0, ty: top, slow: false, attack: None });
        let (dx, dy) = (next.tx - v.x, next.ty - v.y);
        let distance = dx.hypot(dy);
        if distance <= 1e-9 {
            continue;
        }
        if let Some((px, py)) = last {
            turns += (px * dx + py * dy < 0.0) as usize;
        }
        last = Some((dx, dy));
        let step = distance.min(v.pheno.speed);
        let (x0, y0) = (v.x, v.y);
        v.x = (v.x + dx / distance * step).clamp(v.pheno.x_lo, v.pheno.x_hi);
        v.y = (v.y + dy / distance * step).clamp(v.pheno.y_lo, v.pheno.y_hi);
        v.mind.social.heading = Some((v.x - x0, v.y - y0));
    }
    // at most one turn back: when it finds the corridor closed and goes around the open side
    assert!(turns <= 1, "{turns} turns back at the wall");
}
