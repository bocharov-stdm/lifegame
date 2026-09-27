//! Все настройки симуляции в одном месте — перенесены из Python-версии
//! (`python/life/config.py` на теге python-final) вместе с объяснениями, почему значения такие.
//!
//! Значения подобраны перебором через headless-прогоны. Критерии: существа
//! доживают до конца, численность держится в играбельном коридоре, геном
//! приходит к оптимуму (а не убегает вверх). Хищников как отдельного вида больше
//! нет (тег `predators-final`): их заменили мутации и каннибализм.
//!
//! Крутить баланс удобно так:
//!     cargo run -p life-report --release -- --seeds 1 2 3 --ticks 3000

// ── Мир ──────────────────────────────────────────────────────────────────────
// Базовый мир 6000x4000 подбирался под окно 1200x800. В прежнем мире 60000x12000
// на 720 млн px² существа почти не встречали друг друга. Больший мир теперь
// получается масштабом (см. space.rs), при котором плотность всего живого
// остаётся прежней.
pub const WORLD_WIDTH: f64 = 6000.0;
pub const WORLD_HEIGHT: f64 = 4000.0;

/// Раз во сколько тиков существа пробуют делиться.
pub const DIVIDE_PERIOD: u64 = 30;

// ── Растения ────────────────────────────────────────────────────────────────
pub const PLANT_RADIUS: f64 = 10.0;
pub const ENERGY_FROM_PLANT: f64 = 50.0;
/// Доля сырой порции, реально усваиваемая при пятишаговом поедании.
pub const PLANT_BITE_YIELD: f64 = 0.44;

// Где растёт еда — профиль по глубине и по ширине (flora.rs), правила мира.
// By default: the «игровое» profile down, uniform across, in patches.

/// The «игровое» depth profile (`flora::Profile::Game`), a rough real sea: nutritious upper
/// layers and a nearly dead bottom, where the rot settles. Full food down to this share of the
/// depth, then the exponent with the profile's steepness over the rest, so at the default 8 the
/// bottom holds ~3000 times less food, like the plain exponent. Before it the default was the plain
/// exponent with steepness 8; the first «игровое» fell along a straight slope and a cosine.
pub const GAME_PLATEAU: f64 = 0.2;

/// Крутизна экспоненты по глубине: чем больше, тем плотнее еда прижата к
/// поверхности. При 8 у дна еды в e^8 ≈ 3000 раз меньше, чем наверху.
pub const PLANT_DEPTH_DECAY: f64 = 8.0;
/// Крутизна экспоненты по ширине, если её выбрать. Мягче, чем по глубине: при 8
/// почти вся еда жалась бы к левому краю, и мир справа пустовал бы.
pub const PLANT_WIDTH_DECAY: f64 = 3.0;
/// Dead zone at the very surface, % of depth; a property of the surface, so it applies to every
/// profile. It was 5% (200 at height 4000) and left an empty strip along the top; now plants grow
/// up to the surface.
pub const PLANT_TOP_MARGIN_PCT: f64 = 0.0;
/// Параметры остальных профилей, пока их не тронули: линейный — у дальнего
/// края 10% еды ближнего; логарифм — изгиб 20 (на середине оси ещё 79% еды, к
/// дальнему краю — обрыв до нуля); волны — 3 богатые полосы с размахом 80%
/// (между полосами еды в 9 раз меньше, чем на пике).
pub const PLANT_LINEAR_END: f64 = 10.0;
pub const PLANT_LOG_BEND: f64 = 20.0;
pub const PLANT_WAVES: f64 = 3.0;
pub const PLANT_WAVE_AMPLITUDE: f64 = 80.0;

/// Ожидаемое число новых растений за тик на пиксель площади (не вероятность!).
pub const DENSITY_PER_PIXEL: f64 = 1.0417e-7;
/// Для базового мира это 2.50008 растения за тик.
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

// ── Существа ──────────────────────────────────────────────────────────────
// Базовый геном существа — в таблице генов (`genome/creature.rs`).
pub const CREATURES_AT_START: usize = 20;
/// Разброс мутаций.
pub const MUTATION_SIGMA: f64 = 0.3;

/// Запас энергии = размер * это.
pub const ENERGY_PER_SIZE: f64 = 2.5;
/// Энергия, вложенная в единицу выросшего диаметра; запас энергии от неё не зависит.
pub const GROWTH_ENERGY_PER_SIZE: f64 = 2.25;
/// Минимум, что должно остаться у родителя после деления.
pub const REPRO_RESERVE: f64 = 20.0;
/// Фиксированный штраф за размножение.
pub const REPRO_COST: f64 = 10.0;

