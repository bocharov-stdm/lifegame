//! Растения: неподвижная еда, гуще у поверхности.

use crate::config::*;
use crate::rng::Rng;
use crate::space::Space;

#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub x: f64,
    pub y: f64,
    /// Съеденное помечается, а выметается раз за тик в конце хода травоядных.
    pub alive: bool,
}

impl Plant {
    pub fn at(x: f64, y: f64) -> Self {
        Plant { x, y, alive: true }
    }

    /// Случайное растение. Плотность по глубине падает как
    /// exp(-DECAY * y / высота мира); y берётся через обратную функцию
    /// распределения — одна попытка вместо отбраковки.
    pub fn random(space: &Space, rng: &mut Rng) -> Self {
        let lambda = PLANT_DEPTH_DECAY / space.height;
        let e_top = (-lambda * (PLANT_RADIUS + PLANT_TOP_MARGIN)).exp();
        let e_bottom = (-lambda * (space.height - PLANT_RADIUS)).exp();

        let x = rng.uniform(PLANT_RADIUS, space.width - PLANT_RADIUS);
        let u = rng.random();
        let y = -(e_top - u * (e_top - e_bottom)).ln() / lambda;
        Plant::at(x, y)
    }
}
