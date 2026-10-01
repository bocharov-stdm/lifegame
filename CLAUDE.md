# CLAUDE.md

Guidance for Claude Code in this repository.

## Project

lifegame — an evolutionary sandbox: plants and creatures in a 2D world. Creatures carry a genome
(a gene table) and two behaviour programs that mutate on division; selection is emergent, not
scripted. Rust only: engine `crates/life-core`, bounded headless runner and observer
`crates/life-sim`, balance report and sweeps `crates/life-report`, the game (wgpu/egui)
`crates/life-app`. Model `life-behavior/15`. Removed things live under git tags: `python-final`
(the original spec), `predators-final` (old names still read: `--vegetarians`, `--veg-mix`,
`n_vegetarians`), `flocks-final` (flocks, territories, the social layer).

**Language.** Code, comments, docs, CLI/report output, test names and commits in **English**;
translate Russian comments you touch, don't mass-rewrite. Only the **game UI is Russian**: window
texts, settings labels and hints, gene `label`/`about`, chronicle texts (`life_sim::observe`).

**The user's standing rules**
- **Energy is never made from nothing.** It enters only in plants and then only passes along the
  chain (plant → eater → corpse → eater), losing some. Digestion ≤ 100% (`Rules::with` rejects
  more), a saving ≤ the whole upkeep, a body got for free (a founder's, a newborn's birth size) is
  not meat. Check every new mechanic against this.
- **Fix niches through the body, evolutionarily**: a diet may have a different body, never free
  energy. No balance change without the user's word; before a big mechanic change ask rounds of
  questions with the risk of each option.
- **Behaviour is free**: no upkeep for a block. A behaviour's catch is its consequences, not a
  price. All behaviour is blocks of the programs: a new behaviour is a block, never a gene or a
  world constant.
- **Commit straight to `main`**, only when asked; push only on the user's word. `git fetch` and
  fast-forward before starting (`main` also moves from cloud sessions).
- **In a cloud (Linux) container run no simulations and no tests** (libm differs from the user's
  Windows): only reading, editing, `cargo build`, `fmt --check`, `clippy`; say what was not run.
- `Relict/` is a frozen archive: never touch it.
- After diet or gene changes regenerate and send the user's short PDF of genes, strategies and
  diet edges (a throwaway fpdf2 script, Arial for Cyrillic).
- When a mechanic changes, update this file, `README.md`, `AGENTS.md` (short) and `BEHAVIOR.md`
  (the history of models, newest first).

## The model

A creature eats plants and corpses in portions, grows from food up to its size gene, divides when
grown and its program's «делиться» allows, and dies of hunger, in a fight or of old age. Combat is
always on, but a creature strikes (and shoots, under «стрелять») only what its program chose.
Melee damage = 5% of the striker's size × (its size / the target's) ** `melee_size_power` (1.25)
when it is the bigger (`Phenotype::strike_on`, also used by hunters to weigh prey); a strike's
energy cost ignores diet bonuses. Shots: 1% of size, ≤ ¼ of max health, expensive. Contact = the
sum of the radii.

### Behaviour programs (`creature/program.rs`)

An ordered list of ≤ 32 blocks «if TEST and TEST and TEST → ACTION(params)»
(`Block { when: [Test; 3], action, args: [u16; 8] }`). Each tick:
1. **Settings** apply first, wherever they stand; of each kind (`Block::setting_kind`, each mode
   number its own kind) only the **first** whose tests hold applies; a test sees what the settings
   above it set. They fill this tick's `Stance`; without a setting its default is «nothing» (own
   food only, no rivals, any distance, whole depth, no smoothing, no division, no healing, no eating
   on the move, no children known, no shooting).
2. The first **deciding** block whose tests hold and whose action can be done decides; a failed
   action falls through; nothing decides → it stands.

`Mind` keeps `fired`, `applied`, `tried` for the window. Healing and kinship read the last tick's
stance; eating, combat, shots, defence and division read this tick's.

- **Two tracks**: `programs[JUVENILE]` until grown to the size gene, `[ADULT]` after; inherited and
  mutating apart, sharing `Mind::modes`. `Programs` is an `Arc` (unchanged children share it). Each
  `Program` caches a `Summary` (reachable, live, threat range, hunt ratio, defence, wander reach,
  home layer, shoots), recomputed on every change.
