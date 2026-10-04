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
texts). Translate existing Russian comments and test names gradually, only where you edit.
Rustfmt (width 110), four-space indent, `snake_case` functions, `PascalCase` types. Read genes
through `Gene`, append new rows only at the end of the table. Don't mix life state into the genome.

Energy is never made from nothing: it enters in plants and only passes along the chain, losing
some. Behaviour is free: programs are restrained by their consequences and effect limits, never
by upkeep, and a new behaviour is a block, never a gene or a world constant. Plant capacity lives in slots (`flora.rs`): one plant per slot;
don't add a second, global plant limit. Senses read the neighbour snapshot; program blocks return
intents and cannot move, feed or divide a creature. Combat strikes apply simultaneously. The dead don't eat or reproduce; children
don't act on their birth tick. Death counters for every cause must add up with the population.
The window never waits for the engine.

## Checking changes

Add regressions for changed behaviour with fixed seeds and a finite tick count. Don't weaken
checks to make them pass: changing golden requires explaining the mechanic change; re-record
golden and the references in a separate commit.

Balance checks (survival criteria, the user's baseline conditions in `CLAUDE.md`) are the user's
call: run them only when asked. Measurement series run as `life-sweep` plans. When the JSON structure or the model changes, version the format and
check that incompatible references are refused.

Commits: short imperative English subjects, only files relevant to the task, straight to `main`.
Commit and push only when the user asks.

## Current model specifics

Diets are a gene (herbivore, omnivore, scavenger, carnivore): each has its own body edges and its
own foods among plants, fresh meat, rot and bones; by default no founder eats meat (a founders'
diet mix may deal meat diets), and they arise from mutants. A corpse is
fresh for 300 ticks, then rot that sinks and decays to bones by 3000, then bones for 5000. The deep
is cold below a thermocline, where the `cold_blood` gene makes a body cheaper and slower. Upkeep is
the body and eyes plus the speed of the step actually taken. Eleven genes, the body and life
history: `size`, `speed`, `vision`, `strategy`, `mutability`, `maturation`, `diet`, `lifespan`,
`cold_blood`, `burst` (the muscles, paid standing), `program_mutability` (the programs' own rate).
From 70% of its lifespan a creature weakens to 70% at 90%.

All behaviour is a program (`creature/program.rs`): an ordered list of ≤ 32 blocks «if up to
three tests → an action with its parameters». Each tick the settings apply first, wherever they
stand, the first of each kind whose tests hold: eat foreign food, drive off rivals, how far past
its layer, the layer, a smooth step, division, healing, eating on the move, sparing its children,
shooting, and modes (the program's memory, read by a test). Without a setting of a kind the
creature does not do it: a program without «делиться» never divides. Then the first deciding block
whose tests hold and whose action can be done decides. A creature has two programs, juvenile and
adult, shared between relatives that inherited them unchanged. On division a mutating child's
programs drift — a third of the numbers a little, like genes (`program_drift`) — and mutate
(`program_mutation`, 5% × `program_mutability` each; a new block only where it is reached and
never one born dead or killing another, a deletion takes a dead block first, a «pair» makes a
memory in one step, a «transfer» copies a block from the other track). Founders start from their `strategy` template (standard or lurker),
which carries the bases of the deleted genes and of what the world used to do and remembers
hunger, an alarm and a full tank through modes; a founder's layer and the 5% shooters are set in
its program. No world behaviour constants: numbers live in the blocks. A torpid creature eats
nothing; others read a creature by the blocks whose tests held on its last move, so a hunt block
behind an impossible condition scares nobody.

Family is only a parent and its growing child while the parent's «щадить детей» holds; siblings
and grandchildren are strangers. A parent whose program has «защищать детёныша» goes for its
child's enemy whatever its size, within the block's limits. A hunt block weighs the meat the tank
can take in against the strikes it expects (its caution) and gives up a chase that does not close
in within its patience (30 ticks in the templates). Strikes and shots need a target the program
chose (prey, a fight back, a child's enemy) or a rival at the same food under the rival setting.

Flocks are gone (tag `flocks-final`): every creature is a loner.

Graphs and summaries are limited to the last 10 000 ticks. World rendering can be turned off;
this doesn't change the simulation, statistics or the selected card.
