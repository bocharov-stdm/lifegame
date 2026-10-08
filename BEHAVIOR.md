# lifegame: the behaviour model's history

This file is the history of the behaviour model, newest stage first: what changed at each stage
and how it was checked. The current model is in `CLAUDE.md` in brief and in the code exactly
(`config.rs`, the program templates); every section below the first describes an earlier model
and is kept as a record (flocks, cannibalism as a rule and the `--rule cannibalism` commands no
longer exist).

| Stage | Section |
|---|---|
| `/15` | Review fixes of the programs (the current model) |
| `/14` | Every behaviour in blocks, flocks removed (the last `life-sweep`) |
| `/13` | Behaviour programs |
| `/10`–`/12` | The food web, the life reform and the ocean reform |
| `/9` | Plant capacity in fertility cells |
| `/8` | Review fixes, borders and inherited hunger and fear |
| `/7` | Flocks as feeding circles, battles for room |
| after `/5` | Flocking, care and territory modes |
| `/5` | Territories, food portions and shots |
| `/2` | Living flock life |
| — | The calm game profile and area selection |
| up to `/8` | Mechanics, formats, the balance check: the first model, edited in place (out of order) |

Every measurement before `/14` is of worlds with flocks (removed at `/14`, tag `flocks-final`);
none is a baseline for today.

## Review fixes of the programs (`life-behavior/15`, this stage)

Model `life-behavior/15`, format `life-report/12`. Golden and both references were last
re-recorded in `25e3bbc` (2026-10-07), covering every entry below. The balance is unmeasured at
this stage: the last sweep is `/14`'s.

### Fixes since, newest first

**Review round** (2026-10-08, `5517d54`). No behaviour change: the Re-record trigger and push, the
rules' refusal text names only what changed, the lab's body formula and food preview show the rules
«Применить» would send, `life-sweep` checks a reused result's key again at its turn, a test pins the
layer of a target picked after eating.

**The wander target** (2026-09-30 – 10-07; `actions::wander`, `after_eating`). A target belongs to
the layer it was picked in, by a wander or, since 10-07, after eating (`Mind::target_layer`): a
creature that ate in a temporary layer (a hungry dive to the surface) used to keep that target once
back in its own layer and swim out of it. After a layer change a target out of the new band is
dropped at once (09-30; before, it was walked to first). A growing body keeps its layer and its
target (10-03: the band's margin is the body, so a juvenile that grew dropped a target near its
edge), brought within the bounds (10-03) and the band (10-04) the grown body reaches, so it never
stands at a wall or circles under the target.

**Review fixes** (2026-10-07, `3507ee4`, the user: «исправляй всё»). Besides the wander target
after eating, the engine records which program decided (`Mind::decided_by`), so the flowchart
lights the path in the program that decided while the card shows the one it lives by now. The
wander fix changes golden and the references, `Mind::decided_by` golden only (through the mind's
debug print); re-recorded by the workflow (`25e3bbc`), which this commit set off unasked by naming
its marker. Outside the engine: the game, `life-sweep`, the energy test and CI (see `3507ee4`).

**Review fixes** (2026-10-04, `bca1a0b`, the user: «начинай чинить»). In both references re-taken
for this and the entries below (`4c3f760`) all 8 worlds live to 20 000 ticks.
- **A tiny founders' mix deals no diet without a share.** With shares so small that the founder's
  point rounded up to their sum, `variant_for` fell through to the last variant, share or none: a
  mix of only herbivores could deal carnivores. It now falls back to the last variant that has a
  share.
- Outside the engine: a click picks a body where the window drew it at that moment, among the
  bodies of the very frame clicked (`Frame::number`); a reference whose series stops short of its
  run's last tick is refused on loading.

**Bug-hunt fixes** (2026-10-03, `bca1a0b`, the user: «исправь всё»).
- **A hunter on its prey feeds, it does not fight back.** Alarm was told by an attack standing
  still, so a hunter that reached a prey standing still (on its centre) read as fighting back: its
  activity was «тревога» and combat let it strike past its hunt's size ratio. Now a stand counts as
  a defence (alarm, which may strike past the hunt's size ratio, `combat::defending`) only by a
  «дать отпор» block or a defence of a child; a flight still reads «тревога».
- **A parent is no ally of its child against its other child.** A hunt counted the prey's
  defending parent in sight as an ally even when that parent knew the hunter as its child too, and
  kin never strike kin: a careful juvenile left a younger sibling alone for strikes that never come.
- `DIET_OWN` gates only corpse stages, plants never: the docs now say so. Outside the engine: the
  sweep, the click, the genome chart and the flowchart (see `bca1a0b`).

**Review fixes** (2026-10-03, `bca1a0b`). Golden unchanged: none of its worlds met the engine case.
- **Standing costs no speed at `speed_power` 0.** `0 ** 0` is 1, so a standing body paid the
  speed term; the speed term of a speed or step of 0 is 0 at any exponent.
- Outside the engine: `life-sweep`'s late window, references with a cut run, `--mix`/`--diet-mix`
  overflowing shares, the density map's diet highlight, the program a creature picked after it grew
  up decided by (see `bca1a0b`).

**Review fixes** (2026-10-03, `7b1c85c`, `a3359c8`, the user: «исправляй»).
- **Combat knows kin as the decisions do.** The herd's snapshot and the decisions read the last
  tick's «щадить детей», but combat read this tick's: a parent whose setting just switched off
  struck the child the snapshot still called its own, and the reverse wasted a hunter's tick. Now
  every reader takes the last tick's (`Mind::knew_until`, copied as the tick begins,
  `Creature::remember_kin`).
- **A transfer brings any live block of the other track.** It used the duplicate's filter, which
  leaves out unconditional settings and the always-firing block (a copy of those beside the
  original would be dead), so a track that lost its «делиться», «слой» or ending wander could
  never get it back from the other one. The no-junk check still turns away a copy that would be
  dead or kill one.

