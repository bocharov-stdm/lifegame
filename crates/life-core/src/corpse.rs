//! Corpses: a finite stock of meat food, available from the tick after death.
//!
//! Three stages (`Stage`), each its own food (a column of `DIET_DIGESTION`). A corpse's clock
//! (`CorpseClock`, the world's rules at its death; `CORPSE_*` in config by default):
//! - **fresh** until `fresh`, lying where the creature died;
//! - **rot** from then: it sinks `sink` a tick, straight down, to its place in the lowest `rest` %
//!   of the depth (a hash of the owner's id, no draws; never above where it died), and its flesh
//!   decays evenly down to the bones by `decay` ticks from death. It can be eaten all the way
//!   down; the speed is absolute, so a scavenger catches a sinking corpse in a world of any height;
//! - **bones**: `CORPSE_SKELETON_SHARE` of its meat, left when the flesh is eaten or has rotted
//!   away, whichever comes first. They sink `bones_sink` a tick to the corpse's resting place
//!   (never up) and decay evenly over `bones` ticks from then.
//!
//! Everything is a function of the tick, so the result does not depend on how often it is checked.

use crate::config::{
    CORPSE_DECAY_TICKS, CORPSE_FRESH_TICKS, CORPSE_REST_PCT, CORPSE_SINK_SPEED, CORPSE_SKELETON_SHARE,
    GROWTH_ENERGY_PER_SIZE, SKELETON_SINK_SPEED, SKELETON_TICKS,
};
use crate::creature::Creature;
use crate::grid::Grid;
use crate::plant::PORTIONS;
use crate::space::Space;

#[derive(Clone, Debug, PartialEq)]
pub struct Corpse {
    pub owner: u64,
    pub x: f64,
    /// Where it lies now: `y0` while fresh, then sinking to `bottom`.
    pub y: f64,
    /// Depth of death.
    pub y0: f64,
    /// Where it comes to rest, its skeleton too; never above `y0`.
    pub bottom: f64,
    pub size: f64,
    /// Nutrition right after death.
    pub initial: f64,
    /// Nutrition neither eaten nor rotted away.
    pub remaining: f64,
    pub born: u64,
    /// The last tick whose decay has been accounted.
    pub last_decay: u64,
    /// It came to rest with meat left (for the counters).
    pub settled: bool,
    /// Its bones, once the flesh is eaten or rotted away.
    pub skeleton: Option<Skeleton>,
    pub clock: CorpseClock,
}

/// A corpse's times, ticks from death, how fast it sinks and where it rests: set by the world's
/// rules at its death.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorpseClock {
    pub fresh: u64,
    /// How far it sinks a tick after the fresh time.
    pub sink: f64,
    /// The flesh has rotted down to the bones this many ticks from death.
    pub decay: u64,
    /// The lowest share of the depth where it comes to rest, %.
    pub rest: f64,
    /// How long the bones lie, ticks from when they were left, and how fast they sink.
    pub bones: u64,
    pub bones_sink: f64,
}

impl Default for CorpseClock {
    fn default() -> Self {
        CorpseClock {
            fresh: CORPSE_FRESH_TICKS,
            sink: CORPSE_SINK_SPEED,
            decay: CORPSE_DECAY_TICKS,
            rest: CORPSE_REST_PCT,
            bones: SKELETON_TICKS,
            bones_sink: SKELETON_SINK_SPEED,
        }
    }
}

impl CorpseClock {
    pub fn of(rules: &crate::Rules) -> CorpseClock {
        CorpseClock {
            fresh: rules.corpse_fresh as u64,
            sink: rules.corpse_sink,
            decay: rules.corpse_decay as u64,
            rest: rules.corpse_rest,
            bones: rules.corpse_bones as u64,
            bones_sink: rules.corpse_bones_sink,
        }
    }
}

/// What a corpse is to an eater: each stage is its own food, with its own column of
/// `DIET_DIGESTION` and `DIET_OWN` (after plants), read in `Phenotype::corpse_efficiency`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Fresh,
    Rot,
    Bones,
}

