# lifegame

An evolutionary sandbox: plants and creatures in a 2D sea. Every creature carries a genome, a
table of genes that mutates when it divides. Nobody scripts selection: the genomes that pay for
themselves survive. The game, the headless runs and the balance report are Rust; the earlier
Python and pygame version is kept under the tag `python-final`.

The current model is `life-behavior/15` (every behaviour in evolving programs with a memory,
each creature read by the others by its last move, on the ocean reform).
[CLAUDE.md](CLAUDE.md) is its exact
description and the project's working rules; [BEHAVIOR.md](BEHAVIOR.md) records how the model got
here and how each stage was checked.

## The world in short

- **Plants** grow in patches over a depth profile. By default the profile is «океаническое»: 60%
  of the food at the surface, a peak at 15% of the depth, then an exponential fall to a nearly dead
  bottom. The world holds `PLANT_MAX` places for plants, one plant each, so growth is logistic.
- **Creatures** eat plants and corpses in portions, grow from food to their size gene, divide when
  grown and fed, and die of hunger, in a fight or of old age. A body's upkeep is its size and eyes
  plus the speed of the step it actually takes.
- **Diets** are a gene: herbivore, omnivore, scavenger and carnivore. Each has a body of its own
  (the strike, health, the price of size and speed, the sense of smell) and its own foods among
  plants, fresh meat, rot and bones. There are no meat-eating founders: carnivores and scavengers
  arise from mutants.
- **Corpses** stay fresh 300 ticks where the creature died, then rot and sink while the flesh
  decays down to the bones; bones feed only the scavenger.
- **The deep is cold** below a thermocline. The `cold_blood` gene makes a body cheaper and slower
  there.
- **Behaviour is a program that evolves**: an ordered list of blocks «if up to three tests → an
  action with its parameters», such as: flee a hunter closer than a third of its sight, hunt prey
  1.5 times smaller with some caution, defend its child, rest when full, wander at a third of its
  speed, sleep when hungry. The settings apply first — its layer, when it divides and heals, whether
  it eats on the move, spares its children or shoots, eats foreign food, drives off rivals, and
  modes, the program's memory; then the first block that fires decides the step. Everything a
  creature does is in its program: without a division setting it never divides. A creature has two
  programs, one while it grows and one when grown. On division a child's numbers drift a little,
  like genes, and now and then a program mutates: a number moves, a test or an action changes, a
  block is swapped, copied, deleted or added. Founders start from a template (standard or lurker).
  Behaviour costs nothing: it is held back by what it does, never by upkeep. The game draws the
  selected creature's programs as flowcharts (B).
- **Energy is never made from nothing**: it enters the world only in plants and passes along the
  chain, losing some at every step.
- Every creature is a loner: flocks were removed (the tag `flocks-final` keeps them).

## How to run

1. Install Rust 1.95 or newer: https://rustup.rs (once).
2. Run the game:
   - **Windows**: double-click `play.bat`;
   - **Linux and macOS**: `sh play.sh`.

The first build takes a few minutes, the next ones seconds. Flags pass through:
`play.bat --scale 100 --seed 7` opens a world 100 times bigger with seed 7. The game needs a GPU
with Vulkan, DirectX 12 or Metal (an integrated one is enough).

## The game

```
cargo run -p life-app --release                          # the menu (same as play.bat)
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world
```

- **New world**: the scale (×1 to ×10 000) with a speed estimate for this machine, the shape
  (1:1, 3:2, 2:1 or a strip), the seed, the founders' mixes, and every world rule by topic: food,
  body, diets, combat, corpses, evolution.
