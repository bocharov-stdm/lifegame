//! All the simulation's settings in one place — carried over from the Python version
//! (`python/life/config.py` at the tag python-final) together with the explanations of why the
//! values are what they are.
//!
//! The values were picked by iterating through headless runs. The criteria: creatures live to
//! the end, the population stays in a playable corridor, the genome comes to an optimum (and
//! does not run away upwards). Predators as a species of their own are gone (tag
//! `predators-final`): the meat diets (`DIET_*` below) and corpses took their place.
//!
//! A convenient way to tweak the balance:
//!     cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000

// ── World ────────────────────────────────────────────────────────────────────
// The base world 6000x4000 was chosen for a 1200x800 window. In the former world of 60000x12000,
// 720 million px², creatures hardly met one another. A bigger world is now obtained by the scale
// (see space.rs), at which the density of everything alive stays the same.
pub const WORLD_WIDTH: f64 = 6000.0;
pub const WORLD_HEIGHT: f64 = 4000.0;

/// Real units, for showing only (`units.rs`, `docs/scale.md`): the mechanics never read them. The
/// base fish (`size` 40) is a 20 cm fish, so the base world is 30 × 20 m. Two clocks, since life is
/// compressed far more than swimming: by the swimming clock the base speed 10 is a body length a
/// second (a cruising fish), by the life clock the base lifespan 3000 ticks is three years (a small
/// fish of that size) — a tick of life is about nine hours.
pub const CM_PER_PX: f64 = 0.5;
pub const SECONDS_PER_TICK: f64 = 0.25;
pub const TICKS_PER_YEAR: f64 = 1000.0;

/// Once in how many ticks creatures try to divide.
pub const DIVIDE_PERIOD: u64 = 30;

// ── Plants ──────────────────────────────────────────────────────────────────
pub const PLANT_RADIUS: f64 = 10.0;
pub const ENERGY_FROM_PLANT: f64 = 50.0;
/// The share of a raw portion actually digested when eating in five steps.
pub const PLANT_BITE_YIELD: f64 = 0.44;

// Where the food grows — the profile by depth and by width (flora.rs), the world's rules.
// By default: the «игровое» profile down, uniform across, in patches.

/// The «игровое» depth profile (`flora::Profile::Game`), a rough real sea: nutritious upper
/// layers and a nearly dead bottom, where the rot settles. Full food down to this share of the
/// depth, then the exponent with the profile's steepness over the rest, so at the default 8 the
/// bottom holds ~3000 times less food, like the plain exponent. Before it the default was the plain
/// exponent with steepness 8; the first «игровое» fell along a straight slope and a cosine.
pub const GAME_PLATEAU: f64 = 0.2;
/// The «океаническое» depth profile (`flora::Profile::Ocean`): real seas are richest not at the
/// very surface but a little below it, at the deep chlorophyll maximum, where enough light still
/// reaches and nutrients rise from the deep. Food grows from `OCEAN_SURFACE` of the peak at the
/// surface to full at `OCEAN_PEAK` of the depth, then falls by the profile's steepness: at 8 the
/// bottom holds ~3000 times less than the peak, like «игровое».
pub const OCEAN_PEAK: f64 = 0.15;
pub const OCEAN_SURFACE: f64 = 0.6;

/// The steepness of the exponent by depth: the bigger, the tighter the food is pressed to the
/// surface. At 8 there is e^8 ≈ 3000 times less food at the bottom than at the top.
pub const PLANT_DEPTH_DECAY: f64 = 8.0;
/// The steepness of the exponent by width, if it is chosen. Softer than by depth: at 8 nearly
/// all the food would huddle at the left edge, and the world to the right would be empty.
pub const PLANT_WIDTH_DECAY: f64 = 3.0;
/// The parameters of the other profiles while they are untouched: linear — at the far edge 10%
/// of the near edge's food; logarithm — a bend of 20 (in the middle of the axis 79% of the food
/// is still there, towards the far edge a drop to zero); waves — 3 rich strips with an
/// amplitude of 80% (between strips there is 9 times less food than at the peak).
pub const PLANT_LINEAR_END: f64 = 10.0;
pub const PLANT_LOG_BEND: f64 = 20.0;
pub const PLANT_WAVES: f64 = 3.0;
pub const PLANT_WAVE_AMPLITUDE: f64 = 80.0;

