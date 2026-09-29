# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

lifegame — an evolutionary sandbox: plants and creatures in a 2D world. Creatures carry
a genome (a gene table) that mutates on division; selection is emergent, not scripted. Rust only:
engine `crates/life-core`, bounded headless runner and observer `crates/life-sim`, balance report and
sweeps `crates/life-report`, the game (wgpu/egui) `crates/life-app`. Open: phase 3, a parallel tick
(design, no code: `docs/phase3-parallel-tick.md`, written against `life-behavior/7`), and phase 6, a
machine benchmark. Removed things live under git tags: the Python version (`python-final`, the
behavioural spec the game was ported from), predators (`predators-final`; old names still read:
`--vegetarians`, `--veg-mix`, settings key `n_vegetarians`), flocks (`flocks-final`: flocks, flock
battles, territories, the social layer, kin grace).

**Language.** Code, comments, docs, CLI/report output, test names and commit messages in
**English**; translate Russian comments and test names you touch, don't mass-rewrite. Only the
**game UI stays Russian**: window and button texts, settings labels and hints, gene
`label`/`about`, chronicle texts (`life_sim::observe`, which the report prints too).

**The user's standing rules**
- **Energy is never made from nothing.** It enters the world only in plants and then only passes
  along the chain (plant → eater → corpse → eater), losing some. No mechanic, bonus, rule or lab
  limit may create or duplicate it: digestion ≤ 100% of a food (`Rules::with` rejects more), deep
  saving ≤ the whole upkeep, a body got for free (a founder's, a newborn's birth size) is not meat.
  Check every new mechanic against this.
- **Fix niches through the body, evolutionarily**: a diet may have a different body with its own
  advantages, never free energy. Don't adopt a balance change without the user's word; before a big
  mechanic change ask rounds of questions with the risk of each option.
- **Behaviour is free**: no upkeep for a block, nor for anything that gives no physical stat. A
  behaviour needs a behavioural catch, not a price (the free «испуг»/«голод» genes once ran away and
  were removed). No behaviour genes are left: all behaviour is blocks of the programs, and a new
  behaviour is a block, never a gene or a world constant.
- **Flocks are gone** (tag `flocks-final` on `83cdf72`, the user's call 2026-09-28): the flock,
  battle, territory, social and kin-grace code, the six flock genes, the flock blocks and the game's
  «Стаи». What the social layer still did for loners became blocks: the held course and the limited
  turn (setting «плавный ход»), the kept plant (a flag of «к растению»), the parent's cover
  («защищать детёныша»); the pair grace went.
- **Commit straight to `main`**, only when asked; push only on the user's word. `main` also moves
  from cloud sessions: `git fetch` and fast-forward before starting.
- **In a cloud (Linux) container run no simulations and no tests** — no `life-report`,
  `life-sweep`, `life-app`, `cargo test`. The user's world is Windows (libm differs in the last
  bits). Fine there: reading, editing, `cargo build`, `fmt --check`, `clippy`; say which checks
  were not run.
- `Relict/` is a frozen 2025 archive: never edit, fix, modernise or translate anything there.
- The user keeps a short PDF of genes, strategies and diet edges (a throwaway fpdf2 script, Arial
  for Cyrillic); regenerate and send it after diet or gene changes.

## The model (`life-behavior/15`: every behaviour in blocks, on the ocean reform)