- **Settings**: «есть и чужую пищу»; «гнать соперников у еды» (×1.5 smaller); «за едой из слоя»
  (% of depth); «слой» (top 5%, bottom 100%; `Phenotype::band` adds the body's margins); «плавный
  ход» (course held 30 ticks, turn ≤ 1.2 rad a tick, `steer.rs`); «делиться» (from 70% of the
  tank, child gets 40%, ≥ 1%); «лечиться» (tank > 50%, 60 ticks unstruck; rate `HEAL_SHARE` 0.2% of
  health a tick paid from the tank); «есть на ходу» (plants, corpses, while fullness ≤ 100%;
  without it only what the deciding block goes for); «щадить детей» (until the child reaches 100%
  of its size, `Kinship::knows_until`); «стрелять» (from 50% of range, keeping 50% of the tank);
  «режим» K (1–4) on for N ticks (60; 0 = off) — the memory.
- **Tests** (`Cond`): fullness, health, depth ≥ X%; a threat / a hunting stranger closer than X% of
  sight; struck within X ticks; still fleeing; resting; food / plant / corpse seen; prey its hunts
  would take within X% of sight; age ≥ X% of lifespan; winded; water ≥ X% cold; above / below / in
  its layer; mode K on; always (negated: never — the block is off).
- **Actions** (`Action::params`, a `ParamSpec` each: unit, range, base, nudge; flags flip, indices
  pick another, neither drifts): fight back (enemy ≤ ×1.5 bigger); flee (60 ticks after losing it,
  burst, renewed only by a threat nearer than 33% of sight, pace, tilt); hunt (prey ×1.5 smaller,
  caution 100%, patience `CHASE_PATIENCE` 30 ticks, only if better than plants/corpses, burst, leave
  a given-up prey `CHASE_GIVE_UP_TICKS` 180, within 100% of sight, pace); to a corpse (only if
  better, pace); to a plant (pace, keep the chosen one, nearest or `best_plant`); wander (pace,
  targets ¼ to 200% of sight away, a target out of a changed layer dropped); ambush; to the top /
  bottom of its layer; rest (90 ticks, pause 180); torpor (pays `TORPOR_UPKEEP` 30% of standing
  upkeep, **eats nothing**, gets its breath back like standing); defend its child.
- **«Защищать детёныша»**: its child within 50% of sight, struck within 30 ticks by an enemy in
  sight (or, while young, alarmed by a threat) → goes for the enemy and strikes it whatever its
  size, with a tank > 50%, ≤ 90 ticks, then a pause of 60 (`Mind::aid`, `aid_cooldown`). The child
  counts as its own only while «щадить детей» holds.
- **`Menace`** — how others read a creature: by the hunt, fight-back and defence blocks whose tests
  held on its **last move** (the decider and the blocks before it). Feared by its most permissive
  such hunt, only if it could take prey (not full, fresh meat its food this tick); a hunter expects
  strikes back as the first such fight-back block gives them and counts a parent as an ally only by
  such a defence block. Before the first move: by the program's shape.
- **Templates**: the `strategy` gene picks the founders' template, never switches
  (`STRATEGY_SWITCH_CHANCE` 0). `Program::STANDARD` (20 blocks): layer, smoothing, division,
  healing, grazing, sparing; memory in modes (`ALARM_MODE` 1, `FULL_MODE` 2, `HUNGRY_MODE` 3):
  fullness < 30% → mode 3 for 60 ticks, under it foreign food and rivals ×1.5; a hunter within 33% of
  sight or a calm stranger within 16% → mode 1 for 61 ticks; fullness ≥ 95% → mode 2 for 200; then
  fight back at health ≥ 50%; flee under mode 1; defend a child at health ≥ 60%; hunt; rest under
  mode 2 while ≥ 85%; corpse; plant; wander. `Program::LURKER` wanders at 33% pace.
  `Program::founder` sets the layer (5–100%; a quarter, by hash, 0–100%; scavengers 50–100%) and
  gives 5% of founders «стрелять».
- **Heredity** (`CreatureGenome::inherit`): one clone draw (`CLONE_CHANCE` 50%) for genome and
  programs. Otherwise the genes mutate, then each program **drifts** — a third of its numbers
  (`PROGRAM_DRIFT_SHARE`) move by gauss(0, nudge × `program_drift` (rule, 1) × `program_mutability`)
  — and with `program_mutation` (rule, 5%) × `program_mutability` gets one mutation
  (`Program::mutate_with`): nudge 25%, replace a test 12%, negate a test 8%, replace the action 8%
  (same-label parameters kept, `Action::args_from`), swap with a neighbour 12%, duplicate a live
  block 6%, delete 16% (a dead block first), insert 5%, switch a block off/on 3%, **pair** 3% (a mode
  setting + that mode as a test on another live block), **transfer** 2% (a live block from the
  other track).
  - A block is **dead** (`Program::live`) when off, a deciding block never reached (after one that
    always fires), or a setting behind an earlier unconditional one of its kind.
  - No junk is born: an added block (copy, insert, pair, transfer) that would leave any block dead
    is not added; new deciding blocks go above the one that always fires.
  - `Program::changes` counts structural mutations. Past its end a program is padded with the same
    block, so equal blocks are equal programs.
- The interpreter: `scene.rs` (perception, memoised queries), `actions.rs` (one function per
  action and setting), `steer.rs` (the step), `strategy::plan`. `Cond`, `Action` and their
  parameter tables are **append-only**. The game shows both tracks as flowcharts
  (`life-app/src/behaviour.rs`, key B). The report groups tracks by `Program::shape` (no numbers),
  prints medians (`Program::median`) and `METRIC` lines: `{juvenile,adult}_shapes`,
  `_template_share`, `_hunt_ratio`, `_threat_range`, `_mode_share`, `_conditional_layer_share`,
  `_blocks`, `_dead_share`, `_modes`, `_spread` (`Program::spread`: 0 copies, ~0.5 random).

### Diets

The `diet` choice gene, order H/O/S/C (the order of every `DIET_*` table in `config.rs`).
Digestion (`DIET_DIGESTION`: plants, fresh meat, rot, bones; 0 = neither eats nor goes for it) and
the edges are world rules (`Rules::diets`, keys `{diet}_{edge}`, lab tab «Питание»), read in
`Phenotype::of`:

| | strike | health | size cost | speed cost | smell | plants | fresh | rot | bones | young plants |
|---|---|---|---|---|---|---|---|---|---|---|
| herbivore | 1 | 1.1 | 1 | 1 | 1 | 100% | 10% | 0 | 0 | 100% |
| omnivore | 1.5 | 1.05 | 1 | 1 | 1.2 | 80% | 60% | 20% | 0 | 100% |
| scavenger | 1.3 | 1 | 1 | 1 | 3 | 15% | 100% | 90% | 90% | 70% |
| carnivore | 3 | 1 | 1 | 0.5 | 1.5 | 20% | 100% | 30% | 0 | 70% |

- Smell = how far corpses are sensed, × vision, free. Juvenile gut: until grown, plants at
  `max(plants, young_plants)` (0 = as grown). Only the grown divide.
- **Own niche** (`DIET_OWN`): without «есть и чужую пищу» a herbivore or a scavenger skips fresh
  meat and does not hunt, a carnivore skips rot and bones; the omnivore has no foreign food. So a
  hungry herbivore (the template's mode 3) eats fresh meat and hunts, and is feared then.
- Founders: `DIET_START_MIX` 70/30/0/0 — meat diets arise from mutants. Meat founders set in a mix
  start `meat_founder_size` ×2; scavenger founders start deep and fully cold-blooded.

### Body, corpses, space

- **Corpses**: meat = the body grown since birth × `GROWTH_ENERGY_PER_SIZE` + the tank
  (`corpse::meat`). Stages (`corpse::Stage`, clock rules `corpse_*`): **fresh** 300 ticks in place;
  **rot**, sinking 2 a tick to the lowest 25% of depth, flesh decaying to bones by 3000 ticks from
  death; **bones** (10% of the meat), sinking 40 a tick, lying 5000.
- **Cold deep**: warm to `thermo_top` 15% of depth, cold from `thermo_bottom` 45%, smooth between.
  `cold_blood` (%, base 0) makes a body cheaper (`COLD_SAVING` −50%) and slower (`COLD_SLOWING`
  −40%) at full cold (`Phenotype::temper`, at the current depth); it mutates by points (`Shift`).
- **Upkeep** = body and eyes (`still_upkeep`) + the speed term of the step actually taken
  (`step_cost`). `COEF * stat ** POWER` over size, speed, sight (speed also × (size/40) **
  `speed_mass_power`), per-term price rules and `cost_scale`. Cost exponents must stay steeper than
  benefit (~stat²) or stats run away — read `config.rs` before changing them.
- **Movement**: each block's pace (≥ `MIN_PACE` 10%). `burst` gene (×1–2): up to 20 ticks faster
  in a chase or flight when the block asks, then 60 winded; muscles cost standing
  (`BURST_UPKEEP_SHARE` ¼ of the extra speed's term).
- **Life**: `maturation` (%, base 50) — share of digested food into growth until grown.
  `lifespan` (3000, 500–10 000): from 70% of it speed, vision, strike and max health fall linearly
  to 70% at 90% (`phenotype::vigour`).
- **Layer** is a preference, not a wall: a creature goes for visible food within its reach
  (`Taste::admits`) and walks back otherwise; physics clamps only to the world. Never teleport.
- **Space**: scale is area, shape (1:1, 3:2 default, 2:1, strip) proportions (`Space::new`); ×1 3:2 =
  6000×4000. Per-world quantities scale via `per_area`; vertical ecology in % of depth. Real units
  are for showing only (`units.rs`, `docs/scale.md`).
- **Food** (`flora.rs`): x and y drawn by width and depth profiles (default «океаническое» down,
  uniform across). `PLANT_MAX` slots, one plant each (logistic growth); 24 patches per base world
  hold `plant_patch_share` 60% of slots; patch centres come from their own keyed stream. Occupied
  slots are an incremental bitset (`world::Occupancy`), rebuilt when the plant count stops matching
  — keep direct edits of `plants` changing the count.

## Genes

`GeneSpec { key, label, about, kind, base, mutation }`, `kind` = `Absolute` | `Percent` |
`Choice(&[Variant])`. Everything walks the table. **Tables are append-only** (the order fixes RNG
draws and positions in references and JSON). Eleven genes: `size`, `speed`, `vision`, `strategy`,
`mutability`, `maturation`, `diet`, `lifespan`, `cold_blood`, `burst`, `program_mutability`. A choice
gene with one variant is inert.

Mutation (`genome::Heredity`, from the rules): `Scale` for numbers (× (1 + gauss(0, σ·mutability)),
multiplier ≥ 0.1); `Shift` for `cold_blood` (± gauss points, clamped 0–100); `Switch` for choice
genes (0.1% × mutability); `Neighbours` for the diet, independent of mutability: towards meat 2%
(H → O → S or C), another neighbour 0.5%, the herbivore's leaps to C 0.1% and S 0.01%
(`diet_leap_*`), others a general jump 0.01%. Both mutabilities are clamped to `MIN_MUTABILITY` 0.1
and cost nothing. Start mixes are dealt without draws (`variant_for`, `spread_ranks`).

A program **decides** (`strategy::decide` → `Intent`), the creature **acts** (`act`); a block cannot
move, feed or divide the creature. Strategies (`Strategy`, `VARIANTS`) are only founders'
templates: `standard`, `lurker`.

- Adding a gene: a variant at the end of `enum Gene` and a row at the end of `GENES`; effect only in
  `Phenotype::of`, any cost at the end of the upkeep sum; a test; check the genome panel and card at
  960×600 with `LIFEGAME_SHOTS`; re-record golden and references in a separate commit.
- Adding a test or action: a variant at the end of `Cond`/`Action` and their `ALL`, a `ParamSpec`
  table (a new sense = trait method + query + brute-force check), its `Scene::test` or
  `actions::act` arm, window labels, tests in `strategy.rs`, re-record golden.
- Adding a template: a variant at the end of `Strategy`/`VARIANTS` (≤ 8) and its `Program::template`.

## Architecture

**Logic is separated from rendering**, enforced by crate boundaries: `life-core` depends on nothing
graphical and does no I/O; its only threads are rayon's under the `parallel` feature (`par.rs`,
off by default; `life-app` and `life-report` turn it on). `life-sim` adds the bounded runner and
observer, `life-app` alone knows the screen.

`crates/life-core/src/`:
- `config.rs` — every tunable constant with a comment on *why* that value.
- `rules.rs` — `Rules`, the lab's world rules (`World::set_rules`, `apply_rules` recomputes
  phenotypes). `Rules::default()` is `config.rs` bit for bit. Changing an exponent renormalises its
  coefficient. `with()` rejects unknown keys, non-finite and senseless values (an exponent over 10,
  an upkeep that leaves f64), never merely unbalanced ones. Several rules at once (`--rule` flags, a
  reference's rules, the lab's «Применить») go through `with_all`/`with_texts`: the upkeep is checked
  for the whole set, so their order does not matter; the game shows a refusal, never panics. A new
  rule: `RULE_KEYS` + `with`/`get` + a `FIELDS` entry in `life-app/src/settings.rs`.
- `world.rs` — `WorldConfig`, `World::step()` (phase order only), counters, `spawn_*`.
- `genome/` — gene table, `CreatureGenome`, mutation.
- `creature/` — `mod.rs` (`act`, feeding, division, `grow_old`), `phenotype.rs`, `program.rs`,
  `scene.rs`, `actions.rs`, `strategy.rs` (`Mind`, `Intent`, `Stance`, `plan`), `steer.rs`;
  `plant.rs`, `flora.rs`.
- `senses.rs` — what a creature can learn (grid-backed queries + brute-force tests); `grid.rs` —
  counting-sort spatial grid; `rng.rs`, `space.rs`, `units.rs`.
- `profile.rs` — the tick's phases timed (`World::set_profiling`, `phase_times`); `par.rs` — the
  per-creature phases on rayon's pool.
- `combat.rs` (simultaneous strikes and shots, a parent's defence), `corpse.rs`.

`life-sim`: `simulate()`/`run()` under limits; `observe.rs` — snapshots, chronicle, ASCII maps;
`cores.rs` — holds the thread driving the tick on a hybrid processor's fast cores (Windows).
`life-report`: `main.rs`, `story.rs`, `json.rs` (format `life-report/12`), `metrics.rs`
(`--compare`), `bin/life-sweep.rs`.

### Tick order

Plants → old age → neighbour snapshot → decisions (settings, then a deciding block), healing,
ageing, movement → eating plants → simultaneous strikes and shots → survivors eat corpses →
reproduction → removing the dead, adding children → tick number. Strikes are collected before
damage (mutual death possible); a death has one cause (`Starved`, `OldAge`, `Combat`); a creature
killed gets no prey and does not reproduce.

### Determinism and golden

No global RNG: each creature owns an `Rng` forked from its parent's; the world has its own stream.
Results depend on the seed only, not on iteration order or thread count. Any change in how many
numbers are drawn, or their order, shifts every seed — re-validate balance instead of diffing
numbers. The plant spawner draws even when capped.

`tests/golden.rs` pins an FNV digest of the world at checkpoints for nine configs (the ninth is the
player's world in small with every diet), asserted on Windows only. Minds, corpses and shots enter
as debug prints without their type and field names, so a rename keeps the digest. A refactor must
keep it; a deliberate behaviour change re-records it (the test prints the table) in its own commit
with `--save-reference`. Bit-for-bit identity is proven by comparing golden output of two builds
(`git worktree`). The energy rule is a test too: `energy_is_never_made_from_nothing`
(`tests/engine.rs`) checks tick by tick that the living and the corpses gain no more than the
plant bites gave.

`reference/*.json` — balance fingerprints (8 seeds × 20 000). `--compare` checks each metric's
mean against the reference's per-seed range (exit 1) and refuses (exit 2) when world conditions
differ. Re-take with `--save-reference reference/fingerprint.json` (calm: `--rule cost_scale=3`);
a seed a guard cut short refuses it, an extinct one is kept.

### Performance

- Creatures never see the grid: `Creature::step` takes *senses* (`GridSenses`; tests pass
  `senses_from(..)` or `Blind`). Queries are `#[inline(always)]`. Fixed cell `GRID_CELL`;
  `for_each_near` returns a superset, callers check distance; `Grid::nearest` searches rings out
  from the point's cell with the square scan's tie order. The herd keeps a grid of hunters only
  (threat queries) and children by parent; queries that cannot find anything return before the
  grid (a diet with no corpse food, a creature with no target, defence or rival in combat). The
  grid copies coordinates, valid only because queried entities don't move within the phase. Keep
  the brute-force checks.
- **Where the time goes**: `life-report --phases` prints ms a tick and the share of each phase
  (`profile::Phase`); the game shows the dearest phases under «к/с» (setting «Показывать кадры»).
  On ×100 (~4700 creatures, one thread) the decisions are ~60%, eating ~12%, the herd's snapshot
  ~7%, grids and combat ~5% each.
- **Threads**: with `parallel`, the decisions and the herd's snapshot run on rayon's global pool
  (`par::for_each_mut`, `par::map_into`; one thread under `PARALLEL_MIN` 512 creatures, pieces of
  `PARALLEL_CHUNK` 32). Only work that reads the snapshot and writes its own creature goes there,
  so any thread count gives the same world (`tests/parallel.rs`, golden). Eating, combat and
  division stay sequential: the order of IDs decides who gets a portion. Where they run is
  `par::Threads` (`World::set_threads`): rayon's global pool (default; `life-report --threads`, a
  sweep passes 1, and its worlds then run `One`), `One`, or a `Pool` of the caller's — the game's
  own, sized by the settings' «Скорость расчёта» (auto = the cores − 2, one thread, or 2…all by
  hand; `Command::Threads` rebuilds it on the fly). `life-report` drives each seed on a thread of
  its own outside the pool (`by_seed`): a pool thread driving one would take up another seed's
  whole run while it waited for its decisions, and its deadline clock would count it. The driving
  thread keeps to the
  fast cores (`life_sim::cores`, the settings' «Держать расчёт на быстрых ядрах», greyed out on a
  processor with one kind of core): on the user's i7-13650HX Windows moved it onto an efficiency core
  and the one-thread phases went half as slow (×100: 12.5 → 9.7 ms a tick with it, 20.5 on one
  thread).
- `Creature::step` is the hot path: values precomputed in `Phenotype::of`, squared distances, block
  dispatch is a `match`, never `Box<dyn>`, the scene asks only what a block needs. The eating phase
  copies only the corpses claimed that tick. For refactors compare ms/tick against the previous
  build in a worktree (`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1`).
  `тик_растёт_линейно_с_численностью` guards against full scans.
- **Termination**: every headless run is capped by ticks, population, a work budget and a
  wall-clock deadline (`life_sim::Limits`). Tests have no `while` loops.

## The game (`crates/life-app`)

**The window never waits for the simulation.** Defaults are the user's world: ×20, 2:1,
`cost_scale` 2 (`settings::GAME_COST_SCALE`), `speed_cost` 0.5 (`GAME_SPEED_COST`), half lurkers,
depth steepness 5. Settings show common factors per 100 (price of life 100 = ×2, price of speed
100 = ×0.5, other prices, diet edges, drift, meat founders' size 100 = ×1); digestion shares in %;
«Спокойнее» sets the price of life to 150 (×3). A settings file carries `DEFAULTS_VERSION`: keys in
`CHANGED_DEFAULTS` changed after the file's version take the new default.

- `sim.rs` — the simulation thread owns `World`; UI `Command`s apply between ticks. Frames go
  through a one-slot mailbox, never dropped, so history, log and snapshots travel as deltas.
  Tempo: ticks per second with a capped debt; `SLICE` bounds a burst.
- `frame.rs` — 32-byte instances relative to an f64 `origin`, culled, or a density raster;
  `motion.rs` — previous positions, births, ghosts (plants matched by `born` + x bits; `Plant` stays
  24 bytes); `meta` packs 32 bits the shader reads.
- `render.rs` + `creatures.wgsl` — one instanced draw call; `view.rs` — corpses, patches,
  selection, minimap.
- `game.rs` (game screen, «Графики», the lab, creature card), `behaviour.rs` (programs as
  flowcharts), `diets.rs`, `stats.rs`, `census.rs` (the «Внутри видов» census: histograms, a
  scatter and behaviour groups per diet, taken by `Command::Census` only while the world stands),
  `screens.rs`, `charts.rs` (own painter), `settings.rs`
  (`FIELDS` — the single spec of every field; file in `%APPDATA%\lifegame`, atomic, clamped, unknown
  keys ignored).
- `ui_tests.rs` — kittest at 960×600 and 1600×900 under one GPU lock; CI renders through WARP.
- `bin/life-progress.rs` — the sweep's progress window.
- Looking at the game from an agent: never screenshot the desktop or inject input; render headless
  with `LIFEGAME_SHOTS` and drive state through `LifeApp` fields / `sim::Command`.

## Commands

```bash
cargo test --workspace                              # all tests (life-app screens are most of it)
cargo test --workspace --exclude life-app           # engine, runner, report only
cargo test -p life-core --test golden               # world behaves bit for bit as recorded
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # width 110; CI: cargo fmt --all --check

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000 --rule plant_energy=80 --scale 10
cargo run -p life-report --release -- --mix 1 1 --diet-mix 50 0 0 50
cargo run -p life-report --release -- --ticks 20000 --maps 3 --json run.json
cargo run -p life-report --release -- --scale 100 --ticks 3000 --seeds 1 2 --phases  # time by phase
cargo run -p life-report --release -- --compare reference/fingerprint.json

play.bat / sh play.sh                                    # build + run the game
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world
LIFEGAME_SHOTS=some/dir cargo test -p life-app ui_tests  # screen tests + PNGs
```

CI (`.github/workflows/ci.yml`, Windows): fmt, clippy, tests (`--no-fail-fast`: one failing binary
hides no others), `--compare` against both references.
Dev builds use `opt-level = 2` with line tables only, the dependencies without debug info.

**The build cache.** Cargo never deletes old builds, and every feature set of a dependency is a
build of its own: keep a dependency two crates share in `[workspace.dependencies]` with one feature
list (as `serde_json`), so `-p` builds and `--workspace` share it. `play.bat`/`play.sh` drop
`target/debug` and `target/release` past 8 GB (`target/sweeps` stays). rust-analyzer holds its
proc-macro DLLs in `target/debug/deps`, so `cargo clean` stops on them: remove the folders instead.

**Baseline conditions** — the user's own game; judge balance here, not on ×1 defaults:

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 \
  --scale 20 --shape 2:1 --rule plant_rate=0.5 --rule cost_scale=2 --rule speed_cost=0.5 \
  --rule plant_depth_steepness=5 --mix 1 1 --diet-mix 55 25 10 10
```

**Validating a model change**: seeds 1–8 × 20 000 for the base and calm (`cost_scale=3`) profiles
at ×1 (≥ 7 of 8 worlds survive in each), then the baseline conditions. Runs must end on their own
(`--max-work 1e15`).

**Run every measurement series as a `life-sweep`** (the user wants to see it): say the minutes up
front, keep a run ≤ 5 minutes, a Russian description above each variant, run in the background
with a watcher that ends on a press: `until grep -qE "PAUSED by|STOPPED by" LOG; do sleep 2; done`.

```bash
cargo build -p life-report --release && cargo build -p life-app --release --bin life-progress
target/release/life-sweep plan.txt --out sweeps/hunt --jobs 16 --seconds 300
target/release/life-sweep plan.txt --out sweeps/hunt --summary-only
```

- A plan: `seeds:`, shared `args:`, `variant NAME: ARGS` (a capitalised `NAME=VALUE` token is an env
  var for experiment builds); the top comment block describes the plan, a block the variants below
  it, a trailing comment one variant. Flags the sweep sets itself (`--seed`, `--seconds`,
  `--threads`, `--json`, `--progress`, …) are rejected, and so are a second `seeds:` or `args:`
  line, a repeated seed and variant names that differ only in case (one folder on Windows).
- Each variant × seed is its own `life-report` process, guarded by `--seconds` (a cut run is left
  out of medians) and a watchdog kill `--grace` later.
- Summary (`summary.md/csv`, `runs.csv`): ended, survived, carnivores/scavengers *hold* (≥ 10
  creatures and 1% over the last `--late` share — of the planned run for a world that died out),
  coexistence, late diet shares, late and minimum
  population, carnivore births and kills, `ms/tick`, `slowdown` (⚠ from 3×), `METRIC` lines.
  Parallel big worlds slow each other: use fewer `--jobs` for ×20.
- A run is reused when its JSON and `.cmd` (command line + report build size and time) match and
  the wall clock did not cut it. A result the sweep will not reuse is removed when it starts, so a
  stopped sweep summarises only its own runs.
- Progress: `OUT/progress.json` every second; the `life-progress` window shows it with pause /
  resume / stop writing `OUT/control.txt` (a pause requeues running runs); presses are logged to
  `OUT/events.log`.
