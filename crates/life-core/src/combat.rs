//! Одновременные ближние удары и слабые выстрелы. Трупы создаёт мир после боя.
use crate::config::SHOT_RANGE_SIZES;
use crate::creature::{Creature, Death};
use crate::grid::Grid;
use crate::kin_grace::Grace;
use crate::{Counters, Rules, Space};

/// Короткий след фактически совершённого выстрела для окна и хроники.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub tick: u64,
}

/// События фазы боя; энергия погибших остаётся для фазы трупов.
#[derive(Clone, Debug, Default)]
pub(crate) struct CombatResult {
    pub shots: Vec<Shot>,
    pub territorial_attacks: u64,
    pub attacked_flocks: Vec<(u64, u64)>,
}

#[derive(Clone, Copy)]
struct Hit {
    attacker: usize,
    victim: usize,
    damage: f64,
    cost: f64,
    ranged: bool,
    territorial: bool,
}

pub(crate) struct CombatPolicy<'a> {
    pub territorial_targets: &'a [Option<u64>],
    pub grace: &'a Grace,
}

/// Самооборона и помощь разрешают бить противника любого размера. Обычная
/// охота остаётся ограниченной отношением размеров даже после выбора цели.
fn defending(v: &Creature, enemy: u64, territorial: bool, tick: u64) -> bool {
    territorial
        || v.mind.social.aid.is_some_and(|aid| aid.enemy == enemy)
        || v.mind.social.hit.is_some_and(|hit| hit.enemy == enemy && tick.saturating_sub(hit.tick) <= 1)
        || (v.mind.attack == Some(enemy) && v.mind.social.activity == crate::social::Activity::Alarm)
}

#[cfg(test)]
pub(crate) fn resolve(
    space: &Space,
    rules: &Rules,
    creatures: &mut [Creature],
    grid: &mut Grid,
    counters: &mut Counters,
    tick: u64,
    territorial_targets: &[Option<u64>],
) -> CombatResult {
    resolve_with_grace(
        space,
        rules,
        creatures,
        grid,
        counters,
        tick,
        CombatPolicy { territorial_targets, grace: &Grace::default() },
    )
}

