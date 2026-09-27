//! Трупы: конечный запас мясной пищи, доступный со следующего тика после смерти.
//!
//! A corpse's clock (`CorpseClock`, the world's rules at its death; `CORPSE_*` in config by
//! default): it is fresh until `fresh` and lies where the creature died. Then it rots along a
//! smooth step, fully rotten at `rotten`, and sinks `sink` a tick, straight down, to its place in
//! the lowest `rest` % of the depth (a hash of the owner's id, no draws; never above where it died).
//! It can be eaten all the way down. The speed is absolute, so a scavenger catches a sinking corpse
//! in a world of any height; in a tall one it may decay before it gets there. Its store decays
//! evenly until `decay`.
//!
//! A corpse eaten down to `CORPSE_SKELETON_SHARE` of its meat becomes a skeleton: rot from that
//! moment, it sinks `SKELETON_SINK_SPEED` a tick to the corpse's resting place (never up) and its
//! store decays evenly over `SKELETON_TICKS` from then. One left
//! alone is never stripped; it just decays. Everything is a function of the tick, so the result
//! does not depend on how often it is checked.

use crate::config::{
    CORPSE_DECAY_TICKS, CORPSE_FRESH_TICKS, CORPSE_REST_PCT, CORPSE_ROTTEN_TICKS, CORPSE_SINK_SPEED,
    CORPSE_SKELETON_SHARE, GROWTH_ENERGY_PER_SIZE, SKELETON_SINK_SPEED, SKELETON_TICKS,
};
use crate::creature::Creature;
use crate::grid::Grid;
use crate::plant::PORTIONS;
use crate::space::Space;

#[derive(Clone, Debug, PartialEq)]
pub struct Corpse {
    pub owner: u64,
    pub flock: u64,
    pub x: f64,
    /// Where it lies now: `y0` while fresh, then sinking to `bottom`.
    pub y: f64,
    /// Depth of death.
    pub y0: f64,
    /// Where it comes to rest, its skeleton too; never above `y0`.
    pub bottom: f64,
    pub size: f64,
    /// Питательность сразу после смерти.
    pub initial: f64,
    /// Не съеденная и не разложившаяся питательность.
    pub remaining: f64,
    pub born: u64,
    /// Последний тик учтённого разложения.
    pub last_decay: u64,
    /// It came to rest with meat left (for the counters).
    pub settled: bool,
    /// Eaten down to its bones.
    pub skeleton: Option<Skeleton>,
    pub clock: CorpseClock,
}

/// A corpse's times, ticks from death, how fast it sinks and where it rests: set by the world's
/// rules at its death.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorpseClock {
    pub fresh: u64,
    pub rotten: u64,
    /// How far it sinks a tick after the fresh time.
    pub sink: f64,
    pub decay: u64,
    /// The lowest share of the depth where it comes to rest, %.
    pub rest: f64,
}

impl Default for CorpseClock {
    fn default() -> Self {
        CorpseClock {
            fresh: CORPSE_FRESH_TICKS,
            rotten: CORPSE_ROTTEN_TICKS,
            sink: CORPSE_SINK_SPEED,
            decay: CORPSE_DECAY_TICKS,
            rest: CORPSE_REST_PCT,
        }
    }
}

impl CorpseClock {
    pub fn of(rules: &crate::Rules) -> CorpseClock {
        CorpseClock {
            fresh: rules.corpse_fresh as u64,
            rotten: rules.corpse_rotten as u64,
            sink: rules.corpse_sink,
            decay: rules.corpse_decay as u64,
            rest: rules.corpse_rest,
        }
    }
}

/// What is left of an eaten corpse: rot that sinks to the deep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skeleton {
    /// Tick it was stripped.
    pub at: u64,
    /// Depth it was stripped at, and where it comes to rest.
    pub y1: f64,
    pub rest: f64,
    /// Its meat when stripped.
    pub store: f64,
}

