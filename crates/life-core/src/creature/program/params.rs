//! A block's numbers: their units, ranges and nudges (`ParamSpec`), and the thresholds
//! the founders' blocks share.

use crate::config::{FLEE_SIGHT_SHARE, PROGRAM_NUDGE_POINTS};
use crate::rng::Rng;

/// How a parameter's raw number reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Percent: the raw number.
    Percent,
    /// Percent of its sight.
    Sight,
    /// A ratio: raw / 100 (150 = 1.5 times).
    Ratio,
    Ticks,
    /// Yes or no: 1 or 0; a mutation flips it, the drift leaves it.
    Flag,
    /// A number among a few (a mode): the drift leaves it, a mutation picks another.
    Index,
    /// An angle in hundredths of a radian (120 = 1.2 rad), shown in degrees.
    Angle,
    /// A tilt of a course, raw 0‒200: 100 is straight, above it down by (raw − 100)%, below up.
    Tilt,
}

/// A number a test or an action reads: what it is, its range and base, and how far a mutation
/// moves it (gauss sigma in raw units; a flag flips).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// Game UI: «добыча мельче в».
    pub label: &'static str,
    pub unit: Unit,
    pub lo: u16,
    pub hi: u16,
    pub base: u16,
    pub nudge: f64,
}

impl ParamSpec {
    pub(super) const fn percent(label: &'static str, lo: u16, hi: u16, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Percent, lo, hi, base, nudge: PROGRAM_NUDGE_POINTS }
    }

    pub(super) const fn sight(label: &'static str, lo: u16, hi: u16, base: u16, nudge: f64) -> ParamSpec {
        ParamSpec { label, unit: Unit::Sight, lo, hi, base, nudge }
    }

    pub(super) const fn ratio(label: &'static str, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Ratio, lo: 100, hi: 500, base, nudge: 20.0 }
    }

    /// A time: a mutation moves it by a quarter of its base.
    pub(super) const fn ticks(label: &'static str, lo: u16, hi: u16, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Ticks, lo, hi, base, nudge: base as f64 * 0.25 }
    }

    pub(super) const fn flag(label: &'static str, base: bool) -> ParamSpec {
        ParamSpec { label, unit: Unit::Flag, lo: 0, hi: 1, base: base as u16, nudge: 0.0 }
    }

    pub(super) const fn index(label: &'static str, hi: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Index, lo: 1, hi, base: 1, nudge: 0.0 }
    }

    /// Straight by base; a mutation tilts it by 10 points.
    pub(super) const fn tilt(label: &'static str) -> ParamSpec {
        ParamSpec { label, unit: Unit::Tilt, lo: 0, hi: 200, base: 100, nudge: PROGRAM_NUDGE_POINTS }
    }

    /// The value as the engine reads it: a share for percents (0.5), times for ratios (1.5),
    /// ticks, 1 or 0 for a flag, the number of an index, radians, a tilt's vertical share
    /// (+0.3 down, −0.3 up).
    pub fn value(&self, raw: u16) -> f64 {
        match self.unit {
            Unit::Percent | Unit::Sight | Unit::Ratio | Unit::Angle => f64::from(raw) / 100.0,
            Unit::Ticks | Unit::Flag | Unit::Index => f64::from(raw),
            Unit::Tilt => (f64::from(raw) - 100.0) / 100.0,
        }
    }

    /// Game UI: the amount alone, «30%», «33% зрения», «1,5 раза», «31 тик», «69°», «вниз 30%».
    pub fn amount(&self, raw: u16) -> String {
        match self.unit {
            Unit::Percent => format!("{raw}%"),
            Unit::Sight => format!("{raw}% зрения"),
            Unit::Ratio => format!("{} раза", format!("{:.1}", f64::from(raw) / 100.0).replace('.', ",")),
            Unit::Ticks => format!("{raw} {}", ticks_word(raw)),
            Unit::Flag => (if raw != 0 { "да" } else { "нет" }).to_string(),
            Unit::Index => raw.to_string(),
            Unit::Angle => format!("{:.0}°", f64::from(raw) / 100.0 * 180.0 / std::f64::consts::PI),
            Unit::Tilt if raw > 100 => format!("вниз {}%", raw - 100),
            Unit::Tilt if raw < 100 => format!("вверх {}%", 100 - raw),
            Unit::Tilt => "прямо".to_string(),
        }
    }

    /// Game UI: «добыча мельче в 1,5 раза», «рывком», «не рывком», «вниз 30%».
    pub fn show(&self, raw: u16) -> String {
        match self.unit {
            Unit::Flag if raw != 0 => self.label.to_string(),
            Unit::Flag => format!("не {}", self.label),
            Unit::Tilt => self.amount(raw),
            _ => format!("{} {}", self.label, self.amount(raw)),
        }
    }

    /// Whether the drift moves it: every number but a flag and an index.
    pub(super) const fn drifts(&self) -> bool {
        !matches!(self.unit, Unit::Flag | Unit::Index)
    }

    /// The number moved by gauss(0, `sigma`), held in range.
    pub(super) fn moved(&self, raw: u16, sigma: f64, rng: &mut Rng) -> u16 {
        let moved = f64::from(raw) + rng.gauss(sigma);
        moved.round().clamp(f64::from(self.lo), f64::from(self.hi)) as u16
    }

    /// A mutation's new value: a flag flips, an index becomes another one; a number moves by
    /// gauss(0, `nudge`), held in range.
    pub(super) fn nudged(&self, raw: u16, rng: &mut Rng) -> u16 {
        match self.unit {
            Unit::Flag => u16::from(raw == 0),
            Unit::Index => {
                let other = rng.randint(i64::from(self.lo), i64::from(self.hi) - 1) as u16;
                if other >= raw { other + 1 } else { other }
            }
            _ => self.moved(raw, self.nudge, rng),
        }
    }

    /// A random value in range: a new test's threshold.
    pub(super) fn random(&self, rng: &mut Rng) -> u16 {
        rng.randint(i64::from(self.lo), i64::from(self.hi)) as u16
    }
}

/// «1 тик», «3 тика», «30 тиков». Game UI.
pub fn ticks_word(n: u16) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=14) => "тиков",
        (1, _) => "тик",
        (2..=4, _) => "тика",
        _ => "тиков",
    }
}

/// The flight distance of the templates, % of sight (`FLEE_SIGHT_SHARE`).
pub(super) const FLEE_PCT: u16 = (FLEE_SIGHT_SHARE * 100.0 + 0.5) as u16;
/// How far the templates pick a wander target, % of sight: from a quarter of it (the old 0.5‒2
/// of sight).
pub(super) const WANDER_REACH_PCT: u16 = 200;
