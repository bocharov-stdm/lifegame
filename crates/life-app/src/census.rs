//! A census for the «Внутри видов» tab: every creature's characteristics at one tick, grouped by
//! diet, with the distributions already counted. The simulation thread takes it only on request
//! and only while the world stands (pause or an ended game): a census of a big world is a pass
//! with sorts, and a running game must not pay for it.

use std::collections::HashMap;

use life_core::World;
use life_core::creature::{ADULT, Action};
use life_core::genome::creature::{self, Gene};

/// Bars of a histogram.
pub const BINS: usize = 36;
/// Behaviour groups listed for a group of creatures; the rest go together.
pub const TOP_BEHAVIOURS: usize = 5;

/// The characteristics beyond the genes: where a creature is and how it does, all in %.
pub const EXTRAS: [(&str, &str); 3] = [
    ("глубина", "Где существо сейчас: 0% — поверхность, 100% — дно."),
    ("сытость", "Насколько полон бак."),
    ("прожито", "Прожитое от своего срока жизни, %."),
];

/// A census row: the genes in `Gene` order, then `EXTRAS`.
pub const COLUMNS: usize = creature::N + EXTRAS.len();

/// Groups: everybody, then the diets in `Diet` order.
pub const GROUPS: usize = 5;

/// One characteristic within a group: its histogram over `lo..=hi` and its percentiles. For a
/// choice gene `bins[k]` counts variant k.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Column {
    pub lo: f64,
    pub hi: f64,
    pub bins: Vec<u32>,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
}

impl Column {
    /// The bar a value falls in.
    pub fn bin(lo: f64, hi: f64, v: f64) -> usize {
        if hi > lo {
            (((v - lo) / (hi - lo)) * BINS as f64).floor().clamp(0.0, (BINS - 1) as f64) as usize
        } else {
            BINS / 2
        }
    }

    fn numbers(mut xs: Vec<f64>) -> Column {
        if xs.is_empty() {
            return Column::default();
        }
        xs.sort_by(f64::total_cmp);
        let at = |q: f64| xs[((xs.len() - 1) as f64 * q).round() as usize];
        let (lo, hi) = (xs[0], xs[xs.len() - 1]);
        let mut bins = vec![0; BINS];
        for &v in &xs {
            bins[Column::bin(lo, hi, v)] += 1;
        }
        Column { lo, hi, bins, p10: at(0.1), p50: at(0.5), p90: at(0.9) }
    }

    fn choices(xs: impl Iterator<Item = f64>, variants: usize) -> Column {
        let mut bins = vec![0; variants];
        for v in xs {
            if let Some(b) = bins.get_mut(v.round() as usize) {
                *b += 1;
            }
        }
        Column { lo: 0.0, hi: variants as f64, bins, ..Column::default() }
    }
}

/// Creatures whose adult programs decide by the same deciding blocks in the same order: a
/// behavioural subspecies, whatever their tests and numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Behaviour {
    pub chain: Vec<Action>,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub count: usize,
    /// `COLUMNS` of them.
    pub columns: Vec<Column>,
    /// The most common ones first, at most `TOP_BEHAVIOURS`.
    pub behaviours: Vec<Behaviour>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Census {
    pub world_gen: u64,
    pub tick: u64,
    /// The world's edits between ticks it was taken after (`Frame::edits`).
    pub edits: u64,
    /// Per creature: its diet and its row.
    pub diets: Vec<u8>,
    pub rows: Vec<[f32; COLUMNS]>,
    pub groups: [Group; GROUPS],
}

impl Census {
    pub fn of(world: &World, world_gen: u64, edits: u64) -> Census {
        let depth = world.space.height.max(1.0);
        let mut diets = Vec::with_capacity(world.creatures.len());
        let mut rows = Vec::with_capacity(world.creatures.len());
        let mut chains = Vec::with_capacity(world.creatures.len());
        for v in &world.creatures {
            let genes = v.genome.to_values();
            let extras = [
                v.y / depth * 100.0,
                v.energy / v.pheno.max_energy.max(1e-9) * 100.0,
                v.age / v.genome[Gene::Lifespan].max(1.0) * 100.0,
            ];
            diets.push(v.pheno.diet as u8);
            rows.push(std::array::from_fn(
                |c| if c < creature::N { genes[c] } else { extras[c - creature::N] } as f32,
            ));
            let adult = &v.programs[ADULT];
            let chain: Vec<Action> = (0..adult.blocks().len())
                .filter(|&i| adult.live(i) && !adult.blocks()[i].action.is_setting())
                .map(|i| adult.blocks()[i].action)
                .collect();
            chains.push(chain);
        }
        let groups = std::array::from_fn(|g| {
            let members: Vec<usize> =
                (0..rows.len()).filter(|&i| g == 0 || usize::from(diets[i]) == g - 1).collect();
            group(&members, &rows, &chains)
        });
        Census { world_gen, tick: world.tick, edits, diets, rows, groups }
    }
}

