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

python -m unittest discover tests                   # full suite (34 tests, ~2 s, no display)
python -m unittest tests.test_simulation.TestGrid   # one class
python -m unittest tests.test_simulation.TestGrid.test_grid_matches_brute_force   # one test

python sim_report.py                                # headless balance report, seed 1, 600 ticks
python sim_report.py --seeds 1 2 3 --ticks 3000     # multi-seed summary
python sim_report.py --predators 20 --predator-speed 18 --predator-vision 800
python sim_report.py --ticks 3000 --max-work 500000000   # override the compute budget
```

There is no build step, no linter config, and no dependency manifest — only `pygame` is needed
(`pip install pygame`). Tests and `sim_report.py` never touch the display.

On Windows, Russian output from tests and `sim_report.py` comes out as mojibake in a non-UTF-8
console; prefix commands with `PYTHONIOENCODING=utf-8` (bash) to read it.

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
- `life/genome.py` — `Genom`, a `NamedTuple` of the 7 herbivore genes, plus `PERCENT_GENES`
  (clamped to 0‒100 on mutation) and the Russian display labels `GENE_LABELS`. `Vegetarian` accepts a plain list (`config.py` and the tests
  pass lists) and converts it with `Genom(*genom)`; access genes by name, never by index. Being a
  tuple, it still supports `v.genom[i]`, which `World.stats()` relies on for averaging.
- `life/headless.py` — `simulate()`: one engine shared by tests and `sim_report.py`.
- `main.py` — window, controls (pause, single step, speed, click-to-select, graph toggle), game
  loop. Population history for the graph is a `deque` sampled every `GRAPH_EVERY` ticks inside the
  step loop, using list lengths rather than `world.stats()` (which averages the genome).
  `pick_creature()` is deliberately pygame-free so tests can call it.
- `render.py` — all drawing (`draw_world`, `draw_selection`, `draw_panel`, `draw_graph`,
  `draw_stats`, `draw_hud`): the only module that knows about both pygame and the entities. Draw
  order is plants → herbivores → predators, which is what determines overlap. Text and panel
  backgrounds go through the `lru_cache`d `_text` / `_backdrop_box` — render text via `_text`, not
  `font.render`, so per-frame redraws stay cheap. The drawn circle is the body: `size` and `DIAM`
  are diameters, while a herbivore eats anything within `size` of its centre.
- `sim_report.py` — CLI report over `simulate()`. Its compute budget scales with `--ticks`
  (`WORK_PER_TICK`), and the summary lists runs cut short by the budget or deadline instead of
  calling them healthy.
- `Relict/` — **frozen 2025 archive** of early prototypes. See `Relict/ПАМЯТНИК.txt`: nothing
  there is edited, refactored, "fixed" or modernised. Its bugs are part of the monument.

**Window speed is not `TICKS_PER_FRAME`.** That config constant is an engine parameter — it drives
the plant spawn loop inside `World.step()` and `FLEE_TICKS` — so changing it changes the balance.
The window's speed setting just calls `world.step()` several times per frame. For the same reason
anything periodic in `main.py` counts frames, not `world.tick`: at higher speeds the tick counter
skips past multiples.

`tests/test_render.py` draws onto an in-memory `pygame.Surface` under `SDL_VIDEODRIVER=dummy` and
skips itself when pygame is missing. It is the only test file allowed to import pygame. It grows
live worlds through `simulate(..., on_tick=...)`, never a bare `world.step()` loop, so the same
limits apply there as everywhere else.
pygame's default font has no glyph for arrows like `→` (renders as a box) — spell keys out in
on-screen text.

Inside `life/`, intra-package imports are relative (`from .config import *`), so the package does
not care where it is launched from.

### Tick ordering and the death flag

`World.step()` runs: spawn plants → update predators → update herbivores. A herbivore eaten by a
predator earlier in the same tick is skipped (`if not v.alive: continue`) rather than removed
mid-iteration. The same check runs again right after each creature's own `move()`: starving
there sets `alive = False`, and a creature that died on its move must not go on to eat or
divide. Entities that search for targets (`Predator._nearest_prey`, the plant loop in
`Vegetarian.move`) skip `alive = False` candidates for the same reason. Likewise, eaten plants are marked `alive = False` and swept once per tick at the
end of `_update_vegetarians` — removing them inline would cost a linear search and invalidate the
grid cache. Because plants only ever leave by being eaten, spawning stops at `PLANT_MAX`; the cap is
set well above any healthy run's peak so it only bites once herbivores are gone, and
`_spawn_plants` still draws its random number when capped so the RNG stream stays identical.
Offspring go into a separate `offspring` buffer and are appended after the loop, so
children never act on the tick they were born.

### Neighbour search

Entities stayed naive: they still loop over everything handed to them. The change is *what* is
handed to them — `World` builds a `Grid` per tick and passes `grid.near(x, y)`, a 3x3 block of
cells, instead of the whole world. Invariants:

- **Cell size must be ≥ the largest query radius** (vision is a gene, so `World` recomputes it
  every tick from actual values, floored at `GRID_MIN_CELL`). A smaller cell breaks the guarantee
  that 3x3 covers everything within the radius, and creatures start missing neighbours under
  their nose.
- `near()` returns a *superset*; the caller still checks distance.
- **A herbivore's `try_eat` reuses the block fetched for `move`**, i.e. around its position
  *before* the step. That is only complete because the herbivore cell is ≥ `size + speed` and a
  step never moves a creature more than `speed`. The latter holds because `Vegetarian.__init__`
  clamps every position — random or given — into the creature's own band
  (`x_lo..x_hi`, `body_lo..body_hi`), so `move()`'s clamps can only shorten a step. Don't create
  herbivores outside their band or add moves that teleport. Safe only because `try_eat` eats
  *everything* in range, so candidate order does not matter. `Predator.try_eat` eats the *first*
  hit, so predators still re-query after moving.

The 3x3 neighbourhood is cached per cell — that cache is where most of the speedup lives, since
several creatures usually share a cell. `tests/test_simulation.py::TestGrid` checks the grid
against brute force, and `test_creatures_get_every_neighbour_in_live_world` intercepts every
entity call in a live world and checks that what `World` handed over is complete. That catches
a wrong cell formula, a wrong query point or a broken grid. Keep it passing when touching
`life/grid.py`, the cell-size computation in `life/world.py`, or how creatures move.

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
number must match apart from the ms/tick timings (the speed line and the summary column). Any
change to how many random numbers are drawn, or in what order, shifts every seed. That is
fine for a deliberate behaviour change, but then re-validate balance instead of diffing.

### Performance-sensitive code

`Vegetarian.move()` (in `life/vegetarian.py`) is the hottest path (called per herbivore per tick). It deliberately uses
local variables instead of `self.x` in loops, squared distances instead of `math.hypot`, hand-
unrolled `min(max(...))` clamps, and genome-derived values (`upkeep`, `vision2`, layer bounds)
precomputed once in `__init__` because the genome never changes during a lifetime. The band
`body_lo..body_hi` always lies inside the world (a layer thinner than the body collapses to a line
kept inside it), which is why `move()` has no separate world-height clamp. `Predator.upkeep` is
precomputed the same way.
`TestPerformance.test_tick_budget_at_fixed_load` guards against regressions at a fixed 400/400/10
load.

One subtlety in `move()`: the nearest plant is found first and *then* discarded if it falls
outside the creature's vertical layer. Folding the layer check into the loop would search for the
nearest plant *within the layer* — different behaviour, and the creature would stop wandering.
