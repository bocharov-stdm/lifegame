//! Per-creature work spread over the processor's threads, with the `parallel` feature (rayon's
//! global pool); without it, one thread. Only work that reads a shared snapshot and writes nothing
//! but its own item goes here, so the world goes bit for bit the same on any number of threads:
//! each creature's arithmetic is the same whichever thread does it, and results keep their order.

#[cfg(feature = "parallel")]
use crate::config::{PARALLEL_CHUNK, PARALLEL_MIN};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// `f` on every item.
#[cfg(feature = "parallel")]
pub(crate) fn for_each_mut<T: Send>(items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    if items.len() < PARALLEL_MIN {
        items.iter_mut().for_each(f);
    } else {
        items.par_iter_mut().with_min_len(PARALLEL_CHUNK).for_each(f);
    }
}

#[cfg(not(feature = "parallel"))]
pub(crate) fn for_each_mut<T>(items: &mut [T], f: impl Fn(&mut T)) {
    items.iter_mut().for_each(f);
}

/// `out` becomes `f` of every item, in their order; its buffer is reused.
#[cfg(feature = "parallel")]
pub(crate) fn map_into<T: Sync, U: Send>(items: &[T], out: &mut Vec<U>, f: impl Fn(&T) -> U + Sync + Send) {
    if items.len() < PARALLEL_MIN {
        out.clear();
        out.extend(items.iter().map(f));
    } else {
        items.par_iter().with_min_len(PARALLEL_CHUNK).map(f).collect_into_vec(out);
    }
}

#[cfg(not(feature = "parallel"))]
pub(crate) fn map_into<T, U>(items: &[T], out: &mut Vec<U>, f: impl Fn(&T) -> U) {
    out.clear();
    out.extend(items.iter().map(f));
}