// ── Стоимость содержания статов (энергии за тик) ────────────────────────────
// расход = COEF * стат ** POWER, суммарно по трём статам.
//
// ПОКАЗАТЕЛИ ВАЖНЕЕ КОЭФФИЦИЕНТОВ: они решают, есть ли у эволюции компромисс.
// Выгода от стата растёт так:
//   размер   — радиус поедания равен размеру, значит охват ~ размер²
//   зрение   — радиус поиска еды, значит охват ~ зрение²
//   скорость — примерно линейно
// Цена обязана расти КРУЧЕ выгоды, иначе стат убегает вверх без предела:
// при показателях 1.5 для размера и 1.0 для зрения размер доходил до 450 при
// базовых 40, а зрение — до 1000 при базовых 400.
pub const SIZE_ENERGY_POWER: f64 = 2.5;
pub const SPEED_ENERGY_POWER: f64 = 2.0;
pub const SIGHT_ENERGY_POWER: f64 = 2.0;

// Коэффициенты нормированы так, чтобы базовый геном тратил три РАВНЫЕ доли и
// проживал на полном баке ~700 тиков без еды. Одно растение — полбака.
pub const SIZE_ENERGY_COEF: f64 = 4.706e-6;
pub const SPEED_ENERGY_COEF: f64 = 4.762e-4;
pub const SIGHT_ENERGY_COEF: f64 = 2.976e-7;

/// Двигать крупное тело дороже: цена скорости умножается на (размер / 40) ** это.
/// Базовый геном (диаметр 40) платит ровно столько же, сколько без множителя. Без него при обилии растений выживали гиганты размером 150‒250.
pub const SPEED_MASS_POWER: f64 = 1.0;

/// Потолок гена мутагенности (множитель на разброс мутаций и шанс смены
/// стратегии). При 10 сигма существ 3.0: геном потомка почти случаен —
/// дальше расти незачем, а без потолка множитель мог бы уйти в бесконечность.
pub const MAX_MUTABILITY: f64 = 10.0;
/// Flocks are off for now (the user's call, 2026-09-26: they spoiled more than they gave): every
/// founder is a loner and the pack gene never switches on, so no flock of two ever forms. The flock
/// code stays until it is removed under the tag `flocks-final`.
pub const FLOCKS: bool = false;
/// Floor of the mutability gene. Selection pulls it down (a less mutated child is fitter on
/// average), and at 0 evolution froze: one diet, one strategy, one flock kind, forever.
pub const MIN_MUTABILITY: f64 = 0.1;
/// Share of children born an exact copy of their parent, no gene mutated. A lineage keeps its
/// proven genome through them, so selection has less reason to push mutability down.
pub const CLONE_CHANCE: f64 = 0.5;

// ── Поиск соседей ───────────────────────────────────────────────────────────
/// Размер клетки сетки (grid.rs). В Python клетка равнялась самому большому
/// радиусу запроса, и одно дальнозоркое существо раздувало её всем. Здесь
/// клетка фиксирована, а запрос берёт столько клеток, сколько покрывает его
/// собственный радиус. 256 — порядок половины зрения: запрос обычно смотрит
/// 4x4‒5x5 клеток, а пустых клеток немного.
pub const GRID_CELL: f64 = 256.0;

// ── Стратегии ───────────────────────────────────────────────────────────────
/// Шанс, что потомок получит другую стратегию поведения (ген-выбор). Пока у
/// вида один вариант, ген инертен и жребий не тянется (`Mutation::Switch`).
pub const STRATEGY_SWITCH_CHANCE: f64 = 0.001;
/// Такой же редкий переход способности к дальнему бою.
pub const SHOOTER_SWITCH_CHANCE: f64 = 0.001;
/// Ближний удар: доля диаметра, одновременно базовый урон и цена энергии.
pub const MELEE_DAMAGE_SHARE: f64 = 0.05;
/// A bigger body strikes disproportionately harder: melee damage is times (attacker's size /
/// target's size) ** this when the attacker is the bigger (never less than ×1). Equal bodies still
/// trade ~20 strikes; 2× bigger needs ~5; 3× a carnivore kills a herbivore in two (0.075 · 3^2.25 /
/// 1.5 = 0.59 of its health a strike): a carp and a fry, not a duel. There is no cap on a strike's
/// share of the target's health any more; shots keep theirs. At 1.75 (3× = one blow) carnivores
/// boomed, ate the herbivores out and starved; at 1.0 they died out everywhere (seeds 1–8).
pub const MELEE_SIZE_POWER: f64 = 1.25;
/// Выстрел слабее ближнего удара, но требует собственного запаса энергии.
pub const SHOT_DAMAGE_SHARE: f64 = 0.01;
pub const SHOT_ENERGY_SHARE: f64 = 0.02;
pub const SHOT_PERIOD: u64 = 5;
pub const SHOT_RANGE_SIZES: f64 = 4.0;

