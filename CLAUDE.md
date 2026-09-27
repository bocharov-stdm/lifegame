# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tiny Life Simulation — an evolutionary sandbox: plants and creatures (`Creature`) in a 2D
world. Creatures carry a genome (a gene table, see "Genes and strategies") that mutates on
division; selection is emergent, not scripted. Predators were a second species until the git
tag **`predators-final`**: mutations and cannibalism replaced them, and their code was removed
(engine, report, game, tests). Look there for how they worked. While predators existed the
creatures were «травоядные» (`Vegetarian`); old names are still read where files or habits
keep them: flags `--vegetarians` / `--veg-mix`, settings key `n_vegetarians`, reference keys
`vegetarians` / `vegetarian_strategies`.

**Language.** Code, comments, doc comments, docs (`CLAUDE.md`, `AGENTS.md`, `BEHAVIOR.md`,
`README.md`), CLI and report output, test names and commit messages are written in **English**
(switched from Russian on 2026-09-25 to save tokens). Existing Russian comments and test names
are translated gradually: translate what you touch, don't mass-rewrite files. Only the **game
UI stays Russian**: window and button texts, settings labels and hints, gene `label`/`about`
shown in the game, chronicle event texts (`life_sim::observe` — the game's event feed; the
report prints the same texts).

**The project is Rust-only now.** It was migrated from Python + pygame (plan: phases 0‒8, from a
1:1 core to a native wgpu/egui app with player-chosen world scale up to ~1M creatures). Done:
phases 0‒2 — engine (`crates/life-core`), bounded headless runner with an observer
(`crates/life-sim`), balance report (`crates/life-report`) — and the game itself
(`crates/life-app`, phases 4‒5 done ahead of phase 3). The Python version was removed; it lives
at the git tag **`python-final`** (`python/` there) and is the behavioural spec the game was
ported from. Open: phase 3 (parallel tick) and phase 6 (a separate machine benchmark).

The behaviour reform is implemented: kinship and fleeing, food-driven growth, life pace and
ageing, health and simultaneous fights, diets and hunting, inherited flocks. Combat is always on
(the peaceful world and the `cannibalism` rule are gone; `with` names the reason). The engine is
still sequential; the neighbour snapshot and the movement/feeding split do not mean it is
parallelised. A family flock is a circle that moves as one object (settled, nomadic, scouts or
vertical migrants — the inherited `flock_kind`), and its members feed inside it; a young family
(fewer than 4) roams with its members. The circle is also the territory: circles without
territoriality overlap freely, moderate ones push softly and are respected only in sight of a
member, hard ones never overlap anything. Nobody aims at what lies behind a border it respects
(food, a return or wander point), and a circle pressed against the world's edge is walked around
on its open side. A territorial flock squeezed with no room nearby fights every
flock touching its circle that has adults; the beaten move away, young families included, and a
battle ends as soon as no two of its flocks may strike each other (`battle.rs`). Flocks
share local knowledge of food and alarm, rest and to a limited extent cover their own. Every
other founder is flocking; territoriality (undefended, moderate or strict) is drawn for every
founder, loners included. Adult defenders may shoot at intruders; 5% of founders already can
shoot. After a family splits, 600 ticks of mutual protection apply; parents pass energy to a
child at birth and cover it while they still know it. Family is only a parent and its growing
child while the parent still knows it (the inherited `care` sets until what growth). How hungry a
member leaves its circle to forage (`forage`) and how near a stranger that hunts nobody may come
before it flees (`bravery`) are inherited too. A hunter
weighs the meat its tank can take in against the strikes it expects from the prey and the
prey's visible allies (the inherited `caution` sets how much); a creature strikes only a chosen
target, in defence or on a territorial assignment. Plants and corpses are eaten in portions;
shots are weaker than a contact strike and cost energy. There is no cooperative hunting, no
leaders, no sharing of prey, no merging of flocks. Sociability is an inherited gene; groups
that stay separated get a new label. The exact mechanics and their checks: `BEHAVIOR.md`.

**Flocks are off** (`config::FLOCKS = false`, the user's decision: they spoiled more than they
gave). Every founder is a loner, the pack gene never switches, the game hides «Стаи» and the flock
genes' charts. The code is still there; the plan is to remove it from the game and the tests and
keep it under a git tag **`flocks-final`**, the way predators were removed. Never run balance
measurements with flocks.

**Energy is never made from nothing** (the user's hard rule, 2026-09-26). Energy enters the world
only in plants; everything else only passes it along the chain (plant → eater → corpse → eater)
and loses some on the way. No mechanic, bonus, rule or lab limit may duplicate it or create it:
digestion is at most 100% of a food (`Rules::with` rejects more), deep saving at most the whole
upkeep, and a body a creature got for free (a founder's, a newborn's birth half) is not meat.
Founders' bodies and tanks are the one initial condition. Check every new mechanic against this.

The food web (developed on the branch `food-web`, now on `main`): the `diet` choice gene — herbivore, omnivore, scavenger,
carnivore (variant order H/O/S/C, the order of every `DIET_*` table in `config.rs`) — sets what a
creature digests (`DIET_DIGESTION`: plants, fresh meat, rot; 0 means it neither eats nor goes for
that food) and its edges. The edges are world rules (`Rules::diets`, one `DietEdges` per diet, keys
`{diet}_{edge}` such as `carnivore_strike`; defaults are the `DIET_*` tables, edited in the lab's
«Питание» tab), all read in `Phenotype::of`:

| | strike (`DIET_STRIKE`) | other edge |
|---|---|---|
| herbivore | ×1 | health ×1.5 (`DIET_HEALTH`), size term of upkeep ×0.85 (`DIET_SIZE_COST`) |
| omnivore | ×1.15 | eats everything, so no food is foreign to it |
| scavenger | ×1.3 | smells corpses at 3× vision (`DIET_SMELL`, free); upkeep falls linearly from half the depth to −40% on the bottom (`DIET_DEEP_SAVING`, `Phenotype::depth_upkeep`, applied in `Creature::act`); founders start with layer 50–100% (`SCAVENGER_START_LAYER`) |
| carnivore | ×3 | speed term of upkeep ×0.5 (`DIET_SPEED_COST`); smells corpses at 1.5× vision; a juvenile gut: plants at 70% until grown (`DIET_YOUNG_PLANTS`) |

Scavengers digest plants at 15%, carnivores at 20% (`DIET_DIGESTION`), so a lone meat-eater
does not starve outright. Until a creature grows to its own size (the size gene) it digests plants
at its diet's `young_plants` edge (`DIET_YOUNG_PLANTS`, «Растения в детстве»; the grown value for
every diet but the carnivore): a carnivore mutant is born half grown with its herbivore parent's
prey ratio, sees no prey that much smaller, and starved on plants at 20%; now it grows on plants
like an omnivore and hunts once grown. Staying young is no loophole: only the grown divide. There are no meat founders by default (`DIET_START_MIX` 70/30/0/0): at the
start there are neither corpses nor prey small enough, and every one starved without a strike. The
meat diets arise from mutants (`DIET_MEAT_STEP_CHANCE`). Meat founders set in a mix still start
`meat_founder_size` (`MEAT_FOUNDER_SIZE` ×2, `WorldConfig`, `--meat-founders`, «Мясоеды на старте
крупнее») times bigger.

