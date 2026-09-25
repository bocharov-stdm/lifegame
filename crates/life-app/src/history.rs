//! Ограниченная по возрасту история партии для графиков. Хроника хранится отдельно.

use std::collections::VecDeque;

use life_core::genome::creature;
use life_sim::observe::Snapshot;

/// Видимое окно истории, измеряется тиками мира, а не числом срезов.
pub const WINDOW_TICKS: u64 = 10_000;

/// Сглаженные численности и накопленные выстрелы на данном тике.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub plants: f64,
    pub creatures: f64,
    pub shots: u64,
    pub genom: Option<[f64; creature::N]>,
}

#[derive(Clone, Debug)]
pub struct Series<T> {
    points: VecDeque<(u64, T)>,
}

impl<T> Default for Series<T> {
    fn default() -> Self {
        Self { points: VecDeque::new() }
    }
}

impl<T> Series<T> {
    pub fn push(&mut self, tick: u64, point: T) {
        self.points.push_back((tick, point));
        let oldest = tick.saturating_sub(WINDOW_TICKS);
        while self.points.front().is_some_and(|(at, _)| *at < oldest) {
            self.points.pop_front();
        }
    }

    pub fn points(&self) -> Vec<&T> {
        self.points.iter().map(|(_, p)| p).collect()
    }

    pub fn first(&self) -> Option<&T> {
        self.points.front().map(|(_, p)| p)
    }

    pub fn last(&self) -> Option<&T> {
        self.points.back().map(|(_, p)| p)
    }
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub counts: Series<Sample>,
    pub snapshots: Series<Snapshot>,
}

impl History {
    pub fn add_sample(&mut self, s: Sample) {
        self.counts.push(s.tick, s);
    }

    pub fn add_snapshot(&mut self, s: Snapshot) {
        self.snapshots.push(s.tick, s);
    }

    pub fn shots_in_window(&self) -> u64 {
        match (self.counts.first(), self.counts.last()) {
            (Some(first), Some(last)) => last.shots.saturating_sub(first.shots),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn окно_держит_ровно_последние_десять_тысяч_тиков() {
        let mut s = Series::default();
        for tick in [0, 1, 10_000, 10_001, 20_001] {
            s.push(tick, tick);
        }
        let points: Vec<u64> = s.points().into_iter().copied().collect();
        assert_eq!(points, [10_001, 20_001]);
        assert_eq!(s.first(), Some(&10_001));
        assert_eq!(s.last(), Some(&20_001));
    }

    #[test]
    fn число_выстрелов_относится_к_видимому_окну() {
        let mut h = History::default();
        for (tick, shots) in [(0, 2), (10_000, 7), (10_010, 12)] {
            h.add_sample(Sample { tick, plants: 0.0, creatures: 0.0, shots, genom: None });
        }
        assert_eq!(h.shots_in_window(), 5);
    }
}
