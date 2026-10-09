# CLAUDE.md

Working rules for Claude Code in this repository. Numbers live in the code, not here: `config.rs`
(every constant with why), `Rules::default()` (the same, bit for bit), the program templates in
`creature/program.rs`. `README.md` is the overview, `BEHAVIOR.md` the model's history, newest first.

## Project

lifegame — an evolutionary sandbox: plants and creatures in a 2D sea. A creature carries a genome
(a gene table) and two behaviour programs that mutate on division; selection is emergent, not
scripted. Rust only, model `life-behavior/15`:

- `crates/life-core` — the engine; no graphics, no I/O, threads only through rayon (`parallel`).
- `crates/life-sim` — bounded headless runs, snapshots, the chronicle.
- `crates/life-report` — the balance report, `--compare`, `life-sweep`.
- `crates/life-app` — the game (wgpu/egui) and the sweep's progress window.

Removed things live under tags: `python-final`, `predators-final` (old names still read:
`--vegetarians`, `--veg-mix`, `n_vegetarians`), `flocks-final`.

**Language.** Code, comments, docs, CLI/report output, test names and commits in **English** (the
user's rule since 2026-09-25; the project was Russian before). Much is still Russian from then —
`life-report`'s story, tables and refusals, about half the test names and assertion messages:
translate them only where you edit, never mass-rewrite; a new message or test is English even among
Russian neighbours. Only the **game UI is Russian**: window texts (the `life-progress` window too),
settings labels and hints, gene `label`/`about`, chronicle texts (`life_sim::observe`).

## The user's standing rules

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
- When a mechanic changes, update `README.md`, `AGENTS.md` (short), `BEHAVIOR.md` and this file
  where it states the mechanic.

## The model in brief

- **A creature** eats plants and corpses in portions, grows from food to its size gene, divides
  when grown and its program allows, and dies of hunger, in a fight or of old age (one cause).
  Upkeep = body and eyes (`still_upkeep`) + the speed term of the step actually taken
  (`step_cost`). Cost exponents must stay steeper than benefit (~stat²) or stats run away. Combat
  is always on, but a creature strikes (and shoots) only what its program chose.
- **The body's genes** beyond size, speed and sight: `cold_blood` — cheaper and slower in cold
  water (`Phenotype::temper`); `burst` — faster in a chase or flight, then winded; `lifespan` —
  weakening late in life (`phenotype::vigour`); `maturation` — food into growth while young.
- **Programs** (`creature/program.rs`): ≤ 32 blocks «if up to three tests → an action(params)».
  Each tick the **settings** apply first, wherever they stand — of each kind (each mode its own
  kind) only the first whose tests hold; then the first **deciding** block whose tests hold and
  whose action can be done decides; nothing decides → it stands. Without a setting a creature
  doesn't do it (no «делиться» — no division). Healing and kinship read the last tick's stance;
  eating, combat, shots, defence and division read this tick's. A block decides (`Intent`); only
  the creature acts — a block cannot move, feed or divide it.
- **Two tracks**: `programs[JUVENILE]` until grown, `[ADULT]` after; inherited and mutating apart,
  sharing the modes (the memory). `Programs` is an `Arc`; each `Program` caches a `Summary`,
  recomputed on every change.
- **Heredity** (`CreatureGenome::inherit`): one clone draw for genome and programs; otherwise the
  genes mutate, the programs drift and now and then take one structural mutation (`MUTATIONS`). A
  block is **dead** when off, never reached, or a setting behind an unconditional one of its kind;
  **no junk is born** — an added block that would leave any block dead is not added. Past its end a
  program is padded with the same block, so equal blocks are equal programs.
- **Menace**: others read a creature by the hunt, fight-back and defence blocks whose tests held on
  its last move. Family is a parent and its growing child, only while «щадить детей» holds.
- **Templates** (`Strategy`): the `strategy` gene picks the founders' template and never switches.
  There are no world behaviour constants: the numbers live in the blocks.
- **Diets** (`diet` gene, order H/O/S/C in every `DIET_*` table): digestion and body edges are
  world rules (`Rules::diets`, keys `{diet}_{edge}`, the lab's «Питание»), read in
  `Phenotype::of`. Digestion 0 = neither eats nor goes for it. Without «есть и чужую пищу» a diet
  keeps to its own corpse stages (`DIET_OWN`); plants are never gated, digestion alone decides.
  By default the founders are herbivores and omnivores (`DIET_START_MIX`); a founders' diet mix
  may deal meat diets (bigger, `MEAT_FOUNDER_SIZE`), else they arise from mutants.
- **Space and food**: scale is area (`per_area`), vertical ecology in % of depth, real units only
  for showing (`units.rs`). A layer is a preference, not a wall: physics clamps only to the world —
  never teleport. Plants live in `PLANT_MAX` slots, one each; the occupied slots are a bitset
  (`world::Occupancy`) rebuilt when the plant count stops matching — keep direct edits of `plants`
  changing the count.
- **Corpses**: meat = the body grown since birth + the tank (`corpse::meat`); fresh, then rot that
  sinks and decays to bones (`corpse_*` rules).

## Genes, blocks, rules: how to add

Gene, `Cond`, `Action` and parameter tables are **append-only** (the order fixes RNG draws and
positions in references and JSON). Everything walks the gene table (`GeneSpec`); a choice gene with
one variant is inert.

- **A gene**: a variant at the end of `enum Gene` and a row at the end of `GENES`; its effect only
  in `Phenotype::of`, any cost at the end of the upkeep sum; a test; check the genome panel and
  card at 960×600 with `LIFEGAME_SHOTS`; re-record golden and references in a separate commit.
- **A test or action**: a variant at the end of `Cond`/`Action` and their `ALL`, a `ParamSpec`
  table (a new sense = trait method + query + brute-force check), its `Scene::test` or
  `actions::act` arm, window labels, tests in `strategy.rs`, re-record golden.
- **A template**: a variant at the end of `Strategy`/`VARIANTS` (≤ 8) and its `Program::template`.
- **A rule**: `RULE_KEYS` + `with`/`get` + a `FIELDS` entry in `life-app/src/settings.rs`.
  `with()` rejects unknown keys, non-finite and senseless values (an exponent over 10, an upkeep
  that leaves f64), never merely unbalanced ones. Several rules at once go through
  `with_all`/`with_texts`: checked as a whole, so their order does not matter; the game shows a
  refusal, never panics.

## Architecture and invariants

**Logic is separated from rendering** by crate boundaries: `life-core` knows no screen and does no
I/O; `life-app` alone knows the screen.

- `life-core/src`: `config.rs`, `rules.rs`, `world.rs` (`World::step` — the phase order only),
  `genome/`, `creature/` (`mod.rs` act/feed/divide, `phenotype.rs`, `program.rs`, `scene.rs`
  perception, `actions.rs`, `strategy.rs` `plan`, `steer.rs`), `flora.rs` + `plant.rs`,
  `combat.rs`, `corpse.rs`, `senses.rs` + `grid.rs`, `par.rs`, `profile.rs`, `space.rs` +
  `units.rs`, `rng.rs`.
- **Tick order**: plants → corpses decay → old age, the last tick's kinship kept
  (`Creature::remember_kin`) → neighbour snapshot → decisions, healing, ageing, movement →
  eating plants → simultaneous strikes and shots → survivors eat corpses → reproduction → removing
  the dead, adding children → tick number. Strikes are collected before damage (mutual death
  possible); a creature killed gets no prey and does not reproduce.
- **Determinism**: no global RNG — each creature owns an `Rng` forked from its parent's, the world
  has its own stream. Results depend on the seed only, never on iteration order or thread count.
  Any change in how many numbers are drawn, or their order, shifts every seed. The plant spawner
  draws its fractional chance every tick, capped or not (a capped one plants nothing more).
- **Golden** (`tests/golden.rs`): an FNV digest of nine configs at checkpoints, asserted on Windows
  only. Minds, corpses and shots enter it as debug prints without type and field names, so a
  rename keeps the digest. A refactor must keep it; a deliberate behaviour change re-records it
  (the test prints the table) in its own commit.
  Bit-for-bit identity is proven by comparing golden output of two builds (`git worktree`).
  `energy_is_never_made_from_nothing` (`tests/engine.rs`) checks the energy rule tick by tick.
- **Re-recording** happens on Windows only: on the user's machine directly (`GOLDEN_RECORD=1` and
  the two `--save-reference` commands below), from a cloud session through CI — push the change
  (on the user's word) with a line «[re-record]» alone in a commit message, or run Actions →
  Re-record; the marker quoted inside a sentence asks for nothing, so a message may name it. The
  workflow (`rerecord.yml`) prints which seeds of the player's world take the whole food chain in
  (the energy test and golden's case I need it; pick another seed if theirs lost it), records
  golden (`GOLDEN_RECORD`) and both references, runs every test and the balance against them, and
  commits them to the branch — on top of docs that came meanwhile, never of other code (then it
  fails: run it again). Read its log through the GitHub tools.
- **References** (`reference/*.json`, 8 seeds × 20 000): `--compare` checks each metric's mean
  against the reference's per-seed range (exit 1) and refuses differing world conditions (exit 2).
  Re-take with `--save-reference reference/fingerprint.json` and `--save-reference
  reference/calm-fingerprint.json --rule cost_scale=3`; a seed a guard cut short refuses it, an
  extinct one is kept.
- **Performance**: creatures never see the grid — `Creature::step` takes *senses* (tests pass
  `senses_from(..)` or `Blind`); `for_each_near` returns a superset, callers check distance; keep
  the brute-force checks. `Creature::step` is the hot path: values precomputed in `Phenotype::of`,
  squared distances, `match` dispatch, never `Box<dyn>`. `тик_растёт_линейно_с_численностью`
  guards against full scans; `life-report --phases` shows where a tick goes. For refactors compare
  ms/tick against the previous build in a worktree
  (`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1`).
- **Threads**: only work that reads the snapshot and writes its own creature runs in parallel
  (`par::Threads`: rayon's global pool, `One`, or the caller's `Pool`; one thread below
  `PARALLEL_MIN`); results keep creature order and no float is summed across threads
  (`tests/parallel.rs`: the same world on any threads). Eating, combat and division stay
  sequential — the order of IDs decides who gets a portion. `life-report` drives each seed on
  a thread of its own outside the pool (`by_seed`), and on one thread its worlds run `One`. The
  driving thread keeps to a hybrid processor's fast cores (`life_sim::cores`).
- **Termination**: every headless run is capped by ticks, population, a work budget and a
  wall-clock deadline (`life_sim::Limits`). Tests run worlds only in `for` loops
  over ticks, never `while` a world reaches some state.

## The game (`crates/life-app`)

**The window never waits for the simulation.** Defaults are the user's world (×20, 2:1,
`Settings::default`, `settings::GAME_*`) but for the founders' diet mix, which is the engine's
(`DIET_START_MIX`: no scavengers or carnivores); the user plays ×100 and 55/25/10/10. Settings show common
factors per 100. A settings file carries `DEFAULTS_VERSION`: keys in `CHANGED_DEFAULTS` take the
new default.

- `sim.rs` — the simulation thread owns `World`; `Command`s apply between ticks; frames go through
  a one-slot mailbox, never dropped, so history, log and snapshots travel as deltas. Commands built
  from a frame carry its `world_gen`; a click (`Pick`) its `number` and the window's animation
  progress too, and picks the bodies where that frame drew them at that moment.
- `frame.rs` (32-byte instances relative to an f64 origin), `motion.rs`, `render.rs` +
  `creatures.wgsl` (one instanced draw), `view.rs`, `game.rs`, `behaviour.rs` (programs as
  flowcharts), `stats.rs`, `census.rs`, `settings.rs` (`FIELDS` — the single spec of every field;
  the file atomic, clamped, unknown keys ignored).
- `ui_tests.rs` — kittest at 960×600 and 1600×900 under one GPU lock; CI renders through WARP and
  uploads every screen's picture as the `ui-shots` artifact (14 days; the `gallery_*` tests only
  take pictures). A cloud session reads them through the GitHub tools
  (`download_workflow_run_artifact`).
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
cargo run -p life-report --release -- --ticks 20000 --maps 3 --json run.json
cargo run -p life-report --release -- --compare reference/fingerprint.json

play.bat / sh play.sh                                    # build + run the game
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world
LIFEGAME_SHOTS=some/dir cargo test -p life-app ui_tests  # screen tests + PNGs
```

CI (`.github/workflows/ci.yml`, Windows): fmt, clippy, tests (`--no-fail-fast`), `--compare`
against both references. Keep a dependency two crates share in `[workspace.dependencies]` with one
feature list, or cargo builds it twice; `play.bat`/`play.sh` drop `target/debug` and
`target/release` past 8 GB.

## Balance

**Baseline conditions** — the user's own game, at ×20 standing in for the ×100 they play (the user's
choice, 2026-10-04: ×100 is five times the work a tick); judge balance here, not on ×1 defaults:

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 \
  --scale 20 --shape 2:1 --rule plant_rate=0.5 --rule cost_scale=2 --rule speed_cost=0.5 \
  --rule plant_depth_steepness=5 --mix 1 1 --diet-mix 55 25 10 10
```

**Balance checks are the user's call.** Survival criteria (seeds 1–8 × 20 000 for the base and calm
`cost_scale=3` profiles at ×1, ≥ 7 of 8 worlds surviving in each), the baseline conditions and any
other measurement run only when the user asks: a change is done without them, and an unchecked
balance is no open item to report. Runs that are made must end on their own (`--max-work 1e15`).

