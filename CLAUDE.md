# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tiny Life Simulation — an evolutionary sandbox: plants, herbivores (`Vegetarian`) and predators
in a 2D world. Herbivores carry a 7-gene genome that mutates on division; selection is
emergent, not scripted.

Everything — comments, doc comments, commit messages, README, CLI and test output — is written
in Russian. Keep it that way when editing or adding code.

**The project is Rust-only now.** It was migrated from Python + pygame (plan: phases 0‒8, from a
1:1 core to a native wgpu/egui app with player-chosen world scale up to ~1M creatures). Done:
phases 0‒2 — engine (`crates/life-core`), bounded headless runner with an observer
(`crates/life-sim`), balance report (`crates/life-report`). The Python version, including the
only game with a window, was removed; it lives at the git tag **`python-final`** (`python/`
there). There is no Rust game window yet (phases 4‒5); see "Porting the game" below.

## Commands

```bash
cargo test --workspace                              # all tests (~0.5 s)
cargo test -p life-core --test engine сетка         # tests whose name contains «сетка»
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
cargo run -p life-report --release -- --rule plant_energy=80 --scale 10 --threads 4
cargo run -p life-report --release -- --compare reference/fingerprint.json   # parity with Python
```

CI (`.github/workflows/ci.yml`, Windows + Linux) runs fmt, clippy, tests and `--compare`.
Dev builds use `opt-level = 2`: tests run real multi-thousand-tick simulations.

### Watching a run without a window

