//! The lurker («затаившийся»): decides like the standard strategy, but while it sees no food it
//! wanders at a third of its cruise pace (`SLOW_PACE` × `cruise`, `Phenotype::slow_speed`). To food
//! it goes at full speed. It saves where food is scarce, and finds it slower.

use super::standard;
use super::strategy::{Intent, Me, Mind};
use crate::rng::Rng;
use crate::senses::Senses;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl Senses) -> Intent {
    let planned = standard::plan(me, mind, rng, senses, me.pheno.slow_speed);
    standard::wander_slow(mind, planned)
}