- **In the game**: Space pauses, → steps one tick, + and − change the speed, the wheel zooms to
  the cursor, drag or WASD moves the camera, Home shows the whole world, a click selects a
  creature, F follows it, B draws its behaviour programs, Tab opens the side panel (populations by
  diet, who kills whom, the genome, the chronicle, the creature card), L opens the lab (the rules
  mid-game), I opens the statistics
  (fullness, where creatures live, an area's genome, and on pause how every characteristic and
  the adults' behaviour spread within each diet). Esc opens the menu.
- **No freezes at any scale**: the simulation runs in its own thread and the window draws the
  last finished frame. The creatures' decisions use every processor core, and the world stays the
  same on any number of them; «Показывать кадры» also shows where a tick's time goes. Far away the world turns into two-pixel dots or a density map; rendering can
  be switched off while statistics and the card keep working.
- **For research**: «Повторить без окна» gives the `life-report` command that repeats the game.

Settings live in `%APPDATA%\TinyLife\settings.json` (the user's config folder on Linux and macOS).
A broken file does not matter: the game takes the defaults.

## Headless runs

```
cargo run -p life-report --release                                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000    # a summary over seeds
cargo run -p life-report --release -- --rule plant_energy=80 --rule cost_scale=3
cargo run -p life-report --release -- --scale 100 --shape 1:1       # bigger and square
cargo run -p life-report --release -- --mix 1 1 --diet-mix 50 0 0 50   # strategy and founder diet mixes
cargo run -p life-report --release -- --ticks 20000 --maps 3        # story and text maps
cargo run -p life-report --release -- --ticks 5000 --json run.json  # everything as JSON (life-report/12)
```

Seeds run in parallel. Every run is capped by ticks, a population ceiling, a work budget and a
wall-clock deadline, so it cannot hang. The story tells what happened and why: flows of births and
deaths by diet, who killed whom, the genome from start to end, where creatures live by depth, the
chronicle of events and text maps.

Balance searches run as `life-sweep` plans (variants × seeds, each its own process, with a
progress window); see CLAUDE.md.

### The balance reference

```
cargo run -p life-report --release -- --compare reference/fingerprint.json
cargo run -p life-report --release -- --save-reference reference/fingerprint.json
```

`reference/*.json` are balance fingerprints (8 seeds × 20 000 ticks). The comparison repeats the
same seeds and checks the statistics; a reference of another model is refused. The references in
the repository are still `life-behavior/9` and wait to be re-taken once the current balance is
accepted.

## Tests

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Every run in the tests is bounded by a tick count. `tests/golden.rs` pins a digest of the whole
world (asserted on Windows only, where it is recorded). The screen tests draw without a window
(egui_kittest) at 960×600 and 1600×900; with `TINYLIFE_SHOTS=dir` they save pictures of every
screen.

## Layout

A cargo workspace of four crates. The load-bearing rule: **the engine knows nothing of the
screen**, and the crate boundary enforces it.

| where | what it does |
|---|---|
| `crates/life-core/` | the engine: world, tick, genome, rules, senses, combat, corpses, plants |
| `crates/life-sim/` | bounded headless runs and the observer (snapshots, chronicle, maps) |
| `crates/life-report/` | the report, JSON, the balance comparison and `life-sweep` |
| `crates/life-app/` | the game (egui + wgpu) and the sweep's progress window |
| `reference/` | balance references |
| `docs/` | the parallel tick design, real units |
| `Relict/` | a frozen 2025 archive of early prototypes; never edited |

## Scale and shape

`--scale N` makes the world N times bigger by area; `--shape` sets its proportions: `3:2` (the
default), `1:1`, `2:1` or `strip` (4000 high, growing only in width). At ×1 both 3:2 and the strip
are the base 6000×4000 world. Plant rate, the plant cap and the founders grow with the area, so the
density, and with it the balance, stays the same. The vertical ecology is in % of depth.

## Where food grows

Plants are placed along two axes independently: depth (surface to bottom) and width (left to
right), each with its own density profile, and in patches over it.

| profile | what it does | parameter |
|---|---|---|
| `uniform` | even along the axis | — |
| `linear` | thins out along a straight line | food at the far edge, % |
| `exp` | thins out fast, then a long tail | steepness |
| `log` | holds long, then drops off | bend |
| `waves` | several rich bands | bands, amplitude |
| `game` («игровое») | flat to 20% of depth, then an exponential fall | steepness |
| `ocean` («океаническое») | 60% at the surface, a peak at 15%, then an exponential fall | steepness |

The default is `ocean` in depth and `uniform` in width, with 24 patches per base world holding 60%
of the places. Profiles are world rules: `--rule plant_depth_profile=game`, or the «Еда» tab of the
game and the lab (plants already grown stay, new ones follow the new profile).