To understand *what happens and why* in a world (balance work, debugging, answering "why did
they die out"), use the observer rather than raw counts:

```bash
cargo run -p life-report --release -- --ticks 20000 --maps 3        # story + 3 text maps
cargo run -p life-report --release -- --seeds 1 2 3 --story --rows 8
cargo run -p life-report --release -- --ticks 5000 --json -          # everything as JSON on stdout
cargo run -p life-report --release -- --ticks 5000 --json run.json   # JSON to a file + text
```

The story (always on for a single seed) prints: final state; herbivore and predator
births/deaths **by cause** (eaten vs starved) — the counters in `World::counters`; a table by
intervals with flows, gene medians, the depth layer holding 80% of herbivores and their
fullness; genome start → end as median (10‒90%); herbivores vs plants by depth band; a
chronicle of events (crashes and rises with their causes, predators extinct/returning, plants
hitting the cap, gene shifts, herbivores squeezing into a thin layer); ASCII maps (top =
surface, `X` predator, `O`/`o` herbivores, `:`/`.` plants). The JSON has the same plus every
snapshot (`life_sim::observe::Snapshot`: gene spreads, depth histograms, cumulative counters);
keys are English (`gene_keys`, event `kind`), texts Russian. Long runs may stop on the work
budget ("перегрузка") — raise it with `--max-work`.

`observe.rs` lives in `life-sim`, not in the report, so the future app can reuse snapshots
and the event chronicle for its in-game event feed.

### The reference fingerprint

`reference/fingerprint.json` is the balance fingerprint of the last Python version (8 seeds x
20 000 ticks, series every 60 ticks), made by `python/fingerprint.py` at `python-final`.
`--compare` reruns the same seeds in Rust and checks each metric's mean against the reference's
per-seed range; any mismatch exits with code 1 (CI relies on it). It refuses (code 2) when the
world differs from the one the reference was taken on (scale, rules, start counts, predator
speed/vision) — a mismatch there would measure the conditions, not the balance.

A deliberate balance change fails the comparison by design. Then re-take the reference from
Rust (same format, plus `source: "rust"` and the world conditions):

```bash
cargo run -p life-report --release -- --save-reference reference/fingerprint.json   # seeds 1‒8, 20 000 ticks
```

## Architecture

**Logic is separated from rendering, and that split is the load-bearing design decision** — it
is what lets tests and balance tuning run headless, orders of magnitude faster than watching
the screen. It is enforced by crate boundaries: `life-core` depends on nothing graphical (not
even on threads or I/O), `life-sim` adds only the bounded runner and the observer, and the
future `life-app` will be the only crate that knows about both the screen and the entities.

`crates/life-core/src/`:

- `config.rs` — every tunable constant, each with a comment explaining *why* it has that value.
- `rules.rs` — `Rules`, the world rules the game's «Лаборатория» will expose (plant rate and
  energy, mutation sigma, stat cost scale and exponents, predator fertility, tank and
  migration). `World` owns one and every creature gets it at birth. Changing an exponent
  renormalises its coefficient so the *base* genome still pays the same — only the steepness
  changes. `Rules::default()` is `config.rs` bit for bit (the factor is exactly
  `base ** 0.0`); tests guard that. `with()` rejects unknown keys, non-finite values (a NaN
  sigma would hang mutation's rejection loop) and values where a rule stops making sense
  (negative costs, chance outside 0‒1, fractional migration period) — but not merely
  "unbalanced" ones: breaking the balance is what the lab is for.
- `world.rs` — `WorldConfig`, `World` (populations, `step()`, `stats()`, `counters`,
  `spawn_*` for tests and the app).
- `grid.rs` — `Grid`: counting-sort spatial grid with a fixed cell, rebuilt each tick.
- `vegetarian.rs` / `predator.rs` / `plant.rs` — the entities; `genome.rs` — `Genom` with named
  genes, `GENE_KEYS`, Russian `GENE_LABELS`, `PERCENT` (genes clamped to 0‒100 on mutation).
- `rng.rs` — per-creature SplitMix64 streams; `space.rs` — world size and scale.

`crates/life-sim/src/lib.rs` — `simulate()` / `run()` under limits; `observe.rs` — snapshots,
events, ASCII map. `crates/life-report/src/` — `main.rs` (CLI), `story.rs`, `json.rs`,
`metrics.rs` (`--compare`).

`Relict/` — **frozen 2025 archive** of early Python prototypes. See `Relict/ПАМЯТНИК.txt`:
nothing there is edited, refactored, "fixed" or modernised. Its bugs are part of the monument.

### Determinism and RNG

There is no global generator. Each creature owns an `Rng`; a child's stream is forked from its
parent's at birth, the world has its own stream (keyed from the seed) for plants and migrants.
Results depend on the seed only — not on iteration order or thread count — which is what the
parallel tick of phase 3 relies on. Any change to how many random numbers are drawn, or in what
order, shifts every seed: fine for a deliberate behaviour change, but then re-validate the
balance instead of diffing numbers. The plant spawner draws its random number even when capped,
and migration draws only when it fires.

### Tick ordering and the death flag

`World::step()` runs: spawn plants → update predators → update herbivores → `tick += 1` →
migrate predators. Herbivores see predators already moved this tick.

- A herbivore eaten earlier in the tick is skipped (`if !v.alive { continue }`); the same check
  runs right after each creature's own step, because starving there sets `alive = false` and a
  creature that died on its move must not eat, hunt or divide. Target searches skip
  `alive == false` candidates. Dead creatures are removed once per phase (`retain`).
- Eaten plants are marked and swept once per tick at the end of the herbivore phase.
- Offspring go into a separate buffer and get ids after the loop: children never act on the
  tick they were born.
- Migration (`rules.predator_migration`, period in ticks, 0 = off) brings `per_area(1)`
  predators at world edges when fewer than `per_area(PREDATOR_MIGRATION_MIN)` are left, at
  least `per_area(PREDATOR_MIGRATION_PREY)` herbivores exist and the world started with
  predators. `World::migrants` counts arrivals.
- `World::counters` (plants grown/eaten, herbivores born/eaten/starved, predators
  born/starved) must balance with the populations; a test checks it. Update the counters when
  adding any new way to be born or to die.

### Neighbour search

- **Creatures do not see the grid**: `Vegetarian::step` / `Predator::step` take closures
  ("nearest plant", "nearest predator", "nearest prey"); tests pass plain closures. `World`
  builds three grids per tick (prey, food, hunters) and answers the closures from them.
- The cell is fixed (`GRID_CELL`); a query scans as many cells as its own radius covers, so one
  far-sighted creature does not inflate everyone's cell. `for_each_near` returns a *superset*;
  callers check distance.
- The grid stores copies of coordinates. That is valid only because the queried entities do not
  move within the phase (herbivores stand still while predators move and vice versa; plants
  never move). `alive` is always read from the entity, never from the grid.
- Predators see and catch by the prey's body edge (`vision + v.half`, `DIAM/2 + v.half`), so
  their query radius adds half of the largest herbivore.
- A herbivore eats with a fresh query around its position *after* the step; a predator catches
  the first prey in contact after its step.
- The queries themselves are free functions at the bottom of `world.rs` (`nearest_prey`,
  `prey_in_contact`, `nearest_predator`, `nearest_plant`, `eat_plants`), so that
  `запросы_к_сеткам_совпадают_с_перебором_в_живом_мире` (unit test in `world.rs`) can check
  every one of them against brute force on real positions of a live world, including one with
  giant herbivores. A wrong radius does not crash anything — it silently changes the balance.
  Keep that test and `сетка_совпадает_с_перебором` passing when touching `grid.rs`, the
  queries or how creatures move; route any new neighbour query through such a function.

One subtlety in `Vegetarian::step`: the nearest plant is found first and *then* discarded if it
falls outside the creature's vertical layer. Folding the layer check into the search would find
the nearest plant *within the layer* — different behaviour, and the creature would stop
wandering.

Herbivore bands: `Vegetarian::new` clamps every position — random or given — into the
creature's own band (`x_lo..x_hi`, `body_lo..body_hi`), so a step's clamps can only shorten it.
The band always lies inside the world: a layer thinner than the body collapses to a line kept
inside it, and the body margin is capped at half the world, so a body bigger than the world
sits on the middle line instead of flipping `x_lo > x_hi`. Don't create herbivores outside
their band or add moves that teleport.

### World scale

`Space::scaled(scale)` grows the width only (height stays 4000, so the vertical ecology — plant
depth profile, layer genes in % — is unchanged). Everything defined per world (plant rate and
cap, start populations, migration thresholds and arrivals, report and runner limits) is
multiplied by `area_ratio` via `per_area`, so densities — and the balance — stay the same.
Scale is `MIN_SCALE` = 1 to `MAX_SCALE` = 10 000: narrower worlds break predator geometry,
bigger ones run out of memory before they look any different (per-machine memory guards are
phase 6).

### Predators

A predator hunts only while hungry (`energy < max * PREDATOR_HUNGRY`); a full one wanders and
does not eat. Near prey (`PREDATOR_SPRINT_RANGE` between body edges) it sprints at
`PREDATOR_SPRINT_MULT` x speed for `PREDATOR_SPRINT_COST` extra energy a tick, never stepping past
the prey. Catching by body contact is the natural pressure against giant herbivores: a big body
is easier to spot and to grab. Without sprint, predators died out in every seed; without
satiety, they ate everything and died next.

### Balance: exponents, not coefficients

Upkeep is `COEF * stat ** POWER` summed over size, speed and sight, with the speed term also
multiplied by `(size / 40) ** SPEED_MASS_POWER` — moving a big body costs more (the factor is 1
for the base genome and for predators). The *exponents* decide whether evolution has a
trade-off at all: eating radius equals size (benefit ~ size²) and search radius equals vision
(benefit ~ vision²), so cost must grow steeper — hence `size ** 2.5` and `vision ** 2`. With
shallower exponents the stats run away to infinity. Read the comment block in `config.rs`
before changing any of these.

Known open problem: with the current balance long runs (10‒20k ticks) often end in total
extinction — herbivores squeeze into the top few % of depth, repro threshold collapses, they
starve. Use the story (`--ticks 20000 --maps 3`) to work on it.

### Termination guarantees

Tick cost grows with population, so a tick limit alone does not bound wall time. Every headless
run is capped by four independent limits (`life_sim::Limits`): ticks, population ceiling, a
compute budget (`total_work` = herbivores x plants, summed over ticks) and a wall-clock deadline;
the ceiling and budget scale with area. Tests contain no `while` loops at all; every run is
bounded by a tick count. Preserve this property in new tests.

### Performance-sensitive code

`Vegetarian::step` is the hottest path. Genome-derived values (`upkeep`, `vision2`, `size2`,
`half`, `flee2`, layer bounds) are precomputed once in `new()` because the genome never changes
during a lifetime; `Predator::upkeep` likewise. Distances are compared squared. The grid reuses
its buffers between ticks.

`тик_укладывается_в_бюджет_на_фиксированной_нагрузке` guards against regressions at a fixed
4000/4000/100 load in a x10 world: ~2 ms/tick with the grid, ~80 ms if queries degrade to a
full scan, threshold 20 ms. (At the old 400-creature load Rust is fast enough even by brute
force, so the guard would not catch anything there.)

## Porting the game (phases 4‒5)

The Python game at `python-final` is the behavioural spec for the Rust app
(`git show python-final:python/app/<file>`). What to carry over:

- `app/settings.py` — `FIELDS`, the single spec of every slider (label, hint, range, step,
  format, tab «Мир» / «Лаборатория»); the setup screen is built from it and the settings file is
  clamped with it; saved atomically, bad files fall back to defaults. Plant growth is a
  *multiplier* on the config rate. Lab slider ranges were validated at both ends (2000 ticks,
  two seeds): the worst case peaks near 3000 creatures.
- `app/session.py` — speed (1‒32 ticks per frame) within a frame budget so the window never
  freezes; pause, single step, events, end states (extinct, explosion). Anything periodic is
  checked per tick, not per frame (at high speed the tick counter skips past multiples).
- `app/history.py` — graph points: counts averaged over `DIVIDE_PERIOD` (division comes in
  bursts and draws a sawtooth otherwise), a recent window plus a whole-game series thinned 2x;
  `origin` keeps the first average genome for «change from start».
- `app/camera.py` — zoom to cursor, pan, clamp, follow: following moves the camera by the
  target's own displacement first and eases only the remainder.
- `app/render.py` — draw order plants → herbivores → predators; cull by *body*, not centre
  (size is a gene); the circle is the body (`size`, `DIAM` are diameters).
- Layout rules: everything scales from a 960x600 logical minimum; the successor of `TestLayout`
  must fail when a widget leaves the window, widgets overlap or a label does not fit.
