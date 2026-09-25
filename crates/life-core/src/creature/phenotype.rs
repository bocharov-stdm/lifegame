//! Фенотип существа: всё, что выводится из генома и правил мира один раз
//! при рождении и росте тела. Геном не меняется всю жизнь, поэтому ход (самый горячий код)
//! читает готовые числа. Правила меняются на ходу (лаборатория) — тогда фенотип
//! пересчитывается целиком (`Creature::apply_rules`).
//!
//! Единственное место, где ген действует и платит: новый ген получает здесь
//! своё действие, а его цена дописывается в конец суммы расхода.

use super::Strategy;
use crate::config::{ENERGY_PER_SIZE, FLEE_SIGHT_SHARE, SLOW_PACE};
use crate::flock::{FlockKind, Territoriality};
use crate::genome::CreatureGenome;
use crate::genome::creature::Gene;
use crate::rules::Rules;
use crate::space::Space;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    /// Диаметр тела.
    pub size: f64,
    pub speed: f64,
    pub sociability: f64,
    pub pack_instinct: bool,
    pub territoriality: Territoriality,
    /// How the family flock's circle moves; part of the flock mode.
    pub flock_kind: FlockKind,
    /// False: the layer genes do not hold the creature, its band is the whole depth.
    pub layer_bound: bool,
    /// Circle radius per square root of the member count, clamped to 50-500.
    pub flock_spacing: f64,
    pub care: f64,
    pub life_pace: f64,
    pub retreat: f64,
    pub plant_efficiency: f64,
    pub meat_efficiency: f64,
    pub prey_ratio: f64,
    /// How much a hunter weighs the strikes it expects from its prey and the prey's visible
    /// allies: 0 ignores them, 1 is the base, 2 is twice as careful.
    pub caution: f64,
    /// Возможность и личный стиль дальнего боя.
    pub shooter: bool,
    pub fire_preference: f64,
    pub fire_reserve: f64,
    pub plant_energy: f64,
    pub plant_bite_yield: f64,
    pub melee_damage_share: f64,
    pub shot_energy_share: f64,
    pub vision: f64,

    // ── слой обитания и границы ─────────────────────────────────────────────
    /// Слой из генов, без запаса на тело (его рисует окно).
    pub layer_lo: f64,
    pub layer_hi: f64,
    /// Домашняя полоса — слой с запасом на тело, всегда внутри мира. Слой
    /// мягкий: за видимой едой существо выходит из полосы, а без еды бродит в
    /// ней и возвращается в неё.
    pub body_lo: f64,
    pub body_hi: f64,
    /// Границы тела в мире: дальше них центр не заходит никогда.
    pub x_lo: f64,
    pub x_hi: f64,
    pub y_lo: f64,
    pub y_hi: f64,

    // ── энергия и предвычисленное ───────────────────────────────────────────
    pub max_energy: f64,
    /// Расход за тик.
    pub upkeep: f64,
    /// Медленный ход (`SLOW_PACE`): шаг и расход за тик на нём.
    pub slow_speed: f64,
    pub slow_upkeep: f64,
    pub vision2: f64,
    pub size2: f64,
    /// Радиус тела: по нему съедают сородичи (каннибализм).
    pub half: f64,
    /// С какого расстояния до края тела опасного чужака бежать
    /// (`FLEE_SIGHT_SHARE` зрения).
    pub flee: f64,

    /// Стратегия поведения (`strategy.rs`).
    pub strategy: Strategy,
}

impl Phenotype {
    pub fn of(genome: &CreatureGenome, rules: &Rules, space: &Space) -> Self {
        Self::at_size(genome, rules, space, genome[Gene::Size])
    }

