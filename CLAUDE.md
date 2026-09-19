# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tiny Life Simulation — an evolutionary sandbox: plants, herbivores (`Vegetarian`) and predators
in a 2D world. Both herbivores and predators carry a genome (a gene table per species, see
"Genes and strategies") that mutates on division; selection is emergent, not scripted.

Everything — comments, doc comments, commit messages, README, CLI and test output — is written
in Russian. Keep it that way when editing or adding code.

**The project is Rust-only now.** It was migrated from Python + pygame (plan: phases 0‒8, from a
1:1 core to a native wgpu/egui app with player-chosen world scale up to ~1M creatures). Done:
phases 0‒2 — engine (`crates/life-core`), bounded headless runner with an observer
(`crates/life-sim`), balance report (`crates/life-report`) — and the game itself
(`crates/life-app`, phases 4‒5 done ahead of phase 3). The Python version was removed; it lives
at the git tag **`python-final`** (`python/` there) and is the behavioural spec the game was
ported from. Still open: phase 3 (two-phase parallel tick — big worlds are single-threaded and
lag at ×1000), phase 6 (machine benchmark instead of the scale estimate), numeric behaviour
genes (see "Balance").

## Commands

```bash
cargo test --workspace                              # all tests (~0.5 s)
cargo test -p life-core --test engine сетка         # tests whose name contains «сетка»
cargo test -p life-core --test golden               # world behaves bit for bit as recorded
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
cargo run -p life-report --release -- --rule plant_energy=80 --scale 10 --threads 4
cargo run -p life-report --release -- --veg-mix 1 1 --pred-mix 1 1   # start with strategies 50/50
cargo run -p life-report --release -- --scale 100 --shape 1:1 --rule plant_width_profile=waves
cargo run -p life-report --release -- --compare reference/fingerprint.json   # balance vs the reference

play.bat / sh play.sh                                    # launcher for humans: build + run the game
cargo run -p life-app --release                          # the game: menu
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world (same flags as the report)
cargo run -p life-app --release -- --scale 100 --shape 1:1 --rule plant_width_profile=waves
TINYLIFE_SHOTS=some/dir cargo test -p life-app ui_tests  # screen tests + PNGs of every screen
```