/// What is left of a corpse once its flesh is eaten or rotted away: bones that sink to the deep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skeleton {
    /// Tick the bones were left.
    pub at: u64,
    /// Depth they were left at, and where they come to rest.
    pub y1: f64,
    pub rest: f64,
    /// Its meat when left.
    pub store: f64,
}

/// A bite of a corpse: the raw portion, its stage at that tick and where the corpse lies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bite {
    pub amount: f64,
    pub stage: Stage,
    pub x: f64,
    pub y: f64,
}

impl Corpse {
    /// With config's clock (tests and tools); the world passes its rules' (`from_creature_in`).
    pub fn from_creature(v: &Creature, born: u64) -> Self {
        Self::from_creature_in(v, born, CorpseClock::default())
    }

    pub fn from_creature_in(v: &Creature, born: u64, clock: CorpseClock) -> Self {
        let initial = meat(v);
        Self {
            owner: v.id,
            x: v.x,
            y: v.y,
            y0: v.y,
            bottom: resting_place(v.space(), v.id, v.pheno.half, clock.rest, 0xB077_0000_0000_0000).max(v.y),
            size: v.pheno.size,
            initial,
            remaining: initial,
            born,
            last_decay: born,
            settled: false,
            skeleton: None,
            clock,
        }
    }

    /// Its stage at tick `now` (decayed up to it: `decay` leaves the bones when the flesh is gone).
    pub fn stage(&self, now: u64) -> Stage {
        if self.skeleton.is_some() {
            Stage::Bones
        } else if now.saturating_sub(self.born) <= self.clock.fresh {
            Stage::Fresh
        } else {
            Stage::Rot
        }
    }

    /// Depth at tick `now`: it sinks at its speed once no longer fresh; the bones sink faster to
    /// the same place.
    pub fn y_at(&self, now: u64) -> f64 {
        match self.skeleton {
            Some(s) => (s.y1 + self.clock.bones_sink * now.saturating_sub(s.at) as f64).min(s.rest),
            None => {
                let sinking = now.saturating_sub(self.born).saturating_sub(self.clock.fresh);
                (self.y0 + self.clock.sink * sinking as f64).min(self.bottom)
            }
        }
    }

    /// Decay and sinking up to tick `now`. The flesh decays evenly by its first worth, even if some
    /// was eaten, down to the bones by `decay`; the tick it is gone the bones are left, and they
    /// decay by their own store and time. Returns `false` when the corpse is to be removed.
    pub fn decay(&mut self, now: u64) -> bool {
        if self.skeleton.is_none() && now > self.last_decay {
            let bones = self.bones();
            let rate = (self.initial - bones) / self.clock.decay as f64;
            let from = self.last_decay.max(self.born);
            // the tick the flesh is gone: never later than `decay` from death, where an uneaten
            // corpse gets to (the division may land a hair past it)
            let gone = if rate > 0.0 {
                from.saturating_add(((self.remaining - bones) / rate).ceil().max(0.0) as u64)
            } else {
                from
            }
            .min(self.born.saturating_add(self.clock.decay));
            if gone <= now {
                self.remaining = bones.min(self.remaining);
                self.last_decay = gone;
                self.strip(gone);
            } else {
                self.remaining -= rate * (now - from) as f64;
                self.last_decay = now;
            }
        }
        if let Some(s) = self.skeleton {
            let span = self.clock.bones;
            if now >= s.at.saturating_add(span) {
                self.remaining = 0.0;
            } else if now > self.last_decay {
                let elapsed = now - self.last_decay.max(s.at);
                self.remaining = (self.remaining - s.store * elapsed as f64 / span as f64).max(0.0);
            }
        }
        self.last_decay = self.last_decay.max(now);
        self.y = self.y_at(self.last_decay);
        if self.remaining > 0.0
            && self.skeleton.is_none()
            && self.last_decay > self.born + self.clock.fresh
            && self.y >= self.bottom
        {
            self.settled = true;
        }
        self.remaining > 0.0
    }

    /// Meat the eaters cannot strip from a corpse: its skeleton's share. None once stripped.
    fn bones(&self) -> f64 {
        if self.skeleton.is_some() { 0.0 } else { self.initial * CORPSE_SKELETON_SHARE }
    }

