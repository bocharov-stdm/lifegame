# lifegame: the model's history

Newest first: what each stage of the model changed and why. The current model is in `CLAUDE.md` in
brief and in the code exactly (`config.rs`, `Rules::default()`, the templates in
`creature/program/mod.rs`). This file keeps what explains today's model; the full records of every
stage (the review rounds, the measurements and acceptance runs of models long gone) are in git:
`git show fbb85ba:BEHAVIOR.md`.

## `life-behavior/17`: land with a character of its own (current)

Land was the ocean with its axis renamed. It gets its own features, land only: the ocean's physics
stay as they were (checked: with «темнота» out of the mutation pool the ocean's golden configs keep
their digests bit for bit). With it in, the pool grows by one test, so every seed shifts, the
ocean's too, and an ocean program may carry «темнота», which there always reads 0% dark.

- **Day and night** (the user's choice, 2026-10-16). The sun goes round in `day_length` ticks (600),
  full day and full night 40% of the turn each, dusk and dawn smooth (`world::night_at`). At night
  sight shrinks to `night_sight` (40%) of the day's while the nose keeps its reach, so the night
  is the noses' time; the eyes cost what they cost by day. Every distance a block measures in
  shares of sight shrinks with it — the wander, the flight, the hunt, the partner, the guard — and
  so does the shot's range. The night is cold: the whole land at least `night_chill` (50%) cold,
  so the cold-blooded slow down and save while the warm-blooded keep their pace — a niche through
  the existing body, no new gene. Nothing is made: the night only takes. A test «темнота» lets a
  program tell the time; the templates do not use it. The game dims the world under a veil that
  follows the night softly and says the time of day in the top bar.

## `life-behavior/16`: two parents, land, species

Golden (ten configs now) and both references re-recorded with this stage. The balance is unmeasured
until the stage's sweep.

- **Mating beside division.** A setting «спариваться» (`Action::Mate`) lets a creature mate with a
  partner touching it that is grown, mates this tick too, is no family and lies within the world's
  `mate_distance` of its genes (`MateKey`: the mean over the numeric genes of the mutation steps
  between them at the world's `mutation_sigma`, kept by every creature from birth; another diet
  never mates). The one law serves the reproduction, the sense of a partner and the species. The mother pays for the child as a
  division does; the father gives only his genes, so energy is made from nothing no more than
  before. The child takes each gene from either parent and each program track whole from either
  (`CreatureGenome::cross`) — a whole track is a working program, so the mixing bears no junk —
  and then mutates as any child. Without a partner in touch it divides, if it divides. A deciding
  block «идти к паре» and a test «пара ближе» find a partner (`Senses::nearest_mate`). The
  templates mate from half the tank, divide from 70% (both keep `REPRO_RESERVE` besides) and go to
  a partner they see from 70% fullness, so mating has a window for bodies up to the meat founders'
  and selection decides between the two.
- **Land beside the ocean** (`Terrain`, «Новый мир», `--terrain land`). The same physics: the y axis
  is latitude, the warm edge on top, a layer a climate belt, the thermocline the cold's border. The
  dead lie where they fell instead of sinking. The texts say it the land's way (`Terrain::say`).
- **Species and novelty**, observed and never inside a tick (`life_sim::species`): groups of
  creatures that could mate, followed from sample to sample, the tree of which split from which,
  the program shapes alive and new, the share of births of two parents — in the report
  (`METRIC species_*`, `shapes_*`, `mated_share`), the chronicle and the statistics' «Виды» tab.

## `life-behavior/15`: the programs reviewed

Format `life-report/12`. Golden and both references were last re-recorded in `25e3bbc`
(2026-10-07). The balance is unmeasured at this stage: the last sweep is `/14`'s.

- **Others read what a creature did, not what it might do.** Fear, a prey's allies and the strikes
  back a hunter expects come from the hunt, fight-back and defence blocks whose tests held on the
  creature's last move (`Menace`, kept in its `Stance`); a hunt block that could take no prey (a
  full tank, food it would not eat) counts for nothing. Before, the most permissive live block of
  the program counted: a hunt block behind «сытость ≥ 99%» scared prey off for free.
- **No junk is born.** A block is dead when switched off, never reached, or a setting behind an
  unconditional one of its kind. A deletion is as likely as all adding kinds together and takes a
  dead block first; an added block that would leave any block dead is not added; a new deciding
  block goes only where it is reached. A negation turns only a test with a condition; switching a
  block off or on is a mutation of its own.
- **Structure that can evolve.** A child's drift moves a third of the numbers
  (`PROGRAM_DRIFT_SHARE`), so selection sees a few changes at a time. The «pair» mutation inserts a
  mode setting and its test at once, and the founders remember hunger, an alarm and a full tank
  through modes. A «transfer» copies a live block of the other track. A replaced action keeps the
  parameters of the same label and unit. The gene `program_mutability` sets the programs' rate
  apart from the body's.
