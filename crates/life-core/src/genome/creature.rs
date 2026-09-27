//! Геном существа.

use super::{GeneKind, GeneSpec, Genome, Mutation, Variant, bases};
use crate::config::{
    DIET_JUMP_CHANCE, DIET_MEAT_STEP_CHANCE, DIET_STEP_CHANCE, FLOCKS, HERBIVORE_LEAP_CARNIVORE,
    HERBIVORE_LEAP_SCAVENGER, LIFESPAN_BASE, LIFESPAN_MAX, LIFESPAN_MIN, SHOOTER_SWITCH_CHANCE,
    STRATEGY_SWITCH_CHANCE,
};
use crate::creature::strategy::VARIANTS as STRATEGIES;
use crate::rng::Rng;

/// Гены существа — номера строк `GENES`.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gene {
    Size,
    Speed,
    Vision,
    ReproThreshold,
    ReproShare,
    MinY,
    MaxY,
    Strategy,
    Mutability,
    Maturation,
    Bravery,
    Diet,
    PreyRatio,
    Sociability,
    Shooter,
    FirePreference,
    FireReserve,
    PackInstinct,
    Territoriality,
    Care,
    FlockKind,
    LayerBound,
    FlockSpacing,
    Caution,
    Forage,
    Picky,
    Rivalry,
    Lifespan,
}

impl Gene {
    pub const ALL: [Gene; N] = [
        Gene::Size,
        Gene::Speed,
        Gene::Vision,
        Gene::ReproThreshold,
        Gene::ReproShare,
        Gene::MinY,
        Gene::MaxY,
        Gene::Strategy,
        Gene::Mutability,
        Gene::Maturation,
        Gene::Bravery,
        Gene::Diet,
        Gene::PreyRatio,
        Gene::Sociability,
        Gene::Shooter,
        Gene::FirePreference,
        Gene::FireReserve,
        Gene::PackInstinct,
        Gene::Territoriality,
        Gene::Care,
        Gene::FlockKind,
        Gene::LayerBound,
        Gene::FlockSpacing,
        Gene::Caution,
        Gene::Forage,
        Gene::Picky,
        Gene::Rivalry,
        Gene::Lifespan,
    ];
}

pub const N: usize = 28;

pub const PACK_VARIANTS: [Variant; 2] = [
    Variant {
        key: "solitary", label: "одиночка", about: "Не образует стаю с потомками."
    },
    Variant {
        key: "social", label: "стайный", about: "Потомки могут оставаться в семейной стае."
    },
];

pub const TERRITORIALITY_VARIANTS: [Variant; 3] = [
    Variant { key: "none", label: "нет", about: "Не защищает территорию." },
    Variant {
        key: "moderate", label: "умеренная", about: "Предупреждает чужака перед защитой."
    },
    Variant {
        key: "hard", label: "жёсткая", about: "Защищает территорию сразу после вторжения."
    },
];

/// Наследуемая возможность стрелять. Пять процентов основателей — стрелки.
pub const SHOOTER_VARIANTS: [Variant; 2] = [
    Variant {
        key: "no", label: "без выстрела", about: "Атакует только при соприкосновении."
    },
    Variant {
        key: "yes", label: "стреляет", about: "Может потратить энергию на слабый дальний удар."
    },
];

/// How a family flock's circle moves (`flock.rs`). Labels are game UI and stay Russian.
pub const FLOCK_KIND_VARIANTS: [Variant; 4] = [
    Variant {
        key: "settled",
        label: "оседлые",
        about: "Круг стоит на месте и переезжает, когда еда в нём кончается.",
    },
    Variant {
        key: "nomadic",
        label: "кочевые",
        about: "Круг медленно идёт по курсу вдоль слоя и разворачивается у преград.",
    },
    Variant {
        key: "scout", label: "разведчики", about: "Круг идёт к еде, которую заметили участники."
    },
    Variant {
        key: "migrant", label: "мигранты", about: "Круг циклично ходит вверх и вниз по глубине."
    },
];

/// Which diets a child's diet may step to: the herbivore only to the omnivore; the omnivore is a
/// fork to the herbivore, the scavenger and the carnivore, a third each; the scavenger and the
/// carnivore to each other and back to the omnivore. In a chain one meat diet could arise only
/// from the other, and whichever stood last never appeared in the validation worlds.
pub const DIET_NEIGHBOURS: [&[usize]; 4] = [&[1], &[0, 2, 3], &[1, 3], &[1, 2]];