A creature eats plants and corpses in portions, grows from food up to its size gene, divides when
grown and its program's «делиться» says so, and dies of hunger, in a fight or of old age. Combat is
always on: a creature strikes (and shoots, under «стрелять») only what its program chose (prey, a
rival at food, a fight back, its child's enemy); being struck strikes back only through a
fight-back block. Melee damage is 5% of the striker's size
× (its size / the target's) ** `melee_size_power` (1.25) when it is the bigger (`Phenotype::strike_on`;
hunters weigh prey and retaliation with the same function); a strike's energy cost does not
depend on diet bonuses. Shots are weak (1% of size, ≤ ¼ of max health) and expensive. Contact is
the sum of the two radii. A hunt block weighs the meat the tank can take against the strikes it
expects (its caution, and only the strikes the prey's own program and a defending parent in sight
would give back) and gives up a chase that has not closed the gap by one of its steps in its
patience (template `CHASE_PATIENCE` 30 ticks), leaving that prey alone for its block's time
(template 180). What a creature does and when is its behaviour program (below). Exact mechanics and
their checks: `BEHAVIOR.md` (partly stale, see below).

**Behaviour programs** (`creature/program.rs`, the user's design, 2026-09-28; round 3 moved every
behaviour into them). Behaviour is an ordered rule list of ≤ 32 blocks «if TEST and TEST and TEST →
ACTION(parameters)» (`Block { when: [Test; 3], action, args: [u16; 8] }`). Each tick the
**settings** apply first, wherever they stand (a swap cannot hide one behind a deciding block): of
each kind the **first** whose tests hold applies and a later one of that kind is skipped
(`Block::setting_kind`; each mode number is a kind); a test sees what the settings above it set this
tick. They set this tick's `Stance`, which the world's phases read; without a setting of a kind its
default is «nothing» — own food only, no rivals, any distance, the whole depth, no smoothing, no
division, no healing, no eating on the move, no children known, no shooting. Then the first
deciding block whose tests hold and whose action can be done decides; a failed action (no prey, no
corpse, a hopeless chase, not at home to rest, no child in need) falls through; nothing decides →
it stands. `Mind` keeps what the window shows: `fired`, `applied` and `tried` (tests held, action
could not be done).
- **Two tracks**: `Creature::programs[JUVENILE]` while it grows to its size gene, `[ADULT]` after
  (`Creature::stage`, `program()`); inherited and mutating apart. `Programs` is an `Arc`: children
  that inherit them unchanged share them, so a creature holds a pointer, not a kilobyte. Each
  `Program` keeps a summary (reach, threat range, hunt ratio, defence, defends, wander reach, home
  layer, shoots), recomputed whenever it changes: its own tests, the report and a founder's
  placement read it; the others read the creature by its last move (`Menace`, below).
- **Settings** (bases = what the deleted genes and the world did, so the templates act as `/13`):
  «есть и чужую пищу», «гнать соперников у еды» (×1.5 smaller), «за едой из слоя» (X% of depth) as
  before; «слой» top 5%, bottom 100% (swapped if reversed; was `min_y`, `max_y`, `layer_bound`) —
  where it wanders, rests and walks back to, `Phenotype::band` adding the body's margins; «плавный
  ход» a course held 30 ticks, a turn ≤ 1.2 rad (69°) a tick (the social layer's smoothing, `steer.rs`);
  «делиться» from 70% of the tank, the child 40% (at least 1%; was `repro_threshold`, `repro_share`);
  «лечиться» with a tank above 50% and 60 ticks unstruck (automatic healing; the rate
  `config::HEAL_SHARE` 0.2% of health a tick stays physiology, paid from the tank); «есть на ходу»
  plants, corpses, while fullness ≤ 100% (whatever it touched; without it it eats only what its
  deciding block goes for — the food blocks always eat their own food); «щадить детей» until the
  child has grown to 100% (was `care`: `Kinship::knows_until`); «стрелять» from 50% of its range,
  keeping 50% of its tank (was `shooter`, `fire_preference`, `fire_reserve`); «режим» K (1–4) on
  for N ticks (60; 0 = off) — the program's memory (`Mind::modes`, both tracks read the same).
  Healing and the kinship read the last tick's stance (healing at the start of `step`, the
  neighbour snapshot); eating, combat, shots, defence and division read this tick's.
- **Tests** (`Cond`): fullness, health, depth ≥ X%; a threat / a hunting stranger closer than X% of
  sight (`Senses::threats_near`); struck within X ticks (template 1); still fleeing; resting; food
  seen; prey its hunts would take within X% of sight (`Senses::nearest_prey`); a plant / a corpse it
  eats seen; age ≥ X% of its lifespan; winded; the water ≥ X% cold; above / below / in its layer;
  mode K on; always (negated: never, which switches a block off).
- **Actions** with parameters (`Action::params`, a `ParamSpec` each: unit, range, base, nudge; an
  index — a mode's number — does not drift, a nudge picks another; an angle is centiradians shown in
  degrees; a tilt is 100 = straight): fight back (enemy at most ×1.5 bigger), flee (keep running 60
  ticks after losing it, burst; under way only a threat nearer than 33% of sight — the old flight
  distance — renews the flight, a farther one only steers it; pace 100%, tilt straight), hunt (prey
  ×1.5 smaller, caution 100% = the old base weight, patience 30 ticks, only if better than plants
  and corpses, burst, leave given-up prey alone 180 ticks, prey within 100% of sight, chase pace
  100%), to a corpse (only if better, pace), to a plant (pace; keeps the plant it chose while it
  stays visible; the nearest, or the most profitable `Senses::best_plant`), wander (pace, targets a
  quarter to 200% of sight away; the next target after a bite as far), ambush (stand), to the top /
  bottom of its layer (pace), rest (90 ticks, then a pause of 180), torpor (stands paying
  `TORPOR_UPKEEP` 30% of its standing upkeep and **eats nothing**, not even what touches it — no
  sleeping filter feeder), defend its child, and the settings. Labels read in their units («33%
  зрения», «31 тик», plural by `program::ticks_word`).
- **«Защищать детёныша»** (replaced the aid phase and the pair grace): its child within 50% of its
  sight, struck within 30 ticks by an enemy still in sight — or, while young, afraid of a threat it
  sighted (`Mind::alarm`) — makes it go for the enemy and strike it whatever its size
  (`Stance::defending`, `Senses::child_in_need`), with a tank above 50%, for at most 90 ticks, then
  a pause of 60; an episode another block interrupts ends with the pause too (`Mind::aid`,
  `aid_cooldown`). The child is its own only while its «щадить детей» holds. Hunters count a parent
  in sight as the prey's ally only if its defence block's tests held on its last move (`Menace`).
- **No behaviour genes and no world behaviour constants are left**: `/13` deleted `bravery`,
  `prey_ratio`, `caution`, `picky`, `rivalry`, `layer_reach`, `cruise`, `rest`, `torpor`; `/14` the
  layer (`min_y`, `max_y`, `layer_bound`), shooting (`shooter`, `fire_preference`, `fire_reserve`),
  division (`repro_threshold`, `repro_share`), `care` and the six flock genes (the user's calls,
  breaking append-only). `burst` stays: it is the muscles, a body stat with a price; the flee and
  hunt blocks decide whether to use them.
- **Templates**: the `strategy` gene («происхождение») picks the founders' template and never
  switches (`STRATEGY_SWITCH_CHANCE` 0); both tracks start from it. `Program::STANDARD` carries the
  old genes' and the world's bases: always the layer, smoothing, division, healing, eating on the
  move and sparing its children; below 30% fullness eat foreign food and drive rivals ×1.5 smaller;
  fight back above 50% health; flee a hunter within 33% of sight, a calm stranger within 16%, keep
  fleeing; defend a child above 60% health; hunt; rest from 95%, keep resting to 85%; corpse; plant;
  wander. `Program::LURKER` is the same, wandering at 33% of its speed.
  `Program::founder(strategy, layer, shoots)` gives a founder its layer (5–100%; a quarter of them,
  by a hash, 0–100%; scavengers 50–100%) and the 5% shooters «стрелять» after the leading settings;
  a founder is placed in its program's first unconditional layer.
- **Heredity** (`CreatureGenome::inherit`): one clone draw for genome and programs (a clone shares
  them); else the genes mutate, then each program **drifts** — every number but the flags and the
  indices moves by gauss(0, its nudge × `program_drift` (rule, base 1) × mutability): a threshold ~10
  points, a ratio 0.2, a time a quarter of its base, as the genes drift by 30% (through the rare
  mutation alone a given number moved in one child of ~1700, and the old genes' adaptations could not
  happen) — and with `program_mutation` (rule, base 5%) × mutability gets one mutation: nudge a
  number 32%, replace a test 12%, negate a test with a condition 8%, replace the action 8%, swap
  with a neighbour 15%, duplicate 6%, delete 11% (a dead block first — off or never reached; keeps
  ≥ 1), insert a random block 5%, switch a block off or on 3% (its first «всегда» becomes «никогда»
  and back). A deletion is as likely as a copy and an insertion together, so programs do not grow by
  themselves. One that cannot apply (no test with a condition to negate) or lands where it was
  changes nothing, its draws spent. `Program::changes` counts the mutations that changed
  something (not the drift). Past its end a program is filled with the same block, so equal blocks
  are equal programs. A mutation can switch off division, healing or eating on the move: such
  children die out — the catch is the consequence.
- **Free behaviour**: no upkeep, no energy made — an action only chooses where to step or how to
  stand and a setting what the phases may do; `act`, combat, division and healing pay as before.
  Others read a creature by what its blocks did on its **last move** (`Menace` in `Stance::menace`,
  `Creature::menace`, `Herd`): the hunt, fight-back and defence blocks whose tests held — the
  decider and the blocks before it, whether or not the action could be done. They fear it by the
  most permissive such hunt (a hunter resting or fleeing this tick is not feared; a hunt block
  behind a condition that never holds is no bluff); a hunter expects strikes back as the first such
  fight-back block gives them (a hunter within its ratio, for the share of the killing strikes
  above that block's health threshold) and counts a parent as an ally only by such a defence
  block. Before its first move a creature is read by its program's shape (`Program::hunt_ratio`,
  `defence`, `defends`). The templates' prey never strikes a template hunter, which is ≥ 1.5 times
  bigger.
- The interpreter: `scene.rs` (perception, lazily memoised queries), `actions.rs` (one function per
  action and setting), `steer.rs` (the step: the band, smoothing), `strategy::plan` (settings, then
  the deciding blocks, then the step). `Cond`, `Action` and their parameter tables are append-only,
  like gene tables (`/14` broke it once, dropping the flock actions). The game shows both tracks as
  flowcharts (card button «Поведение (B)», `life-app/src/behaviour.rs`): the modes on in the header,
  the settings as their own section on top, then the decisions, this tick's path lit with «не вышло»
  where a block's action failed. The report groups each track by shape (`Program::shape`: tests,
  actions, flags and indices, no numbers), prints the three most common with the medians of their
  numbers (`Program::median`) and `METRIC` lines for sweeps (`{juvenile,adult}_shapes`,
  `_template_share` (the templates' and the shooting founders' shape), `_hunt_ratio`, `_threat_range`,
  `_mode_share` (a working «режим»), `_conditional_layer_share` (a «слой» under a condition)).

