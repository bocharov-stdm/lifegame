# Phase 3: a parallel tick with bit-for-bit determinism

Status: design, no code. Written against `8e94b91` (model `life-behavior/7`); the timings below
are of that model.

## Goal and the one invariant

Run one world's tick on several threads so that big worlds (×100 and up) tick faster, while
**the same seed gives the same world bit for bit at any thread count**, including one. The golden
digests (`tests/golden.rs`) stay what they are: phase 3 is a refactor, not a behaviour change, and
it must not re-record a single constant.

Nothing in the tick may depend on how work is split between threads. The rule that follows:

> A parallel pass is a pure function of state that nobody writes during the pass. Each item's
> result is computed independently; results are combined in item (id) order. Anything whose
> outcome depends on the order of items stays sequential in id order.

Floating-point sums count as order-dependent: `a + b + c` and `a + (b + c)` differ in the last
bit, and the golden digest sees the last bit.

## Where the time goes

Single thread, `8e94b91` plus timers around each block of `World::update_creatures` (a scratch
build, not committed); 1000 ticks, seeds 1 and 2, combat on:

| Phase (share of tick time) | ×100 calm, ~10 000 creatures, 21–22 ms/tick | ×100 base, ~19 000 creatures, 58–60 ms/tick |
|---|---:|---:|
| flock circles (`update_full`) + battles | 7.7% | 5.9% |
| herd grid + `social::prepare` | 11.4% | 9.9% |
| `territory::prepare_full` | 13.4% | 14.3% |
| `social::prepare_aid_with_grace` | 9.0% | 24.9% |
| food/corpse grids + herd snapshot | 4.6% | 2.1% |
| **creature steps** (decide, steer, act) | **29.9%** | **24.8%** |
| feeding on plants | 3.8% | 2.2% |
| combat + corpse bites | 7.5% | 6.6% |
| reproduction + sweep | 2.2% | 1.6% |
| every-tick bookkeeping: splits (every 60), strays, departures, flock recount, alarms | 10.4% | 7.7% |
| plants, grace, corpse decay, food goals | 0.2% | 0.1% |

The step loop is only a quarter to a third of the tick. The passes that can become pure maps
(steps, social context, territory, strike collection, reproduction, contact queries) add up to
roughly 60–65%, so by Amdahl's law the ceiling with many threads is about 2.5–3× — unless the
sequential remainder shrinks first. Two items are worth fixing **before** phase 3, because they
are sequential cost that no thread count removes:

- `prepare_aid_with_grace` (9–25%): every victim scanned the grid with the **largest vision in
  the world** as its radius, and a `BTreeMap` of all ids was rebuilt every tick. **Done** (model
  `life-behavior/8`): the parent is found by binary search and flockmates from a per-flock list,
  bit for bit the same world (50 seeds × 4 worlds); a ×100 world with combat went from 62–64 to
  46–48 ms/tick at the same ~19 000 creatures.
- The every-tick bookkeeping (8–10%): `departures` and the end-of-tick flock recount each walk all
  creatures into `BTreeMap`s; they can share one per-flock pass.

With both done, the parallel share rises to about 75–80%, and 8 threads could give roughly 3–4×.

## The tick, phase by phase

Order as in `World::step` / `update_creatures`. "Pure map" means: every creature's result depends
only on state that no one writes during the pass.

