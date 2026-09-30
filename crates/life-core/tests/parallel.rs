//! The per-creature phases on many threads (the `parallel` feature, `par.rs`) give the very world
//! one thread gives: every creature, plant and corpse bit for bit, tick after tick — on rayon's
//! global pool, on a pool of the caller's, and with no pool at all.
#![cfg(feature = "parallel")]

use std::sync::Arc;

use life_core::par::Threads;
use life_core::{Shape, World, WorldConfig};

/// Everything the world is made of, exactly (`{:?}` prints each float so that it reads back).
fn state(w: &World) -> String {
    format!("{} {:?} {:?} {:?} {:?}", w.tick, w.creatures, w.plants, w.corpses, w.counters)
}

/// The world's states at checkpoints, stepped on `threads`; and the most creatures it had.
fn run(threads: Threads) -> (Vec<String>, usize) {
    let cfg = WorldConfig { seed: 3, scale: 10.0, shape: Shape::R2x1, ..Default::default() };
    let mut w = World::new(&cfg);
    w.set_threads(threads);
    let (mut states, mut most) = (Vec::new(), 0);
    for tick in 1..=300 {
        w.step();
        most = most.max(w.creatures.len());
        if tick % 50 == 0 {
            states.push(state(&w));
        }
    }
    (states, most)
}

fn pool(threads: usize) -> Arc<rayon::ThreadPool> {
    Arc::new(rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("пул потоков"))
}

#[test]
fn many_threads_make_the_same_world_as_one() {
    let (one, most) = run(Threads::One);
    assert!(
        most > life_core::config::PARALLEL_MIN,
        "the world must be big enough to be split among threads: {most}"
    );
    let runs = [
        ("a pool of 8", run(Threads::Pool(pool(8))).0),
        ("a pool of 1", run(Threads::Pool(pool(1))).0),
        ("the global pool of 8", pool(8).install(|| run(Threads::Global).0)),
    ];
    for (name, states) in runs {
        assert_eq!(states.len(), one.len());
        for (k, (a, b)) in one.iter().zip(&states).enumerate() {
            assert!(a == b, "{name}, checkpoint {k}: the world differs from one thread's");
        }
    }
}
