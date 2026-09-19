//! Пространство мира: масштаб и форма.
//!
//! Масштаб — это площадь при прежней плотности. Всё, что задано «на мир» —
//! темп растений, их потолок, стартовая популяция, — умножается
//! на `area_ratio`. Форма решает, куда мир растёт: полоса — только вширь (высота
//! остаётся 4000, как было всегда), остальные формы — в обе стороны, держа
//! пропорции. Вертикальная экология (профиль еды по глубине, гены слоя) задана в
//! процентах глубины, поэтому переносится на любую высоту.

use crate::config::{WORLD_HEIGHT, WORLD_WIDTH};

/// Меньше базового мира не бывает: баланс подобран на нём, а в узком мире
/// ломается геометрия (полоса блуждания уже тела).
pub const MIN_SCALE: f64 = 1.0;
/// И больше этого тоже не бывает: мир x10 000 — уже 200 тыс. существ на
/// старте и 15 млн растений в потолке. Дальше память кончается раньше, чем
/// видна разница, и без предела процесс падал бы на выделении памяти вместо
/// внятной ошибки.
pub const MAX_SCALE: f64 = 10_000.0;

/// Форма мира. Площадь задаёт масштаб, форма — пропорции.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Высота 4000, мир растёт только вширь — так было до форм.
    Strip,
    Square,
    /// Пропорции базового мира 6000x4000: при x1 это он и есть.
    R3x2,
    R2x1,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Square, Shape::R3x2, Shape::R2x1, Shape::Strip];

    /// Имя для флагов и файлов: `--shape 3:2`.
    pub fn key(self) -> &'static str {
        match self {
            Shape::Strip => "strip",
            Shape::Square => "1:1",
            Shape::R3x2 => "3:2",
            Shape::R2x1 => "2:1",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Shape::Strip => "полоса",
            Shape::Square => "квадрат 1:1",
            Shape::R3x2 => "3:2",
            Shape::R2x1 => "2:1",
        }
    }

    pub fn parse(s: &str) -> Result<Shape, String> {
        let s = s.trim();
        Shape::ALL.into_iter().find(|f| f.key() == s || f.label() == s).ok_or_else(|| {
            let keys: Vec<&str> = Shape::ALL.iter().map(|f| f.key()).collect();
            format!("нет такой формы: «{s}»; есть {}", keys.join(", "))
        })
    }

    /// Ширина к высоте; у полосы пропорции растут с масштабом.
    pub fn ratio(self) -> Option<f64> {
        match self {
            Shape::Strip => None,
            Shape::Square => Some(1.0),
            Shape::R3x2 => Some(1.5),
            Shape::R2x1 => Some(2.0),
        }
    }
}

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
    /// Мир в `scale` раз больше базового по площади, заданной формы.
    pub fn new(scale: f64, shape: Shape) -> Self {
        assert!(
            (MIN_SCALE..=MAX_SCALE).contains(&scale),
            "масштаб мира должен быть от {MIN_SCALE} до {MAX_SCALE}, а не {scale}"
        );
        match shape.ratio() {
            None => Space { width: WORLD_WIDTH * scale, height: WORLD_HEIGHT },
            // При 3:2 и x1: 24e6 / 1.5 = 16e6, корень — ровно 4000, ширина —
            // ровно 6000. Базовый мир получается бит в бит.
            Some(r) => {
                let height = (WORLD_WIDTH * WORLD_HEIGHT * scale / r).sqrt();
                Space { width: r * height, height }
            }
        }
    }

    /// Полоса: мир в `scale` раз больше базового по площади (растёт ширина).
    pub fn scaled(scale: f64) -> Self {
        Space::new(scale, Shape::Strip)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn у_каждой_формы_площадь_по_масштабу_и_свои_пропорции() {
        for shape in Shape::ALL {
            for scale in [1.0, 7.0, 100.0, 1234.0, MAX_SCALE] {
                let s = Space::new(scale, shape);
                assert!((s.area_ratio() / scale - 1.0).abs() < 1e-9, "{shape:?} x{scale}: {s:?}");
                if let Some(r) = shape.ratio() {
                    assert!((s.width / s.height / r - 1.0).abs() < 1e-12, "{shape:?} x{scale}: {s:?}");
                } else {
                    assert_eq!(s.height, WORLD_HEIGHT);
                }
            }
        }
    }

    /// Эталон баланса снят на x1: при 3:2 это тот же мир, бит в бит.
    #[test]
    fn три_к_двум_при_единице_это_базовый_мир() {
        assert_eq!(Space::new(1.0, Shape::R3x2), Space::default());
        assert_eq!(Space::new(1.0, Shape::Strip), Space::default());
    }

    #[test]
    fn форма_читается_по_имени() {
        for shape in Shape::ALL {
            assert_eq!(Shape::parse(shape.key()), Ok(shape));
            assert_eq!(Shape::parse(shape.label()), Ok(shape));
        }
        assert!(Shape::parse("круг").is_err());
    }
}
