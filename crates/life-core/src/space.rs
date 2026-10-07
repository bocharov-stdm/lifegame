//! The world's space: scale and shape.
//!
//! Scale is the area at the former density. Everything set «per world» — the plants' pace,
//! their ceiling, the starting population — is multiplied by `area_ratio`. The shape decides
//! where the world grows: a strip only in width (the height stays 4000, as it always was), the
//! other shapes both ways, keeping proportions. The vertical ecology (the food profile by depth,
//! the layer genes) is given in percent of depth, so it carries over to any height.

use crate::config::{WORLD_HEIGHT, WORLD_WIDTH};

/// There is no world smaller than the base one: the balance is tuned on it, and in a narrow
/// world the geometry breaks (the wandering strip is narrower than the body).
pub const MIN_SCALE: f64 = 1.0;
/// And no bigger than this either: a world x10 000 is already 200 thousand creatures at the
/// start and 15 million plants in the ceiling. Beyond that memory runs out before any difference
/// shows, and without a limit the process would fall over on an allocation instead of a clear
/// error.
pub const MAX_SCALE: f64 = 10_000.0;

/// The `--scale` flag of the report and the game: a number from `MIN_SCALE` to `MAX_SCALE`.
pub fn parse_scale(s: &str) -> Result<f64, String> {
    let scale: f64 = s.trim().parse().map_err(|_| format!("«{s}» is not a number"))?;
    if (MIN_SCALE..=MAX_SCALE).contains(&scale) {
        Ok(scale)
    } else {
        Err(format!("the scale is from {MIN_SCALE} to {MAX_SCALE}"))
    }
}

/// The world's shape. The area sets the scale, the shape the proportions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A height of 4000, the world grows only in width — as it was before shapes.
    Strip,
    Square,
    /// The proportions of the base world 6000x4000: at x1 this is it.
    R3x2,
    R2x1,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Square, Shape::R3x2, Shape::R2x1, Shape::Strip];

    /// The name for flags and files: `--shape 3:2`.
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

    /// Width to height; for a strip the proportions grow with the scale.
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
    /// A world `scale` times bigger than the base by area, of the given shape.
    pub fn new(scale: f64, shape: Shape) -> Self {
        assert!(
            (MIN_SCALE..=MAX_SCALE).contains(&scale),
            "масштаб мира должен быть от {MIN_SCALE} до {MAX_SCALE}, а не {scale}"
        );
        match shape.ratio() {
            None => Space { width: WORLD_WIDTH * scale, height: WORLD_HEIGHT },
            // At 3:2 and x1: 24e6 / 1.5 = 16e6, the root is exactly 4000, the width exactly 6000. The
            // base world comes out bit for bit.
            Some(r) => {
                let height = (WORLD_WIDTH * WORLD_HEIGHT * scale / r).sqrt();
                Space { width: r * height, height }
            }
        }
    }

    /// A strip: a world `scale` times bigger than the base by area (the width grows).
    pub fn scaled(scale: f64) -> Self {
        Space::new(scale, Shape::Strip)
    }

    /// How many times the area is bigger than the base world 6000x4000.
    pub fn area_ratio(&self) -> f64 {
        self.width * self.height / (WORLD_WIDTH * WORLD_HEIGHT)
    }

    /// A quantity given for the base world, recomputed for this one (at least 1).
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

    /// The balance reference was taken at x1: at 3:2 this is the same world, bit for bit.
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