| # | Phase | Writes | Reads | Verdict |
|---|---|---|---|---|
| 0 | plants spawn | world RNG, `plants` | rules, flora | sequential (world RNG stream) |
| 1 | grace prune, shot trails, corpse decay | grace, shots, corpses | tick | sequential, cheap |
| 2 | `flock::food_goals` (every 60 ticks) | flocks | creatures' food reports | sequential; ties already broken by observer id |
| 3 | `flock::update_full`: membership sums, circle moves, pushes, shrinking, relocation search | flocks, `creature.circle` | creatures | **sequential**: float sums per flock in creature order, pushes applied pair by pair in grid order, flock RNG streams |
| 4 | `Battles::update` | battles, flocks | creatures (adult counts) | **sequential**: merging order, BTreeMaps |
| 5 | prey grid rebuild | grid | positions | parallel counting sort possible, must stay stable by index; cheap, leave sequential first |
| 6 | `social::prepare` (8 nearest flockmates, alarm and food context) | own `mind.social` | snapshot of all creatures | **pure map**, already two-pass (compute all, then write) |
| 7 | `territory::State::prepare_full` | own `mind.social.territory_*`; `encounters`, `attacks`, `flock.warned` | areas, owners, grid, creatures | owners and the per-creature loop (battle enemy, guard target, areas in sight, members in sight, avoid/escape): **pure map** once split into compute + write; encounter/attack bookkeeping and `warned` lists: sequential, `O(n)` |
| 8 | `social::prepare_aid_with_grace` | `mind.social.aid` | victims, helpers | candidate search per victim is a pure map; **assignment is greedy in victim order** (a helper taken by an earlier victim is skipped): sequential |
| 9 | food grid, corpse grid, herd snapshot | grids, herd | plants, corpses, creatures | build sequentially (or stable parallel build) |
| 10 | **creature step**: decide, steer, act | only the creature itself (mind, rng, position, energy, alive) | senses: food grid + plants, corpse grid + corpses, herd snapshot — none written in the pass | **pure map** — the main target |
| 11 | feeding on plants, corpse reservation | plants (portions), `bitten`, reserved corpses, creature energy | positions after moves | **greedy in id order** (the plant goes to the lower id; the next one takes its next nearest): resolution sequential; the contact queries (plants and corpses within reach, sorted by distance) can be a pure map |
| 12 | combat: collect strikes | — | creatures, grid, targets, grace | **pure map**: each creature yields at most one `Hit` |
| 12b | combat: apply | health, energy, `hit` signals, counters | hits | per victim, damage summed **in attacker index order**; signals keep the smallest enemy id (order-free) |
| 13 | corpse bites after combat | corpses, energy | positions | greedy in id order, as 11 |
| 14 | reproduction | parent (energy, rng, wait), children | own state | **pure map**; children appended in parent order, ids handed out sequentially |
| 15 | corpses from the dead, sweeping, adding children | vectors | — | sequential `O(n)` |
| 16 | every 60 ticks: splits, stragglers | labels, watches | grid, creatures | union-find edges can be found in parallel (roots are the minimum index, so the components do not depend on edge order); the rest sequential |
| 17 | departures, grace transitions, flock recount (`update`, not advancing), alarms | flocks, labels, counters | creatures | sequential |

What already makes this possible:

- `Creature::step` touches only the creature: the strategy returns an intent, `territory::steer`
  and `act` change only `self`. Senses are shared references to structures no one writes in the
  pass (plants are bitten only in phase 11).
- There is no global RNG: every creature has its own stream, children fork the parent's at
  birth, flocks have streams keyed by `(seed, tag)`, the world stream is used only in phases 0
  and by `spawn`.
- There is no `HashMap`/`HashSet` in `life-core` (checked): every map is a `BTreeMap`, every
  scan is in index or id order, every tie is broken by id.

What is order-dependent today and must stay sequential or be reproduced exactly (found in the
review of `add7f96`–`8e94b91`):

1. Feeding (11, 13): first come, first served in id order.
2. Aid assignment (8): greedy in victim order, two helpers per victim.
3. Circle pushes (3): pairs processed in grid order, each push moving circles for the next.
4. Float sums: damage per victim (12b), flock centre/layer/speed/fullness sums (3), summaries.
5. `Herd::prey` keeps the first 64 candidates and 32 flocks **in grid scan order** (rows from the
   top of the view). Deterministic, and it stays deterministic under a parallel tick as long as
   the grid is built identically. It would change if the grid layout changed (a different cell,
   an unstable parallel build). Recommended before phase 3: keep the nearest 64 instead, so the
   result does not depend on scan order at all (a proof test is in `senses.rs`).

## Where the threads live

`life-core` has no threads today, and CLAUDE.md makes that a crate boundary. Two ways to keep it:

**Option A (recommended): an executor trait in the core.**
```rust
pub trait Exec: Sync {
    /// Calls `f(i, &mut items[i])` for every item; the order of calls is unspecified.
    fn for_each_mut<T: Send>(&self, items: &mut [T], f: impl Fn(usize, &mut T) + Sync);
    /// `out[i] = f(i)` for every `i < n`.
    fn map<R: Send>(&self, n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R>;
}
pub struct Sequential; // the default, used by tests and small worlds
```
`World::step_with(&impl Exec)`; `World::step()` calls it with `Sequential`. `life-sim` supplies a
rayon-backed `Exec` (rayon is already a dependency of `life-report`). The core stays free of
threads and of rayon; the contract (unspecified call order, results by index) is what makes
thread-count invariance checkable.

