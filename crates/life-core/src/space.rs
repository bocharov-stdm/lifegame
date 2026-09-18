//! Пространство мира и его масштаб.
//!
//! Масштаб — это площадь при прежней плотности. Мир растёт вширь, высота
//! остаётся 4000: вертикальная экология (растения гуще у поверхности, гены
//! слоя в процентах глубины) от ширины не зависит, поэтому баланс, подобранный
//! на базовом мире, переносится на любой масштаб. Всё, что задано «на мир» —
//! темп растений, их потолок, стартовые популяции, пороги миграции, — умножается
//! на `area_ratio`.

use crate::config::{WORLD_HEIGHT, WORLD_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    pub width: f64,
    pub height: f64,
}

impl Default for Space {
    fn default() -> Self {
        Space { width: WORLD_WIDTH, height: WORLD_HEIGHT }
    }
}

impl Space {
    /// Мир в `scale` раз больше базового по площади (растёт ширина).
    pub fn scaled(scale: f64) -> Self {
        assert!(scale.is_finite() && scale > 0.0, "масштаб мира должен быть > 0, а не {scale}");
        Space { width: WORLD_WIDTH * scale, height: WORLD_HEIGHT }
    }

    /// Во сколько раз площадь больше базового мира 6000x4000.
    pub fn area_ratio(&self) -> f64 {
        self.width * self.height / (WORLD_WIDTH * WORLD_HEIGHT)
    }

    /// Величина, заданная на базовый мир, пересчитанная на этот (не меньше 1).
    pub fn per_area(&self, base: usize) -> usize {
        ((base as f64 * self.area_ratio()).round() as usize).max(1)
    }
}
