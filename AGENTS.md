# Working in this repository

lifegame is an evolutionary simulation in Rust with a native wgpu/egui window. The Python version
is kept at the `python-final` tag. `CLAUDE.md` is the current model (`life-behavior/12`) and the
full set of working rules; this file is the short version.

## Structure

- `crates/life-core`: creatures, genome, life cycle, diets, corpses, combat, plants, spatial
  queries, and the dormant flock layer. No graphics, threads or I/O.
- `crates/life-sim`: bounded runs, statistics snapshots and the chronicle.
- `crates/life-report`: CLI, JSON reports (`life-report/11`), balance comparison and `life-sweep`.
- `crates/life-app`: window, rendering, settings, the simulation thread, the sweep's progress window.
- `reference/*.json`: balance references (still `life-behavior/9`, to be re-taken).
- `Relict/`: frozen archive; never change anything in it.

## Commands

```text
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 20000 --max-work 1e15
cargo run -p life-report --release -- --compare reference/fingerprint.json
cargo run -p life-app --release
```

`play.bat` and `play.sh` build and run the game. Don't open the GUI for automated checks:
use `ui_tests` and `TINYLIFE_SHOTS`, check images at 960×600 and 1600×900.
Settings tests write files only to temporary directories. In a cloud (Linux) container run no
simulations and no tests: the user's machine is Windows, where golden digests are recorded.

## Style and invariants

Language: code, comments, docs, CLI/report output, test names and commit messages are in
English. Only the game UI stays Russian (window texts, settings labels, gene labels, chronicle
texts). Translate existing Russian comments and test names gradually, only where you edit.
Rustfmt (width 110), four-space indent, `snake_case` functions, `PascalCase` types. Read genes
through `Gene`, append new rows only at the end of the table. Don't mix life state into the genome.

Energy is never made from nothing: it enters in plants and only passes along the chain, losing
some. Behaviour genes are free: restrain them by behaviour and effect limits, never by upkeep.
Plant capacity lives in slots (`flora.rs`): one plant per slot; don't add a second, global plant
limit. Senses read the neighbour snapshot; strategies return intents and cannot move, feed or
divide a creature. Combat strikes apply simultaneously. The dead don't eat or reproduce; children
don't act on their birth tick. Death counters for every cause must add up with the population.
The window never waits for the engine.

## Checking changes

Add regressions for changed behaviour with fixed seeds and a finite tick count. Don't weaken
checks to make them pass: changing golden requires explaining the mechanic change and checking
the balance; re-record golden and the references in a separate commit.

For a model change, run seeds 1–8 for 20 000 ticks (`--max-work 1e15`) for the base and the calm
(`--rule cost_scale=3`) profiles at ×1, and check that runs end on their own; at least seven
surviving worlds in each. Then the user's baseline conditions (in `CLAUDE.md`). Measurement series
run as `life-sweep` plans. When the JSON structure or the model changes, version the format and
check that incompatible references are refused.

Commits: short imperative English subjects, only files relevant to the task, straight to `main`.
Commit and push only when the user asks.

## Current model specifics

Diets are a gene (herbivore, omnivore, scavenger, carnivore): each has its own body edges and its
own foods among plants, fresh meat, rot and bones; meat diets arise from mutants. A corpse is
fresh for 300 ticks, then rot that sinks and decays to bones by 3000, then bones for 5000. The deep
is cold below a thermocline, where the `cold_blood` gene makes a body cheaper and slower. Upkeep is
the body and eyes plus the speed of the step actually taken. Free behaviour genes: `cruise`,
`rest`, `burst`, `torpor`, `layer_reach`, `picky`, `rivalry`, `caution`, `bravery`, `care`,
`prey_ratio`, `maturation`, `lifespan`. From 70% of its lifespan a creature weakens to 70% at 90%.

Family is only a parent and its growing child while the parent still knows it (`care`); siblings
and grandchildren are strangers. A hunter weighs the meat its tank can take in against the strikes
it expects (`caution`) and gives up a chase that does not close in within 30 ticks. Strikes need a
chosen target, a defence or a rival at the same food.

Flocks are off (`config::FLOCKS = false`): founders and the base genome are loners, and the game
hides flocks. The flock code (`flock.rs`, `battle.rs`, `social.rs`, `territory.rs`,
`kin_grace.rs`) still runs; parts of the social layer act on loners too (rest, a held course, a
smooth turn), so removing it shifts every seed.

Graphs and summaries are limited to the last 10 000 ticks. World rendering can be turned off;
this doesn't change the simulation, statistics or the selected card.