/// A bite of a corpse: the raw portion, the rot share at that tick and where the corpse lies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bite {
    pub amount: f64,
    pub rot: f64,
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
            flock: v.flock,
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

    /// Rot share at tick `now`: 0 while fresh, then a smooth step to 1 when fully rotten. A
    /// skeleton is rot.
    pub fn rot(&self, now: u64) -> f64 {
        if self.skeleton.is_some() {
            return 1.0;
        }
        smooth_step(now.saturating_sub(self.born), self.clock.fresh, self.clock.rotten)
    }

    /// Depth at tick `now`: it sinks at its speed once no longer fresh; a skeleton sinks faster to
    /// the same place.
    pub fn y_at(&self, now: u64) -> f64 {
        match self.skeleton {
            Some(s) => (s.y1 + SKELETON_SINK_SPEED * now.saturating_sub(s.at) as f64).min(s.rest),
            None => {
                let sinking = now.saturating_sub(self.born).saturating_sub(self.clock.fresh);
                (self.y0 + self.clock.sink * sinking as f64).min(self.bottom)
            }
        }
    }

    /// Равномерное разложение по первоначальной ценности, даже если часть съедена, и оседание.
    /// A skeleton decays by its own store and time. Возвращает `false`, когда труп нужно убрать.
    pub fn decay(&mut self, now: u64) -> bool {
        let (start, store, span) = match self.skeleton {
            Some(s) => (s.at, s.store, SKELETON_TICKS),
            None => (self.born, self.initial, self.clock.decay),
        };
        if now >= start.saturating_add(span) {
            self.remaining = 0.0;
        } else if now > self.last_decay {
            let elapsed = now - self.last_decay.max(start);
            self.remaining = (self.remaining - store * elapsed as f64 / span as f64).max(0.0);
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

    /// Доступная сейчас порция до усвоения: a corpse is eaten down to its bones, then the
    /// skeleton to nothing.
    pub fn portion(&self, plant_energy: f64) -> f64 {
        let size = (plant_energy / f64::from(PORTIONS)).max(self.initial / 12.0);
        let bones = self.bones();
        if self.remaining > bones { size.min(self.remaining - bones) } else { size.min(self.remaining) }
    }

    /// Одна порция. Даже если этот метод вызван до отдельной фазы разложения,
    /// срок жизни и расход учитываются ровно один раз. A corpse eaten down to its bones — by
    /// this bite or by decay before it — becomes a skeleton.
    pub fn bite(&mut self, now: u64, plant_energy: f64) -> Option<Bite> {
        if now <= self.born || !self.decay(now) {
            return None;
        }
        if self.remaining <= self.bones() {
            self.strip(now);
        }
        let amount = self.portion(plant_energy);
        let rot = self.rot(now);
        self.remaining = (self.remaining - amount).max(self.bones().min(self.remaining));
        if self.remaining <= self.bones() {
            self.strip(now);
        }
        Some(Bite { amount, rot, x: self.x, y: self.y })
    }

    /// It becomes a skeleton at tick `now`, where it lies.
    fn strip(&mut self, now: u64) {
        self.skeleton =
            Some(Skeleton { at: now, y1: self.y, rest: self.bottom.max(self.y), store: self.remaining });
    }
}

/// 0 up to `from` ticks of age, then a smooth step to 1 at `to` (at once, if `to` is not later).
fn smooth_step(age: u64, from: u64, to: u64) -> f64 {
    if to <= from {
        return if age >= to { 1.0 } else { 0.0 };
    }
    let t = ((age as f64 - from as f64) / (to - from) as f64).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

/// Ближайший видимый труп. Свежий труп станет целью лишь на следующем тике.
pub fn nearest(
    grid: &Grid,
    corpses: &[Corpse],
    x: f64,
    y: f64,
    r2: f64,
    now: u64,
) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64)> = None;
    grid.for_each_near(x, y, r2.sqrt(), |i, cx, cy| {
        let c = &corpses[i];
        let d2 = (cx - x).powi(2) + (cy - y).powi(2);
        if c.remaining > 0.0
            && c.born < now
            && d2 < r2
            && best.is_none_or(|(j, old)| d2 < old || (d2 == old && c.owner < corpses[j].owner))
        {
            best = Some((i, d2));
        }
    });
    best.map(|(i, _)| (i, corpses[i].x, corpses[i].y))
}

/// Ближайший доступный труп, которого касается едок и который он может есть (`eats`).
pub fn contact(
    grid: &Grid,
    corpses: &[Corpse],
    (x, y): (f64, f64),
    reach: f64,
    max_half: f64,
    now: u64,
    eats: impl Fn(&Corpse) -> bool,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    grid.for_each_near(x, y, reach + max_half, |i, cx, cy| {
        let c = &corpses[i];
        let d2 = (cx - x).powi(2) + (cy - y).powi(2);
        if c.remaining > 0.0
            && c.born < now
            && d2 <= (reach + c.size * 0.5).powi(2)
            && best.is_none_or(|(j, old)| d2 < old || (d2 == old && c.owner < corpses[j].owner))
            && eats(c)
        {
            best = Some((i, d2));
        }
    });
    best.map(|(i, _)| i)
}

