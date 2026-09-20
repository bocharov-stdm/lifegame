//! Одновременные удары: решения читают живое состояние, урон применяется после сбора.
use crate::creature::{Creature, Death};
use crate::grid::Grid;
use crate::{Counters, Rules, Space};

pub(crate) fn resolve(
    space: &Space,
    rules: &Rules,
    creatures: &mut [Creature],
    grid: &mut Grid,
    counters: &mut Counters,
) {
    grid.rebuild(space, creatures.iter().map(|v| (v.x, v.y)));
    let max_half = creatures.iter().filter(|v| v.alive).fold(0.0_f64, |m, v| m.max(v.pheno.half));
    let mut hits = Vec::new();
    for (i, v) in creatures.iter().enumerate() {
        let cost = v.pheno.size * 0.05;
        if !v.alive || v.energy <= cost {
            continue;
        }
        let mut target = None;
        grid.for_each_near(v.x, v.y, v.pheno.half + max_half, |j, _, _| {
            let u = &creatures[j];
            if !u.alive || v.kinship().kin(u.kinship()) || (v.flock != 0 && v.flock == u.flock) {
                return;
            }
            if (v.x - u.x).hypot(v.y - u.y) > v.pheno.half + u.pheno.half {
                return;
            }
            let defending = v.mind.attack == Some(u.id);
            if !defending && (v.fleeing() || v.energy > v.pheno.max_energy * 0.9) {
                return;
            }
            if !defending && u.pheno.size > v.pheno.size / rules.cannibal_ratio.max(v.pheno.prey_ratio) {
                return;
            }
            if target.is_none_or(|k: usize| {
                (u.id != v.mind.attack.unwrap_or(0), u.id)
                    < (creatures[k].id != v.mind.attack.unwrap_or(0), creatures[k].id)
            }) {
                target = Some(j);
            }
        });
        if let Some(j) = target {
            hits.push((i, j, cost.min(creatures[j].max_health() * 0.25), cost));
        }
    }
    let mut damage = vec![0.0; creatures.len()];
    for &(i, j, d, cost) in &hits {
        creatures[i].energy -= cost;
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
    // Победитель определяется только среди переживших всю фазу.
    let mut winners: Vec<Option<(usize, f64)>> = vec![None; creatures.len()];
    for &(i, j, d, _) in &hits {
        if !creatures[i].alive || creatures[j].alive {
            continue;
        }
        if winners[j].is_none_or(|(k, old)| d > old || (d == old && creatures[i].id < creatures[k].id)) {
            winners[j] = Some((i, d));
        }
    }
    let mut gains = vec![0.0; creatures.len()];
    for (j, winner) in winners.into_iter().enumerate() {
        if let Some((i, _)) = winner {
            let prey = &creatures[j];
            gains[i] += prey.energy.max(0.0)
                + (prey.pheno.size - prey.birth_size).max(0.0) * crate::config::ENERGY_PER_SIZE;
        }
    }
    for (v, gain) in creatures.iter_mut().zip(gains) {
        if v.alive && gain > 0.0 {
            v.devour(gain, rules);
        }
    }
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
        resolve(
            &w.space,
            &w.rules,
            &mut w.creatures,
            &mut Grid::new(crate::config::GRID_CELL),
            &mut w.counters,
        );
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
    fn добыча_одному_победителю_при_равном_уроне() {
        let mut w = world();
        for _ in 0..2 {
            w.spawn(CreatureGenome::BASE.with(Gene::Size, 100.0), 1000.0, 1000.0, Some(100.0));
        }
        w.spawn(CreatureGenome::BASE.with(Gene::Size, 30.0), 1000.0, 1000.0, Some(50.0));
        w.creatures[2].health = 5.0;
        hit(&mut w);
        assert_eq!(w.creatures[0].energy, 115.0);
        assert_eq!(w.creatures[1].energy, 95.0);
    }
}