/// The expected number of new plants per tick per pixel of area (not a probability!).
pub const DENSITY_PER_PIXEL: f64 = 1.0417e-7;
/// For the base world this is 2.50008 plants a tick.
pub const PLANT_SPAWN_CHANCE: f64 = DENSITY_PER_PIXEL * WORLD_WIDTH * WORLD_HEIGHT;

/// Plant cap per base world, and the number of fertility cells it is split
/// into (`flora.rs`): one plant per cell, cells shaped by the food profile, so
/// every part of the world has its own share of the cap. A plant disappears only
/// when eaten; without creatures the world fills up to the cap. Healthy runs
/// used to peak at 219‒289 plants, but seeds landing in occupied cells already
/// slow growth well below the cap. Grows with the world's area.
pub const PLANT_MAX: usize = 1500;

/// Patches of food per base world (`flora.rs`): plants grow in islands instead of an even carpet,
/// so creatures are seen between the food rather than inside it. 0 — scattered by the profile.
/// With 1500 slots it is about 60 plants a patch when the world is full.
pub const PLANT_PATCHES: f64 = 24.0;
/// Mean patch radius; each patch draws its own from half to one and a half of it.
pub const PLANT_PATCH_SIZE: f64 = 200.0;
/// Patches are ellipses: width to height from 1/2.5 to 2.5.
pub const PATCH_STRETCH: f64 = 2.5;
/// A patch's weight — its share of its region's slots, and so of the seeds — is drawn from this
/// to 1: patches differ in how dense and how rich they are.
pub const PATCH_WEIGHT_MIN: f64 = 0.2;
/// Share of the slots in patches, %; the rest grow scattered between them by the same profile. A
/// region's share follows its light (the depth profile's density there): bright regions keep more
/// in patches, dark ones scatter more, and the world's mean is this.
pub const PLANT_PATCH_SHARE: f64 = 60.0;
/// A patch's radius shrinks with the light at its centre, times `max(this, sqrt(light))`: deep
/// patches are small, and with fewer slots in their region's patches, poor.
pub const PATCH_DARK_SIZE: f64 = 0.3;

// ── Creatures ─────────────────────────────────────────────────────────────
// The creature's base genome is in the gene table (`genome/creature.rs`).
pub const CREATURES_AT_START: usize = 20;
/// The spread of mutations.
pub const MUTATION_SIGMA: f64 = 0.3;

/// The energy store = size * this.
pub const ENERGY_PER_SIZE: f64 = 2.5;
/// The energy put into a unit of grown diameter; the energy store does not depend on it.
pub const GROWTH_ENERGY_PER_SIZE: f64 = 2.25;
/// The minimum that must remain with the parent after a division.
pub const REPRO_RESERVE: f64 = 20.0;
/// A fixed penalty for reproduction.
pub const REPRO_COST: f64 = 10.0;
/// A healing creature (`Action::Heal`) gains this share of its full health a tick, paid one for
/// one from its tank: physiology, not a choice — whether and when it heals is its program's.
pub const HEAL_SHARE: f64 = 0.002;

// ── The upkeep of the stats (energy per tick) ───────────────────────────────
// upkeep = COEF * stat ** POWER, summed over the three stats.
//
// THE EXPONENTS MATTER MORE THAN THE COEFFICIENTS: they decide whether evolution has a
// trade-off. The benefit of a stat grows like this:
//   size   — the eating radius equals the size, so the reach ~ size²
//   sight  — the food search radius, so the reach ~ sight²
//   speed  — roughly linearly
// The price must grow STEEPER than the benefit, otherwise a stat runs away upwards without
// limit: at exponents of 1.5 for size and 1.0 for sight, size reached 450 against the base 40,
// and sight reached 1000 against the base 400.
pub const SIZE_ENERGY_POWER: f64 = 2.5;
pub const SPEED_ENERGY_POWER: f64 = 2.0;
pub const SIGHT_ENERGY_POWER: f64 = 2.0;

