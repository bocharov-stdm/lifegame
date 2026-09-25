//! Передача еды ребёнку без создания энергии. Новорождённые ещё не находятся в списке.

use std::collections::BTreeMap;

use crate::{Rules, creature::Creature, genome::creature::Gene};

/// Кормит голодных невзрослых детей в порядке их постоянных ID.
/// Проводится перед движением и расходами на жизнь.
pub fn feed_children(creatures: &mut [Creature], rules: &Rules) {
    let mut children: Vec<_> = creatures
        .iter()
        .enumerate()
        .filter(|(_, child)| child.alive && child.parent != 0 && !child.adult())
        .map(|(i, child)| (child.id, i))
        .collect();
    if children.is_empty() {
        return;
    }
    let parents: BTreeMap<_, _> = creatures.iter().enumerate().map(|(i, v)| (v.id, i)).collect();
    children.sort_unstable();
    for (_, child_i) in children {
        let Some(&parent_i) = parents.get(&creatures[child_i].parent) else { continue };
        if parent_i == child_i {
            continue;
        }
        let (parent, child) = if parent_i < child_i {
            let (left, right) = creatures.split_at_mut(child_i);
            (&mut left[parent_i], &mut right[0])
        } else {
            let (left, right) = creatures.split_at_mut(parent_i);
            (&mut right[0], &mut left[child_i])
        };
        let care = (parent.genome[Gene::Care] / 100.0).clamp(0.0, 1.0);
        let parent_cap = parent.pheno.max_energy;
        let child_cap = child.pheno.max_energy;
        let contact = (parent.x - child.x).hypot(parent.y - child.y) <= parent.pheno.half + child.pheno.half;
        if !parent.alive
            || !child.alive
            || child.adult()
            || !contact
            || care == 0.0
            || child.energy >= child_cap * 0.35
            || parent.energy <= parent_cap * 0.65
        {
            continue;
        }
        let sent = (parent_cap * 0.05 * care)
            .min(parent.energy - parent_cap * 0.60)
            .min(child_cap - child.energy)
            .max(0.0);
        if sent > 0.0 {
            parent.energy -= sent;
            child.nourish(sent, rules);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreatureGenome, World, WorldConfig};

    fn pair(care: f64) -> World {
        let mut world = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let parent = world.spawn(CreatureGenome::BASE.with(Gene::Care, care), 1000.0, 1000.0, Some(100.0));
        world.spawn(CreatureGenome::BASE, 1000.0, 1000.0, Some(2.0));
        world.creatures[1].parent = parent;
        world.creatures[1].birth_size = 20.0;
        world.creatures[1].pheno =
            crate::creature::Phenotype::at_size(&world.creatures[1].genome, &world.rules, &world.space, 20.0);
        world.creatures[1].energy = 2.0;
        world
    }

    #[test]
    fn пища_переходит_к_ребёнку_и_растит_его_без_создания_энергии() {
        let mut w = pair(50.0);
        let before = w.creatures.iter().map(|v| v.energy).sum::<f64>();
        feed_children(&mut w.creatures, &w.rules);
        let after = w.creatures.iter().map(|v| v.energy).sum::<f64>();
        assert!(w.creatures[1].pheno.size > 20.0);
        assert!(w.creatures[0].energy < 100.0);
        assert!(after < before); // разница теперь вложена в рост тела
        let stored = (w.creatures[1].pheno.size - 20.0) * crate::config::ENERGY_PER_SIZE;
        assert!((before - after - stored).abs() < 1e-8);
    }

    #[test]
    fn нулевая_забота_и_недостаток_энергии_не_кормят() {
        for care in [0.0, 50.0] {
            let mut w = pair(care);
            if care > 0.0 {
                w.creatures[0].energy = w.creatures[0].pheno.max_energy * 0.65;
            }
            let before = w.creatures[1].energy;
            feed_children(&mut w.creatures, &w.rules);
            assert_eq!(w.creatures[1].energy, before);
        }
    }

    #[test]
    fn принадлежность_к_разным_стаям_не_мешает_кормлению() {
        let mut w = pair(100.0);
        w.creatures[1].flock += 100;
        feed_children(&mut w.creatures, &w.rules);
        assert!(w.creatures[1].energy > 2.0);
    }

    #[test]
    fn передача_энергии_требует_контакта_с_ребёнком() {
        let mut w = pair(100.0);
        w.creatures[1].x += w.creatures[0].pheno.half + w.creatures[1].pheno.half + 1.0;
        let before = w.creatures.iter().map(|v| v.energy).collect::<Vec<_>>();
        feed_children(&mut w.creatures, &w.rules);
        assert_eq!(w.creatures.iter().map(|v| v.energy).collect::<Vec<_>>(), before);
    }
}
