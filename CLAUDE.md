# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tiny Life Simulation — an evolutionary sandbox in Python + pygame: plants, herbivores
(`Vegetarian`) and predators in a 2D world. Herbivores carry a 7-gene genome that mutates on
division; selection is emergent, not scripted.

Everything — comments, docstrings, commit messages, README, test output — is written in
Russian. Keep it that way when editing or adding code.

## Commands

```bash
python main.py                                      # run with the pygame window

python -m unittest discover tests                   # full suite (17 tests, ~1.5 s, no display)
python -m unittest tests.test_simulation.TestGrid   # one class
python -m unittest tests.test_simulation.TestGrid.test_grid_matches_brute_force   # one test

python sim_report.py                                # headless balance report, seed 1, 600 ticks
python sim_report.py --seeds 1 2 3 --ticks 3000     # multi-seed summary
python sim_report.py --predators 20 --predator-speed 18 --predator-vision 800
```

There is no build step, no linter config, and no dependency manifest — only `pygame` is needed
(`pip install pygame`). Tests and `sim_report.py` never touch the display.

## Architecture

**Logic is separated from rendering, and that split is the load-bearing design decision.**
The layout follows one rule: **`life/` is the engine and knows nothing about the screen; the
project root is the application layer** — window, rendering, CLI report. That is what lets tests
and balance tuning run headless — orders of magnitude faster than watching the screen.

The boundary is enforced, not merely documented: `TestLayering.test_engine_does_not_import_pygame`
fails if importing `life/` ever pulls in pygame again. Entities must not grow `draw()` methods —
new drawing goes in `render.py`.

- `life/config.py` — every tunable constant, each with a comment explaining *why* it has
  that value.
- `life/world.py` — `World`: populations, `step()` (one tick), `stats()`.
- `life/grid.py` — `Grid`: uniform spatial hash for neighbour lookup, rebuilt each tick.
- `life/plant.py` / `life/vegetarian.py` / `life/predator.py` — the entities. Pure logic,
  no pygame.
- `life/genome.py` — `Genom`, a `NamedTuple` of the 7 herbivore genes, plus `LAYER_GENES` and the
  Russian display labels `GENE_LABELS`. `Vegetarian` accepts a plain list (`config.py` and the tests
  pass lists) and converts it with `Genom(*genom)`; access genes by name, never by index. Being a
  tuple, it still supports `v.genom[i]`, which `World.stats()` relies on for averaging.
- `life/headless.py` — `simulate()`: one engine shared by tests and `sim_report.py`.
- `main.py` — window, events, game loop.
- `render.py` — `draw_world()`: the only module that knows about both pygame and the entities.
  Draw order is plants → herbivores → predators, which is what determines overlap.
- `sim_report.py` — CLI report over `simulate()`.
- `Relict/` — **frozen 2025 archive** of early prototypes. See `Relict/ПАМЯТНИК.txt`: nothing
  there is edited, refactored, "fixed" or modernised. Its bugs are part of the monument.

Inside `life/`, intra-package imports are relative (`from .config import *`), so the package does
not care where it is launched from.

### Tick ordering and the death flag

`World.step()` runs: spawn plants → update predators → update herbivores. A herbivore eaten by a
predator earlier in the same tick is skipped (`if not v.alive: continue`) rather than removed
mid-iteration. Likewise, eaten plants are marked `alive = False` and swept once per tick at the
end of `_update_vegetarians` — removing them inline would cost a linear search and invalidate the
grid cache. Because plants only ever leave by being eaten, spawning stops at `PLANT_MAX`; the cap is
set well above any healthy run's peak so it only bites once herbivores are gone, and
`_spawn_plants` still draws its random number when capped so the RNG stream stays identical.
Offspring go into a separate `offspring` buffer and are appended after the loop, so
children never act on the tick they were born.

### Neighbour search

Entities stayed naive: they still loop over everything handed to them. The change is *what* is
handed to them — `World` builds a `Grid` per tick and passes `grid.near(x, y)`, a 3x3 block of
cells, instead of the whole world. Two invariants:

- **Cell size must be ≥ the largest query radius** (vision is a gene, so `World` recomputes it
  every tick from actual values, floored at `GRID_MIN_CELL`). A smaller cell breaks the guarantee
  that 3x3 covers everything within the radius, and creatures start missing neighbours under
  their nose.
- `near()` returns a *superset*; the caller still checks distance.

The 3x3 neighbourhood is cached per cell — that cache is where most of the speedup lives, since
several creatures usually share a cell. `tests/test_simulation.py::TestGrid` checks the grid
against brute force in a live world; keep those passing when touching `life/grid.py` or the cell-size
computation in `life/world.py`.

### Balance: exponents, not coefficients

Upkeep is `COEF * stat ** POWER` summed over size, speed and sight. The *exponents* decide
whether evolution has a trade-off at all: eating radius equals size (benefit ~ size²) and search
radius equals vision (benefit ~ vision²), so cost must grow steeper — hence `size ** 2.5` and
`vision ** 2`. With shallower exponents the stats run away to infinity. Read the comment block in
`life/config.py` before changing any of these.

Current values are validated by ~3000-tick runs over several seeds: both populations survive,
predator–prey oscillation is visible, numbers stay in a playable corridor. Re-validate with
`python sim_report.py --seeds 1 2 3 --ticks 3000` after touching `life/config.py`.

### Termination guarantees

Tick cost grows with population, so a tick limit alone does not bound wall time. Every headless
run is capped by four independent limits (`headless.simulate`): ticks, population ceiling, a
compute budget (`total_work` = herbivores x plants, summed over ticks), and a wall-clock
deadline. Tests contain no `while` loops at all; when a limit trips, assertions run against the
state reached rather than failing. Preserve this property in new tests — `run_ticks` /
`BoundedRunMixin` in `tests/test_simulation.py` exist for that.

When a change is supposed to preserve behaviour, diff
`python sim_report.py --seeds 1 2 3 --ticks 600` before and after: seeds are fixed, so every
number must match apart from the ms/tick timing line.

### Performance-sensitive code

`Vegetarian.move()` (in `life/vegetarian.py`) is the hottest path (called per herbivore per tick). It deliberately uses
local variables instead of `self.x` in loops, squared distances instead of `math.hypot`, hand-
unrolled `min(max(...))` clamps, and genome-derived values (`upkeep`, `vision2`, layer bounds)
precomputed once in `__init__` because the genome never changes during a lifetime.
`TestPerformance.test_tick_budget_at_fixed_load` guards against regressions at a fixed 400/400/10
load.

One subtlety in `move()`: the nearest plant is found first and *then* discarded if it falls
outside the creature's vertical layer. Folding the layer check into the loop would search for the
nearest plant *within the layer* — different behaviour, and the creature would stop wandering.
