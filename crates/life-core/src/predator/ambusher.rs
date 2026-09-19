//! «Засадник»: всё время бродит медленным ходом (`SLOW_PACE`) и дёшево.
//! Голодный бросается рывком только на добычу в пределах рывка
//! (`PREDATOR_SPRINT_RANGE` между краями тел); дальнюю не преследует. Живёт
//! тем, что добыча подходит сама, — экономен, но ест реже погонщика.

use super::standard;
use super::strategy::{Intent, Me, Mind};
use crate::config::PREDATOR_SPRINT_RANGE;
use crate::rng::Rng;
use crate::senses::PredatorSenses;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, _rng: &mut Rng, senses: &impl PredatorSenses) -> Intent {
    // Сытый добычу не ищет вовсе. Смотрим на ближайшую по центрам, как и
    // стандартный: чуть более далёкая, но крупная могла бы оказаться ближе
    // краем тела — засада её пропустит.
    let prey = if me.hungry() { senses.nearest_prey(me.x, me.y, me.pheno.vision) } else { None };
    match prey {
        Some(p) if standard::gap(me, &p) < PREDATOR_SPRINT_RANGE => standard::pursue(me, &p),
        _ => standard::wander(me, mind, me.pheno.slow_speed, me.pheno.slow_upkeep),
    }
}
