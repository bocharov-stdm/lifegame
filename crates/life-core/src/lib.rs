//! The lifegame engine: an evolutionary sandbox of plants and creatures with a genome (a gene
//! table, `genome/`). Selection emerges; nothing scripts it.
//!
//! This crate knows nothing of the screen and depends on nothing graphical: the tests and the
//! balance runs stand on it without a window. `life-app` draws.
//!
//! Flocks (family circles, territories, battles, the social layer) lived here until the tag
//! `flocks-final`.

pub mod config;
pub mod corpse;
pub mod creature;
pub mod flora;
pub mod genome;
pub mod grid;
pub mod plant;
pub mod profile;
pub mod rng;
pub mod rules;
pub mod senses;
pub mod space;
pub mod units;
pub mod world;

pub use genome::{CreatureGenome, Genome};
pub use rules::Rules;
pub use space::{Shape, Space};
pub use world::{Counters, DietCounters, Stats, World, WorldConfig};

mod combat;
pub mod par;
pub use combat::Shot;