// The coefficients are normalised so that the base genome spends three EQUAL shares and lives
// on a full tank about 700 ticks without food. One plant is half a tank.
pub const SIZE_ENERGY_COEF: f64 = 4.706e-6;
pub const SPEED_ENERGY_COEF: f64 = 4.762e-4;
pub const SIGHT_ENERGY_COEF: f64 = 2.976e-7;

/// Moving a big body is dearer: the speed's price is multiplied by (size / 40) ** this.
/// The base genome (diameter 40) pays exactly as much as without the multiplier. Without it, with plentiful
/// plants, giants of size 150‒250 survived.
pub const SPEED_MASS_POWER: f64 = 1.0;

/// The ceiling of the mutability gene (a multiplier on the mutations' spread and the chance of
/// a strategy change). At 10 the creatures' sigma is 3.0: a descendant's genome is almost
/// random — no point growing further, and without a ceiling the multiplier could go to infinity.
pub const MAX_MUTABILITY: f64 = 10.0;
/// Floor of the mutability gene. Selection pulls it down (a less mutated child is fitter on
/// average), and at 0 evolution froze: one diet, one strategy, forever.
pub const MIN_MUTABILITY: f64 = 0.1;
/// Share of children born an exact copy of their parent, no gene mutated. A lineage keeps its
/// proven genome through them, so selection has less reason to push mutability down.
pub const CLONE_CHANCE: f64 = 0.5;

// ── The neighbour search ────────────────────────────────────────────────────
/// The grid cell's size (grid.rs). In Python the cell equalled the biggest query radius, and
/// one far-sighted creature blew it up for everyone. Here the cell is fixed, and a query takes
/// as many cells as its own radius covers. 256 is of the order of half a sight: a query usually
/// looks at 4x4‒5x5 cells, and there are few empty cells.
pub const GRID_CELL: f64 = 256.0;

// ── Strategies ──────────────────────────────────────────────────────────────
/// Chance that a child gets another strategy — the template its founders' program started from.
/// None: behaviour is inherited and mutates as the program (`Program::mutate`); the gene keeps its
/// draw (`Mutation::Switch` still draws with two variants), so a child keeps its lineage's name.
pub const STRATEGY_SWITCH_CHANCE: f64 = 0.0;
/// Share of the children that are not exact copies whose behaviour program mutates once
/// (`Program::mutate`), times the parent's mutability; the rule `program_mutation`. A generation
/// is a few hundred ticks, so a lineage gathers some changes in a game while most programs stay
/// recognisable; a mutation that breaks one is weeded out by its bearer's fate.
pub const PROGRAM_MUTATION_CHANCE: f64 = 0.05;
/// A program threshold's mutation moves it by gauss(0, this) points (thresholds are 0‒100%).
pub const PROGRAM_NUDGE_POINTS: f64 = 10.0;
/// Every child that is not an exact copy moves every number of its programs by gauss(0, its
/// nudge × this × the parent's mutability) — a threshold by 10 points, a ratio by 0.2, a time by
/// a quarter of its base; the rule `program_drift`. The numbers evolve like the genes they
/// replaced (the genes drift by `MUTATION_SIGMA` 30% a child): through the rare mutation alone a
/// given number moved in about one child of 1700, and the old genes' adaptations (the pace to
/// ~55%, the rest to ~78%, the layer reach to 69% within 20 000 ticks) could not happen.
pub const PROGRAM_DRIFT: f64 = 1.0;
/// The share of a program's numbers the drift moves in one child: a third, so that selection sees
/// a few changes at a time rather than the sum of sixty (with every number moving, a good change
/// was buried in the noise of the rest).
pub const PROGRAM_DRIFT_SHARE: f64 = 1.0 / 3.0;
/// The same rare switch for the other choice genes: shooting, layer.
pub const CHOICE_SWITCH_CHANCE: f64 = 0.001;
/// A melee strike: a share of the diameter, at once the base damage and the energy price.
pub const MELEE_DAMAGE_SHARE: f64 = 0.05;
/// A bigger body strikes disproportionately harder: melee damage is times (attacker's size /
/// target's size) ** this when the attacker is the bigger (never less than ×1). Equal bodies still
/// trade ~20 strikes; 2× bigger needs ~5; 3× a carnivore kills a herbivore in two (0.075 · 3^2.25 /
/// 1.5 = 0.59 of its health a strike): a carp and a fry, not a duel. There is no cap on a strike's
/// share of the target's health any more; shots keep theirs. At 1.75 (3× = one blow) carnivores
/// boomed, ate the herbivores out and starved; at 1.0 they died out everywhere (seeds 1–8).
pub const MELEE_SIZE_POWER: f64 = 1.25;
/// A shot is weaker than a melee strike, but needs an energy store of its own.
pub const SHOT_DAMAGE_SHARE: f64 = 0.01;
pub const SHOT_ENERGY_SHARE: f64 = 0.02;
pub const SHOT_PERIOD: u64 = 5;
pub const SHOT_RANGE_SIZES: f64 = 4.0;

