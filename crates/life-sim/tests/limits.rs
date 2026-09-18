//! Прогон под лимитами: странные лимиты не роняют его, а сводятся к понятному.

use life_core::WorldConfig;
use life_sim::{Limits, simulate};

/// Шаг срезов 0 — «каждый тик», а не деление на ноль.
#[test]
fn нулевой_шаг_срезов_значит_каждый_тик() {
    let limits = Limits { ticks: 20, sample_every: 0, ..Default::default() };
    let res = simulate(&WorldConfig::default(), &limits, |_| {});
    assert_eq!(res.history.len(), 21, "стартовый срез и по одному на каждый тик");
    assert_eq!(res.snapshots.len(), res.history.len());
}
