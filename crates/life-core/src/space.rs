//! Пространство мира и его масштаб.
//!
//! Масштаб — это площадь при прежней плотности. Мир растёт вширь, высота
//! остаётся 4000: вертикальная экология (растения гуще у поверхности, гены
//! слоя в процентах глубины) от ширины не зависит, поэтому баланс, подобранный
//! на базовом мире, переносится на любой масштаб. Всё, что задано «на мир» —
//! темп растений, их потолок, стартовые популяции, пороги миграции, — умножается
//! на `area_ratio`.

use crate::config::{WORLD_HEIGHT, WORLD_WIDTH};

/// Меньше базового мира не бывает. Баланс подобран на нём, а в узком мире
/// ломается геометрия: при ширине 60 полоса блуждания хищника — [40, 20].
pub const MIN_SCALE: f64 = 1.0;
/// И больше этого тоже не бывает: мир x10 000 — уже 200 тыс. травоядных на
/// старте и 15 млн растений в потолке. Дальше память кончается раньше, чем
/// видна разница, и без предела процесс падал бы на выделении памяти вместо
/// внятной ошибки.
pub const MAX_SCALE: f64 = 10_000.0;

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
        assert!(
            (MIN_SCALE..=MAX_SCALE).contains(&scale),
            "масштаб мира должен быть от {MIN_SCALE} до {MAX_SCALE}, а не {scale}"
        );
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
