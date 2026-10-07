//! The whole food chain in the player's world, as the energy test (`engine.rs`) and golden's case I
//! (`golden.rs`) need it: one predicate for both, which `the_chain_by_seed` prints when a change of
//! behaviour leaves their seed without it.

use life_core::World;

/// Somebody lives on, and hunting, shots, fresh meat, rot, bones and division all took part.
pub fn whole_chain(w: &World) -> bool {
    let c = &w.counters;
    !w.creatures.is_empty()
        && c.born > 0
        && c.combat > 0
        && c.ranged_shots > 0
        && c.meat_bites > c.rot_bites
        && c.rot_bites > 0
        && c.bone_bites > 0
}