/// The slowest pace a program's block goes at, a share of speed: slower, a creature wandering for
/// food would stand. A slow step is paid as taken, by the same law `speed ** SPEED_POWER` as the
/// gene: at the square a third of the speed costs a ninth, and the base creature's whole upkeep
/// falls to about 70% (the lurker's template wanders at 33%).
pub const MIN_PACE: f64 = 0.1;
/// A burst (the `burst` gene, ×1 to `BURST_MAX` its speed) in a chase or in flight, when the goal
/// is farther than a normal step: at most `BURST_TICKS` ticks in a row, then `BURST_REST` ticks
/// winded; a tick without one gives back a tick of it. The step is paid as taken (the square
/// law), and the muscles cost standing too: `BURST_UPKEEP_SHARE` of the speed term the extra speed
/// would add. Without that price a slow body with a big burst was a free speed gene.
pub const BURST_MAX: f64 = 2.0;
pub const BURST_TICKS: u32 = 20;
pub const BURST_REST: u32 = 60;
pub const BURST_UPKEEP_SHARE: f64 = 0.25;
/// Torpor (`Action::Torpor`, when its program chooses it): a creature stands and pays this share of
/// its standing upkeep (times the cold's saving); it wakes when its program chooses otherwise. It
/// only saves: never below nothing, so it never makes energy.
pub const TORPOR_UPKEEP: f64 = 0.3;

// Combat and hunting are always on: the peaceful world (the old `cannibalism` rule) is gone.
// There is no world size ratio either: whom one attacks is up to the blocks of its program (the
// ratios of `Action::Hunt`, `Action::FightBack` and `Action::Rival`).