**Engine review fixes** (2026-09-30 – 10-02; `662b249`, `a0baa83`, `69d7311`). Whether a creature
takes corpses is read once, by its fullness before the fight, for both feeding passes: a strike's
cost no longer opens corpses to one that claimed none (golden and the references re-recorded,
`08875fc`). An added block that kills one block while it revives another is refused as junk. A
diet that digests no plants no longer goes back to the plant it chose while young. A creature with
NaN energy starves. A hunt block after a give-up looks anew instead of getting the given-up prey
back (`Scene::prey`'s cache).

**Healing waits for a calm since it was struck** (2026-10-01, `e54bd1b`, the user's choice: «не
били»). A strike used to break the striker's calm as well as the victim's, so a hunter whom nobody
struck back never healed while it hunted — against the setting's own words, «его столько тиков не
били». Now only being struck or shot breaks it; the victims heal as before.

**Torpor gets the breath back** (2026-09-30, `f20af62`) like standing, so «winded → torpor» no
longer sleeps for ever.

**The diet edges calibrated by the user** (2026-09-29, two passes in the lab's «Питание» tab). The
omnivore strikes ×1.5 (was 1.15), smells corpses at 1.2× vision (was 1) and digests plants, fresh
meat and rot at 80/60/20% (was 70/30/5%); the scavenger digests fresh meat at 100% (was 80%); the
carnivore rot at 30% (was 10%); every diet has a juvenile gut (plants while young: 100/100/70/70%,
was only the carnivore's 70%). Then the herbivore: health ×1.1 (was 1.5) and the omnivore's ×1.05
(was 1); the herbivore's size costs the base (was ×0.85); it digests fresh meat at 10% (was 0) —
foreign food, so only a hungry one (the template's mode 3) eats a fresh corpse or hunts, and then
others fear it. All shares stay ≤ 100%.

### The stage's opening (2026-09-29), in the order it happened

A review of the block programs (`ce026a6`; the user: «исправляй, задавая вопросы») found the
interpreter sound and four things in the way of evolution or of the rules:
- **A free bluff.** Others feared a creature by the most permissive *live* hunt block of its
  program, a hunter counted a parent as its child's ally by a live defence block and expected
  strikes back by the first live fight-back block — never asking whether their conditions could
  hold. A hunt block behind «сытость ≥ 99%» scared prey off the plants for free: a behaviour with no
  catch. Now the others read what a creature's blocks did on its **last move** (`Menace`, kept in
  its `Stance`): the hunt, fight-back and defence blocks whose tests held — the decider and the
  blocks before it, whether or not their action could be done. A hunter resting or fleeing this
  tick is not feared, so an ambush is real; before its first move a creature is read by its
  program's shape. The user chose this over leaving it.
- **Programs grew by themselves**: insertions and copies (6 + 8%) outweighed deletions (8%), so
  every line filled up to 32 blocks with junk. Now a deletion is as likely as all adding kinds
  together and takes a dead block first (switched off or never reached): 11% against 5 + 6 then,
  14% with the pair and the transfer, 16% after the second review (`MUTATIONS`). The user's choice.
- **Two negations in three knocked a block out**: they landed on an «всегда». A negation now turns
  only a test with a condition; switching a block off or on is its own mutation, 3% (taken from the
  nudge, 35 → 32; the nudge is 25% now). The user's choice.
- **A hidden coupling**: a flight's burst, its truce in combat and the window's «убегает» read the
  flight's memory (`flee_ticks`), so a block whose «бежать ещё» drifted to 0 lost them. They read
  the flight block's decision now (`Mind::flight`).

Smaller: a test («видит еду», «видит растение», the weighing of prey and corpses against the
plant) no longer chooses the kept plant — only the plant block does; an ambush and torpor count as
resting in the chronicle; the behaviour window shows a setting skipped behind an earlier one of its
kind as not looked at; the drift allocates nothing.

Then, the user's ask (`15cde91`): «чтобы вследствие мутаций могли появляться сложные и устойчивые
алгоритмы и не было мусора». Four choices put to the user, all taken as recommended:
- **The drift moves a third of the numbers** a child (`PROGRAM_DRIFT_SHARE`), not all sixty at
  once: selection sees a few changes at a time instead of the sum of the noise.
- **Memory that can evolve.** No working mode had ever evolved (`_mode_share` 0): a mode needs a
  setting that switches it on and a test that reads it, two mutations with nothing to select
  between. Now the mutation «pair» (3%) inserts both at once, and the founders remember through
  modes — hunger (mode 3), an alarm (mode 1), a full tank (mode 2) — instead of the special tests
  «ещё убегает» and «отдыхает», so every line starts with a working memory a mutation can rebuild.
  The templates went from 19 to 20 blocks; the flight, the rest and the foreign food act as before
  within a tick or two (the memory expires after the last trigger: 60 ticks for hunger, 61 for an
  alarm since the second review, 200 for a full tank).
- **Structure without junk** (the user: «я сам не понял, что ты предложил, но хочу чтобы было
  лучше», so all four): a new, copied or transferred deciding block goes only where it is reached
  (above the first block that always fires); a copy is only of a live block; a «transfer» (2%)
  copies a live block of the other track (the adult's into the juvenile's or back), so what one
  stage found the other may try; a replaced action keeps the parameters of the same label and unit
  (the pace, a burst, «только если выгоднее») instead of starting over.
- **A gene of its own for the programs' rate**, `program_mutability` (eleventh, appended; base 1,
  Scale, floored by `min_mutability` at the user's word so it cannot fall to zero): the tempo of
  the body and of behaviour need not be one.
The report prints, per track, the median length, the share of dead blocks, the median number of
live mode settings and `Program::spread` — how far the biggest shape's numbers have spread (0
copies, ~0.5 random) — as `METRIC` lines, to see whether these measures work.

The tests of the mutation kinds were re-aimed by expectation (of 8000 mutations ~320 negations,
~240 switch-offs).

A second review (`1858a34`, the same day; the user: «исправь») found junk still born and a bluff
left:
- **Dead settings.** Only the first setting of a kind whose tests hold applies, so one behind an
  earlier unconditional setting of its kind never acts; it counted as live. Now it is dead: the
  window mutes it, a deletion takes it first, `_dead_share` counts it.
- **Junk still born.** An always-firing block (an unconditional wander, ambush, torpor) copied or
  inserted above the ending killed every deciding block below it; with the ending as the last
  block (every template) a new deciding block could land after it; a copy of an unconditional
  setting was dead at once; a pair could hang its mode test on a dead block or on the ending
  (bringing the dead tail back). Now a copy is of a block that can live beside the original, a
  pair's reader is live and never the ending, and any added block that would leave a block dead is
  not added.
- **Growth.** A child always has the other track, so the transfer always applied: 16% adding
  against 14% deleting. Deletion is 16% now (the nudge 27 → 25%).
- **A sated or grazing hunter was feared.** A hunt block whose tests held counted even when it could
  take no prey (a full tank; a scavenger without «есть и чужую пищу»). Now it counts only while the
  hunter could hunt.
- **The alarm ran a tick short.** The templates flee under an alarm mode of 60 ticks, which covers
  the tick it is raised and 59 after; the old flight ran 61. The alarm is 61 ticks now.

Both reviews were checked in a cloud session by `clippy`, `fmt` and the build alone; golden and the
references were re-recorded on Windows for `/15` (`8a5b1b1`).

## Every behaviour in blocks, flocks removed (`life-behavior/14`)

Model `life-behavior/14`, format `life-report/12`.

After `/13` behaviour still lived outside the blocks: genes (the layer `min_y`, `max_y`,
`layer_bound`; shooting `shooter`, `fire_preference`, `fire_reserve`; division `repro_threshold`,
`repro_share`; `care`; the flock genes), world logic (automatic healing, eating whatever it
touches, the social layer's held course and turn limit, the kept plant, the parent's aid, kin
grace, flight straight away at full speed, the nearest plant) — and the language was too weak for
complex strategies (no memory, two tests, few senses). The user: «всё должно быть зашито в блоки…
возможно придётся апгрейдить сильно, но так надо сделать». Decisions from three question rounds:
division, healing, eating on the move and care for children into blocks; the layer and shooting
as settings with their genes deleted; memory as modes; flocks removed now; three tests a block;
senses of food and prey, body and age, place; parameters of flight, hunt, plant choice and the
step's smoothness; structural mutation stays 5%; of two settings of a kind the first that applies
wins.

What changed:
- **Flocks removed** (tag `flocks-final` on the commit before): flocks, flock battles, territories,
  the social layer, kin grace, the six flock genes, the flock blocks, the game's «Стаи», the flock
  events and snapshot fields. What loners still took from the social layer became blocks.
- **The language**: blocks of up to three tests and eight parameters, programs of up to 32 blocks;
  new units (a mode's number that a nudge replaces, an angle, a tilt); new tests (prey seen within
  X% of sight, a plant seen, a corpse seen, age, winded, cold water, above / below / in its layer,
  mode K); flight with a pace and a tilt, a hunt with a distance and a chase pace, a plant chosen
  by keeping it and by profit.
- **Settings**, each with the old gene's or the world's value as its base: «слой», «плавный ход»,
  «делиться», «лечиться», «есть на ходу», «щадить детей», «стрелять», «режим». Without a setting of
  a kind a creature does not do it at all; of a kind the first whose tests hold applies, and a test
  sees what the settings above it set this tick. The templates begin with the six always-on ones,
  so they act as `/13`; a founder's layer (5–100%, a free quarter 0–100%, scavengers 50–100%) and
  the 5% shooters' «стрелять» are written into its program.
- **«Защищать детёныша»** replaced the parent's aid phase and the pair grace: a child within half
  its sight struck lately (or, while young, afraid of a threat) sends the parent at the enemy
  whatever its size, with more than half a tank, for at most 90 ticks, then a pause of 60. The
  templates defend above 60% health. Hunters count a parent in sight as the prey's ally only if its
  program defends.
- Ten genes are left: `size`, `speed`, `vision`, `strategy`, `mutability`, `maturation`, `diet`,
  `lifespan`, `cold_blood`, `burst`.
- The game: the behaviour window shows three-test conditions, the modes on and the new settings;
  the card's layer and the shooters' share come from the programs. The report prints
  `{juvenile,adult}_mode_share` and `_conditional_layer_share`.

How it was checked: each stage against the one before with a throwaway behaviour digest (ids,
positions, energy, health, age, size, plants, corpses, counters) over five configs (seed 1 default,
seed 3 four diets, seed 5 strategies ×10, seed 2 baseline ×20, seed 8 giants with shooters and
scavengers) at `clone_share=1`, so that the drift of the new numbers did not shift the seeds. The
flock removal, the language and the move into settings were **bit for bit** identical; the
defence changes behaviour and is covered by tests (the block's range, tank, duration and pause,
an interrupted episode, a world where a parent defends its child only while it knows it, hunters
not counting a parent that does not defend). Tests cover each setting (without «делиться» no
division, without «лечиться» no healing, without «есть на ходу» only what the block goes for, the
layer and a conditional layer, sparing, shooting, smoothing), the first setting of a kind winning,
modes set, seen, expiring and switched off, the new tests, the flight's pace and tilt, the hunt's
distance and pace, the plant choice, and the brute-force check of the prey and best-plant queries.
Golden re-recorded.

Validation (a `life-sweep`, 8 seeds × 20 000 ticks each): all 24 worlds survive. Baseline
conditions: carnivores hold in 7 of 8 (2.6% of the late population), scavengers in 2, late
population 1179, minimum 511. ×1 base: carnivores hold in all 8 (25.5%), late population 89, the
herbivores of seed 1 died out at the end; ×1 calm: carnivores in 7 (15.7%), 214. No working mode
or conditional layer evolved in 20 000 ticks. The tick at ×100 takes 24–27 ms against 45–49 before
(the flock layer's scans went). `/13` was never measured on its own, so what `/14` changed against
it is not separated. A mutation can now switch off division, healing or eating on the move; such
children die out.

## Behaviour programs (`life-behavior/13`)

Model `life-behavior/13`, format `life-report/11` (gene keys by name; nine of them are gone).

Behaviour itself evolves (the user's request, 2026-09-28). The hand-written strategies
(`standard.rs`, `lurker.rs`) became a **program**: an ordered list of ≤ 24 blocks «if two tests →
an action with its parameters». Each tick the first block whose tests hold and whose action can be
done decides; an action that cannot be done (no prey, no corpse, a hopeless chase) falls through.
**Settings** — eat the other niche's food, drive off rivals X times smaller at the same food, go no
farther than X% of depth past the layer — apply for the tick and let the program go on. Every
number of behaviour moved into the blocks: test thresholds, and each action's parameters (flight
memory, burst, hunt ratio, caution, patience, «only if better», pace, rest length and pause). The
behaviour genes `bravery`, `prey_ratio`, `caution`, `picky`, `rivalry`, `layer_reach`, `cruise`,
`rest`, `torpor` were deleted from the table; `burst` stays as the muscles. A creature has **two
programs**, the juvenile one while it grows and the adult one after. On division, after the clone
draw and the genes, each program mutates with `program_mutation` (5%) × mutability: a number moves,
a test or the action is replaced, a test negated, blocks swapped, copied, deleted or inserted.
The `strategy` gene is only the founders' template now and never switches.

How it was built and checked, with no world runs (the user's word):
- Stage 1 — the interpreter (`scene.rs` perception with lazily memoised queries, `actions.rs`, the
  rule list in `strategy::plan`) replaced the strategies with templates encoding them exactly:
  golden stayed **bit for bit** identical against a worktree of the previous build.
- Stage 2 — mutating programs, the new tests and actions (ambush, to the top or bottom of the
  layer, fullness, health, depth), the rule, the strategy frozen: golden re-recorded.
- Stage 3 — the game's window «Поведение» (flowcharts, this tick's path lit), the report's most
  common programs.
- Round 2 — two tracks, all behaviour in blocks, the genes deleted, the combat phase striking only
  what the program chose (no automatic retaliation), others' fear read from the hunter's program:
  golden re-recorded again. Tests cover each block, settings, both tracks, 2000 random programs
  among food, prey and threats (no panic, the creature stays in the world, no energy made), a
  world at `program_mutation=1` that diversifies and stays deterministic, and the brute-force
  checks of the threat and prey queries with per-creature hunt terms.
- Review fixes — a review of the logic found, and the user had fixed:
  - the numbers were all but frozen: a given number moved only through the rare mutation, in about
    one child of 1700, where the genes it replaced moved in every mutating child. Now every number
    **drifts** in every mutating child (`program_drift`, gauss of its nudge × mutability);
  - round 2 renewed a flight on any threat in sight, so a scared creature ran while any possible
    eater was visible — longer than the old strategy. Now only a threat nearer than the flight's
    «again» share (33%, the old flight distance) renews it, a farther one only steers it;
  - torpor had lost the old gene's guards (hungry, no food in sight), and a torpid creature still
    ate what touched it — a sleeping filter feeder at 30% of the standing upkeep. Now a torpid one
    eats nothing;
  - a hunter priced strikes back the templates' prey never gives (they fight back only an enemy at
    most 1.5 times bigger, and the fleeing never strike). Now it reads the prey's fight-back block;
  - a swap could hide a setting behind a deciding block. Settings now apply first, wherever they
    stand;
  - the flock blocks (reported food, back to the circle) never act with flocks off, yet sat in the
    templates and came with mutations. They are gone from both while flocks are off;
  - the last behaviour constants moved into blocks: how long a given-up prey is left alone, how far
    a wander target lies, the «struck» window. Caution reads 100% at the old base;
  - the window marked a block whose test held but whose action failed with «нет»; it now reads
    «не вышло», the settings have their own section, the mutation count counts only mutations that
    changed something;
  - programs are shared between relatives (an `Arc` instead of a kilobyte inside every creature)
    and keep a summary of what the world reads every tick; the report groups programs by shape with
    median numbers and prints `METRIC` lines for sweeps; a dead shot at the enemy that struck it
    (no longer a target) went.

  Golden re-recorded once more; all tests pass.

**Not measured**: balance and tick rate. The templates carry the old genes' bases, but the old
genes had spread and evolved in a population while the founders' blocks start uniform, so the
balance of `/12` is not expected to hold as is.

## The food web, the life reform and the ocean reform (`life-behavior/10`–`/12`)

Model `life-behavior/12`, format `life-report/11`.

**Food web (`/10`).** The numeric `carnivory` gene became the `diet` choice gene: herbivore,
omnivore, scavenger, carnivore (order H/O/S/C in every `DIET_*` table). Digestion of each food and
the diets' edges (strike, health, the price of size and speed, smell, the juvenile gut) are world
rules, keys `{diet}_{edge}`. Above its `picky` share of the store a creature takes only its own
food. Founders are herbivores and omnivores (70/30); meat diets arise from mutants, with their own
mutation chances towards meat and the herbivore's leaps. Corpses became the meat of the world: the
body grown since birth plus the tank; a body got for free is no meat, so energy is never made from
nothing. Combat is always on (the peaceful world and the `cannibalism` rule went just before); a
creature strikes only a chosen target, a defence or a smaller rival at the same food, and a bigger
body strikes disproportionately harder (`melee_size_power` 1.25). Flocks were switched off
(`FLOCKS = false`): founders are loners (since 2026-09-27 the base genome too).

**Life reform (`/11`).** `life_pace` gave way to `maturation` (the share of food that grows the
body) and `lifespan` (base 3000, 500–10 000 ticks, free); from 70% of its lifespan a creature
weakens linearly to 70% at 90% (speed, vision, strike, health and, since 2026-09-27, shots). A
chase that does not close in within 30 ticks is given up; `prey_ratio` base 1.5.

**Ocean reform (`/12`).** The «океаническое» depth profile (60% at the surface, a peak at 15% of
depth, then an exponential fall) is the default. A corpse has three sharp stages, each its own
food: fresh 300 ticks where it died; rot that sinks at 2 a tick while its flesh decays to the bones
by 3000; bones (10% of the meat, only the scavenger digests them) that sink at 40 and lie 5000
ticks. Below a thermocline (15–45% of depth) the water is cold, and the `cold_blood` gene (moved
by points) makes a body up to 50% cheaper and 40% slower there; it replaced the scavenger's deep
saving. Upkeep is the body and eyes plus the speed of the step actually taken. New free genes:
`cruise` (wandering pace), `rest` (the fullness to rest from), `burst` (up to ×2 speed in a chase
or flight for 20 ticks, the muscles cost standing), `torpor` (hungry with nothing in sight it
stands at 30% of its standing upkeep; food in sight or a near threat wakes it, a rest is never
torpor) and `layer_reach` (how far past its layer it goes for plants, corpses and prey).

Measured on the user's baseline conditions (24 seeds × 20 000 ticks) up to stage D: carnivores
held in 67–88% of worlds and scavengers in 38–46% depending on the stage; the burst, torpor and
the ocean default are not measured yet. The numbers per stage were in `CLAUDE.md`'s «Where the
work stands» (removed in `5528a40`; `git show 5528a40^:CLAUDE.md`). Golden digests follow the
model; the balance references are still `life-behavior/9`.

## Plant capacity in fertility cells (`life-behavior/9`)

Model `life-behavior/9`, format `life-report/9` (unchanged).

With the exponential depth profile the plant cap was one number for the whole world: creatures
grazed the rich surface, the room they freed went to seeds that fell deep, and a forest grew where
the profile promised almost nothing. Now `PLANT_MAX` (per area) is split into cells of equal
fertility — equal steps of the profile's distribution function along each axis, so cells are narrow
near the rich surface and wide in the poor deep. A cell holds at most one plant; a seed landing
in an occupied cell does not sprout. Consequences:
- a full world holds exactly the profile's shape, for any depth and width profile; a grazed band
  regrows into its own cells while the others stay put;
- growth is logistic: the more of a neighbourhood is taken, the more seeds are lost there. In
  ordinary runs plants use a few percent of the cap, so the rate barely changes; an empty world
  fills to 95% of the cap in about 1800 ticks instead of 570;
- plant energy does not affect capacity (as before `life-behavior/9`); two random numbers per
  seed as before, positions of sprouting plants unchanged.

Validated on seeds 1–8 × 20 000 ticks (`--max-work 1e15`), every run finished by itself; all
32 worlds survived:

| Mode | Alive | Median | Range |
|---|---:|---:|---:|
| base, no combat | 8/8 | 1139 | 703–1805 |
| base, combat | 8/8 | 925 | 678–1191 |
| calm, no combat | 8/8 | 732.5 | 544–1108 |
| calm, combat | 8/8 | 564 | 314–875 |

Golden digests and both references (`fingerprint.json`, `calm-fingerprint.json`) are re-recorded
for `life-behavior/9`. On Windows the giants of seed 4 stopped at size 99 (just under the tests'
threshold of 100; on Linux they passed it), while the other seeds of 1–10 pass 100 by tick 1500
and reach 132–534 by tick 2000; the giant worlds of the golden and the senses tests use seed 3 now.

## Review fixes, borders and inherited hunger and fear (`life-behavior/8`)

Model `life-behavior/8`, format `life-report/9` (unchanged).

**Found in the review of `add7f96`–`8e94b91` and fixed** (each has a regression test that failed
before its fix, in `tests/territory.rs` unless noted):
- A beaten flock left with fewer than four members did not move away: a young family's circle
  follows its members and dropped the retreat. That was 43 of 80 retreats in base and 12 of 26 in
  calm worlds (seeds 1–8). Now a young family moves to its retreat place first, then roams again.
- A battle whose remaining flocks may not strike each other (under grace after a split, both
  without territoriality, or one without adults) held them for its full 300 ticks, so none of
  them could relocate. It now ends as soon as no two of its flocks may strike each other.
- A flock without adults was drawn into a battle it could neither fight nor lose. It is left out;
  a cornered flock without adults of its own fights nobody and moves next time.
- A parent covered a growing child it no longer knew (and might hunt), while a hunter counted such
  a parent as the prey's ally only while it knew the child. Covering now follows kinship: a parent
  defends its child only while it knows it (`care`).
- A settled flock counted plants its foragers saw outside the circle as food inside, so an empty
  circle did not move while its hungry members fed elsewhere. Only plants inside count now.
- A full prey buffer (`SEEN_PREY` = 64) kept the first candidates in the grid's scan order (rows
  from the top of the view): a crowd higher up hid the prey next to the hunter. It keeps the
  nearest candidates now, so the result does not depend on the scan order (`senses.rs` test).
  Overflow is rare: at most 0.13% of hunters in base worlds with combat, none in calm ones.
- Performance, bit for bit the same world: `prepare_aid` scanned the grid with the widest vision
  in the world for every victim (9–25% of a tick at ×100); it now looks up the victim's parent and
  flockmates directly. Checked on 50 seeds × 4 worlds, two of them with combat.
- Wording: a flock is beaten when it has lost *more than* half of the adults it brought; a parent
  with `care` below 25% does not know its child even at birth (children are born at half of their
  adult size).

**Borders without zigzags.** Walking around another flock's circle, creatures turned back on 16–26%
of their moves (`social_probe`, seeds 1–8), 1.1–1.5% elsewhere. Three causes:
1. 76% of those turns: a circle pressed against the world's edge leaves a corridor narrower than
   the body. The walk around (clamped by the wall) stepped into the border's margin, the way out
   (radially outwards, clamped by the wall) stepped back, and so on every other tick. Now a side
   that the wall squeezes into the border is closed and the creature goes around the open side,
   and one pinned inside the margin slides along the wall on the side that leaves it, keeping that
   course out of every area it is in.
2. 96% of the moves around a border chased a target behind it: food, a return point in the own
   circle within the neighbour's margin, a wander point. Nobody aims at what lies behind a border
   it respects now; a member returns to the part of its circle away from the neighbour. A starving
   creature (below 25%) still ignores borders, a member inside its own circle may use all of it,
   and a moderate border is still respected only in sight of a member, so what lies behind it is a
   target again once no member is seen.
3. 23.5%: exact right angles when a walk around starts; the probe counts them as sharp through
   rounding (cos ≈ −1e−16). Left as is.

Hard circles still never overlap anything; moderate borders stay leaky.

**Members outside their circle.** With combat, 24–29% of the members were outside their circle.
Of them 50–56% foraged (97.5% saw no plant in their own circle; half were more than 400 beyond
its edge), 28–38% fled (in base worlds 77% of the members' flights were from strangers that never
chose them as a target), 5–9% chased prey, 1.5–5% walked around a border. Two inherited choices
replace world constants:
- **`forage`** (new gene, the last row, 0–100%, base 40%): below this share of its store a member
  takes food anywhere, and keeps foraging until it has 1.75 times as much (70% at the base: the
  former fixed thresholds). Its catch: a member outside is out of its flockmates' cover.
- **Fear by `bravery`** (existing gene): a stranger that could eat a creature but chose no target
  on its last move is feared only within `1 − bravery` of the flight distance; one that is hunting,
  within all of it. A brave creature lets a passer-by come near; the catch is a passer-by that
  turns to hunt when it is already close.

Both drift freely: final medians of `forage` 5–86% and of `bravery` 8–88% over the worlds, with no
runaway to either end.

### Acceptance

Seeds 1–16, 20 000 ticks, every run finished by itself (in brackets: `life-behavior/7`, same
seeds; its no-combat persistence is from the previous section):

| Mode | Alive | Flocks persist | Median | Inside, median |
|---|---:|---:|---:|---:|
| base, no combat | 16/16 | 14/16 (15/16) | 1214.5 | 0.87 |
| base, combat | 16/16 | 14/16 (13/16) | 816 (707.5) | 0.83 (0.77) |
| calm, no combat | 16/16 | 13/16 (10/16) | 848 | 0.97 |
| calm, combat | 16/16 | 13/16 (16/16) | 632 (586) | 0.80 (0.74) |

The share inside is noisy: one model with its RNG stream shifted gave 0.88 instead of 0.74 on the
same 8 seeds. Averaged over the second half of the runs it moved from 0.69 to 0.71 (base) and
from 0.78 to 0.79 (calm) with combat — a small, real gain; the final medians overstate it.

Sharp turns (`social_probe`, seeds 1–8; before → after): around a border 26.4 → 10.6% and
16.5 → 9.7% (base, without / with combat), 17.0 → 10.3% and 15.7 → 9.4% (calm); all moves 2.90 →
2.46%, 2.50 → 2.31%, 1.61 → 1.62%, 2.44 → 2.03%. Moves elsewhere are unchanged (1.2–1.6%).

## Flocks as feeding circles, battles for room (`life-behavior/7`)

Formats: `life-report/9`, model `life-behavior/7`. This section is in English; much of what
follows is still Russian.

**The circle.** A family flock of two or more members is a circle that moves as one object, and
its members feed inside it. Radius: `flock_spacing · √n`, clamped to 80–600 (`flock::MIN_RADIUS`,
`MAX_RADIUS`); `flock_spacing` is a free inherited gene (effect 50–500, base 200). Members take
plants, corpses and prey only inside the circle (its border plus half a body); a chase already
started may lead out of it. With no food in sight they wander inside the circle, and outside it
they walk back (`Mode::Return`, shown as «собираются»). A member below 40% of its store forages
anywhere it sees food and keeps foraging until it has 70% again, so it does not dart back to its
circle after every bite. A member that stays outside its circle for 300 ticks without being on
guard, fighting or foraging leaves the flock with a new label (`strays`). Removed: the old «below
85% search on your own» rule and the pull towards the neighbours' centre.

**A young family** (fewer than 4 members) roams: its circle follows the members' mean position,
never faster than they walk, and is at least as wide as they see. A new family thus forages
almost like loners; from four members on the circle is an object that moves by the flock kind.
Without this rule flocks died out early in about half of the calm worlds: a small circle feeds
worse than free search while the world is empty.

**Flock kinds** (`flock_kind`, dealt in turn among the flocking founders, 25% each):
- settled: the circle stays; it moves when mean fullness stays below 0.4 for 600 ticks or no
  member sees a plant inside for 300 ticks;
- nomadic: a slow drift along x, turning at the world's edge or after 30 ticks pushed back;
- scouts: the best fresh food report of a member (every 60 ticks, held 180); without reports a
  wander 0.5–3 vision away;
- vertical migrants: a period of 1200 ticks, half up and half down; within the layer when bound,
  between 5% and 50% of depth when free.

A circle moves at `SLOW_PACE` of its members' mean speed and waits while fewer than 60% of them
are inside. `layer_bound` (a quarter of the founders is free, by a hash, not a draw): a bound
flock's circle stays in the members' mean layer; a free creature's home band is the whole depth.
Flock kind and the layer switch are part of the family mode: a child with another one leaves.

**Overlaps.** Two circles without territoriality overlap freely. With a moderate one involved the
pair is soft: 5% of the overlap is pushed apart per tick on each side, and both shrink by 2.5% of
it. With a hard one involved the pair is strict: the non-hard circle yields fully, two hard ones
half each; whatever overlap six passes of pushing leave (a wall of the world or of the layer, a
crowd) is removed by shrinking, on every flock update, so no strict pair ever overlaps. A
squeezed circle grows back by 0.5% of its nominal radius per tick. Kinship and the grace after a
split forbid only strikes: kin circles are separated like any others.

**No room.** A circle squeezed to at most half of its radius for 60 ticks looks for room for its
full circle nearby (four rings of candidates, 2r to 8r away, in the flock's layer). If there is
some, the flock moves there, even to a poorer, deeper place.

**Borders.** The areas are the circles of moderate and hard flocks. Everyone walks around a hard
circle; a moderate one only while it sees a member of that flock (a leaky border: a sparse or too
wide circle is not respected everywhere). A member inside its own circle and going somewhere
inside it is not pushed out by a neighbour's overlapping circle. Below 25% of its store any
creature ignores borders: hunger outweighs the risk. With combat on the warning and strike rules
of the previous stages apply to intruders.

**Battles for room** (combat on only). A cornered flock, squeezed with no room nearby, fights
instead of moving: a hard one at once, a moderate one only after one such move did not help; a
flock without territoriality never starts one. The battle gathers every flock whose circle
touches the cornered one (unless under grace with it); battles that share a flock merge. Adult
fighters of territorial flocks (fuller than 25%, not wounded) strike the nearest adult of another
flock of the battle they see; a flock without territoriality only strikes back; kin and freshly
split groups never strike each other. A flock that has lost more than half of the adults it brought leaves
the battle and moves away; the others keep the place. A battle lasts at most 300 ticks, and a
flock that left one neither starts nor joins another for 600 ticks (`battle.rs`).

**Rational hunting** (combat on only). A creature hunts when the hunt pays. The hunt is worth
the meat its tank can still take in (the prey's energy times its meat efficiency, but no more
than the free room in the tank), less the strikes it expects, per tick of the chase, the fight
and the meal. It expects the prey to strike back while it is being killed, and every visible
ally of the prey to join in until the meal ends: the prey's flockmates in sight, and a parent in
sight that still knows it. The new free gene `caution` (0–100%, base 50%, the last row of the
gene table) weighs that risk: at 0 it is ignored; at the base a hunt whose expected strikes
equal the hunter's health is worth nothing; at 100% half of that is enough. A hunt goes on while
it is worth anything; a new one starts only when it is worth more than the best plant or corpse
in sight, and a full tank starts none. A chase that has not closed the gap to the prey's edge by
one of the hunter's own steps within `CHASE_PATIENCE` (30) ticks is given up, and that prey is not
chosen for `CHASE_GIVE_UP_TICKS` (180): an equally fast prey is never caught in the open, and
hunters used to follow one as long as they saw it. Gone are the fixed 90% fullness threshold for hunting and
the bites of whoever a creature bumps into: a strike needs a chosen target, a defence or a
territorial assignment. `prey_ratio` (1–5) still limits whom a creature attacks first; the risk,
not a world floor, restrains a low ratio. A reckless mutant may be born and gets beaten by the
prey's allies: that is the catch of this free behaviour gene.

**Kinship.** Family is only a parent and its growing child while the parent still knows it. A
parent knows its child until the child's body reaches `min(1, 2 × care)` of its adult size: the
base parent (care 50%) until the child is adult, a careless one only while it is small (below care 25% not even at birth: children are born at half of their adult size). Siblings,
grandchildren and grown children are strangers. Family neither strikes nor flees from each
other; members of one flock and groups under the 600-tick grace are still protected. Whom to
spare is thus inherited, not a rule of the world: with combat, care drifts to a median of
36–39%, and parents forget their children at 70–80% of their growth.

**Founders.** Every other founder is flocking. A draw used to make 3 to 15 of the 20 founders
flocking, and that lottery decided whether flocks survived a world. Territoriality is drawn for
the loners as well (50/40/10): it acts only through a flock's circle, so for a loner it is
neutral variation that a flock descending from it inherits.

**Care and growth** (unchanged from the start of this stage). A parent no longer feeds its child
every tick: the inherited `care` gene only controls protection. The base inherited share of
energy at birth is 40%. A child gets no more than its tank holds; the parent pays only the energy
passed and the fixed penalty. Growth costs 2.25 energy per unit of diameter with the tank still
`2.5 × diameter`; the food value of a corpse's grown part uses the new price. The countdown of a
lasting split holds for every separated component of three or more members; a change of the
largest component does not reset its 600 ticks.

**Observability.** Snapshots and JSON: flock circle radius (p50/p90), flocks by kind, the share of
members inside their circle, strict and soft overlaps, squeezed flocks, battles and the flocks in
them; social counters `strays`, `relocations`, `battles`, `battle_retreats`. The chronicle adds
flock moves, battles and retreats, aggregated per interval like alarms. On screen: one circle per
flock, its stroke thin, normal or thick by territoriality, orange with a warned intruder, red in a
battle; circles glide between frames like the bodies; labels «№ · members · kind» go biggest flock
first and never over another, and a circle under 14 points gets none. The flock card shows kind,
layer, circle radius and squeeze, spacing, spread, members inside and the battle.

### Acceptance

The user left the criterion to us; combat is on by default. Proposed and used: survival ≥ 7/8 in
all four modes; in each profile with combat, flocks persist (≥ 2 flocks and flocking carriers ≥
10% at the end) in ≥ 75% of the worlds; no strict overlap in any snapshot; circle radius p90 ≤
600; members inside their circle, median ≥ 80%. Loners may vanish: a flock takeover is a
legitimate outcome of combat.

Seeds 1–8, 20 000 ticks, `--max-work 1e15`, every run finished by itself:

| Mode | Alive | Flocks persist | Both lines ≥ 10% | Median | vs old reference | Inside, median (min) |
|---|---:|---:|---:|---:|---:|---:|
| base, no combat | 8/8 | 7/8 | 3/8 | 1329.5 | −9% (1464) | 0.92 (0.62) |
| base, combat | 8/8 | 8/8 | 3/8 | 748.5 | −12% (848.5) | 0.77 (0.46) |
| calm, no combat | 8/8 | 6/8 | 4/8 | 922.5 | −11% (1032) | 0.94 (0.74) |
| calm, combat | 8/8 | 8/8 | 4/8 | 511 | −17% (614) | 0.74 (0.63) |

Eight seeds cannot tell 5/8 from 7/8 apart, so the combat profiles were also run on seeds
1–48. Flocks persist in base 13/16 on seeds 1–16 and 42/48 on all; calm 16/16 and 40/48.
Without combat, seeds 1–16: base 15/16, calm 10/16. For comparison on 48 seeds (base / calm):
the prey ratio alone as the attack threshold gave 33 / 30, the old floor of 2.5 under it 42 / 33,
rational hunting with drawn founders 36 / 32. Strict overlaps: 0 in every snapshot of every run.
Circle radius p90: at most 600. Battles for room, seeds 1–8: 215 in 6 of 8 base worlds with
combat (75 retreats), 129 in 6 of 8 calm ones (24 retreats); none without combat, as designed.
The `flock_spacing` median drifts to 80–670 (the effect stays clamped at 500).

**Open: members inside their circle.** The median share over the final snapshots of seeds 1–8 is
0.77 in base and 0.74 in calm with combat, below the 80% target. It fell from 0.84 to 0.75 when
the prey ratio alone became the attack threshold, and rational hunting did not restore it: with
combat creatures are hungrier, and members forage and chase outside their circle more often.

**Sharp turns** (`social_probe`, seeds 1–8). With combat: base 2.50% (`8cced8d`: 4.07%), calm
2.44% (1.69%). Without combat: 2.90% and 1.61% (1.24% and 0.95%; `8cced8d` had no territories
without combat). In the calm profile with combat the total is above `8cced8d`, but no context
is: inside the own circle 4.0% (11.6%), around a border 15.7% (16.8%), all territory contexts
4.5% (5.4%), elsewhere 1.52% (1.48%). What grew is the share of moves in territory contexts,
30% instead of 5%: territorial flocks now persist in every calm world.

**Who kills whom** (a temporary probe, seeds 1–8 with combat, base / calm). Kills fell by 13% /
24%. Strikes without a chosen target were 4.0% / 1.1% of the kills, now none. A parent kills its
own child in 0.4% / 1.1% of the kills; the kinship check makes each of them a child grown past
what its parent remembers. Grandparents kill a descendant in 0.2%. Caution ends with a median of
53% (base) and 33% (calm) with combat, 41–50% without; `prey_ratio` ends at 3.7 and 2.8.

A ×100 world with combat ticks about 5–8% slower than before rational hunting at the same
population (400 ticks, seeds 1–2): every creature with room in its tank now values the prey it
sees each tick, allies included.

**`repro_cost`.** The plan asked for the smallest value in 10–20 that lowers the median by 15–25%
in all four modes. None does (seeds 1–8, 20 000 ticks, change vs the old references):

| repro_cost | base, no combat | base, combat | calm, no combat | calm, combat |
|---:|---:|---:|---:|---:|
| 10 | −20% | −7% | −13% | +32% |
| 12 | +2% | −7% | −21% | 0% |
| 14 | −21% | −15% | −31% | −5% |
| 16 | −26% | −20% | −34% | −10% |
| 18 | −29% | +2% | −37% | −15% |
| 20 | −31% | −16% | −47% | −29% |

(all rows come from the model just before two last small fixes: flock kinds dealt in turn, and
circles on one centre parted along x). The medians of eight chaotic worlds
are not even monotone in the price, and a higher price made flocks persist less often in the calm
profile with combat. `repro_cost` stays 10.

Balance tried and rejected (16–32 seeds × 12 000 ticks unless noted): a narrower territory rule
where loners walk around only hard circles (flocks died out in 6–7 of 8 worlds), always respected
moderate borders (flocks took over 5–8 of 8), foraging members searching like loners (flocks
survive, but only 25–50% of members stay inside their circle), a leash of circle + vision for
foragers, a faster circle (0.6 of the members' speed), other hunger thresholds, founders dealt
20/60/20 by territoriality, splitting a flock that outgrew the largest circle (flocks persisted in
72% of base worlds with combat instead of 91%).

## Flocking, care and territory modes (after `life-behavior/5`)

У половины основателей есть стайность. Среди стайных 50% не охраняют границу,
40% защищают её после 30 тиков вторжения, 10% атакуют допустимого чужака сразу.
Способность стрелять есть у 5% основателей независимо от режима территории.
Стайность, территориальность, стратегия и способность стрелять наследуются как
единый режим семейной стаи: потомок с изменившимся режимом получает новую метку.
Одиночки и их дети живут с отдельными метками. После ухода, отделения или
рождения изменившегося потомка бывшие группы 600 тиков взаимно не охотятся и
не защищают территорию друг от друга. Близкое родство защищает бессрочно.

Наследуемая забота о потомстве начинается с 50%. Достаточно здоровый и сытый
родитель может занять одно из двух мест помощников, когда собственному
невзрослому ребёнку угрожают. При контакте тел и энергии ребёнка ниже 35% он отдаёт часть
своего запаса через обычное питание ребёнка, сохраняя себе не менее 60%.
Передача происходит до жизненных расходов; новорождённый не действует в тик
рождения. Разделение стай не разрывает связь родителя и ребёнка.

Графики и сводки ограничены последними 10 000 тиками, хроника сохраняет свою
историю. Панель отдельно показывает мировой выключатель боёв и наследуемую
плотоядность, допустимый размер добычи, долю стрелков и выстрелы за окно.
Рендер можно выключить без остановки движка; при далёком обзоре тела и трупы
становятся двухпиксельными квадратами. На близком масштабе труп сохраняет
диаметр тела при смерти, а расход пищи меняет только прозрачность. Кольцо
контактного ближнего боя показано лишь у выбранного существа.

### Checking `life-behavior/5` before these changes

Профили без ручной настройки прошли seed 1–8 по 20 000 тиков с боями и без.
Все 32 прогона завершились по числу тиков, без остановки по лимиту; во всех
срезах сошлись рождения, причины смерти и численность, а числовые поля остались
конечными. Медианная численность в последнем срезе:

| Профиль | Без боёв | С боями |
|---|---:|---:|
| Базовый | 1464 | 848,5 |
| Спокойный (`cost_scale=3`) | 1032 | 614 |

По сравнению с прежней моделью население заметно выросло. Забота о детёнышах
снижает их гибель, но не объясняет весь прирост: отдельные прогоны без заботы
тоже стали многочисленнее. В нескольких мирах, особенно без боёв, стайные линии
к концу 20 000 тиков почти исчезают из-за преимущества одиночных линий.
Стабильность стай и плотность мира требуют отдельной балансировки; ради прохождения
проверок не менялись согласованные стоимость содержания, питания и боя.

Обновлены оба эталона `life-behavior/5`; они фиксируют именно эту версию модели.
Старые эталоны отклоняются до начала симуляции с указанием несовместимой версии.
Golden включает режимы стаи, родительскую заботу и сроки взаимной защиты.
На нагрузке 4000 существ/4000 растений тик занял 4,999 мс без боёв и 6,551 мс
с боями против примерно 3,9 мс у `d395fd2`; новая логика дороже, но оба
результата ниже предела 20 мс на измеренной машине.
Безэкранные последовательности при 960×600 и 1600×900 проверили дальние
квадраты, близкие тела, размер трупа, выключенный рендер и кольцо выбранного.
При той же нагрузке сборка кадра заняла 0,439 мс для тел, 0,118 мс для
квадратов и 0,001 мс при выключенном рендере; реальную частоту кадров окна
эти замеры не подменяют.

## Territories, food portions and shots (`life-behavior/5`)

Территориальная стая из двух и более участников защищает круг вокруг своего центра. Его радиус
`clamp(1,4 × разброс + 40; 120; 320)` следует за стаей. Чужое существо
обходит замеченную границу или покидает область; после 30 тиков вторжения
здоровые взрослые могут защищать её. Нападение на своего даёт право на защиту
сразу. При выключенных боях территориальная агрессия прекращается. Кнопка
«Стаи» показывает разброс и отдельный контур территории (у разобщённой стаи
контур разброса может оказаться шире охраняемой области); карточка
показывает радиус и число предупреждённых чужаков.

Порог численности стаи — 50: при превышении взрослый с края группы может
получить новую метку и уйти. Это не блокирует рождения и не меняет родство.
Редкая наследуемая способность позволяет стрелять по видимому врагу на
расстоянии до четырёх собственных диаметров. Выстрел наносит 1% собственного
диаметра урона, стоит 2% диаметра энергии и требует пяти тиков восстановления;
при контакте приоритет у ближнего удара. Нападающие по-прежнему ограничены
размером добычи, защитники могут атаковать крупного противника. Удары одного
тика применяются одновременно.

Растение отдаёт пищу за пять контактных тиков и уменьшается на экране после
каждой порции. Часть сырой пищи теряется при обработке: усваивается 44% порции
растения и 10% порции трупа до поправки генов пищеварения. Эти коэффициенты
компенсируют возросшую доступность пищи при порционном питании; прежние
коэффициенты роста, содержания и урона не менялись. Каждая смерть оставляет труп
с конечной питательной ценностью, доступный со следующего тика; едоки берут
порции по возрастанию ID. Остаток разлагается за 600 тиков. При выключении
боёв трупы и мясные цели очищаются. JSON-отчёт `life-report/8` содержит трупы,
порции пищи, выстрелы, столкновения на границе и уходы взрослых. Текущий
эталон модели имеет `life-behavior/5`; старые эталоны отклоняются до сравнения.

«Лаборатория» позволяет менять стоимость рождения и выстрела, силу ближнего
удара и выстрела, паузу между выстрелами и усвоение растений. Это общие правила
мира, а не гены: изменения действуют на уже живущих существ, не меняя их
геном. У каждой строки есть сброс; несколько строк можно отметить и вернуть
к исходным значениям вместе. Начальные коэффициенты остались прежними,
поэтому мир без ручной настройки воспроизводит старый баланс. Боковая панель
с графиками стала уже и плотнее, ползунки лаборатории помещаются в
прокручиваемом окне на экранах 960×600 и 1600×900.

После добавления регуляторов пересняты эталоны штатным `--save-reference`.
При исходных значениях их ряды по восьми seed и 20 000 тиков побитно
совпали с прежними. Во всех четырёх сочетаниях базового/спокойного профиля
и выключенных/включённых боёв выжили 8 из 8 миров; все дошли до конца.
Новые поля присутствуют в JSON, а эталон старой версии отклоняется до прогона.

### Checking the model before

Сравнение с `d395fd2`: seed 1–8, по 20 000 тиков, без преждевременной
остановки. Каждый тик проверены конечность координат, энергии и здоровья,
неотрицательное здоровье и точный баланс рождений и всех причин смерти.

| Профиль | Бои | Выжило | Медиана численности прежде → теперь | Доля резких разворотов прежде → теперь |
|---|---|---|---|---|
| Базовый | нет | 8/8 | 704,5 → 496,5 | 3,52% → 3,67% |
| Базовый | да | 8/8 | 513,5 → 623 | 4,53% → 8,76% |
| Спокойный | нет | 8/8 | 337,5 → 313,5 | 2,81% → 3,57% |
| Спокойный | да | 8/8 | 247 → 301,5 | 3,20% → 6,63% |

При боях рост медианы численности составляет 21–22%, ниже порога 25%.
Чужие движущиеся территории повышают число разворотов при боях; устойчивый
обход уменьшил их по сравнению с первоначальным прямым отступлением.
До изменения состава основателей стрелки появлялись редко: в спокойном прогоне seed 6
возникли 1416 выстрелов, а в остальных семи seed их не было. Тест с тремя
стрелками проверяет совместный залп по крупному нападающему, одновременную
смерть цели и доступность трупа со следующего тика.

Границу, обход, предупреждение, залп и кормёжку проверили последовательными
безэкранными снимками 960×600 и 1600×900 в `target/territory-shots/`.
Golden обновлён после этих прогонов; оба эталона 20 000 тиков повторно совпали,
эталон старой модели отклонён с кодом 2. Нагрузка 4000 существ и 4000 растений:
5,924 мс/тик без боёв и 8,515 мс/тик с боями против 3,659 мс/тик в `d395fd2`;
оба значения ниже 20 мс/тик. Это локальный замер, не гарантия для любого ПК.
`cargo test --workspace` (209 тестов), `cargo fmt --all -- --check`, строгий
Clippy и release-сборка окна прошли.

## Living flock life (`life-behavior/2`)

Социальная память отделена от генома (`social.rs`). Ген `sociability`
добавлен после предыдущих генов: 0–100%, база 50%. Его цена — время сбора и помощи,
конкуренция с соседями и потерянные возможности личного поиска, без отдельного
списания энергии. Воспроизводимые фазы сбора разнесены по ID: доля времени
перед новым поиском пропорциональна общительности. При энергии ниже 25%
сбор не мешает питанию; выбранное растение и начатый бой не бросаются ради него.

Ближайшие восемь видимых своих дают локальный центр и расхождение при тесноте.
Итоговый вес притяжения к локальному центру: `12 × s × min(расстояние / зрение, 1)`;
вес общей цели — 1. При движении к личной пище притяжения нет, только расхождение.
При совпадении координат направление определяется парой ID. Курс перехода
удерживается 30 тиков; спокойный поворот ограничен 1,2 радиана за тик.
Личная еда, бой и бегство поворачивают без этого ограничения. Личное растение
остаётся целью, пока живо и видно. Скорость и расходы движения не менялись.

Сведения о растении живут 180 тиков, тревога — 60. Получатель видит сообщение
на следующем тике только от непосредственно видимого наблюдателя; чужие
сообщения не ретранслируются. Готовность принять чужую находку пропорциональна
общительности и удерживается 180 тиков. Общая кормовая цель пересматривается
раз в 60 тиков, удерживается минимум 180; без свежих сведений стая исследует
новое место. Пустое место забывается, повтор того же сообщения его не восстанавливает.

Отдых длится 60–120 тиков, допускается и одиночкам, стоит медленное содержание.
Начало при сытости выше 95%, выход ниже 85%, при уходе своих или опасности.
После отдыха выдерживается 180 тиков до нового. Вне домашнего слоя сначала
возвращаются домой. Бесплатного лечения, энергии или замедления возраста нет.

Тревога действует при общительности не ниже 25%; направление к безопасным
своим не разворачивает бегущего к угрозе. Затаившийся убегает и помогает на
полной скорости. Подтверждённый удар позволяет назначить максимум двух ближайших
взрослых помощников: общительность ≥50%, сытость >50%, здоровье выше порога
отступления на 0,1. Помощь ограничена 90 тиками, 30 тиками без новых ударов и
видимостью обоих участников; после окончания — перерыв 60 тиков.
Выключение боёв немедленно очищает боевую социальную память.

Компоненты взаимной видимости проверяются раз в 60 тиков. Отделённая группа
из трёх и более участников основывает стаю после 600 тиков разлуки. Смерть
якоря или соединение с основной группой сбрасывают ожидание. Метки выдаются
отдельным монотонным счётчиком; родство от отделения не меняется. Дети в тик
рождения не действуют. Бой, питание победителя и причины смерти остались прежними.

«Стаи» показывает центр, разброс, численность и занятие большинства. Клик
открывает карточку стаи; попадание в тело имеет преимущество. JSON `life-report/5`
содержит занятия, разброс, тревоги, вмешательства и отделения; модель эталона
`life-behavior/2`. Старая модель отклоняется до сравнения. Хроника агрегирует
начала/окончания тревог и отделения между срезами, без каждого отдельного сигнала.

В golden добавлены социальная память, сообщения, курсы, таймеры, сведения о
кормовой цели, ожидания отделений и счётчик меток. Новые мутации, отдых, сообщения
и движение намеренно меняют поведение уже с первых тиков.

Проверка движения и численности: `cargo run -p life-core --example social_probe --release -- 8`.
Каждый из 32 миров проходит ровно 20 000 тиков; на каждом тике проверяются
численность по смертям/рождениям и конечность жизненного состояния. Разворот —
отрицательное скалярное произведение двух последовательных ненулевых перемещений
одного существа без боя и бегства; после остановки или рождения пары нет.
Seed считаются независимо в диагностической программе, сам движок последовательный.

### Accepting the social model

Сравнение с `3745cc3`, seed 1–8, по 20 000 тиков, без остановки по лимитам:

| Профиль | Бои | Выжило | Медиана прежде → теперь | Снижение доли резких разворотов |
|---|---|---|---|---|
| Базовый | нет | 8/8 | 810 → 704,5 | 81,84% |
| Базовый | да | 8/8 | 523,5 → 513,5 | 75,81% |
| Спокойный | нет | 8/8 | 294,5 → 337,5 | 80,48% |
| Спокойный | да | 8/8 | 216 → 247 | 76,85% |

В спокойном профиле финальные численности: 262–554 без боёв и 94–405 с боями.
Рост медиан 14,60% и 14,35% укладывается в 25%; жёсткого потолка населения нет.
Полные численности по seed и исходные показатели лежали в `reference/social-validation.json`
(снят вместе с пробой `social_probe`: стаи выключены, см. CLAUDE.md).
Тесты экранов снимают кормёжку, переход, тревогу, сбор и выбор карточки при
960×600 и 1600×900 (`target/social-shots/` при заданном `LIFEGAME_SHOTS`).

Заключительная проверка: 168 тестов прошли (служебная перезапись golden игнорируется),
форматирование, строгий Clippy и release-сборка окна успешны. Оба эталона повторно
совпали; эталон прежней модели отклонён с кодом 2. Фиксированная нагрузка 4000/4000
в последовательном замере: 1,918 мс/тик прежде, 3,659 мс/тик теперь, ниже лимита 20 мс.
Это локальное измерение времени, не гарантия для любого компьютера.

Прежние варианты с плавным поворотом при личном питании и слабой сплочённостью
давали перенаселение и не приняты. Итог меняет только социальные веса и пороги:
содержание, урон, пищевые коэффициенты, рост и возраст не перенастраивались.

## The calm game profile and area selection

В игре новый профиль по умолчанию: содержание `cost_scale=3`, стартовый темп
30 тиков/с. Кнопка «Спокойнее» применяет эти значения к текущей партии и сохраняет
стоимость для будущих миров. Старые настройки читаются как раньше; для них можно
применить кнопку. Это настройка существующих правил, формулы движка не менялись.

При 20 000 тиков на seed 1–8 выжили все миры: с боями финальная численность
72–311 (медиана 216), без боёв 153–542 (медиана 295). Это снижение плотности,
а не жёсткий потолок: результат зависит от эволюции и правил.
Эталон игрового профиля: `reference/calm-fingerprint.json`.

«Стаи» включает устойчивые цвета групп, одиночки серые. Раскраска работает
на паузе, на миникарте и карте плотности. «Убрать рамку» и Esc снимают область.
После выделения включается обычный выбор, рамка сохраняется при смене вкладок;
запоздавшая сводка снятой области игнорируется.

```text
cargo run -p life-report --release -- --compare reference/calm-fingerprint.json --rule cost_scale=3 --rule cannibalism=1 --max-work 1e15
```

Реализовано 20 сентября 2026. Исходная точка — `f209d97` и незакоммиченная работа
Claude над родством и бегством. Она сохранена и включена в новый жизненный цикл.
Параллельный тик, бенчмарк компьютера и `Relict/` не изменялись.

## Mechanics (the first model, edited in place up to `life-behavior/8`)

- Family is a parent and its child while the parent still knows it (`care`, see «Flocks as
  feeding circles»). Family and carriers of the same flock label may not be attacked. Threats are read
  from the neighbour snapshot. Coinciding coordinates give a reproducible direction; turning
  cannibalism off resets fleeing and the attack target.
- The size gene sets the adult diameter. A child is born at half of it; founders and spawned
  creatures are adult. Only digested food grows the body: the `maturation` share of it (a free
  gene, base 50%, 0–100%) at `GROWTH_ENERGY_PER_SIZE = 2.25` a unit of diameter, the rest fills
  the tank. Growth is limited by the adult size only (it used to stop at a wall until the body
  walked a diameter away from it); a body grown against an edge is pushed inside its new bounds.
  Hunger and giving birth do not shrink the body. Reproduction waits `DIVIDE_PERIOD` ticks.
- `lifespan` (replaced `life_pace`, 2026-09-27): a free gene, base 3000 ticks, 500–10 000. A tick
  is a tick of age for everyone; at its lifespan a creature dies of old age. From 70% of it the
  speed, vision, strike and maximum health fall linearly to 70% at 90% and stay there; the body
  and the tank do not change, and the upkeep follows the speed and sight it has. `life_pace`
  multiplied upkeep and age alike, and selection pinned it to its floor 0.5 everywhere: slow life
  was a free 50% discount. Рост сохраняет долю здоровья. После 60 тиков
  без боя при энергии выше половины восстанавливается 0.2% максимума здоровья
  за тик, по одной единице энергии за единицу здоровья.
- Бой начинается при контакте тел. Не более одного удара за тик: 5% своего
  диаметра, максимум 25% максимального здоровья цели. Энергетическая цена равна
  базовому урону. Все удары рассчитываются до нанесения повреждений и применяются
  одновременно; взаимная гибель допустима. Добычу получает живой участник с
  максимальным нанесённым уроном, при равенстве — с меньшим ID. Трупов нет.
- Bravery: base 50%, range 0–100%; retreat threshold `0.8 − 0.6b`.
  Carnivory: base 25%; plants are digested at `1 − 0.8c`, prey at `0.2 + 0.8c`.
  Prey holds its remaining energy and the cost of the body part it grew.
  `prey_ratio`: base 1.5 (2.5 until 2026-09-27: a base carnivore of 40 could not attack even a
  newborn of 20), range 1–5, a free behaviour gene. It alone decides whom a creature
  attacks first (a body at most `size / prey_ratio`) and, in the eyes of others, whom it
  threatens. The world rule `cannibal_ratio` (2.5) is gone: it came from swallowing prey whole
  and only set a floor under the gene. What restrains a low ratio is the risk a hunter weighs
  (`caution`): an equal and its allies fight back. Defence and territory strike regardless of
  size. A target is chosen by the meat its tank can take in, less the expected strikes, over the
  time of the chase, the fight and the meal (see "Rational hunting"). `cannibalism` turns all
  combat off.
- The flock label is kept apart from the gene table: unique for founders, inherited with a 99%
  chance. Two living carriers make a flock, and a flock has a circle (see «Flocks as feeding circles»).
  There is no cooperative hunting. Empty flocks are removed.

Порядок тика: растения → снимок и решения → движение и жизненные расходы →
питание растениями → одновременный бой → питание победителей → размножение →
удаление погибших и добавление детей. Дети не действуют в тик рождения.
Обе стратегии используют общие правила; затаившийся медленно блуждает.

## Formats and display (as at `life-behavior/8`)

Карточка показывает текущий и взрослый размер, возраст, здоровье, состояние и стаю.
Статистика показывает молодых, стаи и причины смерти. Рисование и выбор мышью
используют фактический размер. JSON is `life-report/9`, the balance reference has
`model: life-behavior/8`. Несовместимый эталон отклоняется с кодом 2 и объяснением.
Старые настройки получают новые значения по умолчанию.

Golden переснят намеренно: старое мгновенное поедание заменено боем, рост и
индивидуальные таймеры меняют размножение, добавлены выбор охоты и движение стаи.
Первое прежнее расхождение каннибализма на тике 100 вызвано уже изменёнными
решениями о родстве и бегстве. Новый отпечаток включает жизненное состояние,
таймеры, намерения, родство, метки, цели и генераторы стай.

## Balance check (as at `life-behavior/8`)

Все прогоны завершили ровно 20 000 тиков, без остановок по лимитам. Во всех
снимках проверено: начальная численность + рождения − все причины смерти =
живая численность. Стартовые согласованные коэффициенты менять не потребовалось.

| Seed | Без боёв, финал | С боями, финал |
|---|---:|---:|
| 1 | 858 | 376 |
| 2 | 762 | 461 |
| 3 | 906 | 665 |
| 4 | 683 | 797 |
| 5 | 323 | 366 |
| 6 | 1107 | 572 |
| 7 | 1066 | 475 |
| 8 | 581 | 684 |

Выживаемость — **8/8 в обоих режимах**, требование — минимум 7/8.
Промежуточные этапы также проверены на seed 1–3 с боями и без них.

```text
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 --rule cannibalism=0
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 --rule cannibalism=1
cargo run -p life-report --release -- --save-reference reference/fingerprint.json --max-work 1e15
cargo run -p life-report --release -- --compare reference/fingerprint.json --max-work 1e15
```

Итог: 150 тестов прошли, один служебный тест печати golden пропущен;
форматирование и строгий Clippy проходят. Повторная сверка нового эталона проходит,
старый эталон отклоняется с кодом 2.

Тесты покрывают рост и энергию, края мира, возраст, ожидание размножения,
восстановление, многотиковые и взаимные бои, распределение добычи, выбор целей,
наследование и исчезновение стай, воспроизводимость и сохранение численности.
Headless-тесты интерфейса проверены на 960×600 и 1600×900; карточка с 13 генами
и новая статистика помещаются. Окно для проверки не запускалось.

На фиксированной нагрузке 4000 существ / 4000 растений три чередующихся замера:
исходная версия 1.350–1.475 мс/тик, новая 1.880–2.258 мс/тик (тестовый профиль
с оптимизацией, одна машина). Медианы: 1.386 и 2.027 мс/тик.
Оба ниже предела 20 мс. Поиск соседей использует сетки, полного перебора
всех существ для каждого участника нет. Замеры не являются бенчмарком компьютера.
