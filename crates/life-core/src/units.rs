//! Real units for showing the world: the mechanics stay in pixels and ticks, these only translate
//! them for the player and the report. The scale is fixed by the base fish (`size` 40 px ≈ a 20 cm
//! fish) and two clocks, since the life cycle is compressed far more than swimming (`docs/scale.md`).

use crate::config::{CM_PER_PX, SECONDS_PER_TICK, TICKS_PER_YEAR};

/// Pixels as centimetres.
pub fn cm(px: f64) -> f64 {
    px * CM_PER_PX
}

/// Pixels as metres.
pub fn metres(px: f64) -> f64 {
    px * CM_PER_PX / 100.0
}

/// A step a tick as centimetres a second, by the swimming clock.
pub fn cm_per_second(px_per_tick: f64) -> f64 {
    cm(px_per_tick) / SECONDS_PER_TICK
}

/// Ticks of life as years, by the life clock.
pub fn years(ticks: f64) -> f64 {
    ticks / TICKS_PER_YEAR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::creature::{GENES, Gene};

    /// The base fish is a 20 cm fish cruising about a body length a second and living three years.
    #[test]
    fn the_base_fish_in_real_units() {
        let base = |g: Gene| GENES[g as usize].base;
        assert_eq!(cm(base(Gene::Size)), 20.0);
        assert_eq!(cm_per_second(base(Gene::Speed)), 20.0);
        assert_eq!(metres(base(Gene::Vision)), 2.0);
        assert_eq!(years(base(Gene::Lifespan)), 3.0);
    }
}