// ── Feeding ─────────────────────────────────────────────────────────────────
/// Chance that a mutating child's diet steps to a neighbour (`genome::creature::DIET_NEIGHBOURS`),
/// whatever its parent's mutability: at 0.1% × mutability meat-eating mutants hardly ever
/// appeared, and the meat niches stayed empty.
pub const DIET_STEP_CHANCE: f64 = 0.005;
/// Chance that a mutating child's diet steps towards meat (`genome::creature::DIET_TOWARDS_MEAT`:
/// herbivore → omnivore, omnivore → scavenger or carnivore), also whatever the mutability. The meat
/// diets have no founders (they starved before there was meat), so they arise from these mutants;
/// at 0.5% a world saw about three in 20 000 ticks, too few to take hold.
pub const DIET_MEAT_STEP_CHANCE: f64 = 0.02;
/// Chances that a mutating herbivore's child leaps straight to the carnivore or the scavenger,
/// past the omnivore (`genome::creature::DIET_LEAPS`, rules `diet_leap_carnivore` and
/// `diet_leap_scavenger`; they replace its general jump): the omnivores
/// dwindle to 1–2% of a world, and the meat diets hung on them alone (user's choice, 2026-09-27).
pub const HERBIVORE_LEAP_CARNIVORE: f64 = 0.001;
pub const HERBIVORE_LEAP_SCAVENGER: f64 = 0.0001;
/// Chance that a mutating child's diet jumps to any other diet, neighbour or not, also whatever
/// the mutability: so a line is not locked into its branch forever.
pub const DIET_JUMP_CHANCE: f64 = 0.0001;
/// Strike damage by diet, times the world's `melee_damage_share` (and `shot_damage_share`): meat
/// eaters are built to kill. The omnivore strikes half as hard again as the herbivore (1.15 until
/// the user's calibration of 2026-09-29), the scavenger ×1.3, the carnivore hardest: at ×1.5 a
/// carnivore could hold only the newborns it caught, and hunting did not pay (user's choice,
/// 2026-09-27). The energy a strike costs does not change.
pub const DIET_STRIKE: [f64; 4] = [1.0, 1.5, 1.3, 3.0];
/// Health by diet, times the body size: the herbivore is hardy. It cannot strike like a meat
/// eater, so it outlasts one — a hunter needs half as many strikes again, and weighs that.
pub const DIET_HEALTH: [f64; 4] = [1.5, 1.0, 1.0, 1.0];
/// The size term of upkeep by diet: plants are a steady, bulky food, so a herbivore carries a
/// big body cheaper — a little, so that size still has a price and selection, not the table,
/// makes it bigger.
pub const DIET_SIZE_COST: [f64; 4] = [0.85, 1.0, 1.0, 1.0];
/// The speed term of upkeep by diet: the carnivore is a runner built to chase — it moves cheaper.
/// Without an edge of its own it died out everywhere once the herbivore grew hardy and the
/// scavenger learned to smell (16 of 16 worlds, 2026-09-26); at ×0.8 it still spent more on the
/// chase than it caught (0.12 energy a tick against 0.15), at half the price it about held.
pub const DIET_SPEED_COST: [f64; 4] = [1.0, 1.0, 1.0, 0.5];
/// How far corpses are sensed, in shares of vision: the scavenger smells them three times as far
/// as it sees, the carnivore half as far again. Smell is not paid for — only vision is (the
/// scavenger's food lies scattered in the deep, and it would never find it by sight alone). The
/// carnivore's nose is what lets it hold (user's choice, 2026-09-27, from an 18-variant sweep on
/// the baseline conditions): without it, it held in 0 of 8 worlds; a nose ×1.5 with the juvenile
/// gut (`DIET_YOUNG_PLANTS`) in 8 of 8, but fresh corpses were eaten before they rotted and the
/// scavengers held in 1; the scavenger's nose ×3 gives both — carnivores in 6, scavengers in 3. A
/// nose paid for like sight at its radius killed the carnivores everywhere: a mutant is born with
/// it and cannot pay for it before it finds meat. The omnivore smells a little past its sight
/// (×1.2, the user's calibration of 2026-09-29).
pub const DIET_SMELL: [f64; 4] = [1.0, 1.2, 3.0, 1.5];
// Cold deep water (the user's choice, 2026-09-27; it replaced the scavenger's own deep saving,
// −40% on the bottom). The water is warm down to the thermocline's top, cold below its bottom, a
// smooth step between (% of depth, rules `thermo_top`, `thermo_bottom`). A cold-blooded body takes
// the water's temperature: in the cold it lives cheaper and moves slower, both by its `cold_blood`
// gene times the coldness. A warm-blooded one pays and moves the same everywhere.
pub const THERMO_TOP: f64 = 15.0;
pub const THERMO_BOTTOM: f64 = 45.0;
/// At full coldness and a fully cold-blooded body: upkeep × (1 − this) and speed × (1 − the next).
/// Saving no more than the whole upkeep, so it never makes energy.
pub const COLD_SAVING: f64 = 0.5;
pub const COLD_SLOWING: f64 = 0.4;
/// The `cold_blood` gene starts at 0 (warm-blooded, as before it) and a mutation moves it by
/// gauss(0, this) points (`Mutation::Shift`).
pub const COLD_BLOOD_STEP: f64 = 10.0;
/// Scavenger founders (in a start mix) start this cold-blooded, in the deep with the rot.
pub const SCAVENGER_START_COLD: f64 = 100.0;
/// Which food is a diet's own (plants, fresh meat, rot, bones): a creature eats and goes for only
/// its own, and takes another niche's food only when its program says so this tick
/// (`Action::EatForeign`; the template, below 30% of its store). The omnivore has no foreign food:
/// it is the generalist (bones it cannot digest at all).
pub const DIET_OWN: [[bool; 4]; 4] = [
    [true, false, false, false], // herbivore
    [true, true, true, false],   // omnivore
    [false, false, true, true],  // scavenger
    [false, true, false, false], // carnivore
];
/// Founders dealt the scavenger diet start with this layer, % of depth (their program's «слой»):
/// in the deep, where rot will settle. A start condition, not a rule — the program drifts.
pub const SCAVENGER_START_LAYER: (u16, u16) = (50, 100);
/// Founders dealt a meat diet (scavenger, carnivore) start this many times bigger. Equal to the
/// others they had no prey (a hunter took prey `prey_ratio` times smaller, 2.5 then, and newborns
/// are half grown) and starved by tick ~400 without a single strike. A start condition: the gene
/// mutates.
pub const MEAT_FOUNDER_SIZE: f64 = 2.0;
/// Digestibility by diet (order of `genome::creature::DIET_VARIANTS`): plants, fresh meat,
/// rot, bones (the corpse's stages, `corpse::Stage`). For plants 1 is the world's yield `plant_bite_yield`; for meat it is the whole raw
/// portion — the diet alone decides how much of it is taken in (the old flat 10% fed a hunter
/// less for a whole corpse than one plant). 0 means the creature neither eats that food nor
/// goes for it. A specialist digests its own food fully; the
/// omnivore takes everything, but worse; rot feeds well only the scavenger, the others barely.
/// Meat-eaters get a little from plants (not their own food: they eat it only when hungry), so a
/// line of them is not starved out before it finds meat.
/// Bones only the scavenger digests (the user's choice, 2026-09-27): the long-lying remains on the
/// bottom are its own food, which nobody else can take.
/// The user's calibration of 2026-09-29: the omnivore 0.7/0.3/0.05 → 0.8/0.6/0.2, the scavenger's
/// fresh meat 0.8 → 1.0, the carnivore's rot 0.1 → 0.3.
pub const DIET_DIGESTION: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],  // herbivore
    [0.8, 0.6, 0.2, 0.0],  // omnivore
    [0.15, 1.0, 0.9, 0.9], // scavenger
    [0.2, 1.0, 0.3, 0.0],  // carnivore
];
/// Plants while not grown to its own size (the size gene), by diet — a juvenile gut: a young
/// carnivore digests plants like an omnivore, grows on them and hunts once grown. A carnivore
/// mutant is born half grown with its herbivore parent's prey ratio, sees no prey that much
/// smaller than itself and starved on plants at 20%. Grown, it is back at `DIET_DIGESTION`. No
/// loophole in staying young: only the grown divide (57‒67% of carnivores were grown in every
/// variant of the sweep). It is a floor under the grown value, not a value of its own: 0 means
/// "young as grown", so the others follow their grown `plants` rule when the lab changes it (a copy
/// of it here kept young scavengers on plants after `scavenger_plants=0`). The user's calibration
/// of 2026-09-29 gave every diet a juvenile gut: the herbivore and the omnivore 100%, the
/// scavenger 70%, as the carnivore.
pub const DIET_YOUNG_PLANTS: [f64; 4] = [1.0, 1.0, 0.7, 0.7];
/// Founders' diets, shares in the same order. Dealt without a draw. No meat eaters: at the start
/// there are neither corpses nor prey small enough, and every such founder starved (none struck
/// once in the user's world, 2026-09-27); the meat diets arise from mutants
/// (`DIET_MEAT_STEP_CHANCE`).
pub const DIET_START_MIX: [f64; 4] = [70.0, 30.0, 0.0, 0.0];

