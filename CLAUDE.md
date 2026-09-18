# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tiny Life Simulation — an evolutionary sandbox: plants, herbivores (`Vegetarian`) and predators
in a 2D world. Herbivores carry a 7-gene genome that mutates on division; selection is
emergent, not scripted.

Everything — comments, docstrings, commit messages, README, test output — is written in
Russian. Keep it that way when editing or adding code.

**The project is mid-migration from Python + pygame to Rust** (plan: phases 0‒8, from a
1:1 core to a native wgpu/egui app with player-chosen world scale up to ~1M creatures). Done so
far: phases 0‒2 — the Python version moved to `python/` and stays as the reference; the Rust
workspace has the engine (`crates/life-core`), the bounded headless runner (`crates/life-sim`)
and the balance report (`crates/life-report`). There is no Rust game window yet (phase 4).

## Commands: Rust

```bash
cargo test --workspace                              # engine tests (~0.1 s)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110

cargo run -p life-report --release                  # seed 1, 600 ticks, trajectory + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000
cargo run -p life-report --release -- --rule plant_energy=80 --scale 10 --threads 4
cargo run -p life-report --release -- --compare reference/fingerprint.json   # parity with Python
```

`reference/fingerprint.json` is the Python balance fingerprint (8 seeds x 20 000 ticks, series
every 60 ticks), made by `python/fingerprint.py`. `--compare` reruns the same seeds in Rust and
checks each metric's Rust mean against the Python per-seed range. Re-take the fingerprint only
when the Python balance changes deliberately.

### Rust core: what differs from Python on purpose

- **Per-creature RNG** (`life-core/src/rng.rs`): no global generator; a child's stream is forked
  from its parent's at birth, the world has its own stream for plants and migrants. Results
  depend on the seed only — not on iteration order or thread count. Not bit-compatible with
  Python, only statistically (`--compare`).
- **Fixed grid cell** (`GRID_CELL`, `grid.rs`): a query scans as many cells as its own radius
  covers; `for_each_near` returns a superset, callers check distance. The grid stores copies of
  coordinates — valid because the queried entities do not move within the phase (herbivores
  stand still while predators move and vice versa). `alive` is always read from the entity.
- **Creatures do not see the grid**: `Vegetarian::step` / `Predator::step` take closures
  ("nearest plant", "nearest predator", "nearest prey"); tests pass plain closures.
- **World scale** (`space.rs`): scale grows the width only; everything defined per world
  (plant rate and cap, start populations, migration thresholds, report limits) is multiplied by
  `area_ratio`, so densities — and the balance — stay the same.
- Stable `id: u64` per creature (for picking and following in the future app).

## Commands: Python reference (run from `python/`)

The rest of this file describes the Python version; its paths (`life/`, `app/`, `tests/`) are
relative to `python/`.

```bash
python main.py                                      # the game: menu, setup, window

python -m unittest discover tests                   # full suite (95 tests, ~4 s, no display)
python -m unittest tests.test_simulation.TestGrid   # one class
python -m unittest tests.test_simulation.TestGrid.test_grid_matches_brute_force   # one test

python sim_report.py                                # headless balance report, seed 1, 600 ticks
python sim_report.py --seeds 1 2 3 --ticks 3000     # multi-seed summary
python sim_report.py --predators 20 --predator-speed 18 --predator-vision 800
python sim_report.py --ticks 3000 --max-work 500000000   # override the compute budget
python sim_report.py --rule plant_energy=80 --rule size_power=1.5   # world rules (life/rules.py)
```

There is no build step, no linter config, and no dependency manifest — only `pygame` is needed
(`pip install pygame`). Tests and `sim_report.py` never touch the display.

On Windows, Russian output from tests and `sim_report.py` comes out as mojibake in a non-UTF-8
console; prefix commands with `PYTHONIOENCODING=utf-8` (bash) to read it.

## Architecture

**Logic is separated from rendering, and that split is the load-bearing design decision.**
The layout follows one rule: **`life/` is the engine and knows nothing about the screen;
`app/` is the application layer** — window, screens, rendering — and the root holds only the
entry points (`main.py`, `sim_report.py`). That is what lets tests
and balance tuning run headless — orders of magnitude faster than watching the screen.

The boundary is enforced, not merely documented: `TestLayering.test_engine_does_not_import_pygame`
fails if importing `life/` ever pulls in pygame again. Entities must not grow `draw()` methods —
new drawing goes in `app/render.py`.

- `life/config.py` — every tunable constant, each with a comment explaining *why* it has
  that value.
