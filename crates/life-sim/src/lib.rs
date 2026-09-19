//! Прогон симуляции без окна под четырьмя независимыми лимитами.
//!
//! Стоимость тика растёт вместе с популяцией, поэтому число тиков время прогона
//! НЕ ограничивает. Кроме тиков есть потолок популяции (ловит взрыв численности),
//! бюджет вычислений (гарантирует завершение, примерно одинаково на разных
//! машинах) и дедлайн по часам (страховка на совсем медленной машине).

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
    /// Машинное имя — для JSON отчёта.
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
    /// Потолок существ на базовый мир; в большом мире растёт с площадью.
    pub max_creatures: usize,
    /// Бюджет «травоядные x растения», просуммированный по тикам, на базовый мир.
    /// С сеткой соседей это сильно завышенная, но честная верхняя оценка работы.
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
    /// Снимки `world.stats()` раз в `sample_every` тиков плюс финальный.
    pub history: Vec<Stats>,
    /// Подробные срезы в те же моменты, что и `history`: разброс генов,
    /// глубина, счётчики рождений и смертей (см. `observe`).
    pub snapshots: Vec<Snapshot>,
    pub ticks_done: u64,
    pub stop: StopReason,
    pub elapsed: Duration,
    /// Конечное состояние — для проверки инвариантов.
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

/// Прогнать мир из `cfg` под лимитами. `on_tick` вызывается после каждого тика.
pub fn simulate(cfg: &WorldConfig, limits: &Limits, mut on_tick: impl FnMut(&World)) -> SimResult {
    let world = World::new(cfg);
    run(world, limits, &mut on_tick)
}

/// То же для уже готового мира (тесты собирают его руками).
pub fn run(mut world: World, limits: &Limits, on_tick: &mut dyn FnMut(&World)) -> SimResult {
    let area = world.space.area_ratio();
    let max_creatures = (limits.max_creatures as f64 * area) as usize;
    // работа ~ травоядные x растения, обе величины растут с площадью
    let max_work = limits.max_total_work * area * area;

    // шаг 0 — «снимать как можно чаще», а не деление на ноль
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

        let creatures = world.vegetarians.len();
        total_work += (world.vegetarians.len() * world.plants.len()) as f64;

        if creatures > max_creatures {
            stop = StopReason::Explosion;
            break;
        }
        if total_work > max_work {
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
        history.push(world.stats()); // финальный снимок всегда в истории
        snapshots.push(Snapshot::of(&world));
    }
    SimResult { history, snapshots, ticks_done: done, stop, elapsed: started.elapsed(), world, total_work }
}
