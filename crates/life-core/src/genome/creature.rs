//! Геном существа.

use super::{GeneKind, GeneSpec, Genome, Mutation, Variant, bases};
use crate::config::{SHOOTER_SWITCH_CHANCE, STRATEGY_SWITCH_CHANCE};
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
    LifePace,
    Bravery,
    Carnivory,
    PreyRatio,
    Sociability,
    Shooter,
    FirePreference,
    FireReserve,
    PackInstinct,
    Territoriality,
    Care,
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
        Gene::LifePace,
        Gene::Bravery,
        Gene::Carnivory,
        Gene::PreyRatio,
        Gene::Sociability,
        Gene::Shooter,
        Gene::FirePreference,
        Gene::FireReserve,
        Gene::PackInstinct,
        Gene::Territoriality,
        Gene::Care,
    ];
}

pub const N: usize = 20;

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
        base: 30.0,
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
    GeneSpec {
        key: "life_pace",
        label: "темп_жизни",
        about: "Быстрее рост и рождения, дороже содержание и короче жизнь (0,5–2).",
        kind: GeneKind::Absolute,
        base: 1.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "bravery",
        label: "храбрость",
        about: "До какой потери здоровья продолжает защищаться, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "carnivory",
        label: "плотоядность",
        about: "Лучше усваивает добычу, хуже растения, %.",
        kind: GeneKind::Percent,
        base: 25.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "prey_ratio",
        label: "отношение_добычи",
        about: "Во сколько раз добыча меньше охотника (1–5).",
        kind: GeneKind::Absolute,
        base: 2.5,
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
        mutation: Mutation::Switch { chance: SHOOTER_SWITCH_CHANCE },
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
        label: "забота_о_детях",
        about: "Готовность защищать и кормить собственных детёнышей, %.",
        kind: GeneKind::Percent,
        base: 50.0,
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

    /// Геном потомка (см. `mutate_values`).
    pub fn mutate(&self, sigma: f64, rng: &mut Rng) -> Self {
        let mut child = *self;
        let mutability = super::mutability_of(self[Gene::Mutability]);
        super::mutate_values(&mut child.0, &GENES, sigma, mutability, rng);
        child.0[Gene::LifePace as usize] = child[Gene::LifePace].clamp(0.5, 2.0);
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
        assert!((base.0 - 0.3 * 0.8).abs() < 0.03, "при 1 разброс — сигма правил: {:.3}", base.0);
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
