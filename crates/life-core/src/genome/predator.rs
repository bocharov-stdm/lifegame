//! Геном хищника.

use super::{GeneKind, GeneSpec, Genome, Mutation, bases};
use crate::config::{PREDATOR_BASE_SPEED, PREDATOR_BASE_VISION, PREDATOR_SIGMA, STRATEGY_SWITCH_CHANCE};
use crate::predator::strategy::VARIANTS as STRATEGIES;
use crate::rng::Rng;

/// Гены хищника — номера строк `GENES`.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gene {
    Speed,
    Vision,
    Strategy,
}

impl Gene {
    pub const ALL: [Gene; N] = [Gene::Speed, Gene::Vision, Gene::Strategy];
}

pub const N: usize = 3;

/// Мутация хищника: с шансом 0.4 ген меняется (жребий «оставить» — больше
/// 0.6), множитель без нижней границы, сигма `PREDATOR_SIGMA`.
const SCALE: Mutation = Mutation::Scale { keep_above: Some(0.6), reject_below: None };

/// Таблица генов. Только дописывать в конец (см. `genome/mod.rs`).
pub const GENES: [GeneSpec; N] = [
    GeneSpec {
        key: "speed",
        label: "скорость",
        about: "Шаг за тик; вблизи добычи — рывок вдвое быстрее.",
        kind: GeneKind::Absolute,
        base: PREDATOR_BASE_SPEED,
        mutation: SCALE,
    },
    GeneSpec {
        key: "vision",
        label: "зрение",
        about: "Радиус, в котором видит добычу (до края её тела).",
        kind: GeneKind::Absolute,
        base: PREDATOR_BASE_VISION,
        mutation: SCALE,
    },
    GeneSpec {
        key: "strategy",
        label: "стратегия",
        about: "Как себя ведёт: варианты — в `predator/strategy.rs`.",
        kind: GeneKind::Choice(&STRATEGIES),
        base: 0.0,
        mutation: Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE },
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredatorGenome([f64; N]);

impl PredatorGenome {
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
    pub fn mutate(&self, rng: &mut Rng) -> Self {
        let mut child = *self;
        super::mutate_values(&mut child.0, &GENES, PREDATOR_SIGMA, rng);
        child
    }
}

impl core::ops::Index<Gene> for PredatorGenome {
    type Output = f64;

    #[inline]
    fn index(&self, gene: Gene) -> &f64 {
        &self.0[gene as usize]
    }
}

impl Genome for PredatorGenome {
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
        assert_eq!(PredatorGenome::BASE[Gene::Speed], PREDATOR_BASE_SPEED);
    }
}
