//! Движок Tiny Life: эволюционная песочница — растения и существа с геномом
//! (таблица генов, `genome/`). Отбор возникает сам, он не прописан.
//!
//! Этот crate ничего не знает об экране и не зависит ни от чего графического:
//! на нём держатся тесты и подбор баланса без окна. Рисует `life-app`.

pub mod care;
pub mod config;
pub mod corpse;
pub mod creature;
pub mod flora;
pub mod genome;
pub mod grid;
pub mod kin_grace;
pub mod plant;
pub mod rng;
pub mod rules;
pub mod senses;
pub mod space;
pub mod territory;
pub mod world;

pub use genome::{CreatureGenome, Genome};
pub use rules::Rules;
pub use space::{Shape, Space};
pub use world::{Counters, Stats, World, WorldConfig};

mod combat;
pub use combat::Shot;

pub mod flock;
pub mod social;
