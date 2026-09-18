//! Геном травоядного.

use super::{GeneKind, GeneSpec, Genome, Mutation, bases};
use crate::config::STRATEGY_SWITCH_CHANCE;
use crate::rng::Rng;
use crate::vegetarian::strategy::VARIANTS as STRATEGIES;

/// Гены травоядного — номера строк `GENES`.
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
    ];
}

pub const N: usize = 8;

/// Мутация травоядных: множитель не ниже 0.1, выпавшее ниже перетягивается
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
        about: "Радиус поиска еды и хищников.",
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
        about: "Верхняя граница слоя обитания, % глубины мира.",
        kind: GeneKind::Percent,
        base: 5.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "max_y",
        label: "max_y%",
        about: "Нижняя граница слоя обитания, % глубины мира.",
        kind: GeneKind::Percent,
        base: 100.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "strategy",
        label: "стратегия",
        about: "Как себя ведёт: варианты — в `vegetarian/strategy.rs`.",
        kind: GeneKind::Choice(&STRATEGIES),
        base: 0.0,
        mutation: Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE },
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VegetarianGenome([f64; N]);

impl VegetarianGenome {
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
        super::mutate_values(&mut child.0, &GENES, sigma, rng);
        child
    }
}

impl core::ops::Index<Gene> for VegetarianGenome {
    type Output = f64;

    #[inline]
    fn index(&self, gene: Gene) -> &f64 {
        &self.0[gene as usize]
    }
}

impl Genome for VegetarianGenome {
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
        assert_eq!(VegetarianGenome::BASE.get("vision"), Some(400.0));
    }
}