**Run every measurement series as a `life-sweep`** (the user wants to see it): say the minutes up
front, keep a run ≤ 5 minutes, a Russian description above each variant, run in the background
with a watcher that ends on a press: `until grep -qE "PAUSED by|STOPPED by" LOG; do sleep 2; done`.

```bash
cargo build -p life-report --release && cargo build -p life-app --release --bin life-progress
target/release/life-sweep plan.txt --out target/sweeps/hunt --jobs 16 --seconds 300
target/release/life-sweep plan.txt --out target/sweeps/hunt --summary-only
```

- A plan: one `seeds:` line, one shared `args:` line, `variant NAME: ARGS` (a capitalised
  `NAME=VALUE` token is an env var for experiment builds); comment blocks describe the plan and the
  variants below them. Flags the sweep sets itself are rejected.
- Each variant × seed is its own `life-report` process under `--seconds` and a watchdog kill
  `--grace` later; a cut run is left out of the medians. Parallel big worlds slow each other: use
  fewer `--jobs` for ×20.
- A run is reused when its command line and report build match and the wall clock did not cut it;
  anything else is removed when the sweep starts, so a stopped sweep summarises only its own runs.
- Progress goes to `OUT/progress.json`; the `life-progress` window pauses, resumes and stops through
  `OUT/control.txt`, presses logged to `OUT/events.log`.
