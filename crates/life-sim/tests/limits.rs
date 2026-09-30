//! A run under limits: odd limits do not crash it, they boil down to something sensible; each guard
//! stops a run on its own and says which it was.

use std::time::Duration;

use life_core::WorldConfig;
use life_sim::{Limits, SimResult, StopReason, simulate};

/// A sample step of 0 means «every tick», not a division by zero.
#[test]
fn нулевой_шаг_срезов_значит_каждый_тик() {
    let limits = Limits { ticks: 20, sample_every: 0, ..Default::default() };
    let res = simulate(&WorldConfig::default(), &limits, |_| {});
    assert_eq!(res.history.len(), 21, "стартовый срез и по одному на каждый тик");
    assert_eq!(res.snapshots.len(), res.history.len());
}

/// What every stopped run keeps: the tick it stopped at is the last in its history and samples.
fn stopped(res: &SimResult, reason: StopReason) {
    assert_eq!(res.stop, reason, "stopped at tick {}", res.ticks_done);
    assert_eq!(res.ok(), reason == StopReason::Done);
    assert_eq!(res.world.tick, res.ticks_done, "{reason:?}");
    assert_eq!(res.last().tick, res.ticks_done, "{reason:?}: the final state is in the history");
    assert_eq!(res.snapshots.last().map(|s| s.tick), Some(res.ticks_done), "{reason:?}");
}

/// The population ceiling, the work budget, the extinction and the wall clock each end a run
/// before its ticks; with none of them reached it runs to the end. The run's time is bounded by
/// them, not by the ticks, so every headless run relies on these.
#[test]
fn every_guard_stops_a_run_and_says_which() {
    let cfg = WorldConfig { seed: 1, ..Default::default() };
    let limits = Limits { ticks: 300, deadline: Duration::from_secs(600), ..Default::default() };
    let run = |limits: &Limits| simulate(&cfg, limits, |_| {});

    let whole = run(&limits);
    stopped(&whole, StopReason::Done);
    assert_eq!(whole.ticks_done, 300);

    // 20 founders over a ceiling of 5: the first tick
    let crowded = run(&Limits { max_creatures: 5, ..limits.clone() });
    stopped(&crowded, StopReason::Explosion);
    assert_eq!(crowded.ticks_done, 1);

    // the budget of «creatures × plants» is spent in the first ticks
    let busy = run(&Limits { max_total_work: 1.0, ..limits.clone() });
    stopped(&busy, StopReason::Overload);
    assert!(
        busy.ticks_done < 50 && busy.total_work > 1.0,
        "{} ticks, work {}",
        busy.ticks_done,
        busy.total_work
    );

    let empty = simulate(&WorldConfig { n_creatures: Some(0), ..cfg.clone() }, &limits, |_| {});
    stopped(&empty, StopReason::Extinct);
    assert_eq!(empty.ticks_done, 1);

    // no time at all: the first tick is already late
    let late = run(&Limits { deadline: Duration::ZERO, ..limits.clone() });
    stopped(&late, StopReason::Deadline);
    assert_eq!(late.ticks_done, 1);
}

/// The ceiling and the budget are the base world's: a world ten times as large holds ten times the
/// creatures and does a hundred times the work (creatures × plants), so limits it would outgrow at
/// once, were they not scaled, leave it alone.
#[test]
fn the_guards_grow_with_the_world() {
    let limits = Limits {
        ticks: 5,
        max_creatures: 25,
        max_total_work: 2000.0,
        deadline: Duration::from_secs(600),
        ..Default::default()
    };
    let big = simulate(&WorldConfig { seed: 2, scale: 10.0, ..Default::default() }, &limits, |_| {});
    stopped(&big, StopReason::Done);
    assert!(
        big.world.creatures.len() > limits.max_creatures && big.total_work > limits.max_total_work,
        "past the base world's limits: {} creatures, work {}",
        big.world.creatures.len(),
        big.total_work
    );
}
