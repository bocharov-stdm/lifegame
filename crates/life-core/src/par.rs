//! Per-creature work spread over the processor's threads, with the `parallel` feature (rayon);
//! without it, one thread. Only work that reads a shared snapshot and writes nothing but its own
//! item goes here, so the world goes bit for bit the same on any number of threads: each
//! creature's arithmetic is the same whichever thread does it, and results keep their order.

#[cfg(feature = "parallel")]
use crate::config::{PARALLEL_CHUNK, PARALLEL_MIN};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Which threads the world's per-creature phases run on (`World::set_threads`).
#[derive(Clone, Debug, Default)]
pub enum Threads {
    /// rayon's global pool (`life-report --threads`); one thread without the feature.
    #[default]
    Global,
    /// One thread, no pool at all.
    One,
    /// A pool of the caller's (the game's, sized in its settings). The thread that steps the world
    /// waits while the pool works, and does the rest of the tick itself.
    #[cfg(feature = "parallel")]
    Pool(std::sync::Arc<rayon::ThreadPool>),
}

/// `f` on every item.
#[cfg(feature = "parallel")]
pub(crate) fn for_each_mut<T: Send>(threads: &Threads, items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    match threads {
        _ if items.len() < PARALLEL_MIN => items.iter_mut().for_each(f),
        Threads::One => items.iter_mut().for_each(f),
        Threads::Global => items.par_iter_mut().with_min_len(PARALLEL_CHUNK).for_each(f),
        Threads::Pool(pool) => pool.install(|| items.par_iter_mut().with_min_len(PARALLEL_CHUNK).for_each(f)),
    }
}

#[cfg(not(feature = "parallel"))]
pub(crate) fn for_each_mut<T>(_: &Threads, items: &mut [T], f: impl Fn(&mut T)) {
    items.iter_mut().for_each(f);
}

/// `out` becomes `f` of every item, in their order; its buffer is reused.
#[cfg(feature = "parallel")]
pub(crate) fn map_into<T: Sync, U: Send>(
    threads: &Threads,
    items: &[T],
    out: &mut Vec<U>,
    f: impl Fn(&T) -> U + Sync + Send,
) {
    let parallel =
        |out: &mut Vec<U>| items.par_iter().with_min_len(PARALLEL_CHUNK).map(&f).collect_into_vec(out);
    match threads {
        _ if items.len() < PARALLEL_MIN => sequential(items, out, &f),
        Threads::One => sequential(items, out, &f),
        Threads::Global => parallel(out),
        Threads::Pool(pool) => pool.install(|| parallel(out)),
    }
}

#[cfg(not(feature = "parallel"))]
pub(crate) fn map_into<T, U>(_: &Threads, items: &[T], out: &mut Vec<U>, f: impl Fn(&T) -> U) {
    sequential(items, out, &f);
}

fn sequential<T, U>(items: &[T], out: &mut Vec<U>, f: &impl Fn(&T) -> U) {
    out.clear();
    out.extend(items.iter().map(f));
}