/// The neighbours towards meat: the herbivore to the omnivore, the omnivore to the scavenger and
/// the carnivore. A step there has its own, larger chance (`DIET_MEAT_STEP_CHANCE`): meat-eating
/// founders starve before there is meat, so the meat diets arise from mutants.
pub const DIET_TOWARDS_MEAT: [&[usize]; 4] = [&[1], &[2, 3], &[], &[]];

/// A diet's own leaps past its neighbours, with their chances: the herbivore may leap straight to
/// the carnivore or the scavenger (`HERBIVORE_LEAP_*`), so the meat diets do not hang on the
/// omnivores alone, who dwindle to 1–2% of a world. The others keep the general jump.
pub const DIET_LEAPS: [&[(usize, f64)]; 4] =
    [&[(3, HERBIVORE_LEAP_CARNIVORE), (2, HERBIVORE_LEAP_SCAVENGER)], &[], &[], &[]];

/// What a creature can digest (`config::DIET_DIGESTION`). A mutation steps to a neighbour in
/// `DIET_NEIGHBOURS`. Labels are game UI.
pub const DIET_VARIANTS: [Variant; 4] = [
    Variant {
        key: "herbivore",
        label: "травоядный",
        about: "Ест только растения и усваивает их лучше всех. Крепкий: здоровья в полтора раза больше, крупное тело дешевле.",
    },
    Variant {
        key: "omnivore",
        label: "всеядный",
        about: "Ест растения, свежее мясо и гниль, но всё усваивает хуже специалистов. Чужой пищи для него нет.",
    },
    Variant {
        key: "scavenger",
        label: "падальщик",
        about: "Ест только мясо: гниль усваивает лучше всех, свежее — чуть хуже мясоеда, но берёт его только голодным. Чует трупы втрое дальше, чем видит; в глубине живёт экономнее.",
    },
    Variant {
        key: "carnivore",
        label: "мясоед",
        about: "Ест мясо: свежее усваивает полностью, гниль — едва и только голодным. Пока не вырос, растёт на растениях, как всеядный. Бьёт сильнее всех, бегает дешевле, чует трупы в полтора раза дальше, чем видит.",
    },
];

/// Whether the depth layer genes hold the creature (and its flock's circle).
pub const LAYER_VARIANTS: [Variant; 2] = [
    Variant {
        key: "bound", label: "держится слоя", about: "Без еды возвращается в свой слой."
    },
    Variant {
        key: "free", label: "свободно", about: "Не привязан к слою: бродит по всей глубине."
    },
];

/// Мутация существ: множитель не ниже 0.1, выпавшее ниже перетягивается
/// заново, как в Python. Сигма — из правил мира.
const SCALE: Mutation = Mutation::Scale { keep_above: None, reject_below: Some(-0.9) };