/// Медленный ход: стратегия может идти на эту долю своей скорости и платить за
/// скорость по фактическому шагу — по тому же закону `speed ** SPEED_POWER`,
/// что и ген. При квадрате треть скорости стоит девятую часть, и весь расход
/// базового существа падает примерно до 70%: заметная экономия, но ищет
/// такое существо втрое медленнее.
pub const SLOW_PACE: f64 = 1.0 / 3.0;

// Combat and hunting are always on: the peaceful world (the old `cannibalism` rule) is gone.
// There is no world size ratio either: whom one attacks first is its own `prey_ratio` gene.

// ── Питание ─────────────────────────────────────────────────────────────────
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
/// past the omnivore (`genome::creature::DIET_LEAPS`; they replace its general jump): the omnivores
/// dwindle to 1–2% of a world, and the meat diets hung on them alone (user's choice, 2026-09-27).
pub const HERBIVORE_LEAP_CARNIVORE: f64 = 0.001;
pub const HERBIVORE_LEAP_SCAVENGER: f64 = 0.0001;
/// Chance that a mutating child's diet jumps to any other diet, neighbour or not, also whatever
/// the mutability: so a line is not locked into its branch forever.
pub const DIET_JUMP_CHANCE: f64 = 0.0001;
/// Strike damage by diet, times the world's `melee_damage_share` (and `shot_damage_share`): meat
/// eaters are built to kill. The omnivore strikes a little harder than the herbivore, the
/// scavenger harder still, the carnivore hardest: at ×1.5 a carnivore could hold only the newborns
/// it caught, and hunting did not pay (user's choice, 2026-09-27). The energy a strike costs does
/// not change.
pub const DIET_STRIKE: [f64; 4] = [1.0, 1.15, 1.3, 3.0];
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
/// it and cannot pay for it before it finds meat.
pub const DIET_SMELL: [f64; 4] = [1.0, 1.0, 3.0, 1.5];
/// Upkeep saved in the deep by diet: from `DEEP_SAVING_FROM` of the depth down to the bottom the
/// share grows linearly to this. The scavenger lives slowly in the cold dark where rot settles
/// and plants do not grow; above that depth it pays like everybody.
pub const DIET_DEEP_SAVING: [f64; 4] = [0.0, 0.0, 0.4, 0.0];
pub const DEEP_SAVING_FROM: f64 = 0.5;
/// Which food is a diet's own (plants, fresh meat, rot): a sated creature eats and goes for only
/// its own, and takes another niche's food only when hungry (the inherited `picky`). The omnivore
/// has no foreign food: it is the generalist.
pub const DIET_OWN: [[bool; 3]; 4] = [
    [true, false, false], // травоядный
    [true, true, true],   // всеядный
    [false, false, true], // падальщик
    [false, true, false], // мясоед
];
/// Founders dealt the scavenger diet start with this layer, % of depth (the `min_y`/`max_y`
/// genes): in the deep, where rot will settle. A start condition, not a rule — the genes mutate.
pub const SCAVENGER_START_LAYER: (f64, f64) = (50.0, 100.0);
/// Founders dealt a meat diet (scavenger, carnivore) start this many times bigger. Equal to the
/// others they had no prey (a hunter takes prey `prey_ratio` ≈ 3 times smaller, and newborns are
/// half grown) and starved by tick ~400 without a single strike. A start condition: the gene
/// mutates.
pub const MEAT_FOUNDER_SIZE: f64 = 2.0;
/// Digestibility by diet (order of `genome::creature::DIET_VARIANTS`): plants, fresh meat,
/// rot. For plants 1 is the world's yield `plant_bite_yield`; for meat it is the whole raw
/// portion — the diet alone decides how much of it is taken in (the old flat 10% fed a hunter
/// less for a whole corpse than one plant). 0 means the creature neither eats that food nor
/// goes for it. A specialist digests its own food fully; the
/// omnivore takes everything, but worse; rot feeds well only the scavenger, the others barely.
/// Meat-eaters get a little from plants (not their own food: they eat it only when hungry), so a
/// line of them is not starved out before it finds meat.
/// A piece of a rotting corpse is a mix: fresh and rot by the corpse's rot share.
pub const DIET_DIGESTION: [[f64; 3]; 4] = [
    [1.0, 0.0, 0.0],  // травоядный
    [0.7, 0.3, 0.05], // всеядный
    [0.15, 0.8, 0.9], // падальщик
    [0.2, 1.0, 0.1],  // мясоед
];
/// Plants while not grown to its own size (the size gene), by diet — a juvenile gut: a young
/// carnivore digests plants like an omnivore, grows on them and hunts once grown. A carnivore
/// mutant is born half grown with its herbivore parent's prey ratio, sees no prey that much
/// smaller than itself and starved on plants at 20%. Grown, it is back at `DIET_DIGESTION`. No
/// loophole in staying young: only the grown divide (57‒67% of carnivores were grown in every
/// variant of the sweep). The others digest plants young as grown.
pub const DIET_YOUNG_PLANTS: [f64; 4] = [1.0, 0.7, 0.15, 0.7];
/// Founders' diets, shares in the same order. Dealt without a draw. No meat eaters: at the start
/// there are neither corpses nor prey small enough, and every such founder starved (none struck
/// once in the user's world, 2026-09-27); the meat diets arise from mutants
/// (`DIET_MEAT_STEP_CHANCE`).
pub const DIET_START_MIX: [f64; 4] = [70.0, 30.0, 0.0, 0.0];

