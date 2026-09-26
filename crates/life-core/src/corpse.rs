//! Трупы: конечный запас мясной пищи, доступный со следующего тика после смерти.
//!
//! A corpse is fresh for `CORPSE_FRESH_TICKS` and lies where the creature died. Then it rots and
//! sinks: its rot share and its depth follow one smooth step, and by `CORPSE_ROTTEN_TICKS` it is
//! fully rotten and rests on the bottom (the lowest `CORPSE_BOTTOM_PCT` of the depth, spread by
//! the owner's id, no draws). The path is in shares of the depth, so rot reaches the bottom in a
//! world of any height. Its store decays evenly over `CORPSE_DECAY_TICKS`. Everything is a
//! function of the tick, so the result does not depend on how often it is checked.

use crate::config::{
    CORPSE_BOTTOM_PCT, CORPSE_DECAY_TICKS, CORPSE_FRESH_TICKS, CORPSE_ROTTEN_TICKS, ENERGY_PER_SIZE,
    GROWTH_ENERGY_PER_SIZE,
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
    /// Where its rot comes to rest; never above `y0`.
    pub bottom: f64,
    pub size: f64,
    /// Питательность сразу после смерти.
    pub initial: f64,
    /// Не съеденная и не разложившаяся питательность.
    pub remaining: f64,
    pub born: u64,
    /// Последний тик учтённого разложения.
    pub last_decay: u64,
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
    pub fn from_creature(v: &Creature, born: u64) -> Self {
        let grown = (v.pheno.size - v.birth_size).max(0.0) * GROWTH_ENERGY_PER_SIZE;
        let birth = v.birth_size * ENERGY_PER_SIZE * 0.25;
        let initial = v.energy.max(0.0) + grown + birth;
        Self {
            owner: v.id,
            flock: v.flock,
            x: v.x,
            y: v.y,
            y0: v.y,
            bottom: bottom(v.space(), v.id, v.pheno.half).max(v.y),
            size: v.pheno.size,
            initial,
            remaining: initial,
            born,
            last_decay: born,
        }
    }

    /// Rot share at tick `now`: 0 while fresh, then a smooth step to 1 when fully rotten.
    pub fn rot(&self, now: u64) -> f64 {
        let age = now.saturating_sub(self.born) as f64;
        let t = ((age - CORPSE_FRESH_TICKS as f64) / (CORPSE_ROTTEN_TICKS - CORPSE_FRESH_TICKS) as f64)
            .clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Depth at tick `now`: it sinks as it rots.
    pub fn y_at(&self, now: u64) -> f64 {
        self.y0 + (self.bottom - self.y0) * self.rot(now)
    }

    /// Равномерное разложение по первоначальной ценности, даже если часть съедена, и оседание.
    /// Возвращает `false`, когда труп нужно убрать.
    pub fn decay(&mut self, now: u64) -> bool {
        if now >= self.born.saturating_add(CORPSE_DECAY_TICKS) {
            self.remaining = 0.0;
        } else if now > self.last_decay {
            let elapsed = now - self.last_decay.max(self.born);
            self.remaining =
                (self.remaining - self.initial * elapsed as f64 / CORPSE_DECAY_TICKS as f64).max(0.0);
        }
        self.last_decay = self.last_decay.max(now);
        self.y = self.y_at(self.last_decay);
        self.remaining > 0.0
    }

    /// Доступная сейчас порция до усвоения.
    pub fn portion(&self, plant_energy: f64) -> f64 {
        (plant_energy / f64::from(PORTIONS)).max(self.initial / 12.0).min(self.remaining)
    }

    /// Одна порция. Даже если этот метод вызван до отдельной фазы разложения,
    /// срок жизни и расход учитываются ровно один раз.
    pub fn bite(&mut self, now: u64, plant_energy: f64) -> Option<Bite> {
        if now <= self.born || !self.decay(now) {
            return None;
        }
        let amount = self.portion(plant_energy);
        self.remaining = (self.remaining - amount).max(0.0);
        Some(Bite { amount, rot: self.rot(now), x: self.x, y: self.y })
    }
}

/// Where the rot of creature `id` with body radius `half` comes to rest: in the lowest
/// `CORPSE_BOTTOM_PCT` of the depth, spread by a hash of the id so the dead do not lie on one line.
fn bottom(space: &Space, id: u64, half: f64) -> f64 {
    let band = space.height * CORPSE_BOTTOM_PCT / 100.0;
    let spread = (crate::rng::mix(id ^ 0xB077_0000_0000_0000) % 1024) as f64 / 1023.0;
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

    #[test]
    fn ценность_учитывает_энергию_рост_и_часть_тела_при_рождении() {
        let mut v = body();
        v.birth_size = 20.0;
        let c = Corpse::from_creature(&v, 12);
        let expected = 40.0 + 20.0 * GROWTH_ENERGY_PER_SIZE + 20.0 * ENERGY_PER_SIZE * 0.25;
        assert_eq!(c.initial, expected);
        assert_eq!((c.owner, c.flock, c.born), (7, 3, 12));
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
        let value = bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4, |_| true).unwrap();
        assert_eq!((value.amount, value.x), (10.0, 995.0));
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

    /// Fresh first, then it rots and sinks along one smooth step, and rests on the bottom.
    #[test]
    fn труп_гниёт_и_оседает_на_дно() {
        let mut c = Corpse::from_creature(&body(), 100);
        let space = Space::default();
        assert!(c.bottom > space.height * (1.0 - CORPSE_BOTTOM_PCT / 100.0) - 20.0 - 1e-9);
        assert!(c.bottom <= space.height - 20.0, "the body stays in the world");
        for tick in [101, 100 + CORPSE_FRESH_TICKS] {
            assert_eq!((c.rot(tick), c.y_at(tick)), (0.0, 1000.0), "fresh at {tick}");
        }
        let mut last = (0.0, 1000.0);
        for tick in (100 + CORPSE_FRESH_TICKS..=100 + CORPSE_ROTTEN_TICKS).step_by(10) {
            let now = (c.rot(tick), c.y_at(tick));
            assert!(now.0 >= last.0 && now.1 >= last.1, "only rots and sinks");
            last = now;
        }
        assert_eq!(last, (1.0, c.bottom), "fully rotten on the bottom");
        c.decay(100 + CORPSE_ROTTEN_TICKS + 50);
        assert_eq!(c.y, c.bottom);
        assert!(c.remaining > c.initial * 0.6, "two thirds left for the scavengers: {}", c.remaining);
        let mut deep = body();
        deep.y = space.height - 5.0;
        let d = Corpse::from_creature(&deep, 0);
        assert_eq!(d.bottom, deep.y, "one that died deeper does not rise");
    }
}
