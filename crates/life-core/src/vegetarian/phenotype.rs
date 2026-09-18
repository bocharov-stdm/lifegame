//! Фенотип травоядного: всё, что выводится из генома и правил мира один раз
//! при рождении. Геном не меняется всю жизнь, поэтому ход (самый горячий код)
//! читает готовые числа. Правила меняются на ходу (лаборатория) — тогда фенотип
//! пересчитывается целиком (`Vegetarian::apply_rules`).
//!
//! Единственное место, где ген действует и платит: новый ген получает здесь
//! своё действие, а его цена дописывается в конец суммы расхода.

use super::Strategy;
use crate::config::VEGETARIAN_ENERGY_PER_SIZE;
use crate::genome::VegetarianGenome;
use crate::genome::vegetarian::Gene;
use crate::rules::Rules;
use crate::space::Space;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    /// Диаметр тела.
    pub size: f64,
    pub speed: f64,
    pub vision: f64,

    // ── слой обитания и границы ─────────────────────────────────────────────
    /// Слой без запаса на тело — по нему решается, дотянемся ли до еды.
    pub layer_lo: f64,
    pub layer_hi: f64,
    /// Полоса, в которой держится само тело. Всегда внутри мира.
    pub body_lo: f64,
    pub body_hi: f64,
    pub x_lo: f64,
    pub x_hi: f64,

    // ── энергия и предвычисленное ───────────────────────────────────────────
    pub max_energy: f64,
    /// Расход за тик.
    pub upkeep: f64,
    pub vision2: f64,
    pub size2: f64,
    /// Радиус тела: по нему ловят и видят хищники.
    pub half: f64,
    /// Квадрат расстояния до хищника, с которого травоядное пугается.
    pub flee2: f64,

    /// Стратегия поведения (`strategy.rs`).
    pub strategy: Strategy,
}

impl Phenotype {
    pub fn of(genome: &VegetarianGenome, rules: &Rules, space: &Space) -> Self {
        let size = genome[Gene::Size];
        let (mut min_pct, mut max_pct) =
            (genome[Gene::MinY].clamp(0.0, 100.0), genome[Gene::MaxY].clamp(0.0, 100.0));
        if min_pct > max_pct {
            std::mem::swap(&mut min_pct, &mut max_pct);
        }
        let layer_lo = min_pct / 100.0 * space.height;
        let layer_hi = max_pct / 100.0 * space.height;

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

        let speed = genome[Gene::Speed];
        let vision = genome[Gene::Vision];
        Phenotype {
            size,
            speed,
            vision,
            layer_lo,
            layer_hi,
            body_lo,
            body_hi,
            x_lo,
            x_hi,
            max_energy: size * VEGETARIAN_ENERGY_PER_SIZE,
            upkeep: rules.upkeep(size, speed, vision),
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            flee2: (vision / 3.0) * (vision / 3.0),
            strategy: Strategy::from_gene(genome[Gene::Strategy]),
        }
    }
}