**Diets** — the `diet` choice gene, variant order H/O/S/C (the order of every `DIET_*` table in
`config.rs`). Digestion (`DIET_DIGESTION`: plants, fresh meat, rot, bones; 0 = neither eats nor
goes for it) and the edges are world rules (`Rules::diets`, `DietEdges`, keys `{diet}_{edge}`, lab tab
«Питание»), all read in `Phenotype::of`:

| | strike | other edges |
|---|---|---|
| herbivore | ×1 | health ×1.5, size term of upkeep ×0.85 |
| omnivore | ×1.15 | eats everything |
| scavenger | ×1.3 | plants 15%; smells corpses at 3× vision (free); alone digests bones (90%); founders start deep and fully cold-blooded |
| carnivore | ×3 | plants 20%; speed term of upkeep ×0.5; smells corpses at 1.5× vision; juvenile gut |

- Juvenile gut: until grown to its size gene a creature digests plants at
  `max(plants, young_plants)` (`DIET_YOUNG_PLANTS` 0/0/0/0.7, «Растения в детстве»; 0 = as grown).
  A carnivore mutant is born half grown and small prey is rare, so it grows on plants and hunts
  grown. Only the grown divide, so staying young is no loophole.
- **Own niche**: without its program's «есть и чужую пищу» setting a creature takes only its own
  food — a scavenger skips fresh corpses and does not hunt, a carnivore skips rot and bones
  (`Phenotype::corpse_efficiency(stage, foreign)`, `DIET_OWN`); the templates set it below 30%
  fullness. Anyone who eats fresh meat (`hunts`) and whose hunt block's tests held on its last move
  is feared.
- No meat founders by default (`DIET_START_MIX` 70/30/0/0): they starved with nothing to eat. Meat
  diets arise from mutants (see mutation laws). Meat founders set in a mix start
  `meat_founder_size` (×2) bigger.