/// A creature eating stops at this share of its reach from the food's centre (a plant is reached
/// within one body diameter, a corpse within that plus its radius) instead of walking onto it:
/// the food lies beside the body, where the window draws the proboscis reaching it.
pub const EAT_STOP_SHARE: f64 = 0.85;

// ── Трупы ───────────────────────────────────────────────────────────────────
/// A corpse stays fresh this long and lies where the creature died.
pub const CORPSE_FRESH_TICKS: u64 = 150;
/// By then it is fully rotten: the rot share follows a smooth step from the fresh time to this one.
pub const CORPSE_ROTTEN_TICKS: u64 = 600;
/// After the fresh time it sinks this far a tick, straight down, and can be eaten all the way. A
/// speed, not a time to the bottom: with a time, a corpse in a world 15 500 deep (×20, 2:1) fell
/// 10‒15 a tick, as fast as a scavenger swims, and the scavengers never caught one. At 2 a fifth of
/// a base creature's speed, in a world of any height; in a tall one a corpse may decay on the way.
pub const CORPSE_SINK_SPEED: f64 = 2.0;
/// What is left disappears by then; the store decays evenly over the whole time, so a corpse
/// reaching its resting place untouched still holds a third of its meat for the scavengers.
pub const CORPSE_DECAY_TICKS: u64 = 1800;
/// Rot and skeletons rest in this lowest share of the depth, %, each at its own place (a hash of
/// the id): spread over the dead deep, not a line on the bottom (at 2% it was a thin strip).
pub const CORPSE_REST_PCT: f64 = 25.0;
/// A corpse eaten down to this share of its meat becomes a skeleton: bones and scraps, rot from
/// the start, that sink to the corpse's resting place. The hunters' last tenth feeds the scavengers.
pub const CORPSE_SKELETON_SHARE: f64 = 0.1;
/// A skeleton sinks to its place this far a tick: bones are heavier than a whole corpse,
pub const SKELETON_SINK_SPEED: f64 = 4.0;
/// and what is left of it decays evenly over this many ticks from the moment it was stripped.
pub const SKELETON_TICKS: u64 = 1800;

// ── Бегство ─────────────────────────────────────────────────────────────────
/// Существо бежит от чужого (не родни), который может его съесть, когда до
/// края его тела ближе этой доли своего зрения. Треть — как бежали от
/// хищников (тег `predators-final`): с дальним порогом мелкие только бегали бы
/// и не ели, а бежать надо успеть до того, как крупный дотянется.
pub const FLEE_SIGHT_SHARE: f64 = 1.0 / 3.0;
/// Сколько тиков существо бежит после испуга (секунда при 60 тиках в секунду):
/// угроза пропала из виду — ещё не значит, что она ушла.
pub const FLEE_TICKS: u32 = 60;

/// Предельный биологический возраст. Темп жизни меняет возраст за тик.
pub const LIFESPAN: f64 = 12_000.0;
