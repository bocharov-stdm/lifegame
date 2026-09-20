//! Стратегии поведения существа.
//!
//! Ход делится на две части: стратегия **решает**, куда идти (`decide`), а
//! существо **делает** шаг (`Creature::act`: движение ровно на speed, зажим в
//! свою полосу, расход, смерть). Стратегия видит только себя (`Me`), свою
//! память (`Mind`), свой генератор и чувства — двигать, кормить или делить
//! существо она не может. Это свойство понадобится параллельному тику: решения
//! можно будет принимать одновременно.
//!
//! Стратегия — ген (`genome/creature.rs`): наследуется, мутирует, отбор
//! решает, какая выживет. Новая стратегия — вариант `Strategy` и `VARIANTS` в
//! конец, свой файл с `decide`, новое состояние — в `Mind`.

use super::{Kinship, Phenotype};
use super::{lurker, standard};
use crate::genome::Variant;
use crate::rng::Rng;
use crate::senses::Senses;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Strategy {
    /// Бежит от опасных чужаков; иначе идёт к ближайшему растению; иначе
    /// бродит в своём слое.
    #[default]
    Standard,
    /// Как стандартный, но без еды в виду бродит медленно и дёшево.
    Lurker,
}

impl Strategy {
    /// Все варианты в порядке `VARIANTS`.
    pub const ALL: [Strategy; 2] = [Strategy::Standard, Strategy::Lurker];

    /// Стратегия по значению гена — номеру варианта.
    #[inline]
    pub fn from_gene(value: f64) -> Strategy {
        debug_assert!(
            value >= 0.0 && value.fract() == 0.0 && (value as usize) < Self::ALL.len(),
            "ген стратегии вне вариантов: {value}"
        );
        Self::ALL.get(value as usize).copied().unwrap_or_default()
    }
}

/// Варианты гена стратегии — в порядке `Strategy`. Только дописывать в конец.
pub const VARIANTS: [Variant; 2] = [
    Variant {
        key: "standard",
        label: "стандартный",
        about: "Идёт к ближайшему растению, иначе бродит в своём слое.",
    },
    Variant {
        key: "lurker",
        label: "затаившийся",
        about: "Как стандартный, но пока не видит еды, бродит втрое медленнее — и тратит меньше.",
    },
];

/// Память существа между ходами. Общая для всех стратегий: новое состояние
/// дописывается сюда (структура остаётся `Copy`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mind {
    /// Выбранная цель удара в текущем тике.
    pub attack: Option<u64>,
    /// Сколько ещё тиков бежать после испуга.
    pub flee_ticks: u32,
    /// Последний вектор бегства (единичный): бежим по нему, и когда угроза
    /// пропала из виду.
    pub flee_dx: f64,
    pub flee_dy: f64,
    /// Цель блуждания. None до первого выбора: иначе новорождённый пошёл бы
    /// потом к месту своего рождения.
    pub target: Option<(f64, f64)>,
}

/// Что стратегия знает о себе.
pub struct Me<'a> {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    /// Номер и родитель: чувства не показывают родню угрозой.
    pub kinship: Kinship,
    pub flock: u64,
    pub flock_goal: Option<crate::flock::FlockGoal>,
    pub pheno: &'a Phenotype,
    pub health_share: f64,
}

/// Решение хода: точка, к которой шагнуть. Шаг — не дальше speed в её сторону;
/// точку ближе шага существо не проскакивает, а встаёт на неё. Поэтому
/// бегство — точка ровно на шаг от себя.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub tx: f64,
    pub ty: f64,
    /// Медленный ход: шаг `slow_speed` и расход `slow_upkeep` (`SLOW_PACE`).
    pub slow: bool,
    pub attack: Option<u64>,
}

/// Куда идти на этом ходу.
#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl Senses) -> Intent {
    match me.pheno.strategy {
        Strategy::Standard => standard::decide(me, mind, rng, senses),
        Strategy::Lurker => lurker::decide(me, mind, rng, senses),
    }
}

/// Только что поело (`me` — уже с новой энергией).
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    match me.pheno.strategy {
        Strategy::Standard | Strategy::Lurker => standard::after_eating(me, mind, rng),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn варианты_совпадают_со_стратегиями() {
        assert_eq!(VARIANTS.len(), Strategy::ALL.len());
        for (i, s) in Strategy::ALL.iter().enumerate() {
            assert_eq!(*s as usize, i);
            assert_eq!(Strategy::from_gene(i as f64), *s);
        }
    }
}
