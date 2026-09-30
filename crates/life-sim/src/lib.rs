//! A simulation run without a window under four independent limits.
//!
//! A tick's cost grows with the population, so the number of ticks does NOT bound the run's
//! time. Besides ticks there is a population ceiling (catches a population explosion), a
//! computation budget (guarantees completion, about the same on different machines) and a
//! wall-clock deadline (insurance on a very slow machine).

pub mod cores;
pub mod observe;

use std::fmt;
use std::time::{Duration, Instant};

use life_core::{Stats, World, WorldConfig};
use observe::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Done,
    Explosion,
    Overload,
    Extinct,
    Deadline,
}

impl StopReason {
    /// The machine name — for the report's JSON.
    pub fn key(self) -> &'static str {
        match self {
            StopReason::Done => "done",
            StopReason::Explosion => "explosion",
            StopReason::Overload => "overload",
            StopReason::Extinct => "extinct",
            StopReason::Deadline => "deadline",
        }
    }
}

impl fmt::Display for StopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StopReason::Done => "готово",
            StopReason::Explosion => "взрыв численности",
            StopReason::Overload => "перегрузка",
            StopReason::Extinct => "вымерли",
            StopReason::Deadline => "дедлайн",
        })
    }
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub ticks: u64,
    pub sample_every: u64,
    /// The ceiling of creatures for the base world; in a big world it grows with the area.
    pub max_creatures: usize,
    /// The «creatures x plants» budget, summed over ticks, for the base world.
    /// With the neighbour grid it is a heavily overestimated but honest upper bound of the work.
    pub max_total_work: f64,
    pub deadline: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            ticks: 400,
            sample_every: 100,
            max_creatures: 3000,
            max_total_work: 25e6,
            deadline: Duration::from_secs(15),
        }
    }
}

pub struct SimResult {
    /// The `world.stats()` snapshots every `sample_every` ticks plus the final one.
    pub history: Vec<Stats>,
    /// Detailed samples at the same moments as `history`: the genes' spread, the depth, the
    /// counters of births and deaths (see `observe`).
    pub snapshots: Vec<Snapshot>,
    pub ticks_done: u64,
    pub stop: StopReason,
    pub elapsed: Duration,
    /// The final state — for checking invariants.
    pub world: World,
    pub total_work: f64,
}

impl SimResult {
    pub fn ok(&self) -> bool {
        self.stop == StopReason::Done
    }

    pub fn last(&self) -> &Stats {
        self.history.last().expect("в истории всегда есть хотя бы стартовый снимок")
    }

    pub fn peak(&self, key: impl Fn(&Stats) -> usize) -> usize {
        self.history.iter().map(key).max().unwrap_or(0)
    }

    pub fn ms_per_tick(&self) -> f64 {
        if self.ticks_done == 0 { 0.0 } else { self.elapsed.as_secs_f64() * 1000.0 / self.ticks_done as f64 }
    }
}

/// Run the world from `cfg` under the limits. `on_tick` is called after every tick.
pub fn simulate(cfg: &WorldConfig, limits: &Limits, mut on_tick: impl FnMut(&World)) -> SimResult {
    let world = World::new(cfg);
    run(world, limits, &mut on_tick)
}

/// The same for a ready-made world (the tests assemble it by hand).
pub fn run(mut world: World, limits: &Limits, on_tick: &mut dyn FnMut(&World)) -> SimResult {
    let area = world.space.area_ratio();
    let max_creatures = (limits.max_creatures as f64 * area) as usize;
    // the work ~ creatures x plants, both quantities grow with the area
    let max_work = limits.max_total_work * area * area;

    // a step of 0 means «sample as often as possible», not a division by zero
    let sample_every = limits.sample_every.max(1);
    let mut history = vec![world.stats()];
    let mut snapshots = vec![Snapshot::of(&world)];
    let mut stop = StopReason::Done;
    let started = Instant::now();
    let mut done = 0;
    let mut total_work = 0.0;

    for tick in 1..=limits.ticks {
        done = tick;
        world.step();
        on_tick(&world);

        let creatures = world.creatures.len();
        total_work += (world.creatures.len() * world.plants.len()) as f64;

        if creatures > max_creatures {
            stop = StopReason::Explosion;
            break;
        }
        // a NaN budget stops the run instead of switching the budget off
        if total_work > max_work || max_work.is_nan() {
            stop = StopReason::Overload;
            break;
        }
        if creatures == 0 {
            stop = StopReason::Extinct;
            break;
        }
        if started.elapsed() > limits.deadline {
            stop = StopReason::Deadline;
            break;
        }
        if tick.is_multiple_of(sample_every) {
            history.push(world.stats());
            snapshots.push(Snapshot::of(&world));
        }
    }

    if history.last().map(|h| h.tick) != Some(world.tick) {
        history.push(world.stats()); // the final snapshot is always in the history
        snapshots.push(Snapshot::of(&world));
    }
    SimResult { history, snapshots, ticks_done: done, stop, elapsed: started.elapsed(), world, total_work }
}