- `life/rules.py` — `Rules`, a frozen dataclass of the world rules the game's «Лаборатория»
  exposes (plant rate and energy, mutation sigma, stat cost scale and exponents, predator
  fertility and tank). `World` owns one and every creature gets it at birth; children inherit
  `self.rules`. Changing an exponent renormalises its coefficient so the *base* genome still pays
  the same — only the steepness changes. `DEFAULT_RULES` is `config.py` bit for bit (the
  renormalising factor is exactly `base ** 0.0`); `TestRules` guards that, because any drift
  shifts every seed. Never reach for module globals in entities for these values — two worlds
  (menu backdrop and the game) live in one process.
- `life/world.py` — `World(seed, rules, n_vegetarians, n_predators, predator_speed,
  predator_vision)`: populations, `step()` (one tick), `stats()`. With default arguments it draws
  random numbers exactly as before.
- `life/grid.py` — `Grid`: uniform spatial hash for neighbour lookup, rebuilt each tick.
- `life/plant.py` / `life/vegetarian.py` / `life/predator.py` — the entities. Pure logic,
  no pygame.
- `life/genome.py` — `Genom`, a `NamedTuple` of the 7 herbivore genes, plus `PERCENT_GENES`
  (clamped to 0‒100 on mutation) and the Russian display labels `GENE_LABELS`. `Vegetarian` accepts a plain list (`config.py` and the tests
  pass lists) and converts it with `Genom(*genom)`; access genes by name, never by index. Being a
  tuple, it still supports `v.genom[i]`, which `World.stats()` relies on for averaging.
- `life/headless.py` — `simulate()`: one engine shared by tests and `sim_report.py`.
- `main.py` — entry point only: `App().run()`.
- `app/` — the game. Four modules are **pygame-free on purpose** so tests check them without a
  display (`TestAppLayering` enforces it):
  - `settings.py` — `FIELDS`, the single spec of every slider (label, hint, range, step, format,
    tab). The setup screen is built from it and `load()` clamps the settings file with it.
    `Settings` holds start conditions, rules and display prefs; `rules()` / `make_world(seed)`
    turn it into an engine world. Plant growth is a *multiplier* on the config rate, because
    `PLANT_SPAWN_CHANCE` is 2.50008, not 2.5, and would not sit on a slider grid. Saved
    atomically to `user_settings.json` (gitignored); bad files fall back to defaults.
  - `session.py` — `Session`, a running game: speed, pause, events, end states (`"extinct"`,
    `"explosion"` above `EXPLOSION_LIMIT`). `advance()` runs at most `speed` ticks within a frame
    budget (at least one), so the window never freezes. **It keeps its own RNG state** and swaps
    it in around its ticks: the menu backdrop and the seed dice also use `random`, and without
    this «Заново» would not replay the same game. `TestSession.test_same_as_headless` checks a
    paused/interleaved session against `simulate()`. `pick_creature()` lives here.
  - `history.py` — graph points every `GRAPH_EVERY` ticks: counts averaged over `DIVIDE_PERIOD`
    ticks (division happens in bursts and draws a sawtooth otherwise) plus the average genome.
    A recent window and a whole-game series thinned 2x when full. `origin` keeps the first
    average genome: the genome chart's «change from start» must not use the first point of
    whatever slice is shown.
  - `camera.py` — world↔screen transform, zoom to cursor, pan, clamp, follow. Following moves
    the camera by the target's own displacement first and eases only the remainder, otherwise a
    creature at x32 outruns the camera.
  - `theme.py` (palette, `S()` UI scaling, Segoe UI loaded by file path — `SysFont("segoeui")`
    picks the Light face — and Windows DPI awareness), `widgets.py`, `render.py`, `scenes/`
    (menu, setup, prefs, help, game with pause/end overlays) and `app.py` (window, scene
    switching, fullscreen, resize).
  - `render.py` is the only module that knows about both pygame and the entities. Draw order is
    plants → herbivores → predators, which is what determines overlap; anything whose *body*
    is outside the camera is skipped (cull by body, not centre — size is a gene and can reach
    thousands). Text goes through the `lru_cache`d `_text` (use `render.text` /
    `blit_text`, not `font.render`), and the game scene caches its chart surface. The drawn
    circle is the body: `size` and `DIAM` are diameters, while a herbivore eats anything within
    `size` of its centre.
  - All UI sizes go through `theme.S()`. `App.ui_scale()` takes the system/user scale but
    shrinks it when the window is smaller than `MIN_W x MIN_H`, so layouts are designed for a
    960x600 logical minimum.
- `sim_report.py` — CLI report over `simulate()`. Its compute budget scales with `--ticks`
  (`WORK_PER_TICK`), and the summary lists runs cut short by the budget or deadline instead of
  calling them healthy.