/// Таблица генов. Только дописывать в конец (см. `genome/mod.rs`).
///
/// Процентные гены при мутации держатся в 0‒100. Для слоя это граница мира;
/// для порога и доли выше 100 размножение всё равно невозможно, но число
/// вроде 180% в среднем геноме только путало бы.
pub const GENES: [GeneSpec; N] = [
    GeneSpec {
        key: "size",
        label: "размер",
        about: "Диаметр тела: радиус поедания и запас энергии.",
        kind: GeneKind::Absolute,
        base: 40.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "speed",
        label: "скорость",
        about: "Шаг за тик.",
        kind: GeneKind::Absolute,
        base: 10.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "vision",
        label: "зрение",
        about: "Радиус поиска еды.",
        kind: GeneKind::Absolute,
        base: 400.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "repro_threshold",
        label: "порог_разм",
        about: "С какой доли полного бака делится, %.",
        kind: GeneKind::Percent,
        base: 70.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "repro_share",
        label: "доля_потомку",
        about: "Сколько энергии отдаёт потомку, %.",
        kind: GeneKind::Percent,
        base: 40.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "min_y",
        label: "min_y%",
        about: "Верх слоя, где держится, % глубины мира; за видимой едой выходит из слоя.",
        kind: GeneKind::Percent,
        base: 5.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "max_y",
        label: "max_y%",
        about: "Низ слоя, где держится, % глубины мира; без еды возвращается в слой.",
        kind: GeneKind::Percent,
        base: 100.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "strategy",
        label: "стратегия",
        about: "Как себя ведёт; потомок изредка получает другую.",
        kind: GeneKind::Choice(&STRATEGIES),
        base: 0.0,
        mutation: Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "mutability",
        label: "мутагенность",
        about: "Множитель на разброс мутаций у потомка — всех генов, и этого тоже.",
        kind: GeneKind::Absolute,
        base: 1.0,
        mutation: SCALE,
    },
    // Replaced the numeric `life_pace` in place (see `genome/mod.rs`): the same law, so the same
    // draws.
    GeneSpec {
        key: "maturation",
        label: "взросление",
        about: "Какая доля съеденного идёт в рост, пока не вырос; остальное — в запас. Быстро растущий ходит с пустым баком, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "bravery",
        label: "храбрость",
        about: "До какой потери здоровья защищается; чем храбрее, тем ближе подпускает чужака, который ни на кого не охотится, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    // Replaced the numeric `carnivory` in place (see `genome/mod.rs`).
    GeneSpec {
        key: "diet",
        label: "питание",
        about: "Что ест и как усваивает; потомок изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед, падальщик ↔ мясоед; травоядный изредка сразу становится мясоедом, ещё реже падальщиком; остальные совсем редко перескакивают в любое питание. Мясоед бьёт сильнее всех, падальщик и всеядный слабее.",
        kind: GeneKind::Choice(&DIET_VARIANTS),
        base: 0.0,
        mutation: Mutation::Neighbours {
            chance: DIET_STEP_CHANCE,
            rise: DIET_MEAT_STEP_CHANCE,
            jump: DIET_JUMP_CHANCE,
            of: &DIET_NEIGHBOURS,
            up: &DIET_TOWARDS_MEAT,
            leaps: &DIET_LEAPS,
        },
    },
    GeneSpec {
        key: "prey_ratio",
        label: "отношение_добычи",
        about: "Во сколько раз добыча меньше охотника (1–5).",
        kind: GeneKind::Absolute,
        base: 1.5,
        mutation: SCALE,
    },
    GeneSpec {
        key: "sociability",
        label: "общительность",
        about: "Привязанность к своим, сообщения и помощь ценой личного времени, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "shooter",
        label: "стрелок",
        about: "Редко наследуемая способность стрелять на расстоянии.",
        kind: GeneKind::Choice(&SHOOTER_VARIANTS),
        base: 0.0,
        mutation: Mutation::Switch { chance: SHOOTER_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "fire_preference",
        label: "предпочтение_выстрела",
        about: "Насколько рано стреляет при сближении с целью, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "fire_reserve",
        label: "резерв_стрельбы",
        about: "Минимальная доля запаса энергии после выстрела, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "pack_instinct",
        label: "стайность",
        about: "Живёт в семейной стае или отдельно от потомков.",
        kind: GeneKind::Choice(&PACK_VARIANTS),
        base: 1.0,
        // flocks are off: a loner's child never turns flocking (the draw stays)
        mutation: Mutation::Switch { chance: if FLOCKS { SHOOTER_SWITCH_CHANCE } else { 0.0 } },
    },
    GeneSpec {
        key: "territoriality",
        label: "территориальность",
        about: "Не защищает территорию, предупреждает чужака или нападает сразу.",
        kind: GeneKind::Choice(&TERRITORIALITY_VARIANTS),
        base: 1.0,
        mutation: Mutation::Switch { chance: SHOOTER_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "care",
        label: "защита_детей",
        about: "Защищает своих невзрослых детей и не трогает их, пока они растут; при малой заботе узнаёт только самых маленьких, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "flock_kind",
        label: "тип_стаи",
        about: "Как перемещается круг семейной стаи.",
        kind: GeneKind::Choice(&FLOCK_KIND_VARIANTS),
        base: 0.0,
        mutation: Mutation::Switch { chance: SHOOTER_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "layer_bound",
        label: "слой",
        about: "Держится ли своего слоя глубины или бродит по всей глубине.",
        kind: GeneKind::Choice(&LAYER_VARIANTS),
        base: 0.0,
        mutation: Mutation::Switch { chance: SHOOTER_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "flock_spacing",
        label: "простор_стаи",
        about: "Радиус круга стаи на корень из числа участников (50–500).",
        kind: GeneKind::Absolute,
        base: 200.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "caution",
        label: "осторожность",
        about: "Насколько охотник боится ответных ударов добычи и её видимых союзников, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "forage",
        label: "вылазки",
        about: "Ниже этой доли запаса член стаи кормится и вне своего круга, пока не наберёт в 1,75 раза больше, %.",
        kind: GeneKind::Percent,
        base: 40.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "picky",
        label: "разборчивость",
        about: "Ниже этой доли запаса ест и чужую пищу (падальщик — свежее мясо, мясоед — гниль), выше — только свою, %.",
        kind: GeneKind::Percent,
        base: 30.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "rivalry",
        label: "задиристость",
        about: "Ниже этой доли запаса бьёт у еды чужака не из своей стаи, если тот мельче в «отношение_добычи» раз; сытым ест рядом мирно, %.",
        kind: GeneKind::Percent,
        base: 30.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "lifespan",
        label: "срок_жизни",
        about: "Сколько тиков живёт. С 70% срока слабеет: к 90% скорость, зрение, удар и здоровье — 70% прежних (500–10 000).",
        kind: GeneKind::Absolute,
        base: LIFESPAN_BASE,
        mutation: SCALE,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreatureGenome([f64; N]);

impl CreatureGenome {
    /// Стартовый геном — базы таблицы.
    pub const BASE: Self = Self(bases(&GENES));

    pub const fn from_values(values: [f64; N]) -> Self {
        Self(values)
    }

    pub const fn to_values(&self) -> [f64; N] {
        self.0
    }

    /// Тот же геном с другим значением одного гена.
    pub const fn with(mut self, gene: Gene, value: f64) -> Self {
        self.0[gene as usize] = value;
        self
    }

    /// The child's genome with config's heredity and this sigma (`mutate_by`).
    pub fn mutate(&self, sigma: f64, rng: &mut Rng) -> Self {
        self.mutate_by(&super::Heredity::with_sigma(sigma), rng)
    }

    /// The child's genome: an exact copy with `clone_share`, otherwise mutated (`mutate_values`).
    pub fn mutate_by(&self, h: &super::Heredity, rng: &mut Rng) -> Self {
        let mut child = *self;
        if rng.random() < h.clone_share {
            return child; // an exact copy: no gene mutates
        }
        let mutability = super::mutability_of(self[Gene::Mutability], h.min_mutability);
        // `DIET_LEAPS` with the world's chances, in the same order
        let herbivore = [(3, h.diet_leap_carnivore), (2, h.diet_leap_scavenger)];
        let diet = super::DietChances {
            step: h.diet_step,
            rise: h.diet_meat_step,
            jump: h.diet_jump,
            leaps: [&herbivore, &[], &[], &[]],
        };
        super::mutate_values(&mut child.0, &GENES, h.sigma, mutability, rng, Some(diet));
        child.0[Gene::Mutability as usize] = super::mutability_of(child[Gene::Mutability], h.min_mutability);
        child.0[Gene::Lifespan as usize] = child[Gene::Lifespan].clamp(LIFESPAN_MIN, LIFESPAN_MAX);
        child.0[Gene::PreyRatio as usize] = child[Gene::PreyRatio].clamp(1.0, 5.0);
        child
    }
}

impl core::ops::Index<Gene> for CreatureGenome {
    type Output = f64;

    #[inline]
    fn index(&self, gene: Gene) -> &f64 {
        &self.0[gene as usize]
    }
}

impl Genome for CreatureGenome {
    const GENES: &'static [GeneSpec] = &GENES;

    fn values(&self) -> &[f64] {
        &self.0
    }

    fn values_mut(&mut self) -> &mut [f64] {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn номера_генов_совпадают_с_таблицей() {
        for (i, g) in Gene::ALL.iter().enumerate() {
            assert_eq!(*g as usize, i);
        }
        let mut keys: Vec<&str> = GENES.iter().map(|g| g.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), N, "имена генов не повторяются");
        assert_eq!(CreatureGenome::BASE.get("vision"), Some(400.0));
        assert_eq!(CreatureGenome::BASE[Gene::Shooter], 0.0);
        assert_eq!(CreatureGenome::BASE[Gene::FirePreference], 50.0);
        assert_eq!(CreatureGenome::BASE[Gene::FireReserve], 50.0);
        assert_eq!(CreatureGenome::BASE[Gene::PackInstinct], 1.0);
        assert_eq!(CreatureGenome::BASE[Gene::Territoriality], 1.0);
        assert_eq!(CreatureGenome::BASE[Gene::Care], 50.0);
        assert_eq!(CreatureGenome::BASE[Gene::Diet], 0.0, "a spawned creature is a herbivore");
    }

    #[test]
    fn диета_меняется_редко_и_почти_всегда_на_шаг() {
        // the neighbours go both ways
        for (k, near) in DIET_NEIGHBOURS.iter().enumerate() {
            assert!(near.iter().all(|&j| j != k && DIET_NEIGHBOURS[j].contains(&k)), "{k}: {near:?}");
        }
        assert_eq!(DIET_NEIGHBOURS[0], &[1]);
        for (k, up) in DIET_TOWARDS_MEAT.iter().enumerate() {
            assert!(up.iter().all(|j| DIET_NEIGHBOURS[k].contains(j)), "{k}: up is among the neighbours");
        }
        let mut rng = Rng::new(21);
        let n = 200_000;
        for (start, near) in DIET_NEIGHBOURS.iter().enumerate() {
            let parent = CreatureGenome::BASE.with(Gene::Diet, start as f64);
            let (mut steps, mut jumps) = (0, 0);
            for _ in 0..n {
                let child = parent.mutate(0.3, &mut rng)[Gene::Diet];
                assert_eq!(child.fract(), 0.0);
                if child != start as f64 {
                    if near.contains(&(child as usize)) { steps += 1 } else { jumps += 1 }
                }
            }
            // half the children are copies; of the rest a step towards meat 2%, another step 0.5%,
            // a jump 0.01% of which some land on neighbours
            let up = DIET_TOWARDS_MEAT[start];
            let chance = if up.is_empty() { 0.0 } else { DIET_MEAT_STEP_CHANCE }
                + if near.len() > up.len() { DIET_STEP_CHANCE } else { 0.0 };
            let expected = n as f64 * 0.5 * chance;
            assert!(
                (expected * 0.85..=expected * 1.15 + 20.0).contains(&(steps as f64)),
                "steps from {start}: {steps} of {n}, expected {expected}"
            );
            let far = 3 - near.len();
            if start == 0 {
                // the herbivore's own leaps: to the carnivore 0.1%, to the scavenger 0.01%
                let expected = n as f64 * 0.5 * (HERBIVORE_LEAP_CARNIVORE + HERBIVORE_LEAP_SCAVENGER);
                assert!(
                    (expected * 0.7..=expected * 1.3).contains(&(jumps as f64)),
                    "leaps from the herbivore: {jumps}, expected {expected}"
                );
            } else {
                assert!(jumps <= 12 * far && (far == 0 || jumps >= 1), "jumps from {start}: {jumps}");
            }
        }
    }

    /// Мутагенность родителя растягивает разброс всех генов, и свой тоже, и
    /// чаще меняет стратегию; потолок `MAX_MUTABILITY` держит её конечной.
    #[test]
    fn мутагенность_растягивает_разброс_потомков() {
        let spread = |m: f64| {
            let parent = CreatureGenome::BASE.with(Gene::Mutability, m);
            let mut rng = Rng::new(3);
            let (mut size, mut own) = (0.0, 0.0);
            for _ in 0..2000 {
                let child = parent.mutate(0.3, &mut rng);
                size += (child[Gene::Size] / 40.0 - 1.0).abs();
                own += (child[Gene::Mutability] / m - 1.0).abs();
            }
            (size / 2000.0, own / 2000.0)
        };
        let (low, high) = (spread(0.2), spread(2.0));
        assert!(high.0 > low.0 * 5.0, "размер: {:.3} против {:.3}", high.0, low.0);
        assert!(high.1 > low.1 * 5.0, "сама мутагенность: {:.3} против {:.3}", high.1, low.1);
        let switches = |m: f64| {
            let parent = CreatureGenome::BASE.with(Gene::Mutability, m);
            let mut rng = Rng::new(317);
            (0..50_000).filter(|_| parent.mutate(0.0, &mut rng)[Gene::Strategy] != 0.0).count()
        };
        let (rare, frequent) = (switches(0.2), switches(2.0));
        assert!(frequent > rare * 3, "смена стратегии: {frequent} против {rare}");
        let base = spread(1.0);
        let expected = 0.3 * 0.8 * (1.0 - crate::config::CLONE_CHANCE);
        assert!(
            (base.0 - expected).abs() < 0.03,
            "at 1 the spread is the rules' sigma (half are copies): {:.3}",
            base.0
        );
        let capped = CreatureGenome::BASE.with(Gene::Mutability, 1e300).mutate(0.3, &mut Rng::new(1));
        assert!(capped.to_values().iter().all(|v| v.is_finite()), "потолок: геном конечен");
    }

    #[test]
    fn способность_стрелять_возникает_редко_и_наследуется() {
        let mut rng = Rng::new(81);
        let mut shooters = 0;
        for _ in 0..20_000 {
            let child = CreatureGenome::BASE.mutate(0.0, &mut rng);
            shooters += (child[Gene::Shooter] == 1.0) as usize;
        }
        assert!((5..=40).contains(&shooters), "редкие стрелки: {shooters}");
        let parent = CreatureGenome::BASE.with(Gene::Shooter, 1.0);
        let mut inherited = 0;
        for _ in 0..1000 {
            inherited += (parent.mutate(0.0, &mut rng)[Gene::Shooter] == 1.0) as usize;
        }
        assert!(inherited >= 990, "способность обычно наследуется: {inherited}");
    }
}
