# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

lifegame — an evolutionary sandbox: plants and creatures in a 2D world. Creatures carry
a genome (a gene table) that mutates on division; selection is emergent, not scripted. Rust only:
engine `crates/life-core`, bounded headless runner and observer `crates/life-sim`, balance report and
sweeps `crates/life-report`, the game (wgpu/egui) `crates/life-app`. Open: phase 3, a parallel tick
(design, no code: `docs/phase3-parallel-tick.md`, written against `life-behavior/7`), and phase 6, a
machine benchmark. Removed things live under git tags: the Python version (`python-final`, the
behavioural spec the game was ported from), predators (`predators-final`; old names still read:
`--vegetarians`, `--veg-mix`, settings key `n_vegetarians`).

**Language.** Code, comments, docs, CLI/report output, test names and commit messages in
**English**; translate Russian comments and test names you touch, don't mass-rewrite. Only the
**game UI stays Russian**: window and button texts, settings labels and hints, gene
`label`/`about`, chronicle texts (`life_sim::observe`, which the report prints too).

**The user's standing rules**
- **Energy is never made from nothing.** It enters the world only in plants and then only passes
  along the chain (plant → eater → corpse → eater), losing some. No mechanic, bonus, rule or lab
  limit may create or duplicate it: digestion ≤ 100% of a food (`Rules::with` rejects more), deep
  saving ≤ the whole upkeep, a body got for free (a founder's, a newborn's birth size) is not meat.
  Check every new mechanic against this.
- **Fix niches through the body, evolutionarily**: a diet may have a different body with its own
  advantages, never free energy. Don't adopt a balance change without the user's word; before a big
  mechanic change ask rounds of questions with the risk of each option.
- **Behaviour genes are free**: no upkeep for a gene that gives no physical stat. A behaviour gene
  needs a behavioural catch, not a price (the free «испуг»/«голод» genes once ran away and were
  removed).
- **Flocks are off** (`config::FLOCKS = false`): every founder is a loner, so is the base genome
  (`pack_instinct` base follows `FLOCKS`: a spawned creature founds no flock), the game hides
  «Стаи». The code still runs every tick (`social::prepare_in`, `flock::update`, battles,
  territories); it is to be removed under a tag `flocks-final`. Never measure balance with flocks.
  **The social layer is not inert for loners** (kept on purpose, user's call 2026-09-27, until the
  layer goes): `social::adjust` holds a wander course 30 ticks and turns at most 1.2 rad a tick,
  `Scene::plant` keeps the plant it chose while it stays visible (`personal_food`), and a child of
  another mode gets `kin_grace` 600 ticks with its parent's label (`world.rs`, `protect`). Resting
  moved from the layer into the programs (`Action::Rest`). Removing the
  layer therefore shifts every seed: `flocks-final` needs the balance validation, and golden proves
  only the flock-only parts inert.
- **Commit straight to `main`**, only when asked; push only on the user's word. `main` also moves
  from cloud sessions: `git fetch` and fast-forward before starting.
- **In a cloud (Linux) container run no simulations and no tests** — no `life-report`,
  `life-sweep`, `life-app`, `cargo test`. The user's world is Windows (libm differs in the last
  bits). Fine there: reading, editing, `cargo build`, `fmt --check`, `clippy`; say which checks
  were not run.
- `Relict/` is a frozen 2025 archive: never edit, fix, modernise or translate anything there.
- The user keeps a short PDF of genes, strategies and diet edges (a throwaway fpdf2 script, Arial
  for Cyrillic); regenerate and send it after diet or gene changes.

## The model (`life-behavior/13`: behaviour programs on the ocean reform)