- `Relict/` — **frozen 2025 archive** of early prototypes. See `Relict/ПАМЯТНИК.txt`: nothing
  there is edited, refactored, "fixed" or modernised. Its bugs are part of the monument.

**Window speed is not `TICKS_PER_FRAME`.** That config constant is an engine parameter — it drives
the plant spawn loop inside `World.step()` and `FLEE_TICKS` — so changing it changes the balance.
The game's speed (`session.SPEEDS`) just calls `world.step()` several times per frame. For the
same reason anything periodic is checked per tick inside `Session._after_tick`, not per frame
against `world.tick`: at higher speeds the tick counter skips past multiples.

`tests/test_app.py` is the only test file allowed to import pygame; its pygame parts skip
themselves when pygame is missing. `App(headless=True)` draws into a plain `Surface` and
`App.frame(events, dt)` is exactly one frame, so tests drive whole screens with synthetic events
and a fixed number of frames — no `while`. `TestLayout` renders every screen at several window
sizes and UI scales and fails if a widget leaves the window, widgets overlap, a label does not
fit (`Widget.problems()`) or the game chart collapses: run it after any layout change, and add
new widgets to a scene's `widgets` list so it sees them. Tests write settings only to temp dirs.
Fonts have no glyphs for arrows and similar symbols (they render as boxes): spell keys out in
on-screen text, and draw icons with primitives (`render.draw_icon`).

To look at the UI, render screens to PNG under `SDL_VIDEODRIVER=dummy` with
`App(headless=True)` and `pygame.image.save(app.screen, ...)`.

Inside `life/`, intra-package imports are relative (`from .config import *`), so the package does
not care where it is launched from.

### Tick ordering and the death flag

`World.step()` runs: spawn plants → update predators → update herbivores → `tick += 1` →
migrate predators. Migration (`rules.predator_migration`, a period in ticks, 0 = off) adds one
predator at a world edge when fewer than `PREDATOR_MIGRATION_MIN` are left, at least
`PREDATOR_MIGRATION_PREY` herbivores exist and the world started with predators; it draws random
numbers only when it fires, and `World.migrants` counts arrivals (the session turns them into
events). A herbivore eaten by a
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

- Predators see and catch by the prey's body edge (`vision + v.half`, `DIAM/2 + v.half`), so
  their cell adds half of the largest herbivore to the radius.
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

### Predators

A predator hunts only while hungry (`energy < max * PREDATOR_HUNGRY`); a full one wanders and
does not eat. Near prey (`PREDATOR_SPRINT_RANGE` between body edges) it sprints at
`PREDATOR_SPRINT_MULT` x speed for `PREDATOR_SPRINT_COST` extra energy a tick, never stepping past
the prey. Catching by body contact is the natural pressure against giant herbivores: a big body
is easier to spot and to grab. Without sprint, predators died out in every seed; without
satiety, they ate everything and died next.

### Balance: exponents, not coefficients

Upkeep is `COEF * stat ** POWER` summed over size, speed and sight, with the speed term also
multiplied by `(size / 40) ** SPEED_MASS_POWER` — moving a big body costs more (the factor is 1
for the base genome and for predators). The *exponents* decide
whether evolution has a trade-off at all: eating radius equals size (benefit ~ size²) and search
radius equals vision (benefit ~ vision²), so cost must grow steeper — hence `size ** 2.5` and
`vision ** 2`. With shallower exponents the stats run away to infinity. Read the comment block in
`life/config.py` before changing any of these.

Current values are validated by ~3000-tick runs over several seeds: both populations survive,
predator–prey oscillation is visible, numbers stay in a playable corridor. Re-validate with
`python sim_report.py --seeds 1 2 3 --ticks 3000` after touching `life/config.py`.

The lab slider ranges in `app/settings.py` were checked with `sim_report.py --rule ...` at both
ends of every range (2000 ticks, two seeds): every run finishes, the worst case (max growth, max
plant energy, min cost) peaks near 3000 creatures at ~11 ms/tick and is caught by the session's
explosion stop. Re-check the extremes when widening a range.

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
kept inside it; the body margin is capped at half the world, so a body bigger than the world —
reachable with a cheap size in the lab — sits on the middle line instead of flipping `x_lo > x_hi`
and teleporting), which is why `move()` has no separate world-height clamp. `Predator.upkeep` is
precomputed the same way.
`TestPerformance.test_tick_budget_at_fixed_load` guards against regressions at a fixed 400/400/10
load.

One subtlety in `move()`: the nearest plant is found first and *then* discarded if it falls
outside the creature's vertical layer. Folding the layer check into the loop would search for the
nearest plant *within the layer* — different behaviour, and the creature would stop wandering.
