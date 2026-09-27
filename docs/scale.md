# The world in real units

The mechanics run in pixels and ticks. `life_core::units` translates them for the player (the
creature card, the «Новый мир» preview) with three constants in `config.rs`. Nothing in the
simulation reads them.

| Constant | Value | Chosen from |
|---|---|---|
| `CM_PER_PX` | 0.5 | the base body (`size` 40) is a 20 cm fish |
| `SECONDS_PER_TICK` | 0.25 | the base speed 10 is a body length a second, a cruising fish |
| `TICKS_PER_YEAR` | 1000 | the base lifespan 3000 ticks is three years, a small fish of that size |

There are two clocks because no single one fits. A tick is a quarter of a second of swimming and
about nine hours of life. The life cycle is compressed about 126 000 times more than swimming.

## What our numbers are

| | Game | Real units | Real seas | Verdict |
|---|---|---|---|---|
| Base world ×1 3:2 | 6000 × 4000 px | 30 × 20 m | — | a pond |
| User's world ×20 2:1 | ~31 000 × 15 500 px | ~155 × 77 m | coastal sea: bottom at 20–100 m | coastal, not open ocean |
| Body | 40 px, evolves to ~85 | 20 cm, ~40 cm | small to medium fish | fits |
| Speed | 10 px a tick | 20 cm/s, 1 body length/s | cruising 1–2 body lengths/s, bursts to 10 | fits cruising; no bursts yet |
| Vision | 400 px | 2 m | turbid coastal water 1–5 m, clear ocean 20–30 m | murky water |
| Scavenger's smell | 3 × vision | 6 m | a scent plume reaches hundreds of metres down-current | far too short; no currents |
| Plants (default «игровое», ×20) | full to 20% of depth | full to ~15 m, then falling | coastal photic zone 20–50 m | a little shallow |
| Plants («океаническое», ×20) | peak at 15% | ~12 m | deep chlorophyll maximum 10–30 m on shelves, 50–150 m in the open ocean | coastal |
| Corpse sinking | 2 px a tick | 4 cm/s | a dead fish sinks about 5–20 cm/s once it loses its gas | a little slow |

## Where the clocks break

- **Movement over a lifetime.** A real 20 cm fish swims thousands of kilometres in three years.
  Ours covers 3000 ticks × 5 cm = 150 m, about one width of the user's world. A life is a few
  swims across the world, not a migration.
- **Corpses fit neither clock.** A corpse stays fresh for 150 ticks. By the swimming clock that is
  37 seconds; by the life clock it is 55 days. A real carcass stays fresh for hours to a few days
  in cold water. Corpse times are tuned for the balance, not for either clock.
- **Growth and division.** By the life clock a newborn is grown in weeks, fast but not absurd for
  small fish. Division gives one big offspring. Real fish spawn thousands of eggs, and almost all
  of the young die.

## Why not make the mechanics real

A world as deep as the real ocean at real fish sizes would be hundreds of times bigger (×100 000
in area for a 1000 m ocean). The tick grows with the population, so it cannot run. The world stays
a coastal sea, and the units only name what the numbers already mean.
