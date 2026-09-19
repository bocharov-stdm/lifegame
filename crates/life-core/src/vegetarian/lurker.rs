//! «Затаившийся»: решает как стандартный, но пока не видит еды — бродит и
//! возвращается в свой слой медленным ходом (`SLOW_PACE`). К еде и от хищника —
//! на полной скорости. Экономит там, где еды мало, зато и находит её медленнее.

use super::standard::{self, Mode};
use super::strategy::{Intent, Me, Mind};
use crate::rng::Rng;
use crate::senses::VegetarianSenses;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl VegetarianSenses) -> Intent {
    let (intent, mode) = standard::plan(me, mind, rng, senses, me.pheno.slow_speed);
    Intent { slow: mode == Mode::Wander, ..intent }
}
