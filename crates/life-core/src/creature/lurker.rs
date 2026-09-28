//! «Затаившийся»: решает как стандартный, но пока не видит еды, бродит втрое медленнее своего
//! крейсерского хода (`SLOW_PACE` × `cruise`, `Phenotype::slow_speed`). К еде — на полной
//! скорости. Экономит там, где еды мало, зато и находит её медленнее.

use super::standard;
use super::strategy::{Intent, Me, Mind};
use crate::rng::Rng;
use crate::senses::Senses;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl Senses) -> Intent {
    let planned = standard::plan(me, mind, rng, senses, me.pheno.slow_speed);
    standard::wander_slow(mind, planned)
}
