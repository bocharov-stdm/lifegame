//! Фенотип хищника: всё, что выводится из генома и правил мира при рождении.
//! Правила меняются на ходу — фенотип пересчитывается целиком
//! (`Predator::apply_rules`). Новый ген хищника получает действие здесь.

use super::{Predator, Strategy};
use crate::config::{PREDATOR_HUNGRY, SLOW_PACE};
use crate::genome::PredatorGenome;
use crate::genome::predator::Gene;
use crate::rules::Rules;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    pub speed: f64,
    pub vision: f64,
    /// Бак — из правил мира, у всех хищников один.
    pub max_energy: f64,
    /// Расход за тик. Размер у хищника постоянный (`Predator::DIAM`).
    pub upkeep: f64,
    /// Медленный ход (`SLOW_PACE`): шаг и расход за тик на нём.
    pub slow_speed: f64,
    pub slow_upkeep: f64,

    /// Стратегия поведения (`strategy.rs`).
    pub strategy: Strategy,
}

impl Phenotype {
    /// Голодный охотится; сытый (бак выше `PREDATOR_HUNGRY`) бродит и не ест.
    #[inline(always)]
    pub fn hungry(&self, energy: f64) -> bool {
        energy < self.max_energy * PREDATOR_HUNGRY
    }

    pub fn of(genome: &PredatorGenome, rules: &Rules) -> Self {
        let (speed, vision) = (genome[Gene::Speed], genome[Gene::Vision]);
        let slow_speed = speed * SLOW_PACE;
        Phenotype {
            speed,
            vision,
            max_energy: rules.predator_max_energy,
            upkeep: rules.upkeep(Predator::DIAM, speed, vision),
            slow_speed,
            slow_upkeep: rules.upkeep(Predator::DIAM, slow_speed, vision),
            strategy: Strategy::from_gene(genome[Gene::Strategy]),
        }
    }
}