Looking at the game from an agent: don't take screenshots of the desktop (other windows get
captured) and don't inject mouse/keyboard input. Render screens headless with `TINYLIFE_SHOTS`
(egui_kittest), and drive state through `LifeApp` fields / `sim::Command` in `ui_tests.rs`.

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
fullness; genome start → end as median (10‒90%); herbivores vs plants by depth band (and by
width band when the width food profile isn't uniform); a
chronicle of events (crashes and rises with their causes, predators extinct/returning, plants
hitting the cap, gene shifts, herbivores squeezing into a thin layer); ASCII maps (top =
surface, `X` predator, `O`/`o` herbivores, `:`/`.` plants). The JSON has the same plus every
snapshot (`life_sim::observe::Snapshot`: per-gene `GeneStat` of both species — a spread for
numeric genes, variant shares for choice genes —, depth and width histograms, cumulative
counters). Format
`life-report/2`: top-level `genes` describes both gene tables (key, label, kind, variants);
keys are English (event `kind`), texts Russian. Long runs may stop on the work budget
("перегрузка") — raise it with `--max-work`.

`observe.rs` lives in `life-sim`, not in the report, so the game reuses snapshots
and the event chronicle for its in-game event feed.

### The reference fingerprint

`reference/fingerprint.json` is the balance fingerprint (8 seeds x 20 000 ticks, series every
60 ticks). It started as the last Python version's (`python/fingerprint.py` at `python-final`)
and is re-taken from Rust after each deliberate balance change. `--compare` reruns the same seeds in Rust and checks each metric's mean against the reference's
per-seed range; any mismatch exits with code 1 (CI relies on it). It refuses (code 2) when the
world differs from the one the reference was taken on (world size — compared as `Space`, not
shape name, since at ×1 strip and 3:2 are the same 6000x4000 —, rules, start counts, predator
speed/vision, start strategy mix) — a mismatch there would measure the conditions, not the
balance. Gene tables are code, not conditions: if the reference's `genes` list differs from
ours (ignoring inert one-variant choice genes) it prints a note and still compares. The size
metric is read from the reference by gene *key*, not position.

A deliberate balance change fails the comparison by design. Then re-take the reference from
Rust (same format, plus `source: "rust"` and the world conditions):

```bash
cargo run -p life-report --release -- --save-reference reference/fingerprint.json   # seeds 1‒8, 20 000 ticks
```

## Architecture

**Logic is separated from rendering, and that split is the load-bearing design decision** — it
is what lets tests and balance tuning run headless, orders of magnitude faster than watching
the screen. It is enforced by crate boundaries: `life-core` depends on nothing graphical (not
even on threads or I/O), `life-sim` adds only the bounded runner and the observer, and
`life-app` is the only crate that knows about both the screen and the entities.

`crates/life-core/src/`:

- `config.rs` — every tunable constant, each with a comment explaining *why* it has that value.
- `rules.rs` — `Rules`, the world rules the game's «Лаборатория» exposes, at setup and live via
  `World::set_rules` (plant rate and energy, mutation sigma, stat cost scale and exponents,
  predator fertility, tank and migration, the food profiles — see "Where food grows"). `World`
  owns one and every creature gets it at birth. Changing an exponent
  renormalises its coefficient so the *base* genome still pays the same — only the steepness
  changes. `Rules::default()` is `config.rs` bit for bit (the factor is exactly
  `base ** 0.0`); tests guard that. `with()` rejects unknown keys, non-finite values (a NaN
  sigma would hang mutation's rejection loop) and values where a rule stops making sense
  (negative costs, chance outside 0‒1, fractional migration period) — but not merely
  "unbalanced" ones: breaking the balance is what the lab is for.
- `world.rs` — `WorldConfig`, `World` (populations, `step()` — the phase order only,
  `stats()`, `counters`, `spawn_*` for tests and the app).
- `genome/` — gene tables (`vegetarian::GENES`, `predator::GENES`), `VegetarianGenome` /
  `PredatorGenome` (`Copy`, `[f64; N]`, indexed by `enum Gene`), the table-driven mutation.
- `vegetarian/`, `predator/` — the entities: `mod.rs` (the creature, its `act` and the world
  hooks: `feed`/`eat`, `maybe_divide`, `apply_rules`), `phenotype.rs`, `strategy.rs` + one file
  per strategy (`standard.rs` — the original behaviour; `lurker.rs`, `ambusher.rs` — see "Genes
  and strategies"). `plant.rs` — plants; `flora.rs` — where they grow (`Flora`, profiles).
- `senses.rs` — what a creature can learn about the world (traits + grid-backed views + the
  query functions and their brute-force test).
- `grid.rs` — `Grid`: counting-sort spatial grid with a fixed cell, rebuilt each tick.
- `rng.rs` — per-creature SplitMix64 streams; `space.rs` — world size, scale and shape.

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

`tests/golden.rs` pins behaviour bit for bit: an FNV digest of the world (positions, energy,
ids, all genes of both species, an RNG probe of every creature and of the world) at checkpoints
for seven configs (defaults, giants, lab rules with migration, ×10 strip, live rules + spawning,
a 50/50 strategy mix — it also asserts both strategies coexist —, a ×10 square with tabulated
food profiles). A case without recorded digests fails too. Any refactor must keep it; a deliberate behaviour change re-records it (the test prints the table) in its own commit,
together with `--save-reference`. The constants are asserted on Windows only: `ln`/`cos`/`powf`
come from the platform libm, so Linux may differ in the last bit (there the test prints its
digests). `--ignored` prints digests of 50 seeds × 2 worlds for a wider before/after diff.

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

- **Creatures do not see the grid**: `Vegetarian::step` / `Predator::step` take *senses*
  (`senses.rs`: `VegetarianSenses` — nearest predator, nearest plant; `PredatorSenses` —
  nearest prey). `World` builds three grids per tick (prey, food, hunters) and answers through
  `GridVegetarianSenses` / `GridPredatorSenses`, built per creature; tests pass
  `vegetarian_senses(|..| .., |..| ..)`, `predator_senses(..)` or `Blind`. The queries and the
  grid senses are `#[inline(always)]`: without it the compiler stopped inlining the plant
  search into the herbivore's step and a ×100 world ran 8% slower than with closures.
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
- The queries themselves are free functions at the bottom of `senses.rs` (`nearest_prey`,
  `prey_in_contact`, `nearest_predator`, `nearest_plant`, `eat_plants`), so that
  `запросы_к_сеткам_совпадают_с_перебором_в_живом_мире` (unit test in `senses.rs`) can check
  every one of them against brute force on real positions of a live world, including one with
  giant herbivores. A wrong radius does not crash anything — it silently changes the balance.
  Keep that test and `сетка_совпадает_с_перебором` passing when touching `grid.rs`, the
  queries or how creatures move; route any new neighbour query through such a function.
- No senses on a creature's *own* species until the parallel tick: herbivores move during their
  own phase, so the grid's copied coordinates of other herbivores would be stale.

### The soft layer

The layer genes (`min_y`, `max_y`) are a *preference*, not a wall. A herbivore goes for any
visible plant, above or below its layer; with no food in sight it wanders inside its home band
(`pheno.body_lo..body_hi` — the layer minus the body margin) and, when outside it (chased food,
fled, was born there), walks straight back (`standard::pick_random_target`: outside the band
the wander target is the nearest band point; every wander target lies in the band).

Physics clamps only to the world: `pheno.x_lo..x_hi`, `pheno.y_lo..y_hi` (body margin capped at
half the world, so a body bigger than the world sits on the middle line instead of flipping).
`Vegetarian::new` puts a random position into the home band and clamps a given one (a child next
to its parent, a spawn) to the world only — its mutated layer may differ, and it walks home
instead of teleporting. A layer thinner than the body collapses the band to a line inside the
world. `act` never steps past a target closer than the step (it lands on it): otherwise a
creature returning to a band thinner than its step would oscillate across it forever. Don't
add moves that teleport.

### World scale

Scale is area; shape (`space::Shape`: 1:1, 3:2, 2:1, strip) is proportions —
`Space::new(scale, shape)`, `WorldConfig::space()`. The default everywhere (`WorldConfig`,
both CLIs' `--shape`, the game's «Новый мир») is **3:2**: at ×1 it is exactly the base
6000x4000 (`sqrt(16e6)` is exact — tested), so the reference and every ×1 golden case are
untouched; bigger worlds grow both ways. `Shape::Strip` is the pre-shape behaviour (height stays
4000, width grows) — golden case D pins it explicitly. The vertical ecology — food profile,
layer genes — is in % of depth, so it transfers to any height; absolute distances (walking back
to the home band, vision) don't scale, which is what the shape balance check (story over 12
seeds at ×10 per shape) watches. Everything defined per world (plant rate and cap, start
populations, migration thresholds and arrivals, report and runner limits) is multiplied by
`area_ratio` via `per_area`, so densities — and the balance — stay the same (in theory: see
below). Scale is
`MIN_SCALE` = 1 to `MAX_SCALE` = 10 000: narrower worlds break predator geometry, bigger ones
run out of memory before they look any different (per-machine memory guards are phase 6).

Measured (12 seeds × 20 000 ticks at ×10): no shape goes extinct, but tall worlds are harsher.
Final herbivores, median: strip 4930, 3:2 1541, 1:1 1244, 2:1 1639; predators about 2× more;
plants often sit at the cap (food is not what limits them); in 1:1 and 3:2 one seed each ends with 4
herbivores. At ×100 3:2 holds ~15k herbivores vs ~60k in the strip. Why (story of 1:1 seed 3):
the start layer is 5‒100% of depth, so in a 15 000-high world most newborns start far from
the rich top and starve (starved 135k vs eaten 80k), the repro threshold collapses to ~7 and
populations swing. Not tuned yet.

### Where food grows

`flora.rs`. A plant's x and y are drawn independently: x by the width profile, y by the depth
profile (density = their product). A profile (`FoodAxis` in `Rules::plant_depth` /
`plant_width`, six rules per axis `plant_{depth,width}_{profile,steepness,end,bend,waves,amplitude}`)
is `f(t)` over the share of the axis from the near edge (surface / left): uniform, linear
(`end` % at the far edge), exp (`steepness`), log (`bend`: plateau, then a cliff), waves
(`waves` rich bands, peaks mid-band, `amplitude` %). Each parameter is read by its profile only;
the UI shows it only then (`Field::shown`). `--rule plant_width_profile=waves` takes names
(`Rules::with_text`). The dead zone at the surface (`PLANT_TOP_MARGIN_PCT` = 5% — 200 at height
4000) belongs to the surface and applies to every profile. The distribution is a world property;
layer genes don't adapt to it.

`Flora` is derived from rules + space like a phenotype from a genome: built in `World::new` and
in `set_rules` (plants already grown stay put). Exactly **two random numbers per plant** for any
profile (x then y); uniform and exp keep the pre-profile expressions (`rng.uniform`, the
analytic inverse CDF), so the default world is bit for bit the old one — a unit test compares
10 000 plants against a copy of the old formula. Linear, log and waves sample a tabulated
inverse CDF (4096 bins; empty bins never picked). `flora::density(rules, tx, ty)` is the
preview the game paints; `flora::describe(rules)` is the story's line. Limits in `with`: profile
an integer index, waves an integer 1‒100 (table resolution), steepness ≤ 100 (`e^-k` underflow),
percents 0‒100. Profiles are not balanced: at ×1 with each non-default profile at its default
parameters (6 seeds × 20 000 ticks), 6 of 48 runs died out (depth log 2, width linear 2, width
exp 1, width log 1); the default profile — 0 of 12.

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

Extinction: before the soft layer and the strategies, 4 of 12 seeds died out within 20k ticks
(herbivores squeezed into the top few % of depth, repro threshold collapsed, they starved);
with them, 0 of 12 (one seed ends with 2 herbivores and no predators). Use the story
(`--ticks 20000 --maps 3`, `--max-work 1e15` for full-length runs) to work on balance.

Behaviour genes without a cost run away. Tried and removed: «испуг» (flee distance, % of
vision) and «голод» (predator hunger threshold) as free numeric genes. Hunger crept up (greed
pays for each predator), predators ate the prey out; fear shot to ~100% of vision during
predator booms and herbivores starved fleeing — 10 of 12 seeds extinct. Clamped to 10‒60% /
30‒75% each alone cost ~2 of 12, both together 9 of 12. Such a gene needs a real trade-off
first (a cost in upkeep or a behavioural catch).

### Termination guarantees

Tick cost grows with population, so a tick limit alone does not bound wall time. Every headless
run is capped by four independent limits (`life_sim::Limits`): ticks, population ceiling, a
compute budget (`total_work` = herbivores x plants, summed over ticks) and a wall-clock deadline;
the ceiling and budget scale with area. Tests contain no `while` loops at all; every run is
bounded by a tick count. Preserve this property in new tests.

### Performance-sensitive code

`Vegetarian::step` is the hottest path. Genome-derived values (`upkeep`, `slow_speed`,
`slow_upkeep`, `vision2`, `size2`, `half`, `flee2`, layer bounds, the strategy) are precomputed once in `Phenotype::of` because the
genome never changes during a lifetime; the predator's phenotype likewise. Distances are
compared squared. The grid reuses its buffers between ticks. Strategy dispatch is a `match` on
an enum (static, inlined), never `Box<dyn>`.

The 20 ms guard below only catches catastrophes. For refactors, compare ms/tick against the
previous version built in a `git worktree`, running both alternately (single runs are noisy):
`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1` (the summary's last column).

`тик_укладывается_в_бюджет_на_фиксированной_нагрузке` guards against regressions at a fixed
4000/4000/100 load in a x10 world: ~2 ms/tick with the grid, ~80 ms if queries degrade to a
full scan, threshold 20 ms. (At the old 400-creature load Rust is fast enough even by brute
force, so the guard would not catch anything there.)

## Genes and strategies

Each species has a gene table (`genome/vegetarian.rs`, `genome/predator.rs`): `GeneSpec { key,
label, about, kind, base, mutation }`, `kind` = `Absolute` | `Percent` (clamped 0‒100 on
mutation) | `Choice(&[Variant])` (the value is a variant index). Everything that walks genes —
mutation, `Stats`, observer, story, JSON, charts, creature card, help — iterates the table, never
positions. **Tables are append-only**: the order fixes the RNG draw order of mutation (every
seed), positions in the reference fingerprint and JSON.

A creature's step is split: its **strategy decides** (`strategy::decide(&Me, &mut Mind, &mut
Rng, &senses) -> Intent`) and the **creature acts** (`act`: movement, clamps, upkeep, death —
per species, their formulas differ). A strategy sees only itself, its memory and senses; it
cannot move, feed or divide the creature — the property a parallel tick needs. Hooks:
`after_eating` (herbivore — re-targets even while fleeing), `settle` (predator, after the move —
even if it just died). Eating, catching and division stay world physics driven by the phenotype.

The strategy is a gene: the last row of each table, `Choice(&strategy::VARIANTS)`,
`Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE }`. **A choice gene with one variant is
inert**: `Switch` draws nothing, so appending it did not shift a single random number; the UI,
the story and the reference check hide such a gene. A start mix
(`WorldConfig::vegetarian_strategies` / `predator_strategies`, shares by variant; `--veg-mix` /
`--pred-mix` in the report, «Затаившихся/Засадников на старте» in «Новый мир») is dealt
*without* drawing (`genome::variant_for`), so the same seed gives the same world.

**Slow pace** is physics a strategy may choose: `SLOW_PACE` (⅓) of its speed, paying the speed
term for the step actually taken (`pheno.slow_speed`, `pheno.slow_upkeep` — `Rules::upkeep` at
the reduced speed, so the lab's exponents apply). Herbivore: `Intent::slow`; predator: the
strategy already sets step and `cost`. Fleeing and sprinting are always at full speed.

The strategies:
- herbivore `standard` — flee, else nearest visible plant, else wander in its layer;
  `lurker` («затаившийся», `lurker.rs`) — the same decision (`standard::plan` returns which
  branch fired), but wanders and returns home at slow pace. In every tested seed it replaces
  `standard` (95‒100% by 20k ticks): the savings pay.
- predator `standard` — hungry: chase the nearest prey, sprint near it; full: wander;
  `ambusher` («засадник», `ambusher.rs`) — always wanders at slow pace, sprints only at prey
  already within `PREDATOR_SPRINT_RANGE`, never chases from afar (`standard::pursue` /
  `wander` are shared). They coexist in most seeds: 0‒97% ambushers by 20k ticks, depending
  on the seed and the start mix.

Adding a gene:
1. A variant at the end of `enum Gene`, a row at the end of `GENES` (law, base, `about`).
2. Its effect only in `Phenotype::of`; its cost appended at the *end* of the upkeep sum (a new
   rule: `RULE_KEYS` + `with`/`get` + `FIELDS`).
3. A test of the effect; the invariant test covers ranges automatically.
4. Look at the genome panel and the card at 960×600 (`TINYLIFE_SHOTS`): the layout test does not
   see painted labels.
5. In a separate, deliberate commit: re-record the golden test and `--save-reference`; read the
   story over 3 seeds × 20 000 ticks.

Adding a strategy:
1. A variant at the end of `Strategy`, `Strategy::ALL` and `VARIANTS` (≤ `MAX_VARIANTS` = 8).
2. Its own file with `decide` (and the hooks), using only `Me`, `Mind`, `rng` and senses; new
   state goes into `Mind` (keep it `Copy`). A new sense = a trait method + a query function in
   `senses.rs` + its brute-force check.
3. Tests: `decide` scenarios with `vegetarian_senses`/`predator_senses`; a mixed population is
   deterministic (same seed, same digest); `StrategyShift` fires in the chronicle.
4. Re-record the golden test and `--save-reference`. With two variants the strategy row, the
   share chart and the card line appear by themselves.

## The game (`crates/life-app`)

**The window never waits for the simulation** — that is the rule every change must keep.

- `sim.rs` — the simulation thread owns `World`. The UI sends `Command`s over a channel (pause,
  speed, step, view rect, pick/select, `SetRules`, spawn, restart, new world); they apply
  between ticks. Frames go through a one-slot mailbox: the thread publishes only when the UI
  took the previous frame, so frames are never dropped — that is why history/log/gene points
  travel as *deltas* in `Frame` (a test checks none are lost). Tempo: ticks per second with a
  capped debt (lag is shown, never caught up in a burst), `SLICE` bounds a tick burst so
  commands stay responsive; frame building is throttled to ≤ 1/3 of the thread's time.
  `Snapshot::of` (sorting) runs every `SNAPSHOT_EVERY` ticks, stretched on big worlds to ≤ 5%.
- `frame.rs` — what the UI needs: instances (32 bytes, relative to `origin` in f64 — f32
  absolute coords break at ×10 000) **culled to the visible rect** (padded by half a view,
  culled by body, not centre), or a density raster when more than `MAX_INSTANCES` are
  visible; the minimap raster every 0.4 s. `кадр_огромного_мира_быстрый_и_лёгкий` guards it.
- `motion.rs` — collects the instances and remembers the previous frame, so nothing jumps or
  pops: previous position and heading (two-pointer merge — creature vecs are sorted by id),
  birth age (a ring "max id / tick in frame → time"; a creature panned into view is not
  "born"), ghosts of the dead (eaten vs starved). Plants have no id: matched by
  (`Plant::born` tick, x bits); `born` sits in padding, `Plant` stays 24 bytes (tested).
  All linear in visible count; reset with a new world.
- `render.rs` + `creatures.wgsl` — one instanced draw call through `egui_wgpu::CallbackTrait`.
  The shader draws each creature between its previous and new position (`k`, from
  `view.rs`: time since the frame arrived / smoothed frame interval — one frame of latency),
  grows newborns, shrinks the eaten and greys the starved (age + `since`), keeps sub-pixel
  dots at 1 px with area-scaled alpha (no shimmer), and only above ~4 px draws detail: rim,
  fullness core, an eye along the heading, the predator's nose (decoration outside the body
  circle; picking and culling still use the circle). Selection ring and follow camera use the
  same interpolated position. The buffer is uploaded only when a new frame arrives.
- `app.rs` — `LifeApp`: screens and transitions, owns the settings and the `SimHandle`; `theme.rs` —
  palette (port of `app/theme.py`).
- `view.rs` (world, selection, minimap), `camera.rs` (port of `camera.py`, f64), `game.rs`
  (game screen, lab window with «Правила»/«Еда» tabs, creature card, `report_command`),
  `screens.rs` (menu, «Новый мир» with tabs «Мир»/«Еда»/«Лаборатория» and buttons pinned in a
  bottom panel, prefs, help; `field_input` — a slider or, for a field with `choices`, a combo
  box; `food_preview` — the world in its proportions shaded by `flora::density`),
  `charts.rs` (drawn with the painter — no plot crate), `history.rs`
  (port of `history.py`), `settings.rs` (`FIELDS`, the single field spec — label, hint,
  range, `choices`, `shown`; start counts are *per base area* and scale with the world; the
  strategy sliders are the share of the second variant; the shape is `Settings::shape`; file in
  `%APPDATA%\TinyLife`, atomic, clamped).
- Chronicle texts come from `life_sim::observe::EventTracker` — the same incremental tracker
  the report's `events()` wraps, so game and report print identical events.
- Live rules: `World::set_rules` recomputes the whole phenotype (`apply_rules`); a test checks it
  equals a newborn's. `World::pick` / `vegetarian(id)` /
  `predator(id)` serve selection and follow (creature vecs stay sorted by id — tested).
- `ui_tests.rs` — egui_kittest: every screen at 960×600 and 1600×900, buttons/sliders inside
  the window and not overlapping (scrolled-away side-panel content excluded). They share one
  GPU lock: parallel wgpu renderers crash the driver on Windows. CI installs lavapipe on Linux.
- Release on Windows builds with `windows_subsystem = "windows"` (no console on double-click)
  and attaches to the parent console so flag errors still print.

### Behavioural spec at `python-final`

The Python game at `python-final` remains the reference for behaviour details
(`git show python-final:python/app/<file>`):

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
