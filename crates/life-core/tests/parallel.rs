//! The per-creature phases on many threads (the `parallel` feature, `par.rs`) give the very world
//! one thread gives: every creature, plant and corpse bit for bit, tick after tick — on rayon's
//! global pool, on a pool of the caller's, with no pool at all, and with the pool changed and the
//! phases timed on the fly, as the game does.
#![cfg(feature = "parallel")]

use std::sync::Arc;

use life_core::par::Threads;
use life_core::{Shape, World, WorldConfig};

/// Everything the world is made of, exactly (`{:?}` prints each float so that it reads back).
fn state(w: &World) -> String {
    format!("{} {:?} {:?} {:?} {:?}", w.tick, w.creatures, w.plants, w.corpses, w.counters)
}

/// The world's states at checkpoints, with `before` called ahead of every tick; the most creatures
/// it had, and the world at the end.
fn run(mut before: impl FnMut(&mut World, u64)) -> (Vec<String>, usize, World) {
    let cfg = WorldConfig { seed: 3, scale: 10.0, shape: Shape::R2x1, ..Default::default() };
    let mut w = World::new(&cfg);
    let (mut states, mut most) = (Vec::new(), 0);
    for tick in 1..=300 {
        before(&mut w, tick);
        w.step();
        most = most.max(w.creatures.len());
        if tick % 50 == 0 {
            states.push(state(&w));
        }
    }
    (states, most, w)
}

/// A run on `threads` from the first tick.
fn on(threads: Threads) -> Vec<String> {
    run(|w, tick| {
        if tick == 1 {
            w.set_threads(threads.clone());
        }
    })
    .0
}

fn pool(threads: usize) -> Arc<rayon::ThreadPool> {
    Arc::new(rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("пул потоков"))
}

#[test]
fn many_threads_make_the_same_world_as_one() {
    let (one, most, _) = run(|w, tick| {
        if tick == 1 {
            w.set_threads(Threads::One);
        }
    });
    assert!(
        most > life_core::config::PARALLEL_MIN,
        "the world must be big enough to be split among threads: {most}"
    );
    // the game's way: its pool resized by the settings while the world runs, every tick timed
    let (three, eight) = (pool(3), pool(8));
    let (switched, _, timed) = run(|w, tick| match tick {
        1 => {
            w.set_profiling(true);
            w.set_threads(Threads::Pool(three.clone()));
        }
        100 => w.set_threads(Threads::One),
        200 => w.set_threads(Threads::Pool(eight.clone())),
        _ => {}
    });
    let times = timed.phase_times().expect("timed");
    assert!(times.ticks == 300 && times.total_nanos() > 0, "{times:?}");
    let runs = [
        ("a pool of 8", on(Threads::Pool(pool(8)))),
        ("a pool of 1", on(Threads::Pool(pool(1)))),
        ("the global pool of 8", pool(8).install(|| on(Threads::Global))),
        ("pools changed on the fly, timed", switched),
    ];
    for (name, states) in runs {
        assert_eq!(states.len(), one.len());
        for (k, (a, b)) in one.iter().zip(&states).enumerate() {
            assert!(a == b, "{name}, checkpoint {k}: the world differs from one thread's");
        }
    }
}