- **Corpses**: meat = the body grown since birth at `GROWTH_ENERGY_PER_SIZE` + the tank
  (`corpse::meat`); a creature with no meat leaves no corpse. Three sharp stages (`corpse::Stage`,
  each its own food column), clock in rules `corpse_*` (`CorpseClock`): **fresh** 300 ticks where it
  died; then **rot** at once, sinking at 2 a tick (a speed, not a time) to its own resting place in
  the lowest 25% of the depth while its flesh decays evenly down to the bones by 3000 ticks from
  death; **bones** (10% of the meat) when the flesh is eaten or rotted away, sinking at 40, lying
  5000 ticks. Bones at 20 000 made scavengers as strong as carnivores (24 seeds: both held in 58%);
  the user chose 5000.
- **Cold deep** (replaced the scavenger's deep saving): warm water down to `thermo_top` (15% of
  depth), cold from `thermo_bottom` (45%), a smooth step between. The `cold_blood` gene (%, base 0 =
  warm-blooded) makes a body cheaper (−50% at 100% in full cold, `COLD_SAVING`) and slower (−40%,
  `COLD_SLOWING`) in cold water (`Phenotype::temper`, applied in `act` at the current depth). It
  moves by points (`Mutation::Shift`, ±10), since a factor never leaves zero.
- **Layer and reach**: the program's «слой» setting is its home band; «за едой из слоя» (% of
  depth; without it anywhere, and the templates have none) is how far past it a creature goes for
  food it sees, checked inside the plant, corpse and prey choices (`Taste::admits`), so it takes the
  best food within reach. A behaviour, not a wall.
- **Paying for the step taken**: upkeep = the body and eyes (`still_upkeep`) + the speed term for
  the step actually taken (`Phenotype::step_cost`); standing, eating or resting costs no speed.
  `upkeep` is the full-speed figure, for showing and weighing.
- **Movement**: the pace of wandering, fleeing, chasing and of going to plants, corpses and the
  layer's edges is each block's pace parameter (% of speed, at least `MIN_PACE` 10%). Resting
  (`Action::Rest`: stands at home up to its length, then its pause without resting; it ends when
  its block no longer decides) and torpor (`Action::Torpor`, `Creature::torpid`, «в оцепенении» on
  the card; placed after the food and flight blocks it sleeps only with nothing to do) are blocks.
  `burst` (gene, ×1–2, base 1) — in a chase to a goal beyond a step or in flight, when its block
  asks for it, it goes that much faster for ≤ 20 ticks, then 60 winded (`Creature::dash`,
  `winded`); the muscles cost standing (¼ of the extra speed's term, `BURST_UPKEEP_SHARE`), so a
  slow body with a big burst is no free speed gene.
- **Life**: `maturation` (%, base 50) — the share of digested food that goes into growth until
  grown, the rest into the tank. `lifespan` (base 3000, 500–10 000 ticks) — death of old age; from
  70% of it speed, vision, strike and max health fall linearly to 70% at 90% (`phenotype::vigour`,
  `Creature::grow_old` at the start of the tick). Both free genes.
- Plants grow in patches over the depth profile «океаническое» (the default since the reform; see
  "Where food grows"). A settings file saved before the reform takes the new defaults of the
  profile and the corpse times and keeps the player's other values (`DEFAULTS_VERSION`).

## Where the work stands

- **Review fixes of the programs** (2026-09-29, model `/15`, the user: «исправляй»): the others
  read a creature by the blocks whose tests held on its last move, not by its program's shape (a
  free bluff before: a hunt or defence block behind an impossible condition); a deletion as likely
  as a copy and an insertion together (11% against 6 + 5), the dead blocks deleted first; a
  negation only of a test with a condition, switching a block off or on its own 3% mutation; a
  flight's burst, its truce in combat and «убегает» by the flight block's decision (`Mind::flight`),
  not by its memory (a «бежать ещё» drifted to 0 lost them); a test («видит еду», «видит растение»,
  the weighing of prey and corpses against the plant) no longer chooses the kept plant; an ambush
  and torpor count as resting; the window shows a setting skipped behind one of its kind as not
  looked at; the drift allocates nothing. **Not run** (cloud): tests, golden, runs — golden and the
  references are to be re-recorded on Windows; the balance is unmeasured.
- **Round 3: every behaviour in blocks, flocks removed** (plan
  `~/.claude/plans/starry-jumping-shannon.md`, 2026-09-28, model `/14`). Stages: A the flock layer
  removed (tag `flocks-final`), B the language (three tests, eight parameters, 32 blocks, units, the
  new tests and modes, flight/hunt/plant parameters, the first setting of a kind wins), C1 the layer,
  smoothing, division, healing, eating on the move, sparing and shooting moved into settings and
  their genes deleted, C2 «защищать детёныша» replacing the aid phase and the pair grace, D the
  window, the report, the docs. A, B and C1 were proven **bit for bit** (a behaviour digest at
  `clone_share=1` over five configs, against the stage before); C2 changes behaviour and is covered
  by tests; golden re-recorded. Validation (sweep 2026-09-28, 8 seeds × 20 000): all 24 worlds
  survive. Baseline: carnivores hold in 7 of 8 (2.6% late), scavengers in 2, population late 1179,
  minimum 511, 1.74 ms/tick. ×1 base: carnivores hold in 8 (25.5% late), late population 89,
  herbivores died out at the end of seed 1; ×1 calm: carnivores in 7 (15.7%), 214. No working mode
  and no conditional layer evolved (their shares 0). Tick rate at ×100 (1000 ticks, seeds 1 2, one
  thread): 24–27 ms against 45–49 at `83cdf72` — the flock layer's scans were half the tick. `/13`
  itself was never measured, so `/13` → `/14` is not separated.
