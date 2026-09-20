//! Стаи: собственная цель и центр, без общих чувств и коллективных атак.
use crate::{Space, creature::Creature, rng::Rng};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlockGoal {
    pub x: f64,
    pub y: f64,
    pub tx: f64,
    pub ty: f64,
}

#[derive(Clone, Debug)]
pub struct Flock {
    pub members: usize,
    pub goal: FlockGoal,
    pub remaining: u32,
    pub rng: Rng,
    lo: f64,
    hi: f64,
}

pub(crate) fn update(
    flocks: &mut BTreeMap<u64, Flock>,
    creatures: &mut [Creature],
    space: &Space,
    seed: u64,
    advance: bool,
) {
    for f in flocks.values_mut() {
        f.members = 0;
        f.goal.x = 0.0;
        f.goal.y = 0.0;
        f.lo = 0.0;
        f.hi = 0.0;
    }
    for v in creatures.iter().filter(|v| v.alive) {
        let f = flocks.entry(v.flock).or_insert_with(|| Flock {
            members: 0,
            goal: FlockGoal { x: 0.0, y: 0.0, tx: 0.0, ty: 0.0 },
            remaining: 0,
            rng: Rng::keyed(seed, v.flock),
            lo: 0.0,
            hi: 0.0,
        });
        f.members += 1;
        f.goal.x += v.x;
        f.goal.y += v.y;
        f.lo += v.pheno.layer_lo;
        f.hi += v.pheno.layer_hi;
    }
    flocks.retain(|_, f| f.members > 0);
    for f in flocks.values_mut() {
        let n = f.members as f64;
        f.goal.x /= n;
        f.goal.y /= n;
        if advance {
            f.remaining = f.remaining.saturating_sub(1);
        }
        if f.members >= 2 && (f.remaining == 0 || (f.goal.x - f.goal.tx).hypot(f.goal.y - f.goal.ty) < 100.0)
        {
            f.goal.tx = f.rng.uniform(0.0, space.width);
            f.goal.ty = f.rng.uniform(f.lo / n, f.hi / n).clamp(0.0, space.height);
            f.remaining = 600;
        }
    }
    for v in creatures {
        v.flock_goal = flocks.get(&v.flock).filter(|f| f.members >= 2).map(|f| f.goal);
    }
}
