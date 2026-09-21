//! Стаи: собственная цель и центр, без общих чувств и коллективных атак.
use crate::{Space, creature::Creature, rng::Rng};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub members: usize,
    pub juveniles: usize,
    pub sociability: f64,
    pub fullness: f64,
    pub activity: crate::social::Activity,
    pub goal: Option<(f64, f64)>,
    pub activities: [usize; 5],
}

pub fn summaries(world: &crate::World) -> Vec<Summary> {
    let mut groups = BTreeMap::<u64, Summary>::new();
    for v in world.creatures.iter().filter(|v| v.alive) {
        let s = groups.entry(v.flock).or_insert_with(|| Summary { id: v.flock, ..Default::default() });
        s.members += 1;
        s.x += v.x;
        s.y += v.y;
        s.juveniles += (!v.adult()) as usize;
        s.sociability += v.pheno.sociability;
        s.fullness += v.energy / v.pheno.max_energy;
        s.activities[v.mind.social.activity as usize] += 1;
    }
    for s in groups.values_mut() {
        let n = s.members as f64;
        s.x /= n;
        s.y /= n;
        s.sociability /= n;
        s.fullness /= n;
        let i = (0..5).max_by_key(|&i| (s.activities[i], std::cmp::Reverse(i))).unwrap();
        s.activity = crate::social::Activity::ALL[i];
        s.goal = world.flocks.get(&s.id).map(|f| (f.goal.tx, f.goal.ty));
    }
    for v in world.creatures.iter().filter(|v| v.alive) {
        let s = groups.get_mut(&v.flock).unwrap();
        s.radius += (v.x - s.x).powi(2) + (v.y - s.y).powi(2) + v.pheno.half.powi(2);
    }
    groups
        .into_values()
        .filter(|s| s.members >= 2)
        .map(|mut s| {
            s.radius = (s.radius / s.members as f64).sqrt();
            s
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitWatch {
    pub flock: u64,
    pub anchors: [u64; 3],
    pub since: u64,
}

pub fn split(
    creatures: &mut [Creature],
    grid: &crate::grid::Grid,
    watches: &mut Vec<SplitWatch>,
    next: &mut u64,
    tick: u64,
) -> u64 {
    fn root(parents: &mut [usize], mut i: usize) -> usize {
        for _ in 0..parents.len() {
            if parents[i] == i {
                return i;
            }
            parents[i] = parents[parents[i]];
            i = parents[i];
        }
        unreachable!("дерево компонент без циклов")
    }
    let mut parents: Vec<_> = (0..creatures.len()).collect();
    for (i, v) in creatures.iter().enumerate() {
        grid.for_each_near(v.x, v.y, v.pheno.vision, |j, x, y| {
            if j <= i || creatures[j].flock != v.flock {
                return;
            }
            let d2 = (x - v.x).powi(2) + (y - v.y).powi(2);
            if d2 <= v.pheno.vision2.min(creatures[j].pheno.vision2) {
                let a = root(&mut parents, i);
                let b = root(&mut parents, j);
                parents[a.max(b)] = a.min(b);
            }
        });
    }
    let mut groups = BTreeMap::<(u64, usize), Vec<usize>>::new();
    for (i, v) in creatures.iter().enumerate() {
        groups.entry((v.flock, root(&mut parents, i))).or_default().push(i);
    }
    let mut main = BTreeMap::<u64, (usize, u64, usize)>::new();
    for (&(tag, r), group) in &groups {
        let min_id = group.iter().map(|&i| creatures[i].id).min().unwrap();
        if main.get(&tag).is_none_or(|&(n, id, _)| group.len() > n || (group.len() == n && min_id < id)) {
            main.insert(tag, (group.len(), min_id, r));
        }
    }
    let mut kept = Vec::new();
    let mut count = 0;
    for ((tag, r), mut group) in groups {
        if main[&tag].2 == r || group.len() < 3 {
            continue;
        }
        group.sort_unstable_by_key(|&i| creatures[i].id);
        let ids: std::collections::BTreeSet<_> = group.iter().map(|&i| creatures[i].id).collect();
        let watch = watches
            .iter()
            .filter(|w| w.flock == tag && w.anchors.iter().all(|a| ids.contains(a)))
            .min_by_key(|w| w.since)
            .cloned()
            .unwrap_or(SplitWatch {
                flock: tag,
                anchors: [creatures[group[0]].id, creatures[group[1]].id, creatures[group[2]].id],
                since: tick,
            });
        if tick.saturating_sub(watch.since) >= 600 {
            let tag = *next;
            *next += 1;
            count += 1;
            for i in group {
                creatures[i].flock = tag;
                creatures[i].mind.social = Default::default();
                creatures[i].mind.attack = None;
            }
        } else {
            kept.push(watch);
        }
    }
    *watches = kept;
    count
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlockGoal {
    pub x: f64,
    pub y: f64,
    pub tx: f64,
    pub ty: f64,
}

#[derive(Clone, Debug)]
pub struct Flock {
    pub alarmed: bool,
    pub food_goal: bool,
    pub food_since: u64,
    pub last_food: Option<u64>,
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
            alarmed: false,
            food_goal: false,
            food_since: 0,
            last_food: None,
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
        if !f.food_goal
            && f.members >= 2
            && (f.remaining == 0 || (f.goal.x - f.goal.tx).hypot(f.goal.y - f.goal.ty) < 100.0)
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

/// Выбор кормового места за два прохода, без перебора всех существ для каждой стаи.
pub(crate) fn food_goals(flocks: &mut BTreeMap<u64, Flock>, creatures: &mut [Creature], tick: u64) {
    if !tick.is_multiple_of(60) {
        return;
    }
    let mut best = BTreeMap::<u64, (f64, crate::social::Food)>::new();
    for v in creatures.iter() {
        let Some(food) = v.mind.social.observed_food.filter(|f| f.fresh(tick)) else { continue };
        let Some(f) = flocks.get(&v.flock).filter(|f| f.members >= 2) else { continue };
        let travel = (food.x - f.goal.x).hypot(food.y - f.goal.y) / v.pheno.speed.max(0.01);
        let score = v.pheno.plant_energy * v.pheno.plant_efficiency / (travel + 1.0);
        if best
            .get(&v.flock)
            .is_none_or(|(old, prior)| score > *old || (score == *old && food.observer < prior.observer))
        {
            best.insert(v.flock, (score, food));
        }
    }
    for (tag, f) in flocks.iter_mut() {
        if let Some((_, food)) = best.get(tag) {
            f.last_food = Some(food.tick);
            if !f.food_goal || tick.saturating_sub(f.food_since) >= 180 {
                f.goal.tx = food.x;
                f.goal.ty = food.y;
                f.food_goal = true;
                f.food_since = tick;
            }
        } else if f.food_goal && f.last_food.is_none_or(|t| tick.saturating_sub(t) >= 180) {
            f.food_goal = false;
            f.remaining = 0;
        }
    }
    for v in creatures {
        v.flock_goal = flocks.get(&v.flock).filter(|f| f.members >= 2).map(|f| f.goal);
    }
}
