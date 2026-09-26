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
/// layers and a dead bottom, where the rot settles. Full food down to 20% of depth (the dead zone
/// at the very surface still applies), a straight slope to 15% of it at 85% of depth, then a
/// cosine fall to nothing on the bottom. Before it the default was the exponent with steepness 8.
pub const GAME_PLATEAU: f64 = 0.2;
pub const GAME_SLOPE_END: f64 = 0.85;
pub const GAME_SLOPE_LEVEL: f64 = 0.15;

/// Крутизна экспоненты по глубине: чем больше, тем плотнее еда прижата к
/// поверхности. При 8 у дна еды в e^8 ≈ 3000 раз меньше, чем наверху.
pub const PLANT_DEPTH_DECAY: f64 = 8.0;
/// Крутизна экспоненты по ширине, если её выбрать. Мягче, чем по глубине: при 8
/// почти вся еда жалась бы к левому краю, и мир справа пустовал бы.
pub const PLANT_WIDTH_DECAY: f64 = 3.0;
/// Мёртвая зона у самой поверхности, % глубины: при высоте 4000 это 200, как
/// было до профилей. Свойство поверхности, поэтому действует при любом профиле.
pub const PLANT_TOP_MARGIN_PCT: f64 = 5.0;
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
/// Chance that a child's diet steps to a neighbour (`genome::creature::DIET_NEIGHBOURS`): as rare
/// as the other choice genes.
pub const DIET_STEP_CHANCE: f64 = 0.001;
/// Chance that a child's diet jumps to any other diet, neighbour or not: ten times rarer than a
/// step, so a line is not locked into its branch forever.
pub const DIET_JUMP_CHANCE: f64 = 0.0001;
/// Strike damage by diet, times the world's `melee_damage_share` (and `shot_damage_share`): meat
/// eaters are built to kill. The omnivore strikes a little harder than the herbivore, the
/// scavenger harder still, the carnivore hardest. The energy a strike costs does not change.
pub const DIET_STRIKE: [f64; 4] = [1.0, 1.15, 1.3, 1.5];
/// Health by diet, times the body size: the herbivore is hardy. It cannot strike like a meat
/// eater, so it outlasts one — a hunter needs half as many strikes again, and weighs that.
pub const DIET_HEALTH: [f64; 4] = [1.5, 1.0, 1.0, 1.0];
/// The size term of upkeep by diet: plants are a steady, bulky food, so a herbivore carries a
/// big body cheaper — a little, so that size still has a price and selection, not the table,
/// makes it bigger.
pub const DIET_SIZE_COST: [f64; 4] = [0.85, 1.0, 1.0, 1.0];
/// The speed term of upkeep by diet: the carnivore is a runner built to chase — it moves cheaper.
/// Without an edge of its own it died out everywhere once the herbivore grew hardy and the
/// scavenger learned to smell (16 of 16 worlds, 2026-09-26).
pub const DIET_SPEED_COST: [f64; 4] = [1.0, 1.0, 1.0, 0.8];
/// How far corpses are sensed, in shares of vision: the scavenger smells them twice as far as it
/// sees. Smell is not paid for — only vision is (the scavenger's food lies scattered in the deep,
/// and it would never find it by sight alone).
pub const DIET_SMELL: [f64; 4] = [1.0, 1.0, 2.0, 1.0];
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
/// Digestibility by diet (order of `genome::creature::DIET_VARIANTS`): plants, fresh meat,
/// rot. For plants 1 is the world's yield `plant_bite_yield`; for meat it is the whole raw
/// portion — the diet alone decides how much of it is taken in (the old flat 10% fed a hunter
/// less for a whole corpse than one plant). 0 means the creature neither eats that food nor
/// goes for it. A specialist digests its own food fully; the
/// omnivore takes everything, but worse; rot feeds well only the scavenger, the others barely.
/// A piece of a rotting corpse is a mix: fresh and rot by the corpse's rot share.
pub const DIET_DIGESTION: [[f64; 3]; 4] = [
    [1.0, 0.0, 0.0],  // травоядный
    [0.7, 0.3, 0.05], // всеядный
    [0.0, 0.8, 0.9],  // падальщик
    [0.0, 1.0, 0.1],  // мясоед
];
/// Founders' diets, shares in the same order. Dealt without a draw.
pub const DIET_START_MIX: [f64; 4] = [55.0, 25.0, 10.0, 10.0];

/// A creature eating stops at this share of its reach from the food's centre (a plant is reached
/// within one body diameter, a corpse within that plus its radius) instead of walking onto it:
/// the food lies beside the body, where the window draws the proboscis reaching it.
pub const EAT_STOP_SHARE: f64 = 0.85;

// ── Трупы ───────────────────────────────────────────────────────────────────
/// A corpse stays fresh this long and lies where the creature died.
pub const CORPSE_FRESH_TICKS: u64 = 150;
/// By then it is fully rotten and has sunk to the bottom: rot share and depth follow one smooth
/// step from the fresh time to this one.
pub const CORPSE_ROTTEN_TICKS: u64 = 600;
/// What is left disappears by then; the store decays evenly over the whole time, so a corpse
/// reaching the bottom untouched still holds two thirds of its meat for the scavengers.
pub const CORPSE_DECAY_TICKS: u64 = 1800;
/// Rot lies in this lowest share of the depth, %: the bottom, where no plants grow.
pub const CORPSE_BOTTOM_PCT: f64 = 2.0;
/// A corpse eaten down to this share of its meat becomes a skeleton: bones and scraps, rot from
/// the start, that sink to the deep. The hunters' last tenth feeds the scavengers.
pub const CORPSE_SKELETON_SHARE: f64 = 0.1;
/// Skeletons settle in this lowest share of the depth, %: near the bottom, spread over the dead
/// deep rather than all on it.
pub const SKELETON_ZONE_PCT: f64 = 15.0;
/// A skeleton sinks to its place in this many ticks,
pub const SKELETON_SINK_TICKS: u64 = 300;
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
