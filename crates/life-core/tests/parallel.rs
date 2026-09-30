//! The per-creature phases on many threads (the `parallel` feature, `par.rs`) give the very world
//! one thread gives: every creature, plant and corpse bit for bit, tick after tick.
#![cfg(feature = "parallel")]

use life_core::{Shape, World, WorldConfig};

/// Everything the world is made of, exactly (`{:?}` prints each float so that it reads back).
fn state(w: &World) -> String {
    format!("{} {:?} {:?} {:?} {:?}", w.tick, w.creatures, w.plants, w.corpses, w.counters)
}

/// The world's states at checkpoints, stepped in a pool of `threads`.
fn run(threads: usize) -> (Vec<String>, usize) {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("пул потоков");
    pool.install(|| {
        let cfg = WorldConfig { seed: 3, scale: 10.0, shape: Shape::R2x1, ..Default::default() };
        let mut w = World::new(&cfg);
        let (mut states, mut most) = (Vec::new(), 0);
        for tick in 1..=300 {
            w.step();
            most = most.max(w.creatures.len());
            if tick % 50 == 0 {
                states.push(state(&w));
            }
        }
        (states, most)
    })
}

#[test]
fn many_threads_make_the_same_world_as_one() {
    let (one, most) = run(1);
    assert!(
        most > life_core::config::PARALLEL_MIN,
        "the world must be big enough to be split among threads: {most}"
    );
    let (many, _) = run(8);
    for (k, (a, b)) in one.iter().zip(&many).enumerate() {
        assert!(a == b, "checkpoint {k}: the worlds differ");
    }
}