    /// The portion available now before digestion: a corpse is eaten down to its bones, then the
    /// skeleton to nothing.
    pub fn portion(&self, plant_energy: f64) -> f64 {
        let size = (plant_energy / f64::from(PORTIONS)).max(self.initial / 12.0);
        let bones = self.bones();
        if self.remaining > bones { size.min(self.remaining - bones) } else { size.min(self.remaining) }
    }

    /// One portion. Even when called before the decay phase, the corpse's time and what it lost
    /// are counted exactly once. A corpse eaten down to its bones by this bite leaves them.
    pub fn bite(&mut self, now: u64, plant_energy: f64) -> Option<Bite> {
        if now <= self.born || !self.decay(now) {
            return None;
        }
        let amount = self.portion(plant_energy);
        let stage = self.stage(now);
        self.remaining = (self.remaining - amount).max(self.bones().min(self.remaining));
        if self.remaining <= self.bones() {
            self.strip(now);
        }
        Some(Bite { amount, stage, x: self.x, y: self.y })
    }

    /// Its bones are left at tick `at`, where the corpse lies then.
    fn strip(&mut self, at: u64) {
        let y = self.y_at(at);
        self.y = y;
        self.skeleton = Some(Skeleton { at, y1: y, rest: self.bottom.max(y), store: self.remaining });
    }
}

/// The meat of creature `v`'s body: the energy that went into growing it since birth
/// (`GROWTH_ENERGY_PER_SIZE` a unit of diameter), plus what is left in the tank. A body that starved
/// is still meat. The body a creature is born with (or spawned with) was nobody's food, so it is
/// no one's meat: when it counted at the full tank's worth (`size × ENERGY_PER_SIZE`), a parent that
/// bore nearly empty children and ate their corpses made energy from nothing — a world of 9500
/// scavengers on 34 plants. A hunter weighs its prey by the same measure.
pub fn meat(v: &Creature) -> f64 {
    v.energy.max(0.0) + (v.pheno.size - v.birth_size).max(0.0) * GROWTH_ENERGY_PER_SIZE
}

/// Where the remains of creature `id` with body radius `half` come to rest: in the lowest `pct` of
/// the depth, spread by a hash of the id (with `salt`) so the dead do not lie on one line.
fn resting_place(space: &Space, id: u64, half: f64, pct: f64, salt: u64) -> f64 {
    let band = space.height * pct / 100.0;
    let spread = (crate::rng::mix(id ^ salt) % 1024) as f64 / 1023.0;
    (space.height - half.min(space.height / 2.0) - spread * band).max(0.0)
}

/// The nearest available corpse that the eater touches and may eat (`eats`).
pub fn contact(
    grid: &Grid,
    corpses: &[Corpse],
    pos: (f64, f64),
    reach: f64,
    max_half: f64,
    now: u64,
    eats: impl Fn(&Corpse) -> bool,
) -> Option<usize> {
    contact_by(grid, |i| &corpses[i], pos, reach, max_half, now, eats)
}

/// `contact` with the corpses read through `corpse(i)`: the world's eating phase reads a corpse
/// somebody already claimed this tick from its copy, the others as they lie.
pub fn contact_by<'a>(
    grid: &Grid,
    corpse: impl Fn(usize) -> &'a Corpse,
    (x, y): (f64, f64),
    reach: f64,
    max_half: f64,
    now: u64,
    eats: impl Fn(&Corpse) -> bool,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    grid.for_each_near(x, y, reach + max_half, |i, cx, cy| {
        let c = corpse(i);
        let d2 = (cx - x).powi(2) + (cy - y).powi(2);
        if c.remaining > 0.0
            && c.born < now
            && d2 <= (reach + c.size * 0.5).powi(2)
            && best.is_none_or(|(j, old)| d2 < old || (d2 == old && c.owner < corpse(j).owner))
            && eats(c)
        {
            best = Some((i, d2));
        }
    });
    best.map(|(i, _)| i)
}

