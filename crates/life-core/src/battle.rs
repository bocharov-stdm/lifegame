//! Battles of flocks for room. A territorial flock squeezed to its smallest circle with no free
//! place nearby (`Flock::cornered`) starts a battle with every flock whose circle touches its
//! own; battles that share a flock merge. Everyone in a battle fights everyone else: an adult
//! fighter strikes the nearest adult of another flock of the battle it sees
//! (`territory::State::prepare_full` assigns the targets). A flock without territoriality only
//! strikes back. Kin and freshly split groups never strike each other.
//!
//! A flock that has lost half of the adults it brought leaves the battle and moves away; the
//! others keep the place. A battle lasts at most `BATTLE_TICKS`. Battles exist only with
//! combat on. Everything runs in id order: the result depends on the seed only.
use std::collections::BTreeMap;

use crate::{
    Space,
    creature::Creature,
    flock::{Circle, Flock},
    kin_grace::Grace,
};

/// The longest battle.
pub const BATTLE_TICKS: u64 = 300;
/// A flock that left a battle neither starts nor joins another for this long.
pub const CALM_TICKS: u32 = 600;
/// Circles closer than this gap touch.
const TOUCH: f64 = 8.0;
/// A hungrier adult eats instead of fighting; a wounded one retreats as from any fight.
pub const FIGHTER_FULLNESS: f64 = 0.25;

#[derive(Clone, Debug, PartialEq)]
pub struct Battle {
    pub id: u64,
    pub since: u64,
    /// Flock -> adults it brought into the battle.
    pub sides: BTreeMap<u64, usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battles {
    pub active: Vec<Battle>,
    next: u64,
}

/// What happened to battles this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub started: u64,
    pub retreats: u64,
}

fn touching(a: Circle, b: Circle) -> bool {
    (a.x - b.x).hypot(a.y - b.y) <= a.radius + b.radius + TOUCH
}

impl Battles {
    pub fn clear(&mut self, flocks: &mut BTreeMap<u64, Flock>) {
        for b in self.active.drain(..) {
            for id in b.sides.keys() {
                if let Some(f) = flocks.get_mut(id) {
                    f.battle = None;
                }
            }
        }
    }

    /// End battles and let beaten flocks go, then start battles of cornered flocks.
    pub fn update(
        &mut self,
        flocks: &mut BTreeMap<u64, Flock>,
        creatures: &[Creature],
        space: &Space,
        tick: u64,
        grace: &Grace,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        let mut adults = BTreeMap::<u64, usize>::new();
        for v in creatures.iter().filter(|v| v.alive && v.adult()) {
            *adults.entry(v.flock).or_default() += 1;
        }
        let mut leaving = Vec::new();
        let mut beaten = Vec::new();
        for b in &mut self.active {
            b.sides.retain(|id, brought| {
                let present = flocks.get(id).is_some_and(|f| f.circle.is_some());
                let lost = 2 * adults.get(id).copied().unwrap_or(0) < *brought;
                if present && lost {
                    beaten.push(*id);
                }
                present && !lost
            });
            if b.sides.len() <= 1 || tick.saturating_sub(b.since) >= BATTLE_TICKS {
                leaving.extend(b.sides.keys().copied());
                b.sides.clear();
            }
        }
        self.active.retain(|b| !b.sides.is_empty());
        for id in leaving.iter().chain(&beaten) {
            if let Some(f) = flocks.get_mut(id) {
                f.battle = None;
                f.calm = CALM_TICKS;
            }
        }
        // a flock that lost its circle (fewer than two members) dropped out as well
        for (id, f) in flocks.iter_mut() {
            if f.battle.is_some_and(|b| !self.active.iter().any(|a| a.id == b && a.sides.contains_key(id))) {
                f.battle = None;
            }
        }
        outcome.retreats = beaten.len() as u64;
        crate::flock::retreat(flocks, space, &beaten);

        let cornered: Vec<u64> =
            flocks.iter().filter(|(_, f)| f.cornered && f.battle.is_none()).map(|(&id, _)| id).collect();
        for id in cornered {
            let Some(own) = flocks[&id].circle.filter(|_| flocks[&id].battle.is_none()) else { continue };
            let zone: Vec<u64> = flocks
                .iter()
                .filter(|&(&other, g)| {
                    other != id
                        && g.members >= 2
                        && g.calm == 0
                        && g.circle.is_some_and(|c| touching(own, c))
                        && !grace.contains(id, other, tick)
                })
                .map(|(&other, _)| other)
                .collect();
            if zone.is_empty() {
                // only kin around: nobody to fight, so the flock moves next time
                flocks.get_mut(&id).unwrap().calm = CALM_TICKS;
                continue;
            }
            let mut joined: Vec<u64> = std::iter::once(id).chain(zone).collect();
            let mut merged: Vec<u64> = joined.iter().filter_map(|x| flocks[x].battle).collect();
            merged.sort_unstable();
            merged.dedup();
            let target = match merged.first() {
                Some(&b) => b,
                None => {
                    self.active.push(Battle { id: self.next, since: tick, sides: BTreeMap::new() });
                    self.next += 1;
                    outcome.started += 1;
                    self.next - 1
                }
            };
            // battles that share a flock become one
            for &other in merged.iter().skip(1) {
                let k = self.active.iter().position(|b| b.id == other).unwrap();
                let absorbed = self.active.remove(k);
                joined.extend(absorbed.sides.keys().copied());
                let into = self.active.iter_mut().find(|b| b.id == target).unwrap();
                into.since = into.since.min(absorbed.since);
                into.sides.extend(absorbed.sides);
            }
            let battle = self.active.iter_mut().find(|b| b.id == target).unwrap();
            for x in joined {
                battle.sides.entry(x).or_insert_with(|| adults.get(&x).copied().unwrap_or(0));
                flocks.get_mut(&x).unwrap().battle = Some(target);
            }
        }
        outcome
    }
}