pub(crate) fn resolve_with_grace(
    space: &Space,
    rules: &Rules,
    creatures: &mut [Creature],
    grid: &mut Grid,
    counters: &mut Counters,
    tick: u64,
    policy: CombatPolicy<'_>,
) -> CombatResult {
    let CombatPolicy { territorial_targets, grace } = policy;
    grid.rebuild(space, creatures.iter().map(|v| (v.x, v.y)));
    let max_half = creatures.iter().filter(|v| v.alive).fold(0.0_f64, |m, v| m.max(v.pheno.half));
    let mut hits = Vec::new();
    for (i, v) in creatures.iter().enumerate() {
        let cost = v.pheno.size * rules.melee_damage_share;
        if !v.alive || v.fleeing() {
            continue;
        }
        // Назначение границы действует лишь пока это выбранное намерение.
        // Личное спасение, самооборона и прикрытие могут сменить цель после снимка.
        let assigned = territorial_targets.get(i).copied().flatten().filter(|&id| v.mind.attack == Some(id));
        if v.energy > cost {
            let mut target = None;
            grid.for_each_near(v.x, v.y, v.pheno.half + max_half, |j, _, _| {
                let u = &creatures[j];
                if !u.alive
                    || v.kinship().kin(u.kinship())
                    || (v.flock != 0 && v.flock == u.flock)
                    || grace.contains(v.flock, u.flock, tick)
                {
                    return;
                }
                if (v.x - u.x).hypot(v.y - u.y) > v.pheno.half + u.pheno.half {
                    return;
                }
                let selected = v.mind.attack == Some(u.id);
                let territorial = assigned == Some(u.id);
                let defense = defending(v, u.id, territorial, tick);
                if !selected && !defense && (v.fleeing() || v.energy > v.pheno.max_energy * 0.9) {
                    return;
                }
                if !defense && u.pheno.size > v.pheno.size / rules.cannibal_ratio.max(v.pheno.prey_ratio) {
                    return;
                }
                if target.is_none_or(|k: usize| {
                    (u.id != assigned.unwrap_or(0), u.id != v.mind.attack.unwrap_or(0), u.id)
                        < (
                            creatures[k].id != assigned.unwrap_or(0),
                            creatures[k].id != v.mind.attack.unwrap_or(0),
                            creatures[k].id,
                        )
                }) {
                    target = Some(j);
                }
            });
            if let Some(j) = target {
                hits.push(Hit {
                    attacker: i,
                    victim: j,
                    damage: cost.min(creatures[j].max_health() * 0.25),
                    cost,
                    ranged: false,
                    territorial: assigned == Some(creatures[j].id),
                });
                continue; // при контакте ближний удар имеет приоритет
            }
        }
        if !v.pheno.shooter
            || (v.mind.social.last_shot != 0
                && tick.saturating_sub(v.mind.social.last_shot) < rules.shot_period as u64)
        {
            continue;
        }
        let shot_cost = v.pheno.size * rules.shot_energy_share;
        if v.energy <= shot_cost || v.energy - shot_cost < v.pheno.max_energy * v.pheno.fire_reserve {
            continue;
        }
        let desired = assigned.or(v.mind.attack).or(v.mind.social.aid.map(|aid| aid.enemy)).or(v
            .mind
            .social
            .hit
            .filter(|hit| tick.saturating_sub(hit.tick) <= 1)
            .map(|hit| hit.enemy));
        let Some(desired) = desired else { continue };
        let range = v.pheno.vision.min(v.pheno.size * SHOT_RANGE_SIZES);
        grid.for_each_near(v.x, v.y, range, |j, _, _| {
            let u = &creatures[j];
            if !u.alive
                || u.id != desired
                || v.kinship().kin(u.kinship())
                || (v.flock != 0 && v.flock == u.flock)
                || grace.contains(v.flock, u.flock, tick)
            {
                return;
            }
            let dist = (v.x - u.x).hypot(v.y - u.y);
            let contact = v.pheno.half + u.pheno.half;
            let ready_range = contact + (range - contact).max(0.0) * v.pheno.fire_preference;
            if dist <= contact || dist > range || dist > ready_range {
                return;
            }
            let territorial = assigned == Some(u.id);
            if !defending(v, u.id, territorial, tick)
                && (v.fleeing() || u.pheno.size > v.pheno.size / rules.cannibal_ratio.max(v.pheno.prey_ratio))
            {
                return;
            }
            hits.push(Hit {
                attacker: i,
                victim: j,
                damage: (v.pheno.size * rules.shot_damage_share).min(u.max_health() * 0.25),
                cost: shot_cost,
                ranged: true,
                territorial,
            });
        });
    }
    let mut result = CombatResult::default();
    let mut damage = vec![0.0; creatures.len()];
    for hit in hits {
        let Hit { attacker: i, victim: j, damage: d, cost, ranged, territorial } = hit;
        let signal =
            crate::social::Alarm { enemy: creatures[i].id, x: creatures[i].x, y: creatures[i].y, tick };
        if creatures[j].mind.social.hit.is_none_or(|h| h.tick != tick || signal.enemy < h.enemy) {
            creatures[j].mind.social.hit = Some(signal);
            creatures[j].mind.social.observed_alarm = Some(signal);
        }
        creatures[i].energy -= cost;
        if ranged {
            creatures[i].mind.social.last_shot = tick;
            result.shots.push(Shot {
                from: (creatures[i].x, creatures[i].y),
                to: (creatures[j].x, creatures[j].y),
                tick,
            });
        }
        result.territorial_attacks += territorial as u64;
        result.attacked_flocks.push((creatures[j].flock, creatures[i].id));
        creatures[i].peaceful_ticks = 0;
        creatures[j].peaceful_ticks = 0;
        damage[j] += d;
    }
    for (v, d) in creatures.iter_mut().zip(damage) {
        if !v.alive {
            continue;
        }
        v.health = (v.health - d).max(0.0);
        if v.health == 0.0 {
            v.alive = false;
            v.death = Some(Death::Combat);
            counters.combat += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::creature::Gene;
    use crate::{CreatureGenome, World, WorldConfig};
    fn world() -> World {
        World::new(&WorldConfig {
            n_creatures: Some(0),
            rules: Rules::default().with("cannibalism", 1.0).unwrap(),
            ..Default::default()
        })
    }
    fn hit(w: &mut World) {
        let territorial_targets = vec![None; w.creatures.len()];
        resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            w.tick + 1,
            &territorial_targets,
        );
    }

    #[test]
    fn временный_мир_между_стаями_запрещает_выстрел_и_ближний_удар() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        w.creatures[0].mind.attack = Some(prey);
        let mut grace = Grace::default();
        grace.register(w.creatures[0].flock, w.creatures[1].flock, 0);
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let during = resolve_with_grace(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut grid,
            &mut w.counters,
            600,
            CombatPolicy { territorial_targets: &[None, None], grace: &grace },
        );
        assert!(during.shots.is_empty());
        assert_eq!(w.creatures[1].health, 15.0);
        let after = resolve_with_grace(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut grid,
            &mut w.counters,
            601,
            CombatPolicy { territorial_targets: &[None, None], grace: &grace },
        );
        assert_eq!(after.shots.len(), 1);

        let mut close = world();
        close.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        let target = close.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1010.0, 1000.0, Some(30.0));
        close.creatures[0].mind.attack = Some(target);
        let mut grace = Grace::default();
        grace.register(close.creatures[0].flock, close.creatures[1].flock, 0);
        resolve_with_grace(
            &close.space,
            &close.rules,
            &mut close.creatures,
            &mut grid,
            &mut close.counters,
            600,
            CombatPolicy { territorial_targets: &[None, None], grace: &grace },
        );
        assert_eq!(close.creatures[1].health, 15.0);
    }
    #[test]
    fn удары_одновременны_и_погибший_не_получает_добычу() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(80.0));
        }
        let ids = [w.creatures[0].id, w.creatures[1].id];
        for (i, v) in w.creatures.iter_mut().enumerate() {
            v.health = 1.0;
            v.mind.attack = Some(ids[1 - i]);
            v.mind.social.activity = crate::social::Activity::Alarm;
        }
        hit(&mut w);
        assert!(w.creatures.iter().all(|v| !v.alive && v.health == 0.0 && v.energy == 78.0));
        assert_eq!(w.counters.combat, 2);
    }
    #[test]
    fn полное_здоровье_требует_нескольких_ударов() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(200.0));
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        for _ in 0..5 {
            hit(&mut w);
            assert!(w.creatures[1].alive);
        }
        hit(&mut w);
        assert!(!w.creatures[1].alive);
        assert_eq!(w.counters.combat, 1);
    }
    #[test]
    fn гибель_не_передаёт_энергию_мгновенно() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(100.0));
        }
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        w.creatures[2].health = 5.0;
        hit(&mut w);
        assert_eq!(w.creatures[0].energy, 95.0);
        assert_eq!(w.creatures[1].energy, 95.0);
        assert!(!w.creatures[2].alive);
        assert!(w.creatures[2].energy > 0.0);
    }

    #[test]
    fn выстрел_слабее_удара_тратит_энергию_и_имеет_паузу() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let target = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(40.0));
        w.creatures[0].mind.attack = Some(target);
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let mut total = 0;
        for tick in 1..=6 {
            let result = resolve(
                &w.space,
                &w.rules,
                &mut w.creatures,
                &mut grid,
                &mut w.counters,
                tick,
                &[None, None],
            );
            total += result.shots.len();
            assert_eq!(result.shots.len(), usize::from(tick == 1 || tick == 6));
        }
        assert_eq!(total, 2);
        assert!((w.creatures[0].energy - 98.4).abs() < 1e-9);
        assert!((w.creatures[1].health - 14.2).abs() < 1e-9);
    }

    #[test]
    fn контакт_выбирает_ближний_удар_и_не_создаёт_следа() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1020.0, 1000.0, Some(40.0));
        w.creatures[0].mind.attack = Some(prey);
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            &[None, None],
        );
        assert!(result.shots.is_empty());
        assert_eq!(w.creatures[0].energy, 98.0);
        assert_eq!(w.creatures[1].health, 13.0);
    }

    #[test]
    fn одна_особь_наносит_не_более_одного_удара_за_тик() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(80.0));
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1020.0, 1000.0, Some(30.0));
        let distant = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        w.creatures[0].mind.attack = Some(distant);
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            &[None, None, None],
        );
        assert!(result.shots.is_empty());
        assert_eq!(w.creatures[1].health, 13.0);
        assert_eq!(w.creatures[2].health, 15.0);
        assert_eq!(w.creatures[0].energy, 78.0);
    }

    #[test]
    fn защита_территории_позволяет_стрелять_по_крупному_чужаку_но_не_по_родне() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1120.0, 1000.0, Some(100.0));
        w.creatures[0].mind.attack = Some(enemy);
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let no_defense =
            resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 1, &[None, None]);
        assert!(no_defense.shots.is_empty());
        let defense = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut grid,
            &mut w.counters,
            2,
            &[Some(enemy), None],
        );
        assert_eq!(defense.shots.len(), 1);
        assert_eq!(defense.territorial_attacks, 1);
        let parent = w.creatures[0].id;
        w.creatures[1].parent = parent;
        let kin = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut grid,
            &mut w.counters,
            7,
            &[Some(enemy), None],
        );
        assert!(kin.shots.is_empty());
    }

    #[test]
    fn одновременные_выстрелы_могут_убить_обоих() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        for x in [1000.0, 1100.0] {
            w.spawn(shooter, x, 1000.0, Some(100.0));
        }
        let ids = [w.creatures[0].id, w.creatures[1].id];
        for (i, v) in w.creatures.iter_mut().enumerate() {
            v.health = 0.2;
            v.mind.attack = Some(ids[1 - i]);
        }
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            &[Some(ids[1]), Some(ids[0])],
        );
        assert_eq!(result.shots.len(), 2);
        assert!(w.creatures.iter().all(|v| !v.alive));
        assert_eq!(w.counters.combat, 2);
    }

    #[test]
    fn трое_стрелков_совместно_останавливают_крупного_хищника() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        for x in [1000.0, 1020.0, 1040.0] {
            w.spawn(shooter, x, 1000.0, Some(100.0));
        }
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1140.0, 1000.0, Some(100.0));
        for v in &mut w.creatures[..3] {
            v.flock = 1;
            v.mind.attack = Some(enemy);
        }
        w.creatures[3].health = 2.0;
        let targets = [Some(enemy), Some(enemy), Some(enemy), None];
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let first = resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 1, &targets);
        assert_eq!(first.shots.len(), 3);
        assert_eq!(first.territorial_attacks, 3);
        assert!((w.creatures[3].health - 0.8).abs() < 1e-9);
        let second = resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 6, &targets);
        assert_eq!(second.shots.len(), 3);
        assert!(!w.creatures[3].alive);
        assert!(w.creatures[..3].iter().all(|v| v.alive && v.health == v.max_health()));
    }

    #[test]
    fn начатое_прикрытие_имеет_приоритет_над_территориальной_целью() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let aid_enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        let intruder = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1120.0, 1000.0, Some(100.0));
        w.creatures[0].mind.attack = Some(aid_enemy);
        w.creatures[0].mind.social.aid =
            Some(crate::social::Aid { victim: 999, enemy: aid_enemy, started: 1, x: 1100.0, y: 1000.0 });
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            &[Some(intruder), None, None],
        );
        assert_eq!(result.shots.len(), 1);
        assert_eq!(result.shots[0].to, (1100.0, 1000.0));
        assert_eq!(result.territorial_attacks, 0);
    }

    #[test]
    fn беглец_не_бьёт_свежего_нападавшего_без_намерения_защищаться() {
        let mut w = world();
        w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        let enemy = w.spawn(CreatureGenome::BASE.with(Gene::Size, 80.0), 1010.0, 1000.0, Some(100.0));
        w.creatures[0].mind.flee_ticks = 5;
        w.creatures[0].mind.social.hit = Some(crate::social::Alarm { enemy, x: 1010.0, y: 1000.0, tick: 1 });
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            2,
            &[Some(enemy), None],
        );
        assert!(result.shots.is_empty());
        assert_eq!(w.creatures[0].energy, 100.0);
        assert_eq!(w.creatures[1].health, 80.0);
    }

    #[test]
    fn все_одновременные_нападающие_дают_право_на_ответ() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(100.0));
        }
        let victim = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1000.0, 1000.0, Some(30.0));
        let attackers = [w.creatures[0].id, w.creatures[1].id];
        let flock = w.creatures[2].flock;
        for v in &mut w.creatures[..2] {
            v.mind.attack = Some(victim);
        }
        let result = resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
            1,
            &[None, None, None],
        );
        assert!(result.attacked_flocks.contains(&(flock, attackers[0])));
        assert!(result.attacked_flocks.contains(&(flock, attackers[1])));
    }

    #[test]
    fn выстрел_не_расходует_энергию_ниже_личного_резерва() {
        let mut w = world();
        let shooter = CreatureGenome::BASE.with(Gene::Shooter, 1.0).with(Gene::FirePreference, 100.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(50.5));
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        w.creatures[0].mind.attack = Some(prey);
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let before =
            resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 1, &[None, None]);
        assert!(before.shots.is_empty());
        assert_eq!(w.creatures[0].energy, 50.5);
        w.creatures[0].energy = 51.0;
        let after =
            resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 2, &[None, None]);
        assert_eq!(after.shots.len(), 1);
        assert!((w.creatures[0].energy - 50.2).abs() < 1e-9);
    }

    #[test]
    fn новые_цена_и_сила_выстрела_действуют_на_живых_без_изменения_генов() {
        let mut w = world();
        let shooter = CreatureGenome::BASE
            .with(Gene::Shooter, 1.0)
            .with(Gene::FirePreference, 100.0)
            .with(Gene::FireReserve, 0.0);
        w.spawn(shooter, 1000.0, 1000.0, Some(100.0));
        let prey = w.spawn(CreatureGenome::BASE.with(Gene::Size, 15.0), 1100.0, 1000.0, Some(30.0));
        let genes = w.creatures.iter().map(|v| v.genome).collect::<Vec<_>>();
        let rules = w
            .rules
            .with("shot_damage_share", 0.05)
            .unwrap()
            .with("shot_energy_share", 0.01)
            .unwrap()
            .with("shot_period", 2.0)
            .unwrap();
        w.set_rules(rules);
        w.creatures[0].mind.attack = Some(prey);
        let mut grid = Grid::new(crate::config::GRID_CELL);
        let first =
            resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 1, &[None, None]);
        assert_eq!(first.shots.len(), 1);
        assert!((w.creatures[0].energy - 99.6).abs() < 1e-9);
        assert!((w.creatures[1].health - 13.0).abs() < 1e-9);
        let second =
            resolve(&w.space, &w.rules, &mut w.creatures, &mut grid, &mut w.counters, 2, &[None, None]);
        assert!(second.shots.is_empty());
        assert_eq!(w.creatures.iter().map(|v| v.genome).collect::<Vec<_>>(), genes);
        let restored = w
            .rules
            .with("shot_damage_share", crate::config::SHOT_DAMAGE_SHARE)
            .unwrap()
            .with("shot_energy_share", crate::config::SHOT_ENERGY_SHARE)
            .unwrap()
            .with("shot_period", crate::config::SHOT_PERIOD as f64)
            .unwrap();
        w.set_rules(restored);
        assert_eq!(w.creatures[0].genome, shooter);
        assert_eq!(w.creatures[0].pheno.shot_energy_share, crate::config::SHOT_ENERGY_SHARE);
    }
}
