//! «Стандартное» поведение хищника — исходное: голодный гонится за ближайшей
//! добычей, вблизи — рывком; сытый бродит к случайной цели.

use super::Predator;
use super::strategy::{Intent, Me, Mind};
use crate::config::*;
use crate::rng::Rng;
use crate::senses::PredatorSenses;
use crate::space::Space;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, _rng: &mut Rng, senses: &impl PredatorSenses) -> Intent {
    // Сытый добычу не ищет вовсе.
    let prey = if me.hungry() { senses.nearest_prey(me.x, me.y, me.pheno.vision) } else { None };
    let mut cost = me.pheno.upkeep;
    let (dx, dy) = match prey {
        Some(p) => {
            let (dx, dy) = (p.x - me.x, p.y - me.y);
            let dist = dx.hypot(dy);
            let mut step = me.pheno.speed;
            // Рывок: вблизи хищник догоняет и того, кто быстрее его на
            // дистанции. Он стоит энергии — гнаться рывком вечно нельзя.
            if dist - p.half - Predator::DIAM / 2.0 < PREDATOR_SPRINT_RANGE {
                step *= PREDATOR_SPRINT_MULT;
                cost += PREDATOR_SPRINT_COST;
            }
            let step = step.min(dist); // не проскакивать добычу насквозь
            if dist != 0.0 { (dx * step / dist, dy * step / dist) } else { (0.0, 0.0) }
        }
        None => {
            let (dx, dy) = (mind.tx - me.x, mind.ty - me.y);
            let dist = dx.hypot(dy);
            if dist == 0.0 { (0.0, 0.0) } else { (dx * me.pheno.speed / dist, dy * me.pheno.speed / dist) }
        }
    };
    Intent { dx, dy, cost, chasing: prey.is_some() }
}

/// Дошёл до цели блуждания — новая цель (даже если только что умер: так было
/// всегда, и порядок случайных чисел на этом держится).
#[inline(always)]
pub(crate) fn settle(me: &Me, mind: &mut Mind, rng: &mut Rng, space: &Space, intent: &Intent) {
    if !intent.chasing && (me.x - mind.tx).hypot(me.y - mind.ty) < me.pheno.speed {
        choose_target(me.x, me.y, mind, rng, space);
    }
}

/// Цель — случайная точка квадрата со стороной 2*WANDER_RADIUS вокруг себя,
/// сдвинутого внутрь тех же границ, в которых держится сам хищник. Цель у
/// самой стены была недостижима: один хищник простоял так 426 тиков и умер.
pub(crate) fn choose_target(x: f64, y: f64, mind: &mut Mind, rng: &mut Rng, space: &Space) {
    let (a, b) = wander_span(x, space.width);
    mind.tx = rng.uniform(a, b);
    let (a, b) = wander_span(y, space.height);
    mind.ty = rng.uniform(a, b);
}

/// Отрезок длиной 2*WANDER_RADIUS вокруг pos, сдвинутый внутрь мира.
fn wander_span(pos: f64, world: f64) -> (f64, f64) {
    let (lo, hi) = (PREDATOR_DIAM, world - PREDATOR_DIAM);
    let (mut a, mut b) = (pos - WANDER_RADIUS, pos + WANDER_RADIUS);
    if a < lo {
        b += lo - a;
        a = lo;
    }
    if b > hi {
        a = (a - (b - hi)).max(lo);
        b = hi;
    }
    (a, b)
}