The strike bonus is on damage only, never on the strike's energy cost (`strike` vs `strike_cost`).
Damage scales with size: melee is 5% of the striker's size, times (its size / the target's) **
`melee_size_power` (rule, `MELEE_SIZE_POWER` 1.25) when it is the bigger — equals trade ~20
strikes, 2× needs ~5, 3× a carnivore at the former strike ×1.5 kills a herbivore in two, 7× in
one (`Phenotype::strike_on`,
`melee_damage`; hunters weigh prey and retaliation by the same function). At 1.75 (3× = one blow)
carnivores boomed, ate the herbivores out and starved; at 1.0 they died out everywhere.
Melee has no cap any more; a shot stays 1% of size, capped at ¼ of the target's max health, and
expensive. Reach grows with size by itself: contact is the sum of the two radii. **Fights at food**:
below its inherited `rivalry` share of the store a creature strikes a stranger (not kin, not its
flock, not under grace) that is `prey_ratio` times smaller and eats the *same* food beside it this
tick — plants side by side or the same corpse (`combat::Feeding`, filled in `world.rs` from the
feeding phase); so a herbivore never fights a carnivore over grass, and a big scavenger clears small
bone-eaters off its corpse. A struck creature strikes back only if the enemy is less than its own
`prey_ratio` times bigger; otherwise it runs (`standard.rs`). Bystanders do not flee a brawler.
Upkeep with the diet's factors is `Rules::upkeep_diet`; `Rules::upkeep` is it with `[1, 1]`, bit for
bit. **Own niche** (`DIET_OWN`): above its inherited `picky` share of the store a creature eats and
goes only for its own food — a sated scavenger neither touches a corpse in its fresher half
(rot < 0.5) nor hunts (`hunts_now`), a sated carnivore does not touch the rotten half or skeletons;
below `picky` it takes whatever it digests (`Phenotype::corpse_efficiency(rot, hungry)`, hunger judged
once per tick before the meal in `world.rs`). Anyone who eats fresh meat at all (`hunts`) is feared,
and a child of another diet leaves its flock. A corpse's meat is the body grown since birth, at what
growing it cost (`GROWTH_ENERGY_PER_SIZE` a unit of size), plus the tank (`corpse::meat`) — hunters
weigh prey by the same measure. The body a creature is born or spawned with is no meat: counted at
`size × ENERGY_PER_SIZE` it made energy from nothing (parents bore empty children and ate their
corpses — 9500 scavengers on 34 plants). A corpse's clock is a rule set (`corpse_fresh`,
`corpse_rotten`, `corpse_sink`, `corpse_decay`, `corpse_rest` → `CorpseClock::of(rules)`, stored in
the corpse; defaults `CORPSE_*`). It is fresh for 150 ticks, fully rotten at 600, and after the fresh
time sinks at `CORPSE_SINK_SPEED` (2 a tick, a fifth of a base creature's speed) straight down,
edible all the way; it rests at its own place in the lowest 25% of the depth (`CORPSE_REST_PCT`, a
hash of the id, never rising) and is gone at 1800. The sinking is a speed, not a time to the bottom:
with a time (1200 ticks) a corpse in the user's 15 500-deep world fell 10–15 a tick, as fast as a
scavenger swims, and scavengers ate 3% of the time with corpses all around. In a tall world a corpse
may decay before it reaches the bottom. A corpse eaten down to `CORPSE_SKELETON_SHARE` (10%) of its
meat becomes a **skeleton** (`Corpse::skeleton`). From then it is rot: it sinks at
`SKELETON_SINK_SPEED` (4) to a place in the same zone (never up) and decays over 1800 ticks from the
stripping. A corpse left alone is never stripped. Creatures stop at `EAT_STOP_SHARE` of their reach instead of
standing on the food, and the game draws a proboscis to it. Plants grow in patches over a new
default depth profile «игровое» (see "Where food grows").

## Where the food-web work stands

The user asked for the work on `main` (2026-09-27): `main` was fast-forwarded to the `food-web`
branch (up to `ed14091`, three niches and the sweep tools); not pushed yet. Commit to `main` from
now on unless told otherwise. What is left:
- `AGENTS.md`, `BEHAVIOR.md`, `README.md` still describe the pre-`food-web` model (this file is
  current): diets and their edges, own niche and `picky`, rot, skeletons, patches, «игровое».
- The golden digests fail by design until re-recorded; re-record them and both references
  (`--save-reference reference/fingerprint.json`, and `reference/calm-fingerprint.json` with
  `--rule cost_scale=3`) in
  one separate commit once the user accepts the balance; golden case H can become "all four diets".
- Then push `main` (on the user's word). Until the digests and references are re-taken, CI on
  `main` fails on the golden test and `--compare`.
- Open (2026-09-26): the meat niches.
  - With meat = grown body + tank, the niches collapsed (seeds 1–8: no carnivores or scavengers
    anywhere, herbivores 79–100%, medians 925 / 755).
  - The user chose: meat founders ×2, plants for meat-eaters, diet step 0.5% independent of
    mutability, 50% clones, a mutability floor of 0.1.
  - On the baseline conditions (below), all 8 worlds still ended ~100% herbivores. Diagnosed with
    a probe (2026-09-27): meat founders never met prey (random deep layers, nothing small enough
    at the start) and starved; corpses in the tall world sank as fast as scavengers swim; and a
    carnivore could catch only newborns and juveniles, 7–15 energy each, so hunting paid 0.10–0.18
    a tick against 0.16–0.26 of upkeep. Adults converted mid-game to scavengers held with slow
    sinking (44 → 49–52, → 197 with half the upkeep); to carnivores only with strike ×3 and half the
    upkeep (44 → ~30).
  - The user then chose (2026-09-27): corpses sink at a constant speed; carnivore strike ×3 and its
    movement at half the price; no meat founders, meat diets from mutants with a 2% step towards
    meat (`diet_meat_step`, «Шаг к мясу»). Every diet edge is a rule now (lab tab «Питание»).
  - Herbivores leap straight to the carnivore (0.1%) or the scavenger (0.01%): the meat niches
    arose, but carnivores held in 0 of 8 worlds (born 28–249 a world, almost all starved).
  - An 18-variant sweep of carnivore body traits (`life-sweep`, 2026-09-27; all conservation-safe):
    a free nose for corpses is the main lever (×1.5: carnivores hold in 7 of 8, population −1%, but
    they live as hyenas, 162 kills a world); a juvenile gut plus the nose ×1.5 gives real hunters
    (8 of 8, 7.6%, 3672 kills, −19% population) but eats the corpses before they rot, and the
    scavengers held in 1; a nose paid like sight at its radius killed the niche (0 of 8); a bigger
    stomach ×1.5 held in 6 of 8 with little hunting; bolder prey choice hurt. The user chose
    **three niches**: the juvenile gut, the carnivore's nose ×1.5, the scavenger's ×3 — carnivores
    hold in 6 of 8, scavengers in 3, population −13% (`DIET_SMELL`, `DIET_YOUNG_PLANTS`).
    Reproduced bit for bit after adoption. Open: in the ×1 worlds (acceptance, 8 seeds × 20 000)
    all 16 survive, but the carnivores are too strong there — base 973 → 470 creatures late
    (carnivores 8% → 20%), calm 750 → 205 (5% → 14%), dips down to 8–15 creatures; with the old
    values carnivores already held in ×1 (8 and 5 of 8).
  - Tried and reverted, don't repeat: the parent paying for the child's body (halved populations,
    no meat diets); birth at ¼ size.
- Flocks: remove the code and tests under the tag `flocks-final`. Prove the removal by the golden
  digests recorded with `FLOCKS = false` before removal, as with predators.
- Planned next: the life/pace reform (see "Balance: exponents, not coefficients").

**Baseline conditions** (the user's own game; measure balance on these, not on ×1 defaults):

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 \
  --scale 20 --shape 2:1 --rule plant_rate=0.5 --rule cost_scale=3 --rule plant_depth_steepness=5 --mix 1 1
```
The user keeps a short PDF of genes, strategies and diet edges (built by a throwaway fpdf2 script
with Arial for Cyrillic); regenerate and send it after diet or gene changes.

## Commands

```bash
cargo test --workspace                              # all tests (~30 s; ~24 s of it — life-app screens)
cargo test --workspace --exclude life-app           # engine, runner, report only (a few seconds)
cargo test -p life-core --test engine сетка         # tests whose name contains «сетка»
cargo test -p life-core --test golden               # world behaves bit for bit as recorded
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110; CI: cargo fmt --all --check

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
cargo run -p life-report --release -- --rule plant_energy=80 --scale 10 --threads 4
cargo run -p life-report --release -- --mix 1 1                 # start with strategies 50/50
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

CI (`.github/workflows/ci.yml`, Windows only) runs fmt, clippy, tests and `--compare` against
both references (base and calm). The game, the golden digests and the references live on
Windows: another platform's libm differs in the last bits, the same seeds grow into another
realization of the world, and its means may leave the Windows per-seed range by chance.
Dev builds use `opt-level = 2`: tests run real multi-thousand-tick simulations.

Validating a model change: seeds 1–8 × 20 000 ticks for the base and the calm (`cost_scale=3`)
profile. Runs must finish on their own, not stop on the work budget, so long runs need
`--max-work 1e15` (`--compare` and `--save-reference` set it themselves). Acceptance: at least 7
of 8 worlds survive in each profile; then the same on the baseline conditions (below), which is
the balance the user judges. Run such series as a `life-sweep` plan (see below). A single-seed run prints the genome start → end, including
the diet shares (`питание`) — that is how to see which diets survived. Detailed social validation
numbers: `reference/social-validation.json`, analysed in `BEHAVIOR.md`.

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 --rule cost_scale=3
cargo run -p life-report --release -- --seeds 3 --ticks 20000 --max-work 1e15 | grep "^  питание"
cargo run -p life-report --release -- --diet-mix 50 0 0 50        # founders' diets: H O S C shares
```

**Many variants at once — `life-sweep`** (`crates/life-report/src/bin/life-sweep.rs`, doc comment at
its top). A plan file lists `seeds:`, shared `args:` and `variant NAME: ARGS` lines (a `NAME=VALUE`
token in capitals is an environment variable, for experiment builds); every variant × seed runs as
its own `life-report` process, `--jobs` at a time, and each variant gets one summary row: worlds
that ended on their own, survived, in how many carnivores / scavengers *hold* (≥ 10 creatures and
1% of the world over the last `--late` share of the run), herbivore + carnivore coexistence, late
diet shares, late and minimum population, carnivore births and kills, plus any `METRIC <name>
<number>` lines a build prints. Guards: the report's own deadline (`--seconds`, 300 by default — a
slowed run is cut and left out of the medians, never counted as finished), a watchdog that kills a
process `--grace` seconds past it, the worst case printed before the start and the time left after
every run. Results in `--out`: `runs.csv`, `summary.csv`, `summary.md` and each run's JSON and text;
a run whose JSON and command line are there already is reused, so a stopped sweep resumes.
Comments describe: the plan's top block, a block above variants, a comment ending a variant's line.
Progress goes to `OUT/progress.json` every second, and a small always-on-top window
(`life-progress`, a `life-app` bin next to `life-sweep`, or `--viewer`; `--no-window` for none)
shows the bar (finished runs plus the running ones' ticks: each run rewrites `s<seed>.tick` about once a second
through the report's `--progress FILE`), the time left (from those ticks even before a run ends),
what runs now with its description, tick and time against the run limit, the last runs, and the same bar
on its taskbar button. Its buttons pause, resume and stop the sweep through `OUT/control.txt` (`run`,
`pause`, `stop`, read four times a second): a pause takes the running runs off at once and puts them
back at the front of the queue (a world depends on its seed only, so the rerun gives the same
result), a stop ends the sweep with a summary of what is done. Every press is printed (`PAUSED` /
`RESUMED` / `STOPPED by the user in the window`) and appended to `OUT/events.log`. The user asked
to see every series of Claude's runs there and to have Claude notice the presses: run measurements
as a sweep in the background, with a Russian description above each variant, and a background
watcher that ends on those lines (`until grep -qE "PAUSED by|STOPPED by" LOG; do sleep 2; done`).

```bash
cargo build -p life-report --release
target/release/life-sweep plan.txt --out sweeps/hunt --jobs 16 --seconds 300
target/release/life-sweep plan.txt --out sweeps/hunt --summary-only   # re-read the summary
cargo build -p life-app --release --bin life-progress                  # the progress window
```

`CLAUDE.md`, `AGENTS.md` (short rules for other agents), `BEHAVIOR.md` and `README.md` describe
one model: when a mechanic changes, update all four.

### Watching a run without a window

To understand *what happens and why* in a world (balance work, debugging, answering "why did
they die out"), use the observer rather than raw counts:

```bash
cargo run -p life-report --release -- --ticks 20000 --maps 3        # story + 3 text maps
cargo run -p life-report --release -- --seeds 1 2 3 --story --rows 8
cargo run -p life-report --release -- --ticks 5000 --json -          # everything as JSON on stdout
cargo run -p life-report --release -- --ticks 5000 --json run.json   # JSON to a file + text
```

The story (always on for a single seed) prints: final state; creature births/deaths **by
cause** (starved / old age / combat) — the counters in `World::counters`, social ones in
`World::social_counts`; per diet (`Counters::by_diet`: born, deaths by cause, who struck and
killed whom, as matrices — `story::print_diets`); a table by intervals with flows, gene medians, the depth layer holding
80% of creatures and their fullness; genome start → end as median (10‒90%); creatures vs plants
by depth band (and by width band when the width food profile isn't uniform); a chronicle of
events (crashes and rises with their causes, extinction, plants hitting the cap, gene shifts,
creatures squeezing into a thin layer); ASCII maps (top = surface, `O`/`o` creatures, `:`/`.`
plants). The JSON has the same plus every snapshot (`life_sim::observe::Snapshot`: per-gene
`GeneStat` — a spread for numeric genes, variant shares for choice genes —, depth and width
histograms, cumulative counters). Format `life-report/10` (social counters, flocking-gene
carriers, territories, corpses, feeding and rot bites, corpse fates — appeared, removed, lain on the bottom,
skeletons, lifetimes —, shots, configurable action costs, flows by diet `counters.by_diet`: born,
deaths by cause, strikes and kills keyed by diet): top-level `genes`
describes the gene table (key, label, kind, variants); keys are English (event `kind`), texts
Russian. Long runs may stop on the work budget ("перегрузка") — raise it with `--max-work`.

`observe.rs` lives in `life-sim`, not in the report, so the game reuses snapshots
and the event chronicle for its in-game event feed.

### The reference fingerprint

`reference/fingerprint.json` is the balance fingerprint (8 seeds x 20 000 ticks, series every
60 ticks). It started as the last Python version's (`python/fingerprint.py` at `python-final`)
and is re-taken from Rust after each deliberate balance change. It is a world of creatures
and plants; metrics: creatures and plants mean, size max and final. The model is
`life-behavior/10` (diets, rotting corpses, plant patches); references without this version are
rejected with an explanation. Both references are still the /9 ones and are re-taken once the
balance is settled.
`--compare` reruns the same seeds in Rust and checks each metric's mean against the reference's
per-seed range; any mismatch exits with code 1 (CI relies on it). It refuses (code 2) when the
world differs from the one the reference was taken on (world size — compared as `Space`, not
shape name, since at ×1 strip and 3:2 are the same 6000x4000 —, rules, start count, start
strategy and diet mixes) or was taken with predators or in the peaceful world (`cannibalism: 0`)
— a mismatch there would measure the conditions, not the balance. `predator_*` rules and
`cannibalism: 1` of old references are skipped. Gene tables are code, not
conditions: if the reference's `genes` list differs from ours (ignoring inert one-variant
choice genes) it prints a note and still compares. The size metric is read from the reference
by gene *key*, not position.

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
  the food profiles — see "Where food grows" —, combat and its costs; the prey size ratio is
  the `prey_ratio` gene, not a rule). `World`
  owns one and every creature gets it at birth. Changing an exponent
  renormalises its coefficient so the *base* genome still pays the same — only the steepness
  changes. `Rules::default()` is `config.rs` bit for bit (the factor is exactly
  `base ** 0.0`); tests guard that. `with()` rejects unknown keys, non-finite values (a NaN
  sigma would hang mutation's rejection loop) and values where a rule stops making sense
  (negative costs, a shot period below 1, a fractional patch count) — but not merely
  "unbalanced" ones: breaking the balance is what the lab is for.
- `world.rs` — `WorldConfig`, `World` (populations, `step()` — the phase order only,
  `stats()`, `counters`, `spawn_*` for tests and the app).
- `genome/` — the gene table (`creature::GENES`), `CreatureGenome` (`Copy`, `[f64; N]`,
  indexed by `enum Gene`), the table-driven mutation.
- `creature/` — the entity: `mod.rs` (the creature, its `act` and the world hooks:
  `feed`/`devour`, `maybe_divide`, `apply_rules`), `phenotype.rs`, `strategy.rs` + one file per
  strategy (`standard.rs` — the original behaviour; `lurker.rs` — see "Genes and strategies").
  `plant.rs` — plants; `flora.rs` — where they grow (`Flora`, profiles).
- `senses.rs` — what a creature can learn about the world (traits + grid-backed views + the
  query functions and their brute-force test).
- `grid.rs` — `Grid`: counting-sort spatial grid with a fixed cell, rebuilt each tick.
- `rng.rs` — per-creature SplitMix64 streams; `space.rs` — world size, scale and shape.
- The social layer (behaviour reform; details in `BEHAVIOR.md`): `combat.rs` — simultaneous
  melee strikes and weak shots; `corpse.rs` — corpses as a finite meat supply that rots and
  sinks to the bottom; `flock.rs` —
  flock circles: membership, radius, movement by kind, pushing apart and shrinking, moves to a
  free place, stragglers; `battle.rs` — battles of flocks for room; `social.rs` — social memory
  and local decisions from a snapshot taken before movement; `territory.rs` — circles as
  territories: walking around (leaky for moderate ones), warnings, guards and battle targets;
  `kin_grace.rs` — 600 ticks of mutual protection between groups after a family splits.

Integration tests (`crates/life-core/tests/`): `engine`, `golden`, `lifecycle`, `social`,
`territory`.

`crates/life-sim/src/lib.rs` — `simulate()` / `run()` under limits; `observe.rs` — snapshots,
events, ASCII map. `crates/life-report/src/` — `main.rs` (CLI), `story.rs`, `json.rs`,
`metrics.rs` (`--compare`), `bin/life-sweep.rs` (plans of many variants).

`Relict/` — **frozen 2025 archive** of early Python prototypes. See `Relict/ПАМЯТНИК.txt`:
nothing there is edited, refactored, "fixed", modernised or translated. Its bugs are part of
the monument.

### Determinism and RNG

There is no global generator. Each creature owns an `Rng`; a child's stream is forked from its
parent's at birth, the world has its own stream (keyed from the seed) for plants and spawns.
Results depend on the seed only — not on iteration order or thread count — which is what the
parallel tick of phase 3 relies on. Any change to how many random numbers are drawn, or in what
order, shifts every seed: fine for a deliberate behaviour change, but then re-validate the
balance instead of diffing numbers. The plant spawner draws its random number even when capped.

`tests/golden.rs` pins behaviour bit for bit: an FNV digest of the world (positions, energy,
ids, all genes, counters, an RNG probe of every creature and of the world) at checkpoints
for eight configs (defaults, giants, lab rules, ×10 strip, live rules + spawning, a 50/50
strategy mix — it also asserts both strategies coexist —, a ×10 square with tabulated food
profiles, seed 8). Removing predators was proven by recording a predator-free digest of
all eight while predators were still in the code and getting the same bits after removal (plus
`--ignored` over 50 seeds); removing the peaceful world the same way (combat-forced digests of
the old code = the new code's, 150 worlds). A case without recorded digests fails too. Any refactor must keep
it; a deliberate behaviour change re-records it (the test prints the table) in its own commit,
together with `--save-reference`. The constants are asserted on Windows only: `ln`/`cos`/`powf`
come from the platform libm, so Linux may differ in the last bit (there the test prints its
digests). `--ignored` prints digests of 50 seeds × 2 worlds for a wider before/after diff.

### Tick order and life cycle

Plants → flock circles (move, push apart, shrink), battles, neighbour snapshot and territories
→ decisions, ageing and movement of all creatures → eating plants → simultaneous strikes and
winners feeding → reproduction of survivors → removing the dead and adding children →
splits, stragglers, departures and flock membership (circles only shrink here) → tick number.

`Creature` stores age, health, diameter at birth, parent, flock label and an individual
reproduction timer. `Phenotype::at_size` accounts for the actual body; `Gene::Size` is the
adult limit. Changing rules keeps age and size.

`combat.rs` collects strikes before applying damage: mutual death is possible. One strike per
participant per tick; the prey goes to one surviving winner. A death has exactly one cause
(`Starved`, `OldAge`, `Combat`). The old `cannibalized` counter is kept for the counters
struct's compatibility; in the new model it is zero and does not duplicate `combat`.

Children are added after fights and reproduction. A creature that died on its own turn does not
eat; one killed in combat gets no prey and does not reproduce. Eaten plants are marked and
removed after the phase (a plant is alive while it has portions). The genome is only extended
at the end of the table.

### Neighbour search

- **Creatures do not see the grid**: `Creature::step` takes *senses* (`senses.rs`:
  `Senses` — nearest plant, threats, prey, corpses). `World` builds a prey grid (all
  creatures), a food grid and a corpse grid and answers through `GridSenses`, built per
  creature; tests pass `senses_from(|x, y, r2| ..)` or `Blind`. The queries and the grid senses
  are `#[inline(always)]`: without it the compiler stopped inlining the plant search into the
  creature's step and a ×100 world ran 8% slower than with closures.
- The cell is fixed (`GRID_CELL`); a query scans as many cells as its own radius covers, so one
  far-sighted creature does not inflate everyone's cell. `for_each_near` returns a *superset*;
  callers check distance.
- The grid stores copies of coordinates. That is valid only because the queried entities do not
  move within the phase (plants never move; creatures are read from the snapshot taken at the
  start of the phase). `alive` is always read from the entity, never from the grid.
- After movement the creature grid is rebuilt for combat; contact is the sum of the two
  half-diameters, and the query radius accounts for the largest body. Eating plants also looks
  up neighbours after movement.
- Kin are seen through the immutable `Herd` taken at the start of the phase; the snapshot holds
  all creatures, since small ones are needed as prey. Kinship and flock labels exclude one's own
  from threats and targets.
- Keep the spatial queries' brute-force checks in the `senses.rs` tests.

### The soft layer

The layer genes (`min_y`, `max_y`) are a *preference*, not a wall. A creature goes for any
visible plant, above or below its layer; with no food in sight it wanders inside its home band
(`pheno.body_lo..body_hi` — the layer minus the body margin) and, when outside it (chased food,
fled, was born there), walks straight back (`standard::pick_random_target`: outside the band
the wander target is the nearest band point; every wander target lies in the band).

Physics clamps only to the world: `pheno.x_lo..x_hi`, `pheno.y_lo..y_hi` (body margin capped at
half the world, so a body bigger than the world sits on the middle line instead of flipping).
`Creature::new` puts a random position into the home band and clamps a given one (a child next
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
population, report and runner limits) is multiplied by `area_ratio` via `per_area`, so
densities — and the balance — stay the same (in theory: see below). Scale is `MIN_SCALE` = 1
to `MAX_SCALE` = 10 000: narrower worlds break the wander geometry, bigger ones run out of
memory before they look any different (per-machine memory guards are phase 6).

Measured with predators (12 seeds × 20 000 ticks at ×10, before `predators-final`): no shape
went extinct, but tall worlds were harsher. Final creatures, median: strip 4930, 3:2 1541, 1:1
1244, 2:1 1639; plants often sat at the cap; in 1:1 and 3:2 one seed each ended with 4
creatures. At ×100 3:2 holds ~15k creatures vs ~60k in the strip. Why (story of 1:1 seed 3):
the start layer is 5‒100% of depth, so in a 15 000-high world most newborns start far from
the rich top and starve (starved 135k vs eaten 80k), the repro threshold collapses to ~7 and
populations swing. Not tuned yet.

### Where food grows

`flora.rs`. A plant's x and y are drawn independently: x by the width profile, y by the depth
profile (density = their product). A profile (`FoodAxis` in `Rules::plant_depth` /
`plant_width`, six rules per axis `plant_{depth,width}_{profile,steepness,end,bend,waves,amplitude}`)
is `f(t)` over the share of the axis from the near edge (surface / left): uniform, linear
(`end` % at the far edge), exp (`steepness`), log (`bend`: plateau, then a cliff), waves
(`waves` rich bands, peaks mid-band, `amplitude` %), and «игровое» (`Profile::Game`: flat to
`GAME_PLATEAU` = 20% of depth, then the exp fall `e^(-steepness·t')` over the rest of the depth,
using the same `steepness` rule as exp). The default is «игровое» down, uniform across (before
`food-web` it was exp, steepness 8). Each parameter is read by its profile only;
the UI shows it only then (`Field::visible`). `--rule plant_width_profile=waves` takes names
(`Rules::with_text`). The surface has no dead zone any more (`PLANT_TOP_MARGIN_PCT` = 0; the knob
still applies to every profile). The distribution is a world property; layer genes don't adapt
to it.

`Flora` is derived from rules + space + the world's seed like a phenotype from a genome: built
in `World::new` and in `set_rules` (plants already grown stay put). Uniform and exp keep the
pre-profile expressions (`rng.uniform`, the analytic inverse CDF) — a unit test compares 10 000
scattered exp plants against a copy of the old formula; the others sample a tabulated inverse CDF
(4096 bins; empty bins never picked). `flora::density(rules, tx, ty)` is the preview the game
paints; `flora::describe(rules)` is the story's line. Limits in `with`: profile an integer index,
waves an integer 1‒100 (table resolution), steepness ≤ 100 (`e^-k` underflow), percents 0‒100,
patches an integer 0‒300, patch size ≥ the plant radius. Profiles are not balanced: with the
pre-`food-web` model, 6 of 48 runs died out at ×1 with each non-exp profile at its defaults.

Capacity is `PLANT_MAX` (per area) **slots**, one plant each; a seed that lands in an occupied
slot does not sprout (its numbers are still drawn), so growth is logistic
(`cap·(1 − e^(−rate·t/cap))` in an empty world), a full world follows the profile and a grazed
surface cannot hand its room to the deep sea. Plant energy does not affect capacity.
- `plant_patches = 0` — scattered: slots are cells of equal fertility, an `nx × ny` grid in the
  coordinates of each axis's distribution function (`Axis::cdf`); a plant draws **two** numbers
  (x, then y) by the profile and takes its cell.
- `plant_patches > 0` (default 24 per base world, radius ~`plant_patch_size` 200) — patches and
  plants between them: coarse regions of equal fertility, ~2 patches each; every region holds the
  same number of slots (so region by region the profile holds — `заросли_не_ломают_профиль`). Part of
  a region's slots lie in its patches, the rest are scattered over the region by the profile (R2
  low-discrepancy points in the distribution functions' coordinates). The part in patches is
  `min(1, c · light)`, light being the depth profile's density at the region's centre (floor 0.05),
  with `c` found so that the world's mean is `plant_patch_share` (rule, default 60%). A patch's
  radius is also times `max(0.3, sqrt(light))` at its centre, so deep patches are rare (deep regions
  are wide), small and poor (`заросли_глубже_реже_мельче_беднее`). A region has 1–3 patches with their
  own size (×0.5–1.5), stretch and weight (a heavier patch takes more of the region's patch slots);
  centres are drawn by the profile inside the region from `Rng::keyed(seed, PATCH_STREAM)`, never
  the world stream. Ellipses are squashed, not clipped, against the plant zone. Patch slots come
  first (a sunflower spiral each), then the scattered ones region by region; a plant draws
  **three** numbers: the slot (uniform), then a jitter inside it.

`Plant` stays 24 bytes: `alive` is `portions > 0`, and the slot (24 bits, `NO_SLOT` for plants put
in by hand) fills the padding next to `born`. Occupied slots are a bitset (`world::Occupancy`)
updated per birth and per eaten plant: a full rebuild doubled the tick of a plant-saturated ×100
world. It is rebuilt when the plant count stops matching it (tests and the app edit `plants`
directly — keep such edits changing the count, or the stale set goes unnoticed). When
`set_rules` changes the layout, every grown plant loses its slot (`Plant::lose_slot`) and new ones
fill the new slots as the old are eaten; rules that do not touch food keep the layout.
`incremental_slots_match_a_rebuild` guards the bookkeeping. The game gets the patch list in the
frame after a new world or new rules (`Frame::patches`) and tints them under the sprouts.

### Balance: exponents, not coefficients

Upkeep is `COEF * stat ** POWER` summed over size, speed and sight, with the speed term also
multiplied by `(size / 40) ** SPEED_MASS_POWER` — moving a big body costs more (the factor is 1
for the base genome). Each term has its own price multiplier (rules `size_cost`, `speed_cost`,
`sight_cost`, applied in `renormalize`) and power, plus `speed_mass_power` and the overall
`cost_scale`; the lab's «Тело» tab shows the formula with these numbers and a chart (`screens::body_formula`). The *exponents* decide whether evolution has a
trade-off at all: eating radius equals size (benefit ~ size²) and search radius equals vision
(benefit ~ vision²), so cost must grow steeper — hence `size ** 2.5` and `vision ** 2`. With
shallower exponents the stats run away to infinity. Read the comment block in `config.rs`
before changing any of these.

Extinction (history, with predators): before the soft layer and the strategies, 4 of 12 seeds
died out within 20k ticks (creatures squeezed into the top few % of depth, repro threshold
collapsed, they starved); with them, 0 of 12. The mutability gene brought it back to 6 of 12
with predators: selection pulls creature mutability from 1 to ~0.2 (a less mutated child is
fitter on average), variation dries up and they lost to predators. Without predators — 0 of
12, ~2000 creatures, mutability settles near 0.4. The user chose deliberately: mutability has
no energy cost.

The `food-web` model (`life-behavior/10`, measured 2026-09-26, seeds 1–8 × 20 000, commit
`16af4f7`): 16/16 worlds survived; median population 802 (base) and 694 (calm). Carnivores hold in
2 worlds (10%, 11%), scavengers in 5 (2–55%); herbivores and omnivores everywhere. Founder
meat-eaters mostly starve by tick ~400 (no corpses yet, equal-sized founders are no prey); meat
diets that persist are later mutants. Corpses: in the base profile ~99% of removed corpses were
stripped to skeletons and they lie 40–150 ticks on average (eaten fast); in the calm one 55–100%,
up to ~1000 ticks. Before the bonuses and skeletons (`e158f9e`): 824 / 544, carnivores in 1 world,
scavengers in none. The history of what was tried is in the commit messages of `16af4f7` and
before.

Lifespan (measured on `16af4f7`): the `life_pace` gene sits at its floor 0.5 in every world
(upkeep × pace makes slow life a near-free 50% discount), so the age limit is ~24 000 ticks, while
creatures live a median of 250–470 ticks (p90 ~2000) and die of hunger or in fights; under 0.1%
reach the ageing fifth. The user agreed to reform life and pace as a separate stage later.

The previous model (`life-behavior/9`, cells, exp profile, one diet): 8/8 in each of the four
modes (with and without the then-optional combat); medians base 1139 / 925, calm 732.5 / 564.
Flock criterion, measured on `life-behavior/8` and not re-measured since: flocks persist — at
least two flocks and 10% flocking carriers — in ≥ 75% of the worlds of each profile (base 14/16,
calm 13/16); loners may vanish, a flock takeover is a legitimate outcome. Judge it on 16+ seeds:
on 8 the lottery of a few worlds decides, and the share of members inside their circle swings by
±0.1 between two RNG realisations of one model. No `repro_cost` in 10–20 lowers all medians by
15–25%; it stays 10. See `BEHAVIOR.md`.

Behaviour genes are free (the user's rule): no upkeep for a gene that gives no physical stat
boost. What restrains one is behaviour and the limits of its effect — e.g. `flock_spacing` is
clamped to 50–500 and a wide circle is leaky (a moderate border holds only in sight of a
member). History: «испуг» (flee distance, % of vision) and «голод» (the former predators' hunger
threshold) were free numeric genes and ran away with predators — hunger crept up and predators
ate the prey out, fear shot to ~100% of vision and creatures starved fleeing, 10 of 12 seeds
extinct; clamped to 10‒60% / 30‒75% each alone cost ~2 of 12, both together 9 of 12. They were
removed. A new behaviour gene needs a behavioural catch, not a price.

### Termination guarantees

Tick cost grows with population, so a tick limit alone does not bound wall time. Every headless
run is capped by four independent limits (`life_sim::Limits`): ticks, population ceiling, a
compute budget (`total_work` = creatures x plants, summed over ticks) and a wall-clock deadline;
the ceiling and budget scale with area. Tests contain no `while` loops at all; every run is
bounded by a tick count. Preserve this property in new tests.

### Performance-sensitive code

`Creature::step` is the hottest path. Genome-derived values (`upkeep`, `slow_speed`,
`slow_upkeep`, `vision2`, `size2`, `half`, layer bounds, the strategy) are precomputed once in
`Phenotype::of` while the genome remains constant; body-dependent values are recomputed when
food grows the body. Distances are compared squared. The grid reuses its buffers between ticks.
Strategy dispatch is a `match` on an enum (static, inlined), never `Box<dyn>`.

The guard below only catches catastrophes. For refactors, compare ms/tick against the
previous version built in a `git worktree`, running both alternately (single runs are noisy):
`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1` (the summary's last column).

`тик_растёт_линейно_с_численностью` (engine tests) compares ms/tick at equal density on the same
machine: a ×10 world with 4000 creatures / 4000 plants against a ×2.5 one with 1000 / 1000,
alternating, best of four. The ratio is ≈4 with the grid and ≈21 when queries degrade to a full
scan; the limit is 8. A ratio, not absolute milliseconds, so a slow machine does not fail it.

## Genes and strategies

The gene table (`genome/creature.rs`): `GeneSpec { key, label, about, kind, base, mutation }`,
`kind` = `Absolute` | `Percent` (clamped 0‒100 on mutation) | `Choice(&[Variant])` (the value
is a variant index). Everything that walks genes — mutation, `Stats`, observer, story, JSON,
charts, creature card, help — iterates the table, never positions. **Tables are append-only**:
the order fixes the RNG draw order of mutation (every seed), positions in the reference
fingerprint and JSON. The one deliberate exception: row 11, the numeric `carnivory`, was
replaced in place by the `diet` choice gene (`food-web`), so no other gene moved.

Mutation laws (`genome::Mutation`): `Scale` for numeric genes (× (1 + gauss(0, σ·mutability)),
multiplier ≥ 0.1); `Switch { chance }` to any other variant (0.1% for the choice genes);
`Neighbours { chance, rise, jump, of, up }` for the diet — a step to a neighbour in
`DIET_NEIGHBOURS` (the omnivore forks to herbivore / scavenger / carnivore, scavenger ↔ carnivore):
towards meat (`DIET_TOWARDS_MEAT`: herbivore → omnivore, omnivore → scavenger or carnivore) with
`DIET_MEAT_STEP_CHANCE` 2%, to any other neighbour with `DIET_STEP_CHANCE` 0.5%; or, on the same
first draw, a leap past the neighbours: the herbivore's own leaps (`DIET_LEAPS`: to the carnivore
`HERBIVORE_LEAP_CARNIVORE` 0.1%, to the scavenger `HERBIVORE_LEAP_SCAVENGER` 0.01%, config only, no
lab rule) replace its general jump; the other diets jump to any other diet (`DIET_JUMP_CHANCE` 0.01%). Switch
chances are multiplied by the parent's mutability; the diet's are not. Before any of it,
`CLONE_CHANCE` (50%) of children are exact copies, and after it mutability is clamped to
`MIN_MUTABILITY` (0.1). All of this is `genome::Heredity`, built from the rules (`clone_share`,
`min_mutability`, `diet_step`, `diet_meat_step`, `diet_jump`) and applied by `CreatureGenome::mutate_by`. Then `picky` («разборчивость», %, base 30, free): the own-niche threshold; the last row
is `rivalry` («задиристость», %, base 30, free): below it a creature fights for its food. A founders' diet mix (`WorldConfig::diets`, 70/30/0/0 in variant order H/O/S/C,
`--diet-mix`) is dealt without draws through `genome::spread_ranks`, so it does not line up with
the strategy mix.

A creature's step is split: its **strategy decides** (`strategy::decide(&Me, &mut Mind, &mut
Rng, &senses) -> Intent`) and the **creature acts** (`act`: movement, clamps, upkeep, death).
A strategy sees only itself, its memory and senses; it cannot move, feed or divide the
creature — the property a parallel tick needs. Hooks: `after_eating` (re-targets right after
eating). Eating, catching and division stay world physics driven by the phenotype.

**Mutability** (`mutability`, base 1): the parent's value multiplies
the mutation sigma of every gene — itself included — and the strategy switch chance
(`mutate_values(.., mutability, ..)`, between `MIN_MUTABILITY` and `MAX_MUTABILITY`). It has no
cost and no phenotype; it acts only at division. Without the floor, selection drove it to 0 in
long worlds, and evolution froze.

The strategy is a gene: a row of each table, `Choice(&strategy::VARIANTS)`,
`Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE }`. **A choice gene with one variant is
inert**: `Switch` draws nothing, so appending it did not shift a single random number; the UI,
the story and the reference check hide such a gene. A start mix
(`WorldConfig::strategies`, shares by variant; `--mix` in the report,
«Затаившихся на старте» in «Новый мир») is dealt
*without* drawing (`genome::variant_for`), so the same seed gives the same world.

**Slow pace** is physics a strategy may choose: `SLOW_PACE` (⅓) of its speed, paying the speed
term for the step actually taken (`pheno.slow_speed`, `pheno.slow_upkeep` — `Rules::upkeep` at
the reduced speed, so the lab's exponents apply): `Intent::slow`.

The strategies:
- `standard` — nearest visible plant, else wander in its layer;
- `lurker` («затаившийся», `lurker.rs`) — the same decision (`standard::plan` returns which
  branch fired), but wanders and returns home at slow pace. In every tested seed (with
  predators) it replaced `standard` (95‒100% by 20k ticks): the savings pay.

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
3. Tests: `decide` scenarios with `senses_from`; a mixed population is
   deterministic (same seed, same digest); `StrategyShift` fires in the chronicle.
4. Re-record the golden test and `--save-reference`. With two variants the strategy row, the
   share chart and the card line appear by themselves.

## The game (`crates/life-app`)

**The window never waits for the simulation** — that is the rule every change must keep.

Default game settings are the user's own world: ×20, 2:1, `cost_scale=3`, energy density 0.2,
half lurkers, food steepness 5 (`Settings::default`); starts at 30 ticks/s. «Спокойнее» applies the profile to
an old game; saved settings are not rewritten automatically (an old file's `cannibalism` key is
ignored). The profile's reference is `reference/calm-fingerprint.json` (compare with `--rule
cost_scale=3`); the engine's base reference is separate.

«Стаи» (hidden while `FLOCKS` is off) toggles flock circles and flock colouring, including the minimap and the density raster.
One circle per flock (its feeding place and territory), gliding between frames like the bodies;
the stroke is thin, normal or thick by territoriality, orange with a warned intruder, red in a
battle. Labels «№ · members · kind» are laid out biggest flock first and never over another
(`view::place_labels`, unit-tested); a circle under 14 points gets none. A click inside a circle
opens the flock card; a hit on a creature's body takes precedence. After dragging
an area the tool returns to selection; Esc and «Убрать рамку» clear the area. Accept an incoming
region summary only for the current rectangle, otherwise a delayed frame can resurrect a
cleared area.

- `sim.rs` — the simulation thread owns `World`. The UI sends `Command`s over a channel (pause,
  speed, step, view rect, pick/select, `SetRules`, spawn, restart, new world); they apply
  between ticks. Frames go through a one-slot mailbox: the thread publishes only when the UI
  took the previous frame, so frames are never dropped — that is why history/log/snapshots
  travel as *deltas* in `Frame` (a test checks none are lost). Tempo: ticks per second with a
  capped debt (lag is shown, never caught up in a burst), `SLICE` bounds a tick burst so
  commands stay responsive; frame building is throttled by its measured cost.
  `Snapshot::of` (sorting) runs every `SNAPSHOT_EVERY` ticks, stretched on big worlds to ≤ 5%.
- `frame.rs` — what the UI needs: instances (32 bytes, relative to `origin` in f64 — f32
  absolute coords break at ×10 000) **culled to the visible rect** (padded by half a view,
  culled by body, not centre), or a density raster when more than `MAX_INSTANCES` are
  visible; the minimap raster every 0.4 s. At distant zoom the shader draws 2-pixel squares;
  `RenderWorld(false)` omits world geometry, density, minimap, corpses and shot trails while
  continuing selected cards and statistics. `кадр_огромного_мира_быстрый_и_лёгкий` guards it.
- `motion.rs` — collects the instances and remembers the previous frame, so nothing jumps or
  pops: previous position and heading (two-pointer merge — creature vecs are sorted by id),
  birth age (a ring "max id / tick in frame → time"; a creature panned into view is not
  "born"), ghosts of the dead (eaten vs starved). Plants have no id: matched by
  (`Plant::born` tick, x bits); `born` sits in padding, `Plant` stays 24 bytes (tested).
  All linear in visible count; reset with a new world or render mode switch. `meta` packs 32
  bits (see the comment at its top; the shader reads the same layout): heading 12 bits, diet 2,
  «eating» and «ate last frame», kind, ghost/starved/dot flags, the food direction (7 bits) and
  the proboscis length (4 bits) from `Creature::meal` — a record the engine writes and never
  reads, outside the golden digest. A plant's heading bits are a hash of its position: the
  sprout's turn and leaf count.
- `render.rs` + `creatures.wgsl` — one instanced draw call through `egui_wgpu::CallbackTrait`.
  The shader draws each creature between its previous and new position (`k`, from
  `view.rs`: time since the frame arrived / smoothed frame interval — one frame of latency),
  grows newborns, shrinks the eaten and greys the starved (age + `since`), keeps sub-pixel
  dots at 1 px with area-scaled alpha (no shimmer), and only above ~4 px draws detail: a rim in
  the diet's colour, fullness core, an eye along the heading, and while eating a proboscis to
  the food (a capsule SDF, extends and retracts with `k`, «gulps» run along it by `time`).
  Plants are dark 3–5-leaf sprouts, a dark dot from afar. Selection ring and follow camera use
  the same interpolated position. The buffer is uploaded only when a new frame arrives.
  `view.rs` paints corpses (red-brown fresh → grey-green rot, sinking between frames; a skeleton
  pale and smaller, `CorpseMark::skeleton`) and the faint patch tint under the flock circles. The
  creature card shows the diet's edges in the world's current rules (`game::diet_bonuses`).
- `app.rs` — `LifeApp`: screens and transitions, owns the settings and the `SimHandle`;
  `theme.rs` — palette (port of `app/theme.py`).
- `stats.rs` — the «Статистика» window (key I): «Энергия» (fullness, plants vs cap),
  «Где живут» (creature depth over time as a heat map + p10/p50/p90, plants vs creatures by
  depth/width band) and «Область»: `Tool::Area` drags a rectangle in `view.rs` (drag draws
  instead of panning), `Command::SetRegion` makes the thread compute `frame::RegionStats` (who
  is inside, their gene stats next to the whole world's) at once — works while paused — and on
  every snapshot. Everything is fed by whole `Snapshot`s, sent to the UI as deltas
  (`Frame::snapshots`, `History::snapshots`). Graphs and summaries retain only the last 10 000
  ticks; the chronicle is independent of that window.
- `view.rs` (world, selection, minimap), `camera.rs` (port of `camera.py`, f64), `game.rs`
  (game screen; its «Графики» side panel: diets first, then the population chart, «Кто кого»,
  folded genome and «Прочее»; lab window with topic tabs `Tab::RULES` — Еда/Тело/Питание/Бой/Трупы/
  Эволюция, «Питание» a table of every diet's edges (`screens::diet_table`, `Key::Diet`); creature card; `report_command`),
  `diets.rs` (the «Кто живёт» block: a row per diet with a click-open comparison to the world,
  flows, kills by diet from `Counters::by_diet`; the «Кто кого» matrix; the «Подсветить:» toggles
  that set `WorldView::highlight` — a shader uniform: highlighted diets drawn ≥ 3 px with a halo,
  the rest dimmed),
  `screens.rs` (menu, «Новый мир» with tab «Мир» plus the lab's topic tabs and buttons pinned in a
  bottom panel, prefs, help; `field_input` — a `DragValue` clamped to the field's hard limits
  (`Field::lo..hi`, why in `Field::limit`, shown by `field_hint`) with the default beside it
  (`base_value`), or, for a field with `choices`, a combo box; `food_preview` — the world in its proportions shaded by `flora::density`, with the
  seed's patches when there are at most 3000),
  `charts.rs` (drawn with the painter — no plot crate; `lines` for any series, `genome` for a
  gene table), `history.rs` (port of `history.py`), `settings.rs` (`FIELDS`, the single field
  spec — label, hint, range, `choices`, `visible`, display scale `shown`, `unit`, `decimals`,
  `limit`; start counts are *per base area* and scale with
  the world; the strategy slider is the share of the second variant, the four diet sliders are
  shares of the founders; the shape is `Settings::shape`; file in
  `%APPDATA%\TinyLife`, atomic, clamped; settings tests write only to temp dirs).
- Chronicle texts come from `life_sim::observe::EventTracker` — the same incremental tracker
  the report's `events()` wraps, so game and report print identical events.
- Live rules: `World::set_rules` recomputes the whole phenotype (`apply_rules`); a test checks it
  equals a newborn's. `World::pick` (returns the id) / `creature(id)` serve selection and
  follow (the creature vec stays sorted by id — tested).
- `ui_tests.rs` — egui_kittest: every screen at 960×600 and 1600×900, buttons/sliders inside
  the window and not overlapping (scrolled-away side-panel content excluded). They share one
  GPU lock: parallel wgpu renderers crash the driver on Windows. CI renders through WARP.
- Release on Windows builds with `windows_subsystem = "windows"` (no console on double-click)
  and attaches to the parent console so flag errors still print.
- `bin/life-progress.rs` — not the game: the `life-sweep` progress window with pause, resume and
  stop (reads `progress.json` twice a second, writes `control.txt`; `View::of` and `draw` are tested,
  `draw` also as PNGs with `TINYLIFE_SHOTS`), its own dark palette whatever the system theme, always
  on top in the bottom right corner, its taskbar button a progress bar through `ITaskbarList3` (the
  `windows` crate at the version eframe already pulls in): green, yellow on pause, red when stopped.

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
- `app/render.py` — draw order plants → creatures (→ predators, then); cull by *body*, not centre
  (size is a gene); the circle is the body (`size`, `DIAM` are diameters).
- Layout rules: everything scales from a 960x600 logical minimum; the successor of `TestLayout`
  must fail when a widget leaves the window, widgets overlap or a label does not fit.
