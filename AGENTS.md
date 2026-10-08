# Working in this repository

lifegame is an evolutionary simulation in Rust with a native wgpu/egui window. The Python version
is kept at the `python-final` tag. `CLAUDE.md` holds the full set of working rules and the model
(`life-behavior/15`) in brief, its numbers living in the code; this file is the short version.

## Structure

- `crates/life-core`: creatures, genome, behaviour programs, life cycle, diets, corpses, combat,
  plants, spatial queries. No graphics or I/O; threads only through rayon under the `parallel`
  feature (per-creature phases, same world on any thread count).
- `crates/life-sim`: bounded runs, statistics snapshots and the chronicle.
- `crates/life-report`: CLI, JSON reports (`life-report/12`), balance comparison and `life-sweep`.
- `crates/life-app`: window, rendering, settings, the simulation thread, the sweep's progress window.
- `reference/*.json`: balance references (`life-behavior/15`).
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
use `ui_tests` and `LIFEGAME_SHOTS`, check images at 960×600 and 1600×900.
Settings tests write files only to temporary directories. In a cloud (Linux) container run no
simulations and no tests: the user's machine is Windows, where golden digests are recorded.

## Style and invariants

Language: code, comments, docs, CLI/report output, test names and commit messages are in
English. Only the game UI stays Russian (window texts, settings labels, gene labels, chronicle
texts). Much is still Russian from before 2026-09-25 (report output, test names, messages):
translate it gradually, only where you edit; a new message or test is English among Russian ones.
Rustfmt (width 110), four-space indent, `snake_case` functions, `PascalCase` types. Read genes
through `Gene`, append new rows only at the end of the table. Don't mix life state into the genome.

Energy is never made from nothing: it enters in plants and only passes along the chain, losing
some. Behaviour is free: programs are restrained by their consequences and effect limits, never
by upkeep, and a new behaviour is a block, never a gene or a world constant. Plant capacity lives
in slots (`flora.rs`): one plant per slot; don't add a second, global plant limit. Senses read the
neighbour snapshot; program blocks return intents and cannot move, feed or divide a creature.
Combat strikes apply simultaneously. The dead don't eat or reproduce; children don't act on their
birth tick. Death counters for every cause must add up with the population. The window never
waits for the engine.

## Checking changes

Add regressions for changed behaviour with fixed seeds and a finite tick count. Don't weaken
checks to make them pass: changing golden requires explaining the mechanic change. Golden and the
references are re-recorded on Windows only, in a commit of their own; from a cloud session through
the Re-record workflow (`CLAUDE.md`, «Re-recording»).

Balance checks (survival criteria, the user's baseline conditions in `CLAUDE.md`) are the user's
call: run them only when asked. Measurement series run as `life-sweep` plans. When the JSON
structure or the model changes, version the format and check that incompatible references are
refused.

Commits: short imperative English subjects, only files relevant to the task, straight to `main`.
Commit and push only when the user asks.

## Current model specifics

The model in brief is in `CLAUDE.md`, its numbers in `config.rs`. What a change most often breaks:

- All behaviour is a program (`creature/program.rs`): ≤ 32 blocks «if up to three tests → an
  action with its parameters». Settings apply first, the first of each kind whose tests hold;
  without a setting of a kind the creature does not do it (no «делиться» — no division). Then the
  first deciding block whose tests hold and whose action can be done decides.
- Two programs, juvenile and adult, shared by relatives that inherited them unchanged; a mutating
  child's programs drift and now and then take one structural mutation that leaves no block dead.
  Founders start from their `strategy` template (standard or lurker).
- Diets are a gene (herbivore, omnivore, scavenger, carnivore), each with its own body edges and
  foods among plants, fresh meat, rot and bones; by default the founders are herbivores and
  omnivores. Upkeep is the body and eyes plus the speed of the step actually taken.
- A creature strikes and shoots only what its program chose; others read it by the blocks whose
  tests held on its last move. Family is a parent and its growing child while «щадить детей»
  holds; every creature is a loner. A torpid creature eats nothing.

Graphs and summaries are limited to the last 10 000 ticks. World rendering can be turned off;
this doesn't change the simulation, statistics or the selected card.