/// Один укус по ближайшему съедобному трупу в контакте. Едоков мир обходит по ID,
/// поэтому конкуренция за остаток воспроизводима.
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
        v.flock = 3;
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
        assert_eq!((c.owner, c.flock, c.born), (7, 3, 12));
        v.birth_size = v.pheno.size;
        v.energy = 3.0;
        assert_eq!(Corpse::from_creature(&v, 12).initial, 3.0, "a newborn is its tank");
    }

    #[test]
    fn труп_не_съедается_в_тик_смерти_делится_на_порции_и_разлагается() {
        let mut c = Corpse::from_creature(&body(), 10);
        assert_eq!(c.bite(10, 50.0), None);
        let initial = c.initial;
        let bite = c.bite(11, 50.0).unwrap();
        assert_eq!(bite.amount, (initial / 12.0).max(10.0));
        assert_eq!((bite.rot, bite.x, bite.y), (0.0, 1000.0, 1000.0), "fresh and where it died");
        let lost = initial / CORPSE_DECAY_TICKS as f64;
        assert!((c.remaining - (initial - lost - bite.amount)).abs() < 1e-12);
        assert!(c.decay(11)); // повторный учёт того же тика ничего не меняет
        let mut untouched = Corpse::from_creature(&body(), 10);
        assert!(untouched.decay(10 + CORPSE_DECAY_TICKS - 1));
        assert!(!untouched.decay(10 + CORPSE_DECAY_TICKS));
        assert_eq!(untouched.remaining, 0.0);
        assert!(!c.decay(10 + CORPSE_DECAY_TICKS));
    }

    #[test]
    fn ближайший_труп_и_одна_порция_при_двух_едоках() {
        let mut corpses = vec![Corpse::from_creature(&body(), 3), Corpse::from_creature(&body(), 3)];
        corpses[0].owner = 8;
        corpses[1].owner = 7;
        corpses[0].x = 1005.0;
        corpses[1].x = 995.0;
        let mut grid = Grid::new(256.0);
        grid.rebuild(&Space::default(), corpses.iter().map(|c| (c.x, c.y)));
        assert_eq!(nearest(&grid, &corpses, 1000.0, 1000.0, 100.0, 3), None);
        assert_eq!(nearest(&grid, &corpses, 1000.0, 1000.0, 100.0, 4).unwrap().0, 1);
        // a portion: a fifth of a plant or a twelfth of the body, whichever is more
        let portion = (50.0 / 5.0_f64).max(corpses[1].initial / 12.0);
        let value = bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4, |_| true).unwrap();
        assert_eq!((value.amount, value.x), (portion, 995.0));
        assert_eq!(bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4, |_| false), None);
        assert_eq!(corpses[0].remaining, corpses[0].initial);
        assert!(corpses[1].remaining < corpses[1].initial);
    }

    #[test]
    fn разложение_не_зависит_от_частоты_проверок() {
        let mut each_tick = Corpse::from_creature(&body(), 1);
        let mut once = each_tick.clone();
        for tick in 2..=400 {
            each_tick.decay(tick);
        }
        once.decay(400);
        assert!((each_tick.remaining - once.remaining).abs() < 1e-10);
        assert_eq!(each_tick.y, once.y, "sinking too");
        assert_eq!(each_tick.bite(400, 50.0), once.bite(400, 50.0));
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

    /// Fresh first, then it rots and sinks at its speed to its place in the lowest quarter of the
    /// depth; it can be eaten on the way down.
    #[test]
    fn труп_гниёт_и_оседает_на_дно() {
        let mut c = Corpse::from_creature(&body(), 100);
        let space = Space::default();
        assert!(c.bottom > space.height * (1.0 - CORPSE_REST_PCT / 100.0) - 20.0 - 1e-9);
        assert!(c.bottom <= space.height - 20.0, "the body stays in the world");
        for tick in [101, 100 + CORPSE_FRESH_TICKS] {
            assert_eq!((c.rot(tick), c.y_at(tick)), (0.0, 1000.0), "fresh at {tick}");
        }
        let at_rest = 100 + CORPSE_FRESH_TICKS + ((c.bottom - 1000.0) / CORPSE_SINK_SPEED).ceil() as u64;
        let mut last = (0.0, 1000.0);
        for tick in (100 + CORPSE_FRESH_TICKS..=at_rest).step_by(10) {
            let now = (c.rot(tick), c.y_at(tick));
            assert!(now.0 >= last.0 && now.1 >= last.1, "only rots and sinks");
            assert!(now.1 - last.1 <= CORPSE_SINK_SPEED * 10.0 + 1e-9, "at its speed");
            last = now;
        }
        assert_eq!((c.rot(at_rest), c.y_at(at_rest)), (1.0, c.bottom), "fully rotten and at rest");
        let rotten = 100 + CORPSE_ROTTEN_TICKS;
        assert!(c.rot(rotten) == 1.0 && c.y_at(rotten) < c.bottom - 100.0, "sinks slower than it rots");
        let bite = c.clone().bite(rotten, 50.0).expect("eaten on the way down");
        assert_eq!((bite.rot, bite.y), (1.0, c.y_at(rotten)));
        c.decay(at_rest + 50);
        assert_eq!(c.y, c.bottom);
        assert!(c.remaining > 0.0, "it reaches its place before it decays: {}", c.remaining);
        let mut deep = body();
        deep.y = space.height - 5.0;
        let d = Corpse::from_creature(&deep, 0);
        assert_eq!(d.bottom, deep.y, "one that died deeper does not rise");
    }

    /// Eaten down to a tenth, a corpse becomes a skeleton: rot from then, it sinks to the corpse's
    /// resting place and lies there `SKELETON_TICKS` from the stripping, not from death.
    #[test]
    fn скелет_остаётся_от_объеденного_трупа() {
        let space = Space::default();
        let mut c = Corpse::from_creature(&body(), 100);
        let bones = c.initial * CORPSE_SKELETON_SHARE;
        let mut stripped = None;
        for tick in 101..140 {
            let bite = c.bite(tick, 50.0).unwrap();
            assert_eq!(bite.rot, 0.0, "fresh bites, the stripping one too");
            if c.skeleton.is_some() {
                stripped = Some(tick);
                break;
            }
        }
        let at = stripped.expect("eaten down to its bones");
        let s = c.skeleton.unwrap();
        assert_eq!((s.at, s.y1), (at, 1000.0));
        assert!((s.store - bones).abs() < 1e-9 && c.remaining == s.store, "a tenth is left: {}", s.store);
        assert_eq!(c.rot(at), 1.0, "a skeleton is rot");
        let zone = space.height * (1.0 - CORPSE_REST_PCT / 100.0);
        assert!(s.rest >= zone - 20.0 && s.rest <= space.height - 20.0, "near the bottom: {}", s.rest);
        assert_eq!(s.rest, c.bottom, "where the corpse would rest");
        let mut once = c.clone();
        let mut last = c.y;
        for tick in at + 1..=at + ((s.rest - s.y1) / SKELETON_SINK_SPEED).ceil() as u64 {
            assert!(c.decay(tick));
            assert!(c.y >= last, "only sinks");
            last = c.y;
        }
        assert_eq!(c.y, s.rest, "at rest");
        assert!(c.decay(100 + CORPSE_DECAY_TICKS), "the corpse's own time no longer counts");
        assert!(c.decay(at + SKELETON_TICKS - 1));
        assert!(once.decay(at + SKELETON_TICKS - 1));
        assert!((c.remaining - once.remaining).abs() < 1e-12 && c.y == once.y, "any check frequency");
        assert!(!c.decay(at + SKELETON_TICKS), "gone after its time");
        // a skeleton is eaten to nothing
        let mut eaten = Corpse::from_creature(&body(), 100);
        for tick in 101..140 {
            if eaten.bite(tick, 50.0).is_none() {
                break;
            }
        }
        assert!(eaten.skeleton.is_some() && eaten.remaining == 0.0);
    }

    /// Only an eaten corpse is stripped: one left alone comes to rest and decays away. One
    /// that died deep does not rise as a skeleton.
    #[test]
    fn нетронутый_труп_не_становится_скелетом() {
        let mut c = Corpse::from_creature(&body(), 0);
        let at_rest = CORPSE_FRESH_TICKS + ((c.bottom - c.y0) / CORPSE_SINK_SPEED).ceil() as u64;
        assert!(c.decay(at_rest - 1) && !c.settled);
        assert!(c.decay(at_rest) && c.settled, "lay at rest");
        assert!(!c.decay(CORPSE_DECAY_TICKS));
        assert!(c.skeleton.is_none());
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