- **Behaviour programs** (same plan, 2026-09-28): stage 1
  (the interpreter replacing `standard.rs`/`lurker.rs`) was proven bit for bit against the old
  strategies; stage 2 (mutating programs), stage 3 (the window, the report) and round 2 (two tracks,
  all behaviour in blocks, nine genes deleted) made the model `/13`, golden re-recorded on Windows.
  A review of the logic (the user: «исправь») then brought: the numbers' drift (without it they were
  all but frozen), a flight renewed only within 33% of sight (round 2 had renewed it on any threat
  in sight, a longer flight than the old strategy's), a torpid creature that eats nothing, hunters
  weighing the prey's own defence, settings applied wherever they stand, no flock blocks while
  flocks are off, the last behaviour constants moved into blocks (the given-up prey's time, the
  wander's reach, the «struck» window), shared programs with a cached summary; golden re-recorded
  again. **Unmeasured**: no world runs were made (the user's word) — balance and tick rate are
  unknown; the templates carry the old gene bases, but the old genes had spread and evolved, the
  blocks start uniform. The drift, the torpor and the hunter's weighing are balance changes.
- **Ocean reform** (plan `~/.claude/plans/snug-puzzling-pixel.md`, 2026-09-27, all stages in):
  A ocean profile + real units (bit for bit), B corpse stages, C cold deep and genes, D paying for
  the step taken with `cruise` and `rest`, E burst, F torpor, G the ocean profile as the default,
  golden digests re-recorded, model `/12`. The user stopped the measurements after D ("не тестируй
  а делай изменения"): E, F and the ocean default are **unmeasured**. Baseline conditions, 24 seeds
  × 20 000:
  - before B: carnivores hold in 80%, scavengers 20% (35 seeds), late population 1320;
  - B (bones 5000): carnivores 71%, scavengers 46%, 1424; the corpse count rose from ~1000 to
    6000–21 000 at bones 20 000;
  - C: carnivores 88% (21/24), scavengers 38%, carnivores 3.4% late, 1413; `cold_blood` evolves to a
    mean of 14–18%, `layer_reach` falls to a median of 69%. ×1 (8 seeds): all survive, carnivores
    22–33% late (47% base before B), populations 57 / 185.
  - D (speed price ×1): carnivores 67% (16/24), scavengers 38%, 1.7% late, 1613. Paying for the
    step helps grazers most; carnivores chase at full speed. `cruise` evolves to ~55%, `rest` to
    ~78%, the speed gene up from 15.5 to 19. A dearer speed (×1.5, ×2) only hurt carnivores more
    (12 and 14 of 24). The burst (E) is the hunter's answer, not yet measured.
  - «океаническое» vs «игровое» before B (8 seeds): carnivores held alike (7) but at 2.8% late
    against 7.8%, population 1384 against 1228.
- Balance after the life reform (sweep 2026-09-27, 8 seeds × 20 000): all 24 worlds survive.
  - Baseline conditions: carnivores hold in 7 of 8 (7.8% late), scavengers in 1, population late
    median 1228, minimum 496.
  - ×1: populations collapsed — late median 90 (base) and 136 (calm), against 470 / 205 before the
    reform and 973 / 750 before the three niches; carnivores 22–25%; herbivores died out in 3 worlds
    (base seeds 5, 8; calm seed 5), leaving carnivores alone on plants. The carnivores were already
    too strong at ×1 before the reform. **Open, the user decides.**
- Before that: the three niches (the user's choice) — juvenile gut, the carnivore's nose ×1.5, the
  scavenger's ×3 — carnivores held in 6 of 8 baseline worlds, scavengers in 3. Don't repeat: the
  parent paying for the child's body (halved populations, no meat diets); birth at ¼ size; a nose
  paid like sight (killed the niche); `melee_size_power` 1.75 (carnivores boomed and starved) or 1.0
  (they died out).
- Golden digests are re-recorded for `/14` (2026-09-28, Windows), not yet for `/15`. Both references
  (`reference/fingerprint.json`, `calm-fingerprint.json`) are still the `/9` ones, so `--compare`
  refuses them (another model) and CI fails there by design: re-take them with `--save-reference`
  once the user accepts the balance (8 seeds × 20 000 each).
- `README.md` and `AGENTS.md` describe the `/15` model in short; `BEHAVIOR.md` is the history of
  the models, newest first. This file is the exact model.
  When a mechanic changes, update all four.

**Baseline conditions** — the user's own game; judge balance here, not on ×1 defaults:

```bash
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 \
  --scale 20 --shape 2:1 --rule plant_rate=0.5 --rule cost_scale=2 --rule speed_cost=0.5 \
  --rule plant_depth_steepness=5 --mix 1 1
```

The price of life went from 3 to 2 on 2026-09-29 (sweep of `cost_scale` 2–5, these conditions, 8 seeds
× 40 000 ticks; results in `target/sweeps/cost-2026-09-29`): at 2 all 8 worlds survive, carnivores hold in 8
(6.8% late), scavengers in 4 (1.3%), late population 970, minimum 416; at 3 carnivores 8 (4.9%),
scavengers 1, 1136 / 604; at 4 carnivores 4, at 5 none. The same day the price of speed went 1 → 0.5
on top of it (the user's pick after the second sweep, 60 000 ticks × 8 seeds, `target/sweeps/cost-stats-2026-09-29`,
prices at ×2: control carnivores 7 / scavengers 6, late carnivores 2.9%, population 1396 / min 416;
`speed_cost=0.5` 8 / 7, 7.9%, 1464 / 420; all stats at 1.5 — 8 / 6, 9.7%, 1378 / 356). **Not settled:** the
differences between the variants were a world or two of eight, within the noise, and the control itself moved
between 40 000 and 60 000 ticks; 16 seeds of the candidates would tell more. Measurements before those dates
were taken at 3 and speed 1.

## Commands

```bash
cargo test --workspace                              # all tests (~30 s; ~24 s of it — life-app screens)
cargo test --workspace --exclude life-app           # engine, runner, report only (a few seconds)
cargo test -p life-core --test engine сетка         # tests whose name contains «сетка»
cargo test -p life-core --test golden               # world behaves bit for bit as recorded
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # rustfmt.toml: width 110; CI: cargo fmt --all --check

cargo run -p life-report --release                  # seed 1, 600 ticks: story + summary
cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000 --rule plant_energy=80 --scale 10
cargo run -p life-report --release -- --mix 1 1 --diet-mix 50 0 0 50   # strategy and founder diet mixes
cargo run -p life-report --release -- --ticks 20000 --maps 3           # story + text maps
cargo run -p life-report --release -- --ticks 5000 --json run.json     # everything as JSON (+ text)
cargo run -p life-report --release -- --compare reference/fingerprint.json

play.bat / sh play.sh                                    # build + run the game
cargo run -p life-app --release -- --scale 100 --seed 7  # straight into a world (same flags as the report)
TINYLIFE_SHOTS=some/dir cargo test -p life-app ui_tests  # screen tests + PNGs of every screen
```

Looking at the game from an agent: never screenshot the desktop or inject input. Render headless
with `TINYLIFE_SHOTS` (egui_kittest) and drive state through `LifeApp` fields / `sim::Command`.

CI (`.github/workflows/ci.yml`, Windows only): fmt, clippy, tests, `--compare` against both
references. Golden digests and references are Windows-only (libm). Dev builds use `opt-level = 2`.

**Validating a model change**: seeds 1–8 × 20 000 for the base and calm (`cost_scale=3`) profiles
at ×1 (at least 7 of 8 worlds survive in each), then the baseline conditions. Runs must end on
their own (`--max-work 1e15`). A single-seed run prints genome start → end including diet shares
(`питание`).

**Run every measurement series as a `life-sweep`** (the user wants to see it): say the minutes up
front, keep a run ≤ 5 minutes, a Russian description above each variant, run it in the background
with a watcher that ends on a press:
`until grep -qE "PAUSED by|STOPPED by" LOG; do sleep 2; done`.

```bash
cargo build -p life-report --release && cargo build -p life-app --release --bin life-progress
target/release/life-sweep plan.txt --out sweeps/hunt --jobs 16 --seconds 300
target/release/life-sweep plan.txt --out sweeps/hunt --summary-only
```

- A plan: `seeds:`, shared `args:`, `variant NAME: ARGS` (a capitalised `NAME=VALUE` token is an env
  var for experiment builds); comments: the top block describes the plan, a block the variants
  below it, a trailing comment one variant. Flags the sweep sets itself (`--seed`, `--seconds`,
  `--threads`, `--json`, `--progress`, …) are rejected in either spelling.
- Each variant × seed is its own `life-report` process. Guards: the report's `--seconds` deadline
  (a cut run is left out of medians), a watchdog kill `--grace` later, the worst case printed first.
- Summary per variant (`summary.md/csv`, `runs.csv`): ended, survived, carnivores/scavengers
  *hold* (≥ 10 creatures and 1% over the last `--late` share), coexistence, late diet shares, late
  and minimum population, carnivore births and kills, the tick rate (`ms/tick` — the median run's
  median lap) and `slowdown` (a run's worst lap over its median, ⚠ and a `TICK RATE FELL` line from
  3×), and any `METRIC name value` lines. The report measures the tick rate every 500 ticks
  (`pace` in its JSON, the third number of the tick file). Parallel ×20 runs slow each other down:
  16 at once ran at 3.3–5.1 ms a tick against 2.4–2.6 with 8, so use fewer `--jobs` for big worlds.
- A run is reused when its JSON and `.cmd` (command line + the report build's size and time) match,
  so a stopped sweep resumes and a rebuilt report reruns everything.
- Progress: `OUT/progress.json` every second (runs' ticks from `s<seed>.tick`, written by the
  report's `--progress FILE`, one seed, atomically). The window `life-progress` (always on top,
  own dark palette, taskbar bar via `ITaskbarList3`) shows the bar, time left, running variants
  with a «+» in the card's corner to unfold each seed (tick, time, ms a tick), the last runs (amber:
  its tick rate fell), and pause / resume / stop
  buttons writing `OUT/control.txt`. A pause kills the running runs and requeues them (a world
  depends on its seed only); presses are printed as `PAUSED/RESUMED/STOPPED by the user in the
  window` and appended to `OUT/events.log`.

## Architecture

**Logic is separated from rendering** — the load-bearing decision, enforced by crate boundaries:
`life-core` depends on nothing graphical (not even threads or I/O), `life-sim` adds the bounded
runner and the observer, `life-app` alone knows the screen.

`crates/life-core/src/`:
- `config.rs` — every tunable constant with a comment on *why* that value.
- `rules.rs` — `Rules`, the lab's world rules, set at start or live (`World::set_rules`,
  `apply_rules` recomputes phenotypes). `Rules::default()` is `config.rs` bit for bit. Changing an
  exponent renormalises its coefficient so the base genome pays the same. `with()` rejects unknown
  keys, non-finite values and values where a rule stops making sense — never merely unbalanced
  ones. A new rule: `RULE_KEYS` + `with`/`get` + a `FIELDS` entry in `life-app/src/settings.rs`.
- `world.rs` — `WorldConfig`, `World::step()` (phase order only), counters, `spawn_*`.
- `genome/` — the gene table, `CreatureGenome` (`[f64; N]` indexed by `enum Gene`), mutation.
- `creature/` — `mod.rs` (`act`, feeding, division, `grow_old`), `phenotype.rs`, behaviour:
  `program.rs` (blocks, parameter tables, templates, mutation), `scene.rs` (perception),
  `actions.rs` (one function per action), `strategy.rs` (`Mind`, `Intent`, `Stance`, the
  interpreter `plan`), `steer.rs` (the step); `plant.rs`, `flora.rs`.
- `senses.rs` — what a creature can learn (traits, grid-backed views, queries + brute-force tests);
  `grid.rs` — counting-sort spatial grid; `rng.rs`, `space.rs`.
- `combat.rs` (simultaneous strikes and shots, a parent's defence), `corpse.rs`.

`life-sim`: `simulate()`/`run()` under limits; `observe.rs` — snapshots, the event chronicle (the
game reuses it), ASCII maps. `life-report`: `main.rs`, `story.rs`, `json.rs` (format
`life-report/12`), `metrics.rs` (`--compare`), `bin/life-sweep.rs`.

### Determinism and the golden test

No global RNG: each creature owns an `Rng` forked from its parent's; the world has its own stream.
Results depend on the seed only, not on iteration order or thread count (phase 3 relies on it). Any
change in how many numbers are drawn, or in what order, shifts every seed — then re-validate the
balance instead of diffing numbers. The plant spawner draws even when capped.

`tests/golden.rs` pins an FNV digest of the whole world at checkpoints for eight configs. A refactor
must keep it; a deliberate behaviour change re-records it (the test prints the table) in its own
commit together with `--save-reference`. Asserted on Windows only. `--ignored` prints 50 seeds × 2
worlds for a wider before/after diff; comparing golden output of two builds (a `git worktree`) is
how "bit for bit identical" is proven.

`reference/*.json` are balance fingerprints (8 seeds × 20 000). `--compare` checks each metric's mean
against the reference's per-seed range (exit 1 on a mismatch) and refuses (exit 2) when the world
conditions differ (space, rules, start counts and mixes). A deliberate balance change fails it by
design; re-take with `--save-reference reference/fingerprint.json` (and the calm one with
`--rule cost_scale=3`).

### Tick order

Plants → old age → neighbour snapshot → decisions (settings, then a deciding block), healing,
ageing and movement → eating plants → simultaneous strikes and shots → the survivors eating
corpses → reproduction → removing the dead and adding children → tick number. Strikes are collected
before damage, so mutual death is possible; a death has one cause (`Starved`, `OldAge`, `Combat`).
A creature killed in combat gets no prey and does not reproduce.

### Neighbour search and performance

- Creatures never see the grid: `Creature::step` takes *senses* (`GridSenses` per creature; tests
  pass `senses_from(..)` or `Blind`). Queries are `#[inline(always)]` (without it a ×100 world ran
  8% slower). The cell is fixed (`GRID_CELL`); `for_each_near` returns a superset, callers check
  distance. The grid copies coordinates, valid only because queried entities don't move within the
  phase. Keep the brute-force checks in `senses.rs` tests.
- `Creature::step` is the hot path: genome-derived values are precomputed in `Phenotype::of`,
  distances compared squared, block dispatch is a `match` over `Action`, never `Box<dyn>`, and the
  scene asks the senses only what a block needs (a memo per tick). The eating phase
  copies only the corpses claimed that tick (`world.rs`, `claimed`). For refactors,
  compare ms/tick against the previous build in a worktree, alternating runs
  (`life-report --scale 100 --ticks 1000 --seeds 1 2 --threads 1`).
  `тик_растёт_линейно_с_численностью` guards against queries degrading to a full scan.
- **Termination**: every headless run is capped by ticks, a population ceiling, a work budget
  (creatures × plants) and a wall-clock deadline (`life_sim::Limits`). Tests have no `while` loops;
  every run is bounded by a tick count.

### Space and food

- The layer («слой», a setting) is a preference, not a wall: a creature goes for any visible food
  within its reach and walks back to its home band otherwise; physics clamps only to the world.
  Never add moves that teleport.
- Real units are for showing only (`units.rs`, `docs/scale.md`): 1 px = 0.5 cm (the base fish is
  20 cm), a tick is 0.25 s of swimming but ~9 hours of life (3000 ticks = 3 years).
- Scale is area, shape (1:1, 3:2 default, 2:1, strip) is proportions (`Space::new`); at ×1 3:2 is
  exactly the base 6000×4000. Per-world quantities scale with `area_ratio` via `per_area`; the
  vertical ecology is in % of depth. Tall worlds are harsher (newborns start far from the rich top).
- Food (`flora.rs`): x and y drawn independently by width and depth profiles (uniform, linear, exp,
  log, waves, «игровое» — flat to 20% of depth, then an exp fall; «океаническое» — 60% at the
  surface rising to a peak at 15%, then an exp fall; default «океаническое» down, uniform across).
  Capacity is `PLANT_MAX` slots, one plant each, so growth is logistic. Default 24 patches
  per base world holding `plant_patch_share` 60% of the slots, fewer, smaller and poorer with depth;
  patch centres come from their own keyed stream, never the world's. Occupied slots are an
  incremental bitset (`world::Occupancy`), rebuilt when the plant count stops matching — keep
  direct edits of `plants` changing the count.
- Upkeep is `COEF * stat ** POWER` over size, speed and sight (speed also × (size/40) **
  `speed_mass_power`), with per-term price rules and `cost_scale`. The *exponents* make the
  trade-off: benefit grows ~ stat², so cost must be steeper (`size ** 2.5`, `vision ** 2`), or stats
  run away. Read the block in `config.rs` before changing them.

## Genes and strategies

`GeneSpec { key, label, about, kind, base, mutation }`, `kind` = `Absolute` | `Percent` |
`Choice(&[Variant])`. Everything walks the table, never positions. **Tables are append-only** — the
order fixes the RNG draw order and positions in references and JSON; the only in-place
replacements were `carnivory` → `diet` and `life_pace` → `maturation`, same law so same draws, and
the deletions the behaviour genes that moved into the programs (nine in `/13`, fifteen with the
flock genes in `/14`, the user's calls). Ten genes are left: `size`, `speed`, `vision`, `strategy`,
`mutability`, `maturation`, `diet`, `lifespan`, `cold_blood`, `burst`. A choice gene with one
variant is inert (draws nothing, hidden in the UI).

Mutation (`genome::Heredity`, built from the rules): `CLONE_CHANCE` 50% of children are exact
copies; otherwise `Scale` for numbers (× (1 + gauss(0, σ·mutability)), multiplier ≥ 0.1), `Shift`
for a percent gene whose zero means something (+ gauss points, clamped 0–100: `cold_blood`), `Switch`
for choice genes (0.1% × mutability), and `Neighbours { chance, rise, jump, of, up, leaps }` for the
diet, independent of mutability: a step towards meat 2% (herbivore → omnivore → scavenger or
carnivore), another neighbour step 0.5%, the herbivore's own leaps to the carnivore 0.1% and the
scavenger 0.01% (rules `diet_leap_*`, replacing its general jump), others a general jump 0.01%.
Mutability is clamped to `MIN_MUTABILITY` 0.1 (without a floor selection froze evolution) and costs
nothing. Start mixes (strategies, diets) are dealt without draws (`variant_for`, `spread_ranks`).

A program **decides** (`strategy::decide(&Me, &Program, &mut Mind, &mut Rng, &senses) -> Intent`),
the creature **acts** (`act`); a block cannot move, feed or divide the creature — a setting only
allows a phase to. The strategies
(`Strategy`, `VARIANTS`) are only the founders' templates: `standard`, and `lurker`, which wanders
at a third of its speed.

Adding a gene: a variant at the end of `enum Gene` and a row at the end of `GENES`; its effect only
in `Phenotype::of`, any cost appended at the end of the upkeep sum; a test of the effect; check the
genome panel and the card at 960×600 with `TINYLIFE_SHOTS`; re-record golden and references in a
separate commit. A behaviour is no gene: add a block instead. Adding a test or an action: a variant
at the end of `Cond`/`Action` and their `ALL`, its parameters as a `ParamSpec` table (a new sense =
trait method + query + brute-force check), its `Scene::test` or `actions::act` arm, labels for the
window, tests of it in `strategy.rs`, re-record golden. Adding a template: a variant at the end of
`Strategy`/`VARIANTS` (≤ 8) and its `Program::template`.

## The game (`crates/life-app`)

**The window never waits for the simulation.** Defaults are the user's world (×20, 2:1,
`cost_scale=2` = `settings::GAME_COST_SCALE`, `speed_cost=0.5` = `GAME_SPEED_COST`, half lurkers,
steepness 5). The settings show every factor with a common value as a plain number per 100 of it
(the user: no %): the price of life 100 = ×2, the price of speed 100 = ×0.5 (200 is as designed), the
size and sight prices, the diet edges, the drift and the meat founders' size 100 = ×1; the diet's
digestion shares stay in %, the rules and the report keep the raw factors. «Спокойнее» sets the price
of life to 150 (×3). A settings file carries the version of
its defaults (`DEFAULTS_VERSION`): a key whose default changed after it takes the new one.

- `sim.rs` — the simulation thread owns `World`; the UI sends `Command`s applied between ticks.
  Frames go through a one-slot mailbox and are never dropped, so history, log and snapshots travel
  as deltas. Tempo: ticks per second with a capped debt; `SLICE` bounds a burst.
- `frame.rs` — instances (32 bytes, relative to an f64 `origin`) culled to the visible rect, or a
  density raster when too many; `motion.rs` — previous positions, births and ghosts so nothing jumps
  (plants matched by `born` tick + x bits; `Plant` stays 24 bytes). `meta` packs 32 bits the shader
  reads (see its comment).
- `render.rs` + `creatures.wgsl` — one instanced draw call; interpolation between frames, newborns
  growing, detail (diet rim, fullness, eye, proboscis) only above ~4 px. `view.rs` paints corpses,
  patches, selection, minimap.
- `game.rs` (game screen, «Графики» panel, the lab by topic tabs, creature card with the diet's
  edges), `behaviour.rs` (the selected creature's programs as flowcharts, a tab per track, the
  modes on in the header, the settings as a section on top, conditions of up to three tests, this
  tick's path lit with «не вышло» where an action failed; the
  card's «Поведение (B)», key B), `diets.rs` («Кто живёт», «Кто кого»,
  highlight), `stats.rs` («Статистика», area
  selection), `screens.rs` (menu, «Новый мир», prefs, help), `charts.rs` (painter, no plot crate),
  `settings.rs` (`FIELDS` — the single spec of every field: label, hint, hard limits and why, tab,
  rule; file in `%APPDATA%\TinyLife`, atomic, clamped, unknown keys ignored).
- `ui_tests.rs` — kittest at 960×600 and 1600×900: widgets inside the window, not overlapping. They
  share one GPU lock (parallel wgpu renderers crash the Windows driver); CI renders through WARP.
- Release builds on Windows use `windows_subsystem = "windows"` and attach to the parent console.
- `bin/life-progress.rs` — the sweep's progress window (see `life-sweep` above); `View::of` and
  `draw` are tested, `draw` also as PNGs.