fn group(members: &[usize], rows: &[[f32; COLUMNS]], chains: &[Vec<Action>]) -> Group {
    let column = |c: usize| {
        let xs = members.iter().map(|&i| f64::from(rows[i][c]));
        match creature::GENES.get(c).and_then(|s| s.variants()) {
            Some(variants) => Column::choices(xs, variants.len()),
            None => Column::numbers(xs.collect()),
        }
    };
    let mut counts: HashMap<&[Action], usize> = HashMap::new();
    for &i in members {
        *counts.entry(&chains[i]).or_default() += 1;
    }
    let mut behaviours: Vec<Behaviour> =
        counts.into_iter().map(|(chain, count)| Behaviour { chain: chain.to_vec(), count }).collect();
    // by count, then by the chain itself: the order must not depend on the hash map's
    behaviours.sort_by(|a, b| {
        b.count.cmp(&a.count).then_with(|| {
            let key = |x: &Behaviour| x.chain.iter().map(|&a| a as u8).collect::<Vec<_>>();
            key(a).cmp(&key(b))
        })
    });
    behaviours.truncate(TOP_BEHAVIOURS);
    Group { count: members.len(), columns: (0..COLUMNS).map(column).collect(), behaviours }
}

/// The label of a census column.
pub fn label(c: usize) -> &'static str {
    creature::GENES.get(c).map_or_else(|| EXTRAS[c - creature::N].0, |s| s.label)
}

/// The hint of a census column.
pub fn about(c: usize) -> &'static str {
    creature::GENES.get(c).map_or_else(|| EXTRAS[c - creature::N].1, |s| s.about)
}

/// Whether a column is in % (the extras are).
pub fn is_percent(c: usize) -> bool {
    creature::GENES.get(c).is_none_or(|s| s.is_percent())
}

/// A column's value as the window writes it.
pub fn number(c: usize, v: f64) -> String {
    match (is_percent(c), v.abs() < 20.0) {
        (true, _) => format!("{v:.0}%"),
        (false, true) => format!("{v:.1}"),
        (false, false) => format!("{v:.0}"),
    }
}

/// The numeric columns worth a histogram: every number gene and the extras (not the choice genes,
/// they get share bars, and not the diet, by which the groups are made).
pub fn numeric() -> impl Iterator<Item = usize> {
    (0..COLUMNS).filter(|&c| creature::GENES.get(c).is_none_or(|s| s.variants().is_none()))
}

/// The choice genes worth a share bar: more than one variant, and not the diet.
pub fn choices() -> impl Iterator<Item = usize> {
    (0..creature::N)
        .filter(|&c| c != Gene::Diet as usize && creature::GENES[c].variants().is_some_and(|v| v.len() >= 2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::WorldConfig;

    #[test]
    fn перепись_делит_по_питанию_и_считает_всех() {
        let cfg = WorldConfig { seed: 3, diets: vec![1.0, 1.0, 0.0, 1.0], ..Default::default() };
        let mut w = World::new(&cfg);
        for _ in 0..200 {
            w.step();
        }
        let c = Census::of(&w, 7, 2);
        assert_eq!((c.world_gen, c.tick, c.edits), (7, w.tick, 2));
        assert_eq!(c.rows.len(), w.creatures.len());
        assert_eq!(c.groups[0].count, w.creatures.len());
        assert_eq!(c.groups[1..].iter().map(|g| g.count).sum::<usize>(), w.creatures.len());
        for (g, group) in c.groups.iter().enumerate() {
            let n = group.count as u32;
            for (col, column) in group.columns.iter().enumerate() {
                if n > 0 {
                    assert_eq!(column.bins.iter().sum::<u32>(), n, "группа {g}, признак {}", label(col));
                }
                assert!(column.p10 <= column.p50 && column.p50 <= column.p90);
            }
            assert!(group.behaviours.iter().map(|b| b.count).sum::<usize>() <= group.count);
            assert!(group.behaviours.windows(2).all(|b| b[0].count >= b[1].count));
        }
        let size = Gene::Size as usize;
        let herbivores = &c.groups[1].columns[size];
        let smallest =
            w.creatures.iter().filter(|v| v.pheno.diet as usize == 0).map(|v| v.genome[Gene::Size]);
        assert_eq!(herbivores.lo, smallest.fold(f64::INFINITY, f64::min) as f32 as f64);
    }

    #[test]
    fn пустой_мир_даёт_пустую_перепись() {
        let w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let c = Census::of(&w, 1, 0);
        assert!(c.rows.is_empty() && c.groups.iter().all(|g| g.count == 0 && g.behaviours.is_empty()));
    }

    #[test]
    fn столбцы_и_подписи() {
        assert_eq!(Column::bin(0.0, 10.0, 10.0), BINS - 1);
        assert_eq!(Column::bin(0.0, 10.0, 0.0), 0);
        assert_eq!(Column::bin(5.0, 5.0, 5.0), BINS / 2);
        assert_eq!(label(creature::N), "глубина");
        assert!(numeric().all(|c| c != Gene::Diet as usize && c != Gene::Strategy as usize));
        assert_eq!(choices().collect::<Vec<_>>(), vec![Gene::Strategy as usize]);
        assert_eq!(number(Gene::Size as usize, 42.4), "42");
        assert_eq!(number(creature::N + 1, 42.4), "42%");
    }
}
