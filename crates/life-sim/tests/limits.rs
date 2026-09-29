//! A run under limits: odd limits do not crash it, they boil down to something sensible.

use life_core::WorldConfig;
use life_sim::{Limits, simulate};

/// A sample step of 0 means «every tick», not a division by zero.
#[test]
fn нулевой_шаг_срезов_значит_каждый_тик() {
    let limits = Limits { ticks: 20, sample_every: 0, ..Default::default() };
    let res = simulate(&WorldConfig::default(), &limits, |_| {});
    assert_eq!(res.history.len(), 21, "стартовый срез и по одному на каждый тик");
    assert_eq!(res.snapshots.len(), res.history.len());
}
