//! The world's space: scale, shape and terrain.
//!
//! Scale is the area at the former density. Everything set «per world» — the plants' pace,
//! their ceiling, the starting population — is multiplied by `area_ratio`. The shape decides
//! where the world grows: a strip only in width (the height stays 4000, as it always was), the
//! other shapes both ways, keeping proportions. The vertical ecology (the food profile by depth,
//! a program's layer) is given in percent of depth, so it carries over to any height.

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

/// What the world is. The same physics either way, and the y axis means the same to the engine:
/// in the sea it is depth (warm and rich on top, the cold deep below), on land it is latitude (the
/// warm edge on top, the cold one below). Only the dead differ: in water they sink to the bottom,
/// on land they lie where they fell (`corpse.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Terrain {
    #[default]
    Ocean,
    Land,
}

impl Terrain {
    pub const ALL: [Terrain; 2] = [Terrain::Ocean, Terrain::Land];

    /// The name for flags and files: `--terrain land`.
    pub fn key(self) -> &'static str {
        match self {
            Terrain::Ocean => "ocean",
            Terrain::Land => "land",
        }
    }

    /// Game UI.
    pub fn label(self) -> &'static str {
        match self {
            Terrain::Ocean => "океан",
            Terrain::Land => "суша",
        }
    }

    pub fn parse(s: &str) -> Result<Terrain, String> {
        let s = s.trim();
        Terrain::ALL.into_iter().find(|t| t.key() == s || t.label() == s).ok_or_else(|| {
            let keys: Vec<&str> = Terrain::ALL.iter().map(|t| t.key()).collect();
            format!("no such terrain: «{s}»; there are {}", keys.join(", "))
        })
    }

    /// A game text written for the sea, said for this terrain: on land the water's phrases
    /// (`LAND_PHRASES`) and then whole words (`LAND_WORDS`, a capital kept) are replaced — a layer is
    /// a belt, depth latitude, the surface the warm edge, the bottom the cold one. Game UI.
    pub fn say(self, text: &str) -> std::borrow::Cow<'_, str> {
        if self == Terrain::Ocean {
            return text.into();
        }
        let mut phrased = text.to_string();
        for (sea, land) in LAND_PHRASES {
            phrased = phrased.replace(sea, land);
        }
        let mut out = String::with_capacity(phrased.len() + 16);
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String| {
            let lower = word.to_lowercase();
            match LAND_WORDS.iter().find(|(sea, _)| *sea == lower) {
                Some((_, land)) if word.starts_with(char::is_uppercase) => {
                    let mut chars = land.chars();
                    out.extend(chars.next().into_iter().flat_map(char::to_uppercase).chain(chars));
                }
                Some((_, land)) => out.push_str(land),
                None => out.push_str(word),
            }
            word.clear();
        };
        for c in phrased.chars() {
            if c.is_alphabetic() {
                word.push(c);
            } else {
                flush(&mut word, &mut out);
                out.push(c);
            }
        }
        flush(&mut word, &mut out);
        out.into()
    }
}

/// The water's phrases on land, replaced before the words (`Terrain::say`).
const LAND_PHRASES: [(&str, &str); 20] = [
    // the climate tab's thermocline fields, their labels and hints
    ("До какой глубины вода тёплая", "До какой широты тепло"),
    ("С какой глубины вода холодная", "С какой широты холодно"),
    ("Ниже она остывает до «Холодной воды с»", "Дальше холодает до «Холодно с»"),
    ("Тёплая вода до", "Тепло до"),
    ("Холодная вода с", "Холодно с"),
    ("ко дну", "к холодному краю"),
    ("к верху слоя", "к тёплому краю слоя"),
    ("Поднимается к верхнему краю", "Идёт к тёплому краю"),
    ("Опускается к нижнему краю", "Идёт к холодному краю"),
    ("наверху не срабатывает", "там не срабатывает"),
    ("внизу не срабатывает", "там не срабатывает"),
    ("вместе с водой", "вместе с воздухом"),
    ("В холодной глубине", "В холодных широтах"),
    ("в холодной глубине", "в холодных широтах"),
    ("в самой холодной воде", "в самом холодном краю"),
    ("В тёплой воде", "В тепле"),
    ("холод глубины", "холод полюса"),
    ("тёплая вода наверху", "тепло наверху"),
    ("Насколько холодна вода", "Насколько там холодно"),
    ("медленнее плавает", "медленнее ходит"),
];