- **The mechanics fixed.** Every reader takes kin as the last tick knew them
  (`Creature::remember_kin`). A hunter standing on its prey feeds and does not count as fighting
  back; a parent is no ally of its child against its other child. Healing waits for a calm since
  the creature was struck, not since it struck. Torpor gets the breath back. Standing pays no speed
  at `speed_power` 0. Whether a creature takes corpses is read once, by its fullness before the
  fight. A wander target belongs to the layer it was picked in, and a growing body keeps its layer
  and its target. A tiny founders' mix never deals a diet without a share.
- **The diet edges** were calibrated by the user in the lab (2026-09-29): a stronger omnivore that
  digests better, a juvenile gut for every diet, a herbivore that eats fresh meat only when hungry.

## `life-behavior/14`: every behaviour in blocks, flocks removed

- Behaviour that still lived in genes and world logic became blocks. The layer, shooting, division,
  healing, eating on the move and care for children are **settings** («слой», «плавный ход»,
  «делиться», «лечиться», «есть на ходу», «щадить детей», «стрелять», «режим»): without one a
  creature does not do it, and of a kind the first whose tests hold applies.
- The language grew: up to three tests and eight parameters a block, 32 blocks a program, memory as
  modes, senses of food, prey, body, age and place. «Защищать детёныша» replaced the parent's aid.
- **Flocks were removed** (tag `flocks-final`): circles, territories, battles, the social layer and
  six genes. Ten genes were left; `program_mutability` made eleven at `/15`.
- Checked bit for bit against the step before with a behaviour digest, but for the defence,
  which is covered by tests. The validation sweep (8 seeds × 20 000 ticks) is the last balance
  measurement: all 24 worlds survived; on the baseline conditions carnivores held in 7 of 8 worlds
  and scavengers in 2. A tick at ×100 took 24–27 ms instead of 45–49 with flocks.

## `life-behavior/13`: behaviour programs

The hand-written strategies (standard, lurker) became **programs**: an ordered list of blocks «if
tests → an action with its parameters»; the first block whose tests hold and whose action can be
done decides, an action that cannot be done falls through, settings apply first. The behaviour
genes (`bravery`, `prey_ratio`, `caution`, `picky`, `rivalry`, `layer_reach`, `cruise`, `rest`,
`torpor`) became the blocks' numbers; `burst` stayed as the muscles. Two tracks, juvenile and
adult. A child's programs drift and now and then mutate; the `strategy` gene only picks the
founders' template. Combat strikes only what a program chose. Programs are shared through an `Arc`
and keep a `Summary` of what the world reads every tick. The interpreter came in bit for bit
against the strategies it replaced.

## `life-behavior/10`–`/12`: the food web, the life reform, the ocean reform

- **Food web.** The numeric `carnivory` became the `diet` choice gene: herbivore, omnivore,
  scavenger, carnivore. Digestion and the diets' body edges are world rules (`{diet}_{edge}`). The
  founders are herbivores and omnivores; the meat diets arise from mutants. Corpses are the
  world's meat: the body grown since birth plus the tank, never a body got for free. Combat is
  always on (the peaceful world and the `cannibalism` rule went), and a bigger body strikes
  disproportionately harder.
- **Life.** `life_pace` gave way to `maturation` (the share of food that grows the body) and
  `lifespan` (weakening from 70% of it). A chase that does not close in is given up.
- **Ocean.** The «океаническое» depth profile is the default. A corpse is fresh where it died,
  then rot that sinks and decays to bones, which only the scavenger digests. Below a thermocline
  the water is cold, and `cold_blood` makes a body cheaper and slower there. Upkeep is the body
  and eyes plus the speed of the step actually taken.

## `life-behavior/9`: plant capacity in slots

`PLANT_MAX` (per area) became slots, one plant each, laid out by fertility: a full world holds the
profile's shape, a grazed band regrows into its own slots, and growth is logistic. Patches came
later (`plant_patches`).

## Before `/9`: the first model

Edited in place up to `/8`: flocks with feeding circles, territories and battles for room, a social
layer of messages, alarms and aid, a parent feeding its child, combat as a world switch, predators
as a kind of their own, behaviour as genes. All of it is gone or rebuilt above: the predators
(tag `predators-final`), the flocks at `/14` (tag `flocks-final`), the rest at `/10`–`/13`. What
lasts from it: strikes collected before damage and applied at once (mutual death is possible), the
dead neither eat nor divide, children do not act on their birth tick, food in portions, kin as a
parent and its growing child, death counters that add up with the population, neighbour queries
through grids, and the calm profile (`cost_scale=3`, the «Спокойнее» button,
`reference/calm-fingerprint.json`). The Python prototype is at the tag `python-final`.
