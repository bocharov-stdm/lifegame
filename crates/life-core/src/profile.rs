//! Where a tick's time goes, phase by phase (`World::set_profiling`). Off by default: then a
//! phase's end costs one branch and no clock is read. It never touches the world's state, so the
//! world goes bit for bit the same measured or not.

use std::time::Instant;

/// The phases of `World::step`, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// New plants.
    Plants,
    /// Corpses decaying, shots fading, old age.
    Ageing,
    /// The food and corpse grids.
    Grids,
    /// The herd's snapshot (`Herd::rebuild`).
    Herd,
    /// Every creature's program and step (`Creature::step`).
    Decisions,
    /// Plants eaten and corpses claimed on contact.
    EatPlants,
    /// Strikes and shots, the prey grid included.
    Combat,
    /// The survivors eating corpses.
    EatCorpses,
    /// Division.
    Division,
    /// Removing the dead, leaving corpses, adding children.
    Sweep,
}

impl Phase {
    pub const ALL: [Phase; 10] = [
        Phase::Plants,
        Phase::Ageing,
        Phase::Grids,
        Phase::Herd,
        Phase::Decisions,
        Phase::EatPlants,
        Phase::Combat,
        Phase::EatCorpses,
        Phase::Division,
        Phase::Sweep,
    ];
    pub const N: usize = Phase::ALL.len();

    pub fn key(self) -> &'static str {
        [
            "plants",
            "ageing",
            "grids",
            "herd",
            "decisions",
            "eat_plants",
            "combat",
            "eat_corpses",
            "division",
            "sweep",
        ][self as usize]
    }

    /// The game's and the report's name.
    pub fn label(self) -> &'static str {
        ["растения", "старость", "сетки", "стадо", "решения", "еда", "бой", "трупы", "деление", "уборка"]
            [self as usize]
    }
}

/// Time spent in each phase since profiling was switched on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhaseTimes {
    pub nanos: [u64; Phase::N],
    pub ticks: u64,
}

impl PhaseTimes {
    pub fn total_nanos(&self) -> u64 {
        self.nanos.iter().sum()
    }

    /// A phase's mean time a tick, ms.
    pub fn ms_per_tick(&self, phase: Phase) -> f64 {
        self.nanos[phase as usize] as f64 / 1e6 / self.ticks.max(1) as f64
    }

    /// A phase's share of all the measured time, 0‒1.
    pub fn share(&self, phase: Phase) -> f64 {
        self.nanos[phase as usize] as f64 / self.total_nanos().max(1) as f64
    }

    /// The times of `other` added to these.
    pub fn add(&mut self, other: &PhaseTimes) {
        for (a, b) in self.nanos.iter_mut().zip(other.nanos) {
            *a += b;
        }
        self.ticks += other.ticks;
    }
}

/// One tick's stopwatch: each `lap` gives the time since the last one to a phase. Made off, it
/// reads no clock.
pub(crate) struct Stopwatch {
    last: Option<Instant>,
    times: PhaseTimes,
}

impl Stopwatch {
    pub fn start(on: bool) -> Stopwatch {
        Stopwatch { last: on.then(Instant::now), times: PhaseTimes { ticks: 1, ..Default::default() } }
    }

    #[inline]
    pub fn lap(&mut self, phase: Phase) {
        if let Some(last) = self.last {
            let now = Instant::now();
            self.times.nanos[phase as usize] += now.duration_since(last).as_nanos() as u64;
            self.last = Some(now);
        }
    }

    /// This tick's times into the world's total, if it is measured.
    pub fn finish(self, total: &mut Option<PhaseTimes>) {
        if let (Some(total), Some(_)) = (total, self.last) {
            total.add(&self.times);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_off_stopwatch_measures_nothing() {
        let mut total = Some(PhaseTimes::default());
        let mut off = Stopwatch::start(false);
        off.lap(Phase::Plants);
        off.finish(&mut total);
        assert_eq!(total, Some(PhaseTimes::default()));
        let mut on = Stopwatch::start(true);
        std::hint::black_box((0..1000).sum::<u64>());
        on.lap(Phase::Decisions);
        on.finish(&mut total);
        let t = total.unwrap();
        assert_eq!(t.ticks, 1);
        assert_eq!(t.total_nanos(), t.nanos[Phase::Decisions as usize]);
        assert!((t.share(Phase::Decisions) - 1.0).abs() < 1e-12 || t.total_nanos() == 0);
    }
}