/// One bite of the nearest edible corpse in contact. The world goes through the eaters by ID,
/// so competition for the remainder is reproducible.
#[allow(clippy::too_many_arguments)]
pub fn bite(
    grid: &Grid,
    corpses: &mut [Corpse],
    pos: (f64, f64),
    reach: f64,
    max_half: f64,
    plant_energy: f64,
    now: u64,
    eats: impl Fn(&Corpse) -> bool,
) -> Option<Bite> {
    let i = contact(grid, corpses, pos, reach, max_half, now, eats)?;
    corpses[i].bite(now, plant_energy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreatureGenome, Rules, Space, grid::Grid, rng::Rng};

    fn body() -> Creature {
        let mut v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE,
            Some(1000.0),
            Some(1000.0),
            Some(40.0),
            Rng::new(1),
        );
        v.id = 7;
        v
    }

    /// The grown body is meat at what growing it cost, plus what was left in the tank; the body
    /// it was born with is not: a newborn's corpse is its tank.
    #[test]
    fn meat_is_the_grown_body_plus_the_tank() {
        let mut v = body();
        v.birth_size = 20.0;
        let grown = (v.pheno.size - 20.0) * GROWTH_ENERGY_PER_SIZE;
        assert_eq!(grown, 45.0);
        let c = Corpse::from_creature(&v, 12);
        assert_eq!(c.initial, 40.0 + grown);
        v.energy = 0.0;
        assert_eq!(Corpse::from_creature(&v, 12).initial, grown, "a starved body");
        assert_eq!((c.owner, c.born), (7, 12));
        v.birth_size = v.pheno.size;
        v.energy = 3.0;
        assert_eq!(Corpse::from_creature(&v, 12).initial, 3.0, "a newborn is its tank");
    }

    #[test]
    fn труп_не_съедается_в_тик_смерти_делится_на_порции_и_разлагается() {
        let mut c = Corpse::from_creature(&body(), 10);
        assert_eq!(c.bite(10, 50.0), None);
        let initial = c.initial;
        let bones = initial * CORPSE_SKELETON_SHARE;
        let bite = c.bite(11, 50.0).unwrap();
        assert_eq!(bite.amount, (initial / 12.0).max(10.0));
        assert_eq!((bite.stage, bite.x, bite.y), (Stage::Fresh, 1000.0, 1000.0), "fresh and where it died");
        // the flesh decays, the bones do not
        let lost = (initial - bones) / CORPSE_DECAY_TICKS as f64;
        assert!((c.remaining - (initial - lost - bite.amount)).abs() < 1e-12);
        assert!(c.decay(11)); // accounting the same tick again changes nothing
        let mut untouched = Corpse::from_creature(&body(), 10);
        assert!(untouched.decay(10 + CORPSE_DECAY_TICKS - 1) && untouched.skeleton.is_none());
        assert!(untouched.decay(10 + CORPSE_DECAY_TICKS), "the bones are left");
        assert!((untouched.remaining - bones).abs() < 1e-9, "{} of {bones}", untouched.remaining);
        assert_eq!(untouched.skeleton.map(|s| s.at), Some(10 + CORPSE_DECAY_TICKS));
        assert!(untouched.decay(10 + CORPSE_DECAY_TICKS + SKELETON_TICKS - 1));
        assert!(!untouched.decay(10 + CORPSE_DECAY_TICKS + SKELETON_TICKS), "the bones are gone too");
        assert_eq!(untouched.remaining, 0.0);
        // one bitten into rots to its bones sooner
        assert!(c.decay(10 + CORPSE_DECAY_TICKS - 1) && c.skeleton.is_some());
    }

    /// Two corpses in contact at the same distance: the one with the smaller owner id is bitten,
    /// and neither on the tick of its death.
    #[test]
    fn the_nearest_corpse_and_one_portion_between_two_eaters() {
        let mut corpses = vec![Corpse::from_creature(&body(), 3), Corpse::from_creature(&body(), 3)];
        corpses[0].owner = 8;
        corpses[1].owner = 7;
        corpses[0].x = 1005.0;
        corpses[1].x = 995.0;
        let mut grid = Grid::new(256.0);
        grid.rebuild(&Space::default(), corpses.iter().map(|c| (c.x, c.y)));
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 30.0, 20.0, 3, |_| true), None);
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 30.0, 20.0, 4, |_| true), Some(1));
        // a portion: a fifth of a plant or a twelfth of the body, whichever is more
        let portion = (50.0 / 5.0_f64).max(corpses[1].initial / 12.0);
        let value = bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4, |_| true).unwrap();
        assert_eq!((value.amount, value.x), (portion, 995.0));
        assert_eq!(bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4, |_| false), None);
        assert_eq!(corpses[0].remaining, corpses[0].initial);
        assert!(corpses[1].remaining < corpses[1].initial);
    }

    /// Decay, sinking and the bones being left do not depend on how often the corpse is checked.
    #[test]
    fn разложение_не_зависит_от_частоты_проверок() {
        for end in [400, CORPSE_DECAY_TICKS + 700] {
            let mut each_tick = Corpse::from_creature(&body(), 1);
            let mut once = each_tick.clone();
            for tick in 2..=end {
                each_tick.decay(tick);
            }
            once.decay(end);
            assert!((each_tick.remaining - once.remaining).abs() < 1e-9, "{end}");
            assert_eq!((each_tick.y, each_tick.skeleton), (once.y, once.skeleton), "{end}: sinking too");
            let (a, b) = (each_tick.bite(end, 50.0).unwrap(), once.bite(end, 50.0).unwrap());
            assert!((a.amount - b.amount).abs() < 1e-9 && (a.stage, a.y) == (b.stage, b.y), "{a:?} {b:?}");
        }
    }

    #[test]
    fn контакт_учитывает_радиус_каждого_трупа() {
        let mut small = Corpse::from_creature(&body(), 1);
        small.owner = 2;
        small.x = 1110.0;
        small.size = 10.0;
        let mut big = Corpse::from_creature(&body(), 1);
        big.owner = 3;
        big.x = 1120.0;
        big.size = 60.0;
        let corpses = [small, big];
        let mut grid = Grid::new(256.0);
        grid.rebuild(&Space::default(), corpses.iter().map(|c| (c.x, c.y)));
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 80.0, 30.0, 2, |_| true), None);
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 100.0, 30.0, 2, |_| true), Some(1));
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 100.0, 30.0, 2, |c| c.owner != 3), None);
    }

    /// Fresh first, then rot at once: it sinks at its speed to its place in the lowest quarter of
    /// the depth, and can be eaten on the way down.
    #[test]
    fn труп_гниёт_и_оседает_на_дно() {
        let mut c = Corpse::from_creature(&body(), 100);
        let space = Space::default();
        assert!(c.bottom > space.height * (1.0 - CORPSE_REST_PCT / 100.0) - 20.0 - 1e-9);
        assert!(c.bottom <= space.height - 20.0, "the body stays in the world");
        for tick in [101, 100 + CORPSE_FRESH_TICKS] {
            assert_eq!((c.stage(tick), c.y_at(tick)), (Stage::Fresh, 1000.0), "fresh at {tick}");
        }
        assert_eq!(c.stage(101 + CORPSE_FRESH_TICKS), Stage::Rot, "then rot at once");
        let at_rest = 100 + CORPSE_FRESH_TICKS + ((c.bottom - 1000.0) / CORPSE_SINK_SPEED).ceil() as u64;
        let mut last = 1000.0;
        for tick in (100 + CORPSE_FRESH_TICKS..=at_rest).step_by(10) {
            let y = c.y_at(tick);
            assert!(y >= last && y - last <= CORPSE_SINK_SPEED * 10.0 + 1e-9, "only sinks, at its speed");
            last = y;
        }
        assert_eq!(c.y_at(at_rest), c.bottom, "at rest");
        let sinking = 100 + CORPSE_FRESH_TICKS + 100;
        let bite = c.clone().bite(sinking, 50.0).expect("eaten on the way down");
        assert_eq!((bite.stage, bite.y), (Stage::Rot, c.y_at(sinking)));
        c.decay(at_rest + 50);
        assert_eq!(c.y, c.bottom);
        assert!(
            c.remaining > c.initial / 2.0,
            "it reaches its place with most of its flesh: {}",
            c.remaining
        );
        let mut deep = body();
        deep.y = space.height - 5.0;
        let d = Corpse::from_creature(&deep, 0);
        assert_eq!(d.bottom, deep.y, "one that died deeper does not rise");
    }

    /// Eaten down to a tenth, a corpse leaves its bones at once: they sink fast to the corpse's
    /// resting place and lie there `SKELETON_TICKS` from then, not from death.
    #[test]
    fn скелет_остаётся_от_объеденного_трупа() {
        let space = Space::default();
        let mut c = Corpse::from_creature(&body(), 100);
        let bones = c.initial * CORPSE_SKELETON_SHARE;
        let mut stripped = None;
        for tick in 101..140 {
            let bite = c.bite(tick, 50.0).unwrap();
            assert_eq!(bite.stage, Stage::Fresh, "fresh bites, the stripping one too");
            if c.skeleton.is_some() {
                stripped = Some(tick);
                break;
            }
        }
        let at = stripped.expect("eaten down to its bones");
        let s = c.skeleton.unwrap();
        assert_eq!((s.at, s.y1), (at, 1000.0));
        assert!((s.store - bones).abs() < 1e-9 && c.remaining == s.store, "a tenth is left: {}", s.store);
        assert_eq!(c.stage(at), Stage::Bones);
        let zone = space.height * (1.0 - CORPSE_REST_PCT / 100.0);
        assert!(s.rest >= zone - 20.0 && s.rest <= space.height - 20.0, "near the bottom: {}", s.rest);
        assert_eq!(s.rest, c.bottom, "where the corpse would rest");
        let mut once = c.clone();
        let mut last = c.y;
        let falling = ((s.rest - s.y1) / SKELETON_SINK_SPEED).ceil() as u64;
        assert!(falling < 100, "bones fall fast: {falling} ticks");
        for tick in at + 1..=at + falling {
            assert!(c.decay(tick));
            assert!(c.y >= last && c.y - last <= SKELETON_SINK_SPEED + 1e-9, "only sinks, at their speed");
            last = c.y;
        }
        assert_eq!(c.y, s.rest, "at rest");
        assert!(c.decay(100 + CORPSE_DECAY_TICKS), "the corpse's own time no longer counts");
        assert!(c.decay(at + SKELETON_TICKS - 1));
        assert!(once.decay(at + SKELETON_TICKS - 1));
        assert!((c.remaining - once.remaining).abs() < 1e-12 && c.y == once.y, "any check frequency");
        assert!(!c.decay(at + SKELETON_TICKS), "gone after its time");
        // bones are eaten to nothing
        let mut eaten = Corpse::from_creature(&body(), 100);
        for tick in 101..140 {
            if eaten.bite(tick, 50.0).is_none() {
                break;
            }
        }
        assert!(eaten.skeleton.is_some() && eaten.remaining == 0.0);
    }

    /// One left alone comes to rest with its flesh and rots down to its bones where it lies. One
    /// that died deep does not rise as bones.
    #[test]
    fn an_untouched_corpse_rots_down_to_its_bones() {
        let mut c = Corpse::from_creature(&body(), 0);
        let at_rest = CORPSE_FRESH_TICKS + ((c.bottom - c.y0) / CORPSE_SINK_SPEED).ceil() as u64;
        assert!(c.decay(at_rest - 1) && !c.settled);
        assert!(c.decay(at_rest) && c.settled, "lay at rest");
        assert!(c.decay(CORPSE_DECAY_TICKS));
        let s = c.skeleton.expect("rotted to its bones");
        assert_eq!((s.at, s.y1, s.rest), (CORPSE_DECAY_TICKS, c.bottom, c.bottom), "where it lay");
        assert_eq!(c.stage(CORPSE_DECAY_TICKS), Stage::Bones);
        let mut deep = body();
        deep.y = Space::default().height - 25.0;
        let mut d = Corpse::from_creature(&deep, 0);
        for tick in 1..40 {
            d.bite(tick, 50.0);
        }
        let s = d.skeleton.expect("eaten down");
        assert_eq!(s.rest, s.y1, "never rises");
        let mut fresh = Corpse::from_creature(&body(), 0);
        for tick in 1..40 {
            fresh.bite(tick, 50.0);
        }
        assert!(!fresh.settled, "eaten fresh, never lay on the bottom");
    }
}
