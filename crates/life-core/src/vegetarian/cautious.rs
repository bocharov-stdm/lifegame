//! «Осторожный» — исходное поведение травоядного: бежит от хищника; иначе идёт
//! к ближайшему растению своего слоя; иначе бродит.

use super::strategy::{Intent, Me, Mind};
use crate::config::FLEE_TICKS;
use crate::rng::Rng;
use crate::senses::VegetarianSenses;

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl VegetarianSenses) -> Intent {
    let (x, y, speed) = (me.x, me.y, me.pheno.speed);

    // (x, y, квадрат расстояния) ближайшего хищника в пределах зрения
    let pred = senses.nearest_predator(x, y, me.pheno.vision2);

    let mut fleeing = false;
    if mind.flee_ticks > 0 {
        fleeing = true;
        mind.flee_ticks -= 1;
    }
    if let Some((_, _, d2)) = pred
        && d2 < me.pheno.flee2
    {
        mind.flee_ticks = FLEE_TICKS;
        fleeing = true;
    }

    let (tx, ty) = if fleeing {
        if let Some((px, py, _)) = pred {
            let (dx, dy) = (x - px, y - py);
            let d = dx.hypot(dy);
            if d != 0.0 {
                mind.flee_dx = dx / d;
                mind.flee_dy = dy / d;
            }
        }
        (x + mind.flee_dx * speed, y + mind.flee_dy * speed)
    } else {
        // ВАЖНО: сперва ищем ближайшее вообще и только потом отбрасываем
        // чужой слой. Искать ближайшее В СЛОЕ — другое поведение: существо
        // перестанет отвлекаться на недосягаемую еду и не будет блуждать.
        let food = senses
            .nearest_plant(x, y, me.pheno.vision2)
            .filter(|&(_, py)| me.pheno.layer_lo <= py && py <= me.pheno.layer_hi);
        match food {
            Some(p) => p,
            None => {
                match mind.target {
                    None => pick_random_target(me, mind, rng),
                    Some((tx, ty)) => {
                        let (dx, dy) = (x - tx, y - ty);
                        if dx * dx + dy * dy < speed * speed {
                            pick_random_target(me, mind, rng);
                        }
                    }
                }
                mind.target.unwrap()
            }
        }
    };
    Intent { tx, ty }
}

/// Поело — сразу новая цель, чтобы не топтаться (даже во время бегства).
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    pick_random_target(me, mind, rng);
}

/// Случайная точка в пределах зрения и своей полосы; 10 попыток, иначе стоим.
fn pick_random_target(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    let (lo, hi) = (me.pheno.body_lo, me.pheno.body_hi);
    // слой схлопнут в линию: случайная точка на неё не попадёт никогда,
    // и существо стояло бы столбом — гуляем только вдоль линии
    let flat = lo == hi;
    for _ in 0..10 {
        let angle = rng.uniform(0.0, std::f64::consts::TAU);
        let dist = rng.uniform(me.pheno.vision * 0.5, me.pheno.vision * 2.0);
        let tx = me.x + angle.cos() * dist;
        let ty = if flat { lo } else { me.y + angle.sin() * dist };
        if me.pheno.x_lo <= tx && tx <= me.pheno.x_hi && lo <= ty && ty <= hi {
            mind.target = Some((tx, ty));
            return;
        }
    }
    mind.target = Some((me.x, me.y));
}
