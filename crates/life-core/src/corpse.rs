//! Трупы: конечный запас мясной пищи, доступный со следующего тика после смерти.

use crate::config::ENERGY_PER_SIZE;
use crate::creature::Creature;
use crate::grid::Grid;
use crate::plant::PORTIONS;

pub const DECAY_TICKS: u64 = 600;

#[derive(Clone, Debug, PartialEq)]
pub struct Corpse {
    pub owner: u64,
    pub flock: u64,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// Питательность сразу после смерти.
    pub initial: f64,
    /// Не съеденная и не разложившаяся питательность.
    pub remaining: f64,
    pub born: u64,
    /// Последний тик учтённого разложения.
    pub last_decay: u64,
}

impl Corpse {
    pub fn from_creature(v: &Creature, born: u64) -> Self {
        let grown = (v.pheno.size - v.birth_size).max(0.0) * ENERGY_PER_SIZE;
        let birth = v.birth_size * ENERGY_PER_SIZE * 0.25;
        let initial = v.energy.max(0.0) + grown + birth;
        Self {
            owner: v.id,
            flock: v.flock,
            x: v.x,
            y: v.y,
            size: v.pheno.size,
            initial,
            remaining: initial,
            born,
            last_decay: born,
        }
    }

    /// Равномерное разложение по первоначальной ценности, даже если часть съедена.
    /// Возвращает `false`, когда труп нужно убрать.
    pub fn decay(&mut self, now: u64) -> bool {
        if now >= self.born.saturating_add(DECAY_TICKS) {
            self.remaining = 0.0;
        } else if now > self.last_decay {
            let elapsed = now - self.last_decay.max(self.born);
            self.remaining = (self.remaining - self.initial * elapsed as f64 / DECAY_TICKS as f64).max(0.0);
        }
        self.last_decay = self.last_decay.max(now);
        self.remaining > 0.0
    }

    /// Доступная сейчас порция до эффективности плотоядности.
    pub fn portion(&self, plant_energy: f64) -> f64 {
        (plant_energy / f64::from(PORTIONS)).max(self.initial / 12.0).min(self.remaining)
    }

    /// Одна порция. Даже если этот метод вызван до отдельной фазы разложения,
    /// срок жизни и расход учитываются ровно один раз.
    pub fn bite(&mut self, now: u64, plant_energy: f64) -> Option<f64> {
        if now <= self.born || !self.decay(now) {
            return None;
        }
        let amount = self.portion(plant_energy);
        self.remaining = (self.remaining - amount).max(0.0);
        Some(amount)
    }
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

/// Ближайший доступный труп, которого касается едок.
pub fn contact(
    grid: &Grid,
    corpses: &[Corpse],
    (x, y): (f64, f64),
    reach: f64,
    max_half: f64,
    now: u64,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    grid.for_each_near(x, y, reach + max_half, |i, cx, cy| {
        let c = &corpses[i];
        let d2 = (cx - x).powi(2) + (cy - y).powi(2);
        if c.remaining > 0.0
            && c.born < now
            && d2 <= (reach + c.size * 0.5).powi(2)
            && best.is_none_or(|(j, old)| d2 < old || (d2 == old && c.owner < corpses[j].owner))
        {
            best = Some((i, d2));
        }
    });
    best.map(|(i, _)| i)
}

/// Один укус по ближайшему трупу в контакте. Едоков мир обходит по ID,
/// поэтому конкуренция за остаток воспроизводима.
pub fn bite(
    grid: &Grid,
    corpses: &mut [Corpse],
    pos: (f64, f64),
    reach: f64,
    max_half: f64,
    plant_energy: f64,
    now: u64,
) -> Option<f64> {
    let i = contact(grid, corpses, pos, reach, max_half, now)?;
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
        let expected = 40.0 + 20.0 * ENERGY_PER_SIZE + 20.0 * ENERGY_PER_SIZE * 0.25;
        assert_eq!(c.initial, expected);
        assert_eq!((c.owner, c.flock, c.born), (7, 3, 12));
    }

    #[test]
    fn труп_не_съедается_в_тик_смерти_делится_на_порции_и_разлагается() {
        let mut c = Corpse::from_creature(&body(), 10);
        assert_eq!(c.bite(10, 50.0), None);
        let initial = c.initial;
        let amount = c.bite(11, 50.0).unwrap();
        assert_eq!(amount, (initial / 12.0).max(10.0));
        assert!((c.remaining - (initial - initial / 600.0 - amount)).abs() < 1e-12);
        assert!(c.decay(11)); // повторный учёт того же тика ничего не меняет
        let mut untouched = Corpse::from_creature(&body(), 10);
        assert!(untouched.decay(609));
        assert!(!untouched.decay(610));
        assert_eq!(untouched.remaining, 0.0);
        assert!(!c.decay(610));
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
        let value = bite(&grid, &mut corpses, (1000.0, 1000.0), 30.0, 20.0, 50.0, 4).unwrap();
        assert_eq!(value, 10.0);
        assert_eq!(corpses[0].remaining, corpses[0].initial);
        assert!(corpses[1].remaining < corpses[1].initial);
    }

    #[test]
    fn разложение_не_зависит_от_частоты_проверок() {
        let mut each_tick = Corpse::from_creature(&body(), 1);
        let mut once = each_tick.clone();
        for tick in 2..=300 {
            each_tick.decay(tick);
        }
        once.decay(300);
        assert!((each_tick.remaining - once.remaining).abs() < 1e-10);
        assert_eq!(each_tick.bite(300, 50.0), once.bite(300, 50.0));
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
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 80.0, 30.0, 2), None);
        assert_eq!(contact(&grid, &corpses, (1000.0, 1000.0), 100.0, 30.0, 2), Some(1));
    }
}