Option B: `rayon` behind a `parallel` feature of `life-core`. Less code, but the core then depends
on a thread pool, and every test build has to decide which feature it runs.

Pool sizing: the report already runs seeds in parallel on the global pool. When there are at
least as many seeds as threads, each world ticks sequentially (seed-level parallelism is
perfect); only when there are fewer seeds does a world get several threads. The game gets its own
pool of `cores − 1` threads so that the window thread is never starved. A world below a few
thousand creatures ticks sequentially: the result is identical either way, so the threshold is a
pure performance knob.

## Steps

1. **Measure** (half a day): keep the phase timers from this document behind a
   `cfg(feature = "timing")` or in `life-sim`, so later steps can prove their gain.
1b. **Shrink the sequential remainder** (half a day): one per-flock bookkeeping pass (the helper
   lookup in `prepare_aid` is already done). A pure refactor: golden must not move.
2. **Executor and the step loop** (1–2 days): `Exec`, `step_with`, the step loop (10) through
   `for_each_mut`; death counts summed after the loop. This alone gives most of the gain.
3. **Pure preparation passes** (2–3 days): social context (6), territory (7) split into
   compute + write, combat strike collection (12), reproduction (14), contact queries for feeding
   (11, 13) computed in parallel and resolved sequentially.
4. **Optional**: stable parallel grid builds (5, 9), split edges (16). Only if the timers show it.
5. **Validation** (1–2 days, see the test plan) and the ms/tick comparison at ×100.

Total: about 1.5–2 weeks of work. Nothing changes the model, so no balance runs are needed beyond
confirming that golden and both references are untouched.

## Risks

- **A hidden order dependency** turns up only at some thread counts and some seeds. Mitigation:
  the test plan below runs every golden case at several thread counts and with the smallest
  possible chunks, plus a verifying executor.
- **Floating-point reductions** creep in (a `par_iter().sum()` for a flock centre or a damage
  total). Rule: no parallel reductions of floats; sums are taken sequentially over results stored
  by index.
- **Shared mutable state in "pure" code**: `Creature::step` must keep writing only `self`. The
  compiler enforces most of it (`&mut` through `for_each_mut`); interior mutability (`Cell`,
  statics, atomics) must stay out of the core — a grep in CI.
- **Libm differences** between platforms are unchanged: golden constants stay Windows-only.
- **Overhead at ×1**: a thousand creatures do not pay for thread wake-ups; the sequential
  threshold handles it.
- **Memory bandwidth**: the herd snapshot and grids are shared read-only; creatures are large
  structs (hundreds of bytes), so false sharing between neighbouring items is not an issue, but
  the step loop may become memory-bound on many cores. Measure before tuning.
- **The game**: the simulation thread must not use every core; see pool sizing.

## Test plan

Everything bounded by tick counts, no `while` loops, as today.

1. **Golden at every thread count.** `tests/golden.rs` gains a loop over executors: `Sequential`,
   and a rayon pool with 1, 2, 3, 4, 8 and 16 threads (odd counts catch chunk-boundary bugs). Every
   case must hit the recorded constants. The test fails if any executor differs from any other,
   so on Linux (where constants are only printed) it still checks thread-count invariance.
2. **Smallest chunks.** A test executor that runs every item as its own task, in reverse and in a
   shuffled (seeded) order. Any hidden dependency on call order shows up here first.
3. **Verifying executor** (debug builds of the tests): runs each parallel pass on a clone
   sequentially and in parallel and compares the per-item results, so a mismatch names the pass,
   not just "the digest differs at tick 250".
4. **Wide sweep** (`--ignored`): the 50 seeds × 4 worlds digests (two of them with combat),
   sequential vs 8 threads.
5. **Report and references**: `--compare reference/fingerprint.json` and the calm reference give
   the same numbers with `--threads 1` and with the default pool.
6. **Performance**: ms/tick at ×100 (`--scale 100 --ticks 1000 --seeds 1 2`) for 1, 2, 4, 8 threads
   against the sequential build of the previous commit, run alternately; the 4000/4000 guard stays
   under 20 ms/tick with the sequential executor.
7. **Crate boundary**: a CI grep that `life-core` has no `std::thread`, `rayon`, `static mut`,
   `thread_local!`, `Cell`/`RefCell` in simulation code.