A creature eats plants and corpses in portions, grows from food up to its size gene, divides when
grown and fed, and dies of hunger, in a fight or of old age. Combat is always on: a creature strikes
(and shoots) only what its program chose (prey, a rival at food, a fight back); being struck strikes
back only through a fight-back block. Melee damage is 5% of the striker's size
× (its size / the target's) ** `melee_size_power` (1.25) when it is the bigger (`Phenotype::strike_on`;
hunters weigh prey and retaliation with the same function); a strike's energy cost does not
depend on diet bonuses. Shots are weak (1% of size, ≤ ¼ of max health) and expensive. Contact is
the sum of the two radii. A hunt block weighs the meat the tank can take against the strikes it
expects (its caution, and only the strikes the prey's own program would give back) and gives up a
chase that has not closed the gap by one of its steps in its patience (template `CHASE_PATIENCE` 30
ticks), leaving that prey alone for its block's time (template 180). What a creature does and when
is its behaviour program (below). Exact mechanics and their checks: `BEHAVIOR.md` (partly stale, see
below).

**Behaviour programs** (`creature/program.rs`, the user's design, 2026-09-28). Behaviour is an
ordered rule list of ≤ 24 blocks «if TEST and TEST → ACTION(parameters)». Each tick the
**settings** (`EatForeign`, `Rival`, `Reach`) whose tests hold apply first, wherever they stand (a
swap cannot hide one behind a deciding block): they set this tick's `Stance` — eat the other
niche's food; strike strangers X times smaller at the same food; go no farther than X% of depth
past its layer for food — which the world's eating and combat phases read. Then the first deciding
block whose tests hold and whose action can be done decides; a failed action (no prey, no corpse, a
hopeless chase, not at home to rest) falls through; nothing decides → it stands. `Mind` keeps what
the window shows: `fired`, `applied` and `tried` (tests held, action could not be done).
- **Two tracks**: `Creature::programs[JUVENILE]` while it grows to its size gene, `[ADULT]` after
  (`Creature::stage`, `program()`); inherited and mutating apart. `Programs` is an `Arc`: children
  that inherit them unchanged share them, so a creature holds a pointer, not a kilobyte. Each
  `Program` keeps a summary the world reads every tick (reach, threat range, hunt ratio, defence,
  wander reach), recomputed whenever it changes.
- **Tests** (`Cond`): fullness, health, depth ≥ X%; a threat / a hunting stranger closer than X% of
  sight (`Senses::threats_near`); struck within X ticks (template 1); still fleeing; resting; food
  seen; always (negated: never, which switches a block off).
- **Actions** with parameters (`Action::params`, a `ParamSpec` each: range, base, nudge): fight
  back (enemy at most ×1.5 bigger), flee (keep running 60 ticks after losing it, burst; under way
  only a threat nearer than 33% of sight — the old flight distance — renews the flight, a farther one
  only steers it), hunt (prey ×1.5 smaller, caution 100% = the old base weight, patience 30 ticks,
  only if better than plants and corpses, burst, leave given-up prey alone 180 ticks), to a corpse
  (only if better, pace), to a plant, to reported food, back to the circle, wander (pace, targets a
  quarter to 200% of sight away; the next target after a bite as far), ambush (stand), to the top /
  bottom of its layer (pace), rest (90 ticks, then a pause of 180), torpor (stands paying
  `TORPOR_UPKEEP` 30% of its standing upkeep and **eats nothing**, not even what touches it — no
  sleeping filter feeder), and the three settings. Labels read in their units («33% зрения», «31
  тик», plural by `program::ticks_word`).
- **No behaviour genes and no world behaviour constants are left**: `bravery`, `prey_ratio`,
  `caution`, `picky`, `rivalry`, `layer_reach`, `cruise`, `rest`, `torpor` were deleted from the gene
  table (the user's call, breaking append-only once). `burst` stays: it is the muscles, a body stat
  with a price; the flee and hunt blocks decide whether to use them.
- **Templates**: the `strategy` gene («происхождение») picks the founders' template and never
  switches (`STRATEGY_SWITCH_CHANCE` 0); both tracks start from it. `Program::STANDARD` carries the
  old genes' bases: below 30% fullness eat foreign food and drive rivals ×1.5 smaller; fight back
  above 50% health; flee a hunter within 33% of sight, a calm stranger within 16%, keep fleeing;
  hunt; rest from 95%, keep resting to 85%; corpse; plant; wander. `Program::LURKER` is the same,
  wandering at 33% of its speed. While flocks are off the templates leave out the flock blocks (to
  reported food, back to the circle: `Action::needs_flock`) and no mutation brings them in;
  `Program::IN_FLOCKS` has them for the flock layer's tests.
- **Heredity** (`CreatureGenome::inherit`): one clone draw for genome and programs (a clone shares
  them); else the genes mutate, then each program **drifts** — every number but the flags moves by
  gauss(0, its nudge × `program_drift` (rule, base 1) × mutability): a threshold ~10 points, a ratio
  0.2, a time a quarter of its base, as the genes drift by 30% (through the rare mutation alone a
  given number moved in one child of ~1700, and the old genes' adaptations could not happen) — and
  with `program_mutation` (rule, base 5%) × mutability gets one mutation: nudge a number 35%,
  replace a test 12%, negate one 8%, replace the action 8%, swap with a neighbour 15%, duplicate 8%,
  delete 8% (keeps ≥ 1), insert a random block 6%. One that cannot apply or lands where it was
  changes nothing, its draws spent. `Program::changes` counts the mutations that changed something
  (not the drift). Past its end a program is filled with the same block, so equal blocks are equal
  programs.
- **Free behaviour**: no upkeep, no energy made — an action only chooses where to step or how to
  stand, `act` pays. Others fear a creature by the most permissive hunt block of the program it
  lives by (`Program::hunt_ratio`, `Herd`); a hunter expects strikes back only as the prey's program
  gives them (`Program::defence`: a hunter within its first fight-back block's ratio, for the share
  of the killing strikes above that block's health threshold) — the templates' prey never strikes a
  template hunter, which is ≥ 1.5 times bigger. Aid and flock guards read `stands_firm` (its
  fight-back block's health threshold).
- The interpreter: `scene.rs` (perception, lazily memoised queries), `actions.rs` (one function per
  action), `strategy::plan` (settings, then the deciding blocks, then `social::adjust`). `Cond`,
  `Action` and their parameter tables are append-only, like gene tables. The game shows both tracks
  as flowcharts (card button «Поведение (B)», `life-app/src/behaviour.rs`): the settings as their
  own section on top, then the decisions, this tick's path lit with «не вышло» where a block's
  action failed. The report groups each track by shape (`Program::shape`: tests, actions and flags,
  no numbers), prints the three most common with the medians of their numbers (`Program::median`)
  and `METRIC` lines for sweeps (`{juvenile,adult}_shapes`, `_template_share`, `_hunt_ratio`,
  `_threat_range`).

**Diets** — the `diet` choice gene, variant order H/O/S/C (the order of every `DIET_*` table in
`config.rs`). Digestion (`DIET_DIGESTION`: plants, fresh meat, rot, bones; 0 = neither eats nor
goes for it) and the edges are world rules (`Rules::diets`, `DietEdges`, keys `{diet}_{edge}`, lab tab
«Питание»), all read in `Phenotype::of`:

| | strike | other edges |
|---|---|---|
| herbivore | ×1 | health ×1.5, size term of upkeep ×0.85 |
| omnivore | ×1.15 | eats everything |
| scavenger | ×1.3 | plants 15%; smells corpses at 3× vision (free); alone digests bones (90%); founders start deep and fully cold-blooded |
| carnivore | ×3 | plants 20%; speed term of upkeep ×0.5; smells corpses at 1.5× vision; juvenile gut |

- Juvenile gut: until grown to its size gene a creature digests plants at
  `max(plants, young_plants)` (`DIET_YOUNG_PLANTS` 0/0/0/0.7, «Растения в детстве»; 0 = as grown).
  A carnivore mutant is born half grown and small prey is rare, so it grows on plants and hunts
  grown. Only the grown divide, so staying young is no loophole.
- **Own niche**: without its program's «есть и чужую пищу» setting a creature takes only its own
  food — a scavenger skips fresh corpses and does not hunt, a carnivore skips rot and bones
  (`Phenotype::corpse_efficiency(stage, foreign)`, `DIET_OWN`); the templates set it below 30%
  fullness. Anyone who eats fresh meat (`hunts`) and has a working hunt block is feared.
- No meat founders by default (`DIET_START_MIX` 70/30/0/0): they starved with nothing to eat. Meat
  diets arise from mutants (see mutation laws). Meat founders set in a mix start
  `meat_founder_size` (×2) bigger.
- **Corpses**: meat = the body grown since birth at `GROWTH_ENERGY_PER_SIZE` + the tank
  (`corpse::meat`); a creature with no meat leaves no corpse. Three sharp stages (`corpse::Stage`,
  each its own food column), clock in rules `corpse_*` (`CorpseClock`): **fresh** 300 ticks where it
  died; then **rot** at once, sinking at 2 a tick (a speed, not a time) to its own resting place in
  the lowest 25% of the depth while its flesh decays evenly down to the bones by 3000 ticks from
  death; **bones** (10% of the meat) when the flesh is eaten or rotted away, sinking at 40, lying
  5000 ticks. Bones at 20 000 made scavengers as strong as carnivores (24 seeds: both held in 58%);
  the user chose 5000.
- **Cold deep** (replaced the scavenger's deep saving): warm water down to `thermo_top` (15% of
  depth), cold from `thermo_bottom` (45%), a smooth step between. The `cold_blood` gene (%, base 0 =
  warm-blooded) makes a body cheaper (−50% at 100% in full cold, `COLD_SAVING`) and slower (−40%,
  `COLD_SLOWING`) in cold water (`Phenotype::temper`, applied in `act` at the current depth). It
  moves by points (`Mutation::Shift`, ±10), since a factor never leaves zero.
- **Layer reach**: the program's «за едой из слоя» setting (% of depth; without it anywhere, and the
  templates have none) — how far past its layer a creature goes for food it sees; checked inside
  the plant, corpse and prey choices (`Phenotype::within_reach(y, reach)`), so it takes the best
  food within reach. A behaviour, not a wall.
- **Paying for the step taken**: upkeep = the body and eyes (`still_upkeep`) + the speed term for
  the step actually taken (`Phenotype::step_cost`); standing, eating or resting costs no speed.
  `upkeep` is the full-speed figure, for showing and weighing.
- **Movement**: the pace of wandering and of going to plants, reported food, corpses and the
  layer's edges is each block's pace parameter (% of speed, at least `MIN_PACE` 10%). Resting
  (`Action::Rest`: stands at home up to its length, then its pause without resting; it ends when
  its block no longer decides) and torpor (`Action::Torpor`, `Creature::torpid`, «в оцепенении» on
  the card; placed after the food and flight blocks it sleeps only with nothing to do) are blocks.
  `burst` (gene, ×1–2, base 1) — in a chase to a goal beyond a step or in flight, when its block
  asks for it, it goes that much faster for ≤ 20 ticks, then 60 winded (`Creature::dash`,
  `winded`); the muscles cost standing (¼ of the extra speed's term, `BURST_UPKEEP_SHARE`), so a
  slow body with a big burst is no free speed gene.
- **Life**: `maturation` (%, base 50) — the share of digested food that goes into growth until
  grown, the rest into the tank. `lifespan` (base 3000, 500–10 000 ticks) — death of old age; from
  70% of it speed, vision, strike and max health fall linearly to 70% at 90% (`phenotype::vigour`,
  `Creature::grow_old` at the start of the tick). Both free genes.
- Plants grow in patches over the depth profile «океаническое» (the default since the reform; see
  "Where food grows"). A settings file saved before the reform takes the new defaults of the
  profile and the corpse times and keeps the player's other values (`DEFAULTS_VERSION`).

## Where the work stands

- **Behaviour programs** (plan `~/.claude/plans/starry-jumping-shannon.md`, 2026-09-28): stage 1
  (the interpreter replacing `standard.rs`/`lurker.rs`) was proven bit for bit against the old
  strategies; stage 2 (mutating programs), stage 3 (the window, the report) and round 2 (two tracks,
  all behaviour in blocks, nine genes deleted) made the model `/13`, golden re-recorded on Windows.
  A review of the logic (the user: «исправь») then brought: the numbers' drift (without it they were
  all but frozen), a flight renewed only within 33% of sight (round 2 had renewed it on any threat
  in sight, a longer flight than the old strategy's), a torpid creature that eats nothing, hunters
  weighing the prey's own defence, settings applied wherever they stand, no flock blocks while
  flocks are off, the last behaviour constants moved into blocks (the given-up prey's time, the
  wander's reach, the «struck» window), shared programs with a cached summary; golden re-recorded
  again. **Unmeasured**: no world runs were made (the user's word) — balance and tick rate are
  unknown; the templates carry the old gene bases, but the old genes had spread and evolved, the
  blocks start uniform. The drift, the torpor and the hunter's weighing are balance changes.
- **Ocean reform** (plan `~/.claude/plans/snug-puzzling-pixel.md`, 2026-09-27, all stages in):
  A ocean profile + real units (bit for bit), B corpse stages, C cold deep and genes, D paying for
  the step taken with `cruise` and `rest`, E burst, F torpor, G the ocean profile as the default,
  golden digests re-recorded, model `/12`. The user stopped the measurements after D ("не тестируй
  а делай изменения"): E, F and the ocean default are **unmeasured**. Baseline conditions, 24 seeds
  × 20 000:
  - before B: carnivores hold in 80%, scavengers 20% (35 seeds), late population 1320;
  - B (bones 5000): carnivores 71%, scavengers 46%, 1424; the corpse count rose from ~1000 to
    6000–21 000 at bones 20 000;
  - C: carnivores 88% (21/24), scavengers 38%, carnivores 3.4% late, 1413; `cold_blood` evolves to a
    mean of 14–18%, `layer_reach` falls to a median of 69%. ×1 (8 seeds): all survive, carnivores
    22–33% late (47% base before B), populations 57 / 185.
  - D (speed price ×1): carnivores 67% (16/24), scavengers 38%, 1.7% late, 1613. Paying for the
    step helps grazers most; carnivores chase at full speed. `cruise` evolves to ~55%, `rest` to
    ~78%, the speed gene up from 15.5 to 19. A dearer speed (×1.5, ×2) only hurt carnivores more
    (12 and 14 of 24). The burst (E) is the hunter's answer, not yet measured.
  - «океаническое» vs «игровое» before B (8 seeds): carnivores held alike (7) but at 2.8% late
    against 7.8%, population 1384 against 1228.
- Balance after the life reform (sweep 2026-09-27, 8 seeds × 20 000): all 24 worlds survive.
  - Baseline conditions: carnivores hold in 7 of 8 (7.8% late), scavengers in 1, population late
    median 1228, minimum 496.
  - ×1: populations collapsed — late median 90 (base) and 136 (calm), against 470 / 205 before the
    reform and 973 / 750 before the three niches; carnivores 22–25%; herbivores died out in 3 worlds
    (base seeds 5, 8; calm seed 5), leaving carnivores alone on plants. The carnivores were already
    too strong at ×1 before the reform. **Open, the user decides.**
- Before that: the three niches (the user's choice) — juvenile gut, the carnivore's nose ×1.5, the
  scavenger's ×3 — carnivores held in 6 of 8 baseline worlds, scavengers in 3. Don't repeat: the
  parent paying for the child's body (halved populations, no meat diets); birth at ¼ size; a nose
  paid like sight (killed the niche); `melee_size_power` 1.75 (carnivores boomed and starved) or 1.0
  (they died out).
- Golden digests are re-recorded for `/13` (2026-09-28, Windows). Both references
  (`reference/fingerprint.json`, `calm-fingerprint.json`) are still the `/9` ones, so `--compare`
  refuses them (another model) and CI fails there by design: re-take them with `--save-reference`
  once the user accepts the balance (8 seeds × 20 000 each).
- `README.md` and `AGENTS.md` describe the `/13` model in short; `BEHAVIOR.md` is the history of
  the models, its first section the food web to the ocean reform. This file is the exact model.
  When a mechanic changes, update all four.

**Baseline conditions** — the user's own game; judge balance here, not on ×1 defaults:

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 \
  --scale 20 --shape 2:1 --rule plant_rate=0.5 --rule cost_scale=3 --rule plant_depth_steepness=5 --mix 1 1
```

## Commands

```bash
cargo test --workspace                              # all tests (~30 s; ~24 s of it — life-app screens)
cargo test --workspace --exclude life-app           # engine, runner, report only (a few seconds)
cargo test -p life-core --test engine сетка         # tests whose name contains «сетка»
cargo test -p life-core --test golden               # world behaves bit for bit as recorded
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110; CI: cargo fmt --all --check

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000 --rule plant_energy=80 --scale 10
cargo run -p life-report --release -- --mix 1 1 --diet-mix 50 0 0 50   # strategy and founder diet mixes
cargo run -p life-report --release -- --ticks 20000 --maps 3           # story + text maps
cargo run -p life-report --release -- --ticks 5000 --json run.json     # everything as JSON (+ text)
cargo run -p life-report --release -- --compare reference/fingerprint.json

play.bat / sh play.sh                                    # build + run the game
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world (same flags as the report)
TINYLIFE_SHOTS=some/dir cargo test -p life-app ui_tests  # screen tests + PNGs of every screen
```

Looking at the game from an agent: never screenshot the desktop or inject input. Render headless
with `TINYLIFE_SHOTS` (egui_kittest) and drive state through `LifeApp` fields / `sim::Command`.

CI (`.github/workflows/ci.yml`, Windows only): fmt, clippy, tests, `--compare` against both
references. Golden digests and references are Windows-only (libm). Dev builds use `opt-level = 2`.

**Validating a model change**: seeds 1–8 × 20 000 for the base and calm (`cost_scale=3`) profiles
at ×1 (at least 7 of 8 worlds survive in each), then the baseline conditions. Runs must end on
their own (`--max-work 1e15`). A single-seed run prints genome start → end including diet shares
(`питание`).

**Run every measurement series as a `life-sweep`** (the user wants to see it): say the minutes up
front, keep a run ≤ 5 minutes, a Russian description above each variant, run it in the background
with a watcher that ends on a press:
`until grep -qE "PAUSED by|STOPPED by" LOG; do sleep 2; done`.

```bash
cargo build -p life-report --release && cargo build -p life-app --release --bin life-progress
target/release/life-sweep plan.txt --out sweeps/hunt --jobs 16 --seconds 300
target/release/life-sweep plan.txt --out sweeps/hunt --summary-only
```

- A plan: `seeds:`, shared `args:`, `variant NAME: ARGS` (a capitalised `NAME=VALUE` token is an env
  var for experiment builds); comments: the top block describes the plan, a block the variants
  below it, a trailing comment one variant. Flags the sweep sets itself (`--seed`, `--seconds`,
  `--threads`, `--json`, `--progress`, …) are rejected in either spelling.
- Each variant × seed is its own `life-report` process. Guards: the report's `--seconds` deadline
  (a cut run is left out of medians), a watchdog kill `--grace` later, the worst case printed first.
- Summary per variant (`summary.md/csv`, `runs.csv`): ended, survived, carnivores/scavengers
  *hold* (≥ 10 creatures and 1% over the last `--late` share), coexistence, late diet shares, late
  and minimum population, carnivore births and kills, the tick rate (`ms/tick` — the median run's
  median lap) and `slowdown` (a run's worst lap over its median, ⚠ and a `TICK RATE FELL` line from
  3×), and any `METRIC name value` lines. The report measures the tick rate every 500 ticks
  (`pace` in its JSON, the third number of the tick file). Parallel ×20 runs slow each other down:
  16 at once ran at 3.3–5.1 ms a tick against 2.4–2.6 with 8, so use fewer `--jobs` for big worlds.
- A run is reused when its JSON and `.cmd` (command line + the report build's size and time) match,
  so a stopped sweep resumes and a rebuilt report reruns everything.
- Progress: `OUT/progress.json` every second (runs' ticks from `s<seed>.tick`, written by the
  report's `--progress FILE`, one seed, atomically). The window `life-progress` (always on top,
  own dark palette, taskbar bar via `ITaskbarList3`) shows the bar, time left, running variants
  with a «+» in the card's corner to unfold each seed (tick, time, ms a tick), the last runs (amber:
  its tick rate fell), and pause / resume / stop
  buttons writing `OUT/control.txt`. A pause kills the running runs and requeues them (a world
  depends on its seed only); presses are printed as `PAUSED/RESUMED/STOPPED by the user in the
  window` and appended to `OUT/events.log`.

## Architecture

**Logic is separated from rendering** — the load-bearing decision, enforced by crate boundaries:
`life-core` depends on nothing graphical (not even threads or I/O), `life-sim` adds the bounded
runner and the observer, `life-app` alone knows the screen.

`crates/life-core/src/`:
- `config.rs` — every tunable constant with a comment on *why* that value.
- `rules.rs` — `Rules`, the lab's world rules, set at start or live (`World::set_rules`,
  `apply_rules` recomputes phenotypes). `Rules::default()` is `config.rs` bit for bit. Changing an
  exponent renormalises its coefficient so the base genome pays the same. `with()` rejects unknown
  keys, non-finite values and values where a rule stops making sense — never merely unbalanced
  ones. A new rule: `RULE_KEYS` + `with`/`get` + a `FIELDS` entry in `life-app/src/settings.rs`.
- `world.rs` — `WorldConfig`, `World::step()` (phase order only), counters, `spawn_*`.
- `genome/` — the gene table, `CreatureGenome` (`[f64; N]` indexed by `enum Gene`), mutation.
- `creature/` — `mod.rs` (`act`, feeding, division, `grow_old`), `phenotype.rs`, behaviour:
  `program.rs` (blocks, parameter tables, templates, mutation), `scene.rs` (perception),
  `actions.rs` (one function per action), `strategy.rs` (`Mind`, `Intent`, `Stance`, the
  interpreter `plan`); `plant.rs`, `flora.rs`.
- `senses.rs` — what a creature can learn (traits, grid-backed views, queries + brute-force tests);
  `grid.rs` — counting-sort spatial grid; `rng.rs`, `space.rs`.
- `combat.rs` (simultaneous strikes), `corpse.rs`, and the dormant flock layer (`flock.rs`,
  `battle.rs`, `social.rs`, `territory.rs`, `kin_grace.rs`).

`life-sim`: `simulate()`/`run()` under limits; `observe.rs` — snapshots, the event chronicle (the
game reuses it), ASCII maps. `life-report`: `main.rs`, `story.rs`, `json.rs` (format
`life-report/11`), `metrics.rs` (`--compare`), `bin/life-sweep.rs`.

### Determinism and the golden test

No global RNG: each creature owns an `Rng` forked from its parent's; the world has its own stream.
Results depend on the seed only, not on iteration order or thread count (phase 3 relies on it). Any
change in how many numbers are drawn, or in what order, shifts every seed — then re-validate the
balance instead of diffing numbers. The plant spawner draws even when capped.

`tests/golden.rs` pins an FNV digest of the whole world at checkpoints for eight configs. A refactor
must keep it; a deliberate behaviour change re-records it (the test prints the table) in its own
commit together with `--save-reference`. Asserted on Windows only. `--ignored` prints 50 seeds × 2
worlds for a wider before/after diff; comparing golden output of two builds (a `git worktree`) is
how "bit for bit identical" is proven.

`reference/*.json` are balance fingerprints (8 seeds × 20 000). `--compare` checks each metric's mean
against the reference's per-seed range (exit 1 on a mismatch) and refuses (exit 2) when the world
conditions differ (space, rules, start counts and mixes). A deliberate balance change fails it by
design; re-take with `--save-reference reference/fingerprint.json` (and the calm one with
`--rule cost_scale=3`).

### Tick order

Plants → old age → (flock circles, battles), neighbour snapshot and territories → decisions,
ageing and movement → eating plants → simultaneous strikes and winners feeding → reproduction →
removing the dead and adding children → flock membership → tick number. Strikes are collected
before damage, so mutual death is possible; a death has one cause (`Starved`, `OldAge`, `Combat`).
A creature killed in combat gets no prey and does not reproduce.

### Neighbour search and performance

- Creatures never see the grid: `Creature::step` takes *senses* (`GridSenses` per creature; tests
  pass `senses_from(..)` or `Blind`). Queries are `#[inline(always)]` (without it a ×100 world ran
  8% slower). The cell is fixed (`GRID_CELL`); `for_each_near` returns a superset, callers check
  distance. The grid copies coordinates, valid only because queried entities don't move within the
  phase. Keep the brute-force checks in `senses.rs` tests.
- `Creature::step` is the hot path: genome-derived values are precomputed in `Phenotype::of`,
  distances compared squared, block dispatch is a `match` over `Action`, never `Box<dyn>`, and the
  scene asks the senses only what a block needs (a memo per tick). The eating phase
  copies only the corpses claimed that tick (`world.rs`, `claimed`), and `social::prepare_in` skips
  the neighbour scan of a creature whose label counts one member. For refactors,
  compare ms/tick against the previous build in a worktree, alternating runs
  (`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1`).
  `тик_растёт_линейно_с_численностью` guards against queries degrading to a full scan.
- **Termination**: every headless run is capped by ticks, a population ceiling, a work budget
  (creatures × plants) and a wall-clock deadline (`life_sim::Limits`). Tests have no `while` loops;
  every run is bounded by a tick count.

### Space and food

- The layer genes (`min_y`, `max_y`) are a preference, not a wall: a creature goes for any visible
  food and walks back to its home band otherwise; physics clamps only to the world. Never add moves
  that teleport.
- Real units are for showing only (`units.rs`, `docs/scale.md`): 1 px = 0.5 cm (the base fish is
  20 cm), a tick is 0.25 s of swimming but ~9 hours of life (3000 ticks = 3 years).
- Scale is area, shape (1:1, 3:2 default, 2:1, strip) is proportions (`Space::new`); at ×1 3:2 is
  exactly the base 6000×4000. Per-world quantities scale with `area_ratio` via `per_area`; the
  vertical ecology is in % of depth. Tall worlds are harsher (newborns start far from the rich top).
- Food (`flora.rs`): x and y drawn independently by width and depth profiles (uniform, linear, exp,
  log, waves, «игровое» — flat to 20% of depth, then an exp fall; «океаническое» — 60% at the
  surface rising to a peak at 15%, then an exp fall; default «океаническое» down, uniform across).
  Capacity is `PLANT_MAX` slots, one plant each, so growth is logistic. Default 24 patches
  per base world holding `plant_patch_share` 60% of the slots, fewer, smaller and poorer with depth;
  patch centres come from their own keyed stream, never the world's. Occupied slots are an
  incremental bitset (`world::Occupancy`), rebuilt when the plant count stops matching — keep
  direct edits of `plants` changing the count.
- Upkeep is `COEF * stat ** POWER` over size, speed and sight (speed also × (size/40) **
  `speed_mass_power`), with per-term price rules and `cost_scale`. The *exponents* make the
  trade-off: benefit grows ~ stat², so cost must be steeper (`size ** 2.5`, `vision ** 2`), or stats
  run away. Read the block in `config.rs` before changing them.

## Genes and strategies

`GeneSpec { key, label, about, kind, base, mutation }`, `kind` = `Absolute` | `Percent` |
`Choice(&[Variant])`. Everything walks the table, never positions. **Tables are append-only** — the
order fixes the RNG draw order and positions in references and JSON; the only in-place
replacements were `carnivory` → `diet` (row 11) and `life_pace` → `maturation` (row 9), same law so
same draws, and the one deletion the nine behaviour genes that moved into the programs (`/13`, the
user's call). A choice gene with one variant is inert (draws nothing, hidden in the UI).

Mutation (`genome::Heredity`, built from the rules): `CLONE_CHANCE` 50% of children are exact
copies; otherwise `Scale` for numbers (× (1 + gauss(0, σ·mutability)), multiplier ≥ 0.1), `Shift`
for a percent gene whose zero means something (+ gauss points, clamped 0–100: `cold_blood`), `Switch`
for choice genes (0.1% × mutability), and `Neighbours { chance, rise, jump, of, up, leaps }` for the
diet, independent of mutability: a step towards meat 2% (herbivore → omnivore → scavenger or
carnivore), another neighbour step 0.5%, the herbivore's own leaps to the carnivore 0.1% and the
scavenger 0.01% (rules `diet_leap_*`, replacing its general jump), others a general jump 0.01%.
Mutability is clamped to `MIN_MUTABILITY` 0.1 (without a floor selection froze evolution) and costs
nothing. Start mixes (strategies, diets) are dealt without draws (`variant_for`, `spread_ranks`).

A program **decides** (`strategy::decide(&Me, &Program, &mut Mind, &mut Rng, &senses) -> Intent`),
the creature **acts** (`act`); a block cannot move, feed or divide the creature. The strategies
(`Strategy`, `VARIANTS`) are only the founders' templates: `standard`, and `lurker`, which wanders
at a third of its speed.

Adding a gene: a variant at the end of `enum Gene` and a row at the end of `GENES`; its effect only
in `Phenotype::of`, any cost appended at the end of the upkeep sum; a test of the effect; check the
genome panel and the card at 960×600 with `TINYLIFE_SHOTS`; re-record golden and references in a
separate commit. A behaviour is no gene: add a block instead. Adding a test or an action: a variant
at the end of `Cond`/`Action` and their `ALL`, its parameters as a `ParamSpec` table (a new sense =
trait method + query + brute-force check), its `Scene::test` or `actions::act` arm, labels for the
window, tests of it in `strategy.rs`, re-record golden. Adding a template: a variant at the end of
`Strategy`/`VARIANTS` (≤ 8) and its `Program::template`.

## The game (`crates/life-app`)

**The window never waits for the simulation.** Defaults are the user's world (×20, 2:1,
`cost_scale=3`, half lurkers, steepness 5).

- `sim.rs` — the simulation thread owns `World`; the UI sends `Command`s applied between ticks.
  Frames go through a one-slot mailbox and are never dropped, so history, log and snapshots travel
  as deltas. Tempo: ticks per second with a capped debt; `SLICE` bounds a burst.
- `frame.rs` — instances (32 bytes, relative to an f64 `origin`) culled to the visible rect, or a
  density raster when too many; `motion.rs` — previous positions, births and ghosts so nothing jumps
  (plants matched by `born` tick + x bits; `Plant` stays 24 bytes). `meta` packs 32 bits the shader
  reads (see its comment).
- `render.rs` + `creatures.wgsl` — one instanced draw call; interpolation between frames, newborns
  growing, detail (diet rim, fullness, eye, proboscis) only above ~4 px. `view.rs` paints corpses,
  patches, selection, minimap.
- `game.rs` (game screen, «Графики» panel, the lab by topic tabs, creature card with the diet's
  edges), `behaviour.rs` (the selected creature's programs as flowcharts, a tab per track, the
  settings as a section on top, this tick's path lit with «не вышло» where an action failed; the
  card's «Поведение (B)», key B), `diets.rs` («Кто живёт», «Кто кого»,
  highlight), `stats.rs` («Статистика», area
  selection), `screens.rs` (menu, «Новый мир», prefs, help), `charts.rs` (painter, no plot crate),
  `settings.rs` (`FIELDS` — the single spec of every field: label, hint, hard limits and why, tab,
  rule; file in `%APPDATA%\TinyLife`, atomic, clamped, unknown keys ignored).
- `ui_tests.rs` — kittest at 960×600 and 1600×900: widgets inside the window, not overlapping. They
  share one GPU lock (parallel wgpu renderers crash the Windows driver); CI renders through WARP.
- Release builds on Windows use `windows_subsystem = "windows"` and attach to the parent console.
- `bin/life-progress.rs` — the sweep's progress window (see `life-sweep` above); `View::of` and
  `draw` are tested, `draw` also as PNGs.