    /// Фенотип по фактическому телу: наследственный предел хранится в геноме.
    pub fn at_size(genome: &CreatureGenome, rules: &Rules, space: &Space, size: f64) -> Self {
        let life_pace = genome[Gene::LifePace].clamp(0.5, 2.0);
        let (mut min_pct, mut max_pct) =
            (genome[Gene::MinY].clamp(0.0, 100.0), genome[Gene::MaxY].clamp(0.0, 100.0));
        if min_pct > max_pct {
            std::mem::swap(&mut min_pct, &mut max_pct);
        }
        let layer_bound = genome[Gene::LayerBound] < 0.5;
        // A free creature has no layer: its home band is the whole depth.
        let (layer_lo, layer_hi) = if layer_bound {
            (min_pct / 100.0 * space.height, max_pct / 100.0 * space.height)
        } else {
            (0.0, space.height)
        };

        // Запас на тело — не больше половины мира. Размер — ген, и при дешёвом
        // размере (лаборатория) тело бывает больше мира: с полным запасом
        // границы переворачивались, и зажимы перекидывали существо от края к краю.
        let margin_x = size.min(space.width / 2.0);
        let margin_y = size.min(space.height / 2.0);
        let (mut body_lo, mut body_hi) = (layer_lo + margin_y, layer_hi - margin_y);
        // Слой уже собственного тела — схлопываем полосу в линию посередине,
        // держа её в мире: у слоя на самом краю середина легла бы за границу.
        if body_lo > body_hi {
            let mid = ((body_lo + body_hi) / 2.0).clamp(margin_y, space.height - margin_y);
            body_lo = mid;
            body_hi = mid;
        }
        let (x_lo, x_hi) = (margin_x, space.width - margin_x);
        // Домашняя полоса лежит внутри этих границ: слой — в пределах мира.
        let (y_lo, y_hi) = (margin_y, space.height - margin_y);

        let speed = genome[Gene::Speed];
        let vision = genome[Gene::Vision];
        let slow_speed = speed * SLOW_PACE;
        Phenotype {
            size,
            speed,
            sociability: genome[Gene::Sociability].clamp(0.0, 100.0) / 100.0,
            pack_instinct: genome[Gene::PackInstinct] >= 0.5,
            territoriality: Territoriality::from_gene(genome[Gene::Territoriality]),
            flock_kind: FlockKind::from_gene(genome[Gene::FlockKind]),
            layer_bound,
            flock_spacing: genome[Gene::FlockSpacing].clamp(50.0, 500.0),
            care: genome[Gene::Care].clamp(0.0, 100.0) / 100.0,
            life_pace,
            plant_efficiency: 1.0 - 0.8 * genome[Gene::Carnivory].clamp(0.0, 100.0) / 100.0,
            meat_efficiency: 0.2 + 0.8 * genome[Gene::Carnivory].clamp(0.0, 100.0) / 100.0,
            prey_ratio: genome[Gene::PreyRatio].clamp(1.0, 5.0),
            caution: genome[Gene::Caution].clamp(0.0, 100.0) / 50.0,
            shooter: genome[Gene::Shooter] >= 0.5,
            fire_preference: genome[Gene::FirePreference].clamp(0.0, 100.0) / 100.0,
            fire_reserve: genome[Gene::FireReserve].clamp(0.0, 100.0) / 100.0,
            plant_energy: rules.plant_energy,
            plant_bite_yield: rules.plant_bite_yield,
            melee_damage_share: rules.melee_damage_share,
            shot_energy_share: rules.shot_energy_share,
            retreat: 0.8 - 0.6 * genome[Gene::Bravery].clamp(0.0, 100.0) / 100.0,
            vision,
            layer_lo,
            layer_hi,
            body_lo,
            body_hi,
            x_lo,
            x_hi,
            y_lo,
            y_hi,
            max_energy: size * ENERGY_PER_SIZE,
            upkeep: rules.upkeep(size, speed, vision) * life_pace,
            slow_speed,
            slow_upkeep: rules.upkeep(size, slow_speed, vision) * life_pace,
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            flee: vision * FLEE_SIGHT_SHARE,
            strategy: Strategy::from_gene(genome[Gene::Strategy]),
        }
    }
}
