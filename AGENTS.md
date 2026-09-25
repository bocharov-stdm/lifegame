# Working in this repository

Tiny Life is an evolutionary simulation in Rust with a native wgpu/egui window.
The Python version is kept at the `python-final` tag; detailed invariants are in `CLAUDE.md`.

## Structure

- `crates/life-core`: creatures, genome, life cycle, combat, flocks, plants and spatial queries. No graphics, threads or I/O.
- `crates/life-sim`: bounded runs, statistics snapshots and the chronicle.
- `crates/life-report`: CLI, JSON reports and balance comparison against the reference.
- `crates/life-app`: window, rendering, settings and the simulation thread.
- `reference/fingerprint.json`: reference of the current behaviour model.
- `Relict/`: frozen archive; never change anything in it.

## Commands

```text
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 20000 --max-work 1e15
cargo run -p life-report --release -- --compare reference/fingerprint.json --max-work 1e15
cargo run -p life-app --release
```

`play.bat` and `play.sh` build and run the game. Don't open the GUI for automated checks:
use `ui_tests` and `TINYLIFE_SHOTS`, check images at 960×600 and 1600×900.
Settings tests write files only to temporary directories.

## Style and invariants

Language: code, comments, docs, CLI/report output, test names and commit messages are in
English. Only the game UI stays Russian (window texts, settings labels, gene labels, chronicle
texts). Translate existing Russian comments and test names gradually, only where you edit.
Rustfmt, four-space indent, `snake_case` functions, `PascalCase` types. Read genes through
`Gene`, append new rows only at the end of the table. Don't mix life state into the genome.

Senses read the neighbour snapshot; strategies return intents. Combat strikes apply
simultaneously. The dead don't eat or reproduce; children don't act on their birth tick.
Death counters for every cause must add up with the population. The window never waits for
the engine.

## Checking changes

Add regressions for changed behaviour with fixed seeds and a finite tick count.
Don't weaken checks to make them pass: changing golden requires explaining the mechanic change
and checking the balance. Update the reference with `--save-reference`.

For a model change, run seeds 1–8 for 20 000 ticks with `cannibalism=0` and `=1`, and check
explicitly that runs finish without stopping on a limit. Acceptance: at least seven surviving
worlds in each mode. The 4000/4000 load test must stay under 20 ms/tick.
When the JSON structure or the model changes, version the format and check incompatible references.

Commits: short imperative English subjects, only files relevant to the task.
Describe behaviour, verification commands and UI screenshots in the change description.
Don't commit personal settings. Commit and push only when the user asks.

## Current model specifics

Graphs and summaries are limited to the last 10 000 ticks. The world combat rule and the
inherited predatory adaptation are shown separately. World rendering can be turned off; this
doesn't change the simulation, statistics or the selected card.

Flocking, territoriality, strategy, shooting, flock kind and the layer switch are uniform within
a family flock. A flock of two or more is a circle (radius `flock_spacing · √n`, 80–600) that
moves by its kind (settled, nomadic, scouts, vertical migrants); members feed inside it unless
below their inherited `forage` share of their store (base 40%; then they forage until 1.75 times
as much). A young family (fewer than 4) roams: its
circle follows the members and is at least as wide as they see. Circles without territoriality
overlap freely, moderate ones push softly and are respected only in sight of a member, hard
ones never overlap anything; no strict overlap may ever remain. Nobody targets food, a return
or a wander point behind a border it respects, and a circle pressed against the world's edge is
walked around on its open side. With combat on, a territorial flock squeezed with no room nearby
fights every flock with adults touching its circle; a flock that lost more than half of its
adults moves away (a young family too); a battle ends when no two of its flocks may strike each
other. Loners live with separate labels. After a split, two groups don't
attack each other for 600 ticks. Family is only a parent and its growing child while the parent
still knows it (`care` sets until what growth); siblings and grandchildren are strangers. A
parent defends its child only while it knows it; energy is passed to it at birth. A stranger
that could eat a creature but hunts nobody is feared only within `1 − bravery` of the flight
distance. A hunter weighs the meat
its tank can take in against the expected strikes of the prey and its visible allies
(`caution`); strikes need a chosen target, a defence or a territorial assignment. Every other
founder is flocking. A melee strike requires bodies to touch;
corpses are available to everyone from the next tick. Behaviour genes are free: restrain them by
behaviour and effect limits, never by upkeep.
