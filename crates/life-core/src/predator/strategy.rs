//! Стратегии поведения хищника — тот же образец, что у травоядного
//! (`vegetarian/strategy.rs`): стратегия решает (`decide`), хищник делает
//! (`Predator::act`), после хода стратегия может передумать о цели (`settle`).

use super::Phenotype;
use super::standard;
use crate::genome::Variant;
use crate::rng::Rng;
use crate::senses::PredatorSenses;
use crate::space::Space;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Strategy {
    /// Голодный гонится за ближайшей добычей с рывком вблизи, сытый бродит.
    #[default]
    Standard,
}

impl Strategy {
    /// Все варианты в порядке `VARIANTS`.
    pub const ALL: [Strategy; 1] = [Strategy::Standard];

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
pub const VARIANTS: [Variant; 1] = [Variant {
    key: "standard",
    label: "стандартный",
    about: "Голодный гонится за ближайшей добычей, вблизи — рывком; сытый бродит и не ест.",
}];

/// Память хищника между ходами.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mind {
    /// Цель блуждания.
    pub tx: f64,
    pub ty: f64,
}

/// Что стратегия знает о себе.
pub struct Me<'a> {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub pheno: &'a Phenotype,
}

impl Me<'_> {
    pub fn hungry(&self) -> bool {
        self.pheno.hungry(self.energy)
    }
}

/// Решение хода: сдвиг, его цена в энергии и гонится ли хищник за добычей.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub dx: f64,
    pub dy: f64,
    pub cost: f64,
    pub chasing: bool,
}

/// Куда идти на этом ходу.
#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl PredatorSenses) -> Intent {
    match me.pheno.strategy {
        Strategy::Standard => standard::decide(me, mind, rng, senses),
    }
}

/// После хода (`me` — на новом месте, возможно уже мёртвый).
#[inline(always)]
pub(crate) fn settle(me: &Me, mind: &mut Mind, rng: &mut Rng, space: &Space, intent: &Intent) {
    match me.pheno.strategy {
        Strategy::Standard => standard::settle(me, mind, rng, space, intent),
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