/// The sea's words on land, whole words in lower case (`Terrain::say`).
const LAND_WORDS: [(&str, &str); 26] = [
    ("слой", "пояс"),
    ("слоя", "пояса"),
    ("слою", "поясу"),
    ("слоем", "поясом"),
    ("слое", "поясе"),
    ("слои", "пояса"),
    ("слоёв", "поясов"),
    ("глубина", "широта"),
    ("глубины", "широты"),
    ("глубине", "широте"),
    ("глубину", "широту"),
    ("глубиной", "широтой"),
    ("глубже", "холоднее"),
    ("поверхность", "тёплый край"),
    ("поверхности", "тёплого края"),
    ("дно", "холодный край"),
    ("дна", "холодного края"),
    ("дну", "холодному краю"),
    ("дне", "холодном крае"),
    ("термоклин", "граница холода"),
    ("термоклина", "границы холода"),
    ("термоклине", "границе холода"),
    ("всплывает", "уходит к теплу"),
    ("тонут", "лежат"),
    ("тонет", "лежит"),
    ("плавают", "ходят"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    pub width: f64,
    pub height: f64,
    pub terrain: Terrain,
}

impl Default for Space {
    fn default() -> Self {
        Space { width: WORLD_WIDTH, height: WORLD_HEIGHT, terrain: Terrain::Ocean }
    }
}

impl Space {
    /// A world `scale` times bigger than the base by area, of the given shape.
    pub fn new(scale: f64, shape: Shape) -> Self {
        assert!(
            (MIN_SCALE..=MAX_SCALE).contains(&scale),
            "масштаб мира должен быть от {MIN_SCALE} до {MAX_SCALE}, а не {scale}"
        );
        let terrain = Terrain::Ocean;
        match shape.ratio() {
            None => Space { width: WORLD_WIDTH * scale, height: WORLD_HEIGHT, terrain },
            // At 3:2 and x1: 24e6 / 1.5 = 16e6, the root is exactly 4000, the width exactly 6000. The
            // base world comes out bit for bit.
            Some(r) => {
                let height = (WORLD_WIDTH * WORLD_HEIGHT * scale / r).sqrt();
                Space { width: r * height, height, terrain }
            }
        }
    }

    /// The same space as another terrain.
    pub fn on(self, terrain: Terrain) -> Self {
        Space { terrain, ..self }
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
        for terrain in Terrain::ALL {
            assert_eq!(Terrain::parse(terrain.key()), Ok(terrain));
            assert_eq!(Terrain::parse(terrain.label()), Ok(terrain));
        }
        assert!(Terrain::parse("болото").is_err());
    }

    /// On land the sea's words become the land's, whole words only, a capital kept; the sea's
    /// texts stay as they are.
    #[test]
    fn land_says_the_seas_words_its_own_way() {
        let text = "Слой глубины: одно дно и ко дну слоя, медленно.";
        assert_eq!(Terrain::Ocean.say(text), text);
        assert_eq!(
            Terrain::Land.say(text),
            "Пояс широты: одно холодный край и к холодному краю пояса, медленно."
        );
        assert_eq!(Terrain::Land.say("В холодной глубине вода"), "В холодных широтах вода");
        // the climate tab's thermocline fields speak of warmth, not water
        assert_eq!(Terrain::Land.say("Тёплая вода до"), "Тепло до");
        assert_eq!(
            Terrain::Land
                .say("До какой глубины вода тёплая, % глубины мира. Ниже она остывает до «Холодной воды с»."),
            "До какой широты тепло, % широты мира. Дальше холодает до «Холодно с»."
        );
        assert_eq!(Terrain::Land.say("С какой глубины вода холодная"), "С какой широты холодно");
    }
}
