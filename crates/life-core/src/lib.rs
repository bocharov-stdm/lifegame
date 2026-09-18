//! Движок Tiny Life: эволюционная песочница — растения, травоядные с геномом
//! из семи генов и хищники. Отбор возникает сам, он не прописан.
//!
//! Этот crate ничего не знает об экране и не зависит ни от чего графического:
//! на нём держатся тесты и подбор баланса без окна. Рисует `life-app`.

pub mod config;
pub mod genome;
pub mod grid;
pub mod plant;
pub mod predator;
pub mod rng;
pub mod rules;
pub mod space;
pub mod vegetarian;
pub mod world;

pub use genome::Genom;
pub use rules::Rules;
pub use space::Space;
pub use world::{Counters, Creature, Stats, World, WorldConfig};