/// A creature eating stops at this share of its reach from the food's centre (a plant is reached
/// within one body diameter, a corpse within that plus its radius) instead of walking onto it:
/// the food lies beside the body, where the window draws the proboscis reaching it.
pub const EAT_STOP_SHARE: f64 = 0.85;

// ── Corpses ─────────────────────────────────────────────────────────────────
// Three stages (`corpse::Stage`), each longer than before (the user's choice, 2026-09-27: fresh
// 150, a smooth rot to 600, gone by 1800; bones only from an eaten corpse, 1800 ticks, sinking 4).
/// A corpse stays fresh this long and lies where the creature died; then it is rot at once.
pub const CORPSE_FRESH_TICKS: u64 = 300;
/// After the fresh time it sinks this far a tick, straight down, and can be eaten all the way. A
/// speed, not a time to the bottom: with a time, a corpse in a world 15 500 deep (×20, 2:1) fell
/// 10‒15 a tick, as fast as a scavenger swims, and the scavengers never caught one. At 2 a fifth of
/// a base creature's speed, in a world of any height; in a tall one a corpse may decay on the way.
pub const CORPSE_SINK_SPEED: f64 = 2.0;
/// The flesh has rotted down to the bones by then, decaying evenly over the whole time, so a corpse
/// reaching its resting place untouched still holds most of its meat for the scavengers.
pub const CORPSE_DECAY_TICKS: u64 = 3000;
/// Rot and skeletons rest in this lowest share of the depth, %, each at its own place (a hash of
/// the id): spread over the dead deep, not a line on the bottom (at 2% it was a thin strip).
pub const CORPSE_REST_PCT: f64 = 25.0;
/// This share of a corpse's meat is its bones, left when the flesh is eaten or rotted away. The
/// hunters' last tenth feeds the scavengers, who alone digest bones.
pub const CORPSE_SKELETON_SHARE: f64 = 0.1;
/// Bones sink to the corpse's resting place this far a tick, ten times a corpse: heavy, they fall
/// «со свистом»,
pub const SKELETON_SINK_SPEED: f64 = 40.0;
/// and lie there long, decaying evenly over this many ticks from when they were left: the
/// scavenger's store on the bottom. The longer they lie, the stronger the scavengers and the
/// weaker the carnivores (24 seeds, the user's world, 2026-09-27: at 20 000 carnivores held in
/// 58% of worlds and scavengers in 58%, at 5000 71% and 46%, at 1800 75% and 46%; before the
/// stages 80% and 20%). The user chose 5000.
pub const SKELETON_TICKS: u64 = 5000;

// ── Flight ──────────────────────────────────────────────────────────────────
/// A creature runs from a stranger (not kin) that can eat it when the gap to the edge of its
/// body is under this share of its own sight. A third — as they ran from predators (tag
/// `predators-final`): with a far threshold the small would only run and not eat, and one has to
/// manage to run before the big one reaches.
pub const FLEE_SIGHT_SHARE: f64 = 1.0 / 3.0;
/// How many ticks a creature runs after a fright (a second at 60 ticks a second): a threat
/// gone from sight does not mean it has gone.
pub const FLEE_TICKS: u32 = 60;

// ── Life and old age ────────────────────────────────────────────────────────
/// The `lifespan` gene: every founder starts with this many ticks of life (user's call, 2026-09-27).
/// The gene is free, like behaviour genes: a long life buys nothing but more time, and the old
/// are weak (below). Before, the `life_pace` gene cut upkeep in proportion to a longer life, and
/// selection pinned it to its floor everywhere: slow life was a free 50% discount.
pub const LIFESPAN_BASE: f64 = 3000.0;
/// The physical ceiling of the gene, and its floor: shorter than a few divide periods of growing up
/// a line could not bear a child at all.
pub const LIFESPAN_MAX: f64 = 10_000.0;
pub const LIFESPAN_MIN: f64 = 500.0;
/// Old age: from this share of its lifespan a creature weakens linearly, and by `OLD_AGE_FULL` its
/// speed, vision, strike and health are `OLD_AGE_VIGOUR` of what they were; at the whole lifespan it
/// dies. The body and its tank stay: shrinking would destroy meat.
pub const OLD_AGE_FROM: f64 = 0.7;
pub const OLD_AGE_FULL: f64 = 0.9;
pub const OLD_AGE_VIGOUR: f64 = 0.7;

// ── The chase ───────────────────────────────────────────────────────────────
/// A hunter that has not closed in on its prey by at least one of its own steps within this many
/// ticks gives the chase up: a prey as fast as it, fleeing, is never caught in the open, and the
/// hunter used to follow it as long as it saw it, starving on the way.
pub const CHASE_PATIENCE: u64 = 30;
/// For this long it does not choose the prey it gave up again.
pub const CHASE_GIVE_UP_TICKS: u64 = 180;
