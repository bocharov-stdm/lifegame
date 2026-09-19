//! «Стандартное» поведение травоядного — исходное: бежит от хищника; иначе
//! идёт к ближайшему видимому растению; иначе бродит в своём слое, а оказавшись
//! вне его (ушло за едой, убегало) — возвращается.
//!
//! Решение разбито на `plan`, который говорит ещё и какая ветка сработала:
//! «затаившийся» (`lurker.rs`) ведёт себя так же, но бродит медленно.

use super::strategy::{Intent, Me, Mind};
use crate::config::FLEE_TICKS;
use crate::rng::Rng;
use crate::senses::VegetarianSenses;

/// Какая ветка решения сработала.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Flee,
    Food,
    Wander,
}

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl VegetarianSenses) -> Intent {
    plan(me, mind, rng, senses, me.pheno.speed).0
}

/// Куда идти и почему. `step` — длина шага, если существо будет бродить: по
/// ней решается, дошло ли оно до цели.
#[inline(always)]
pub(super) fn plan(
    me: &Me,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl VegetarianSenses,
    step: f64,
) -> (Intent, Mode) {
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

    if fleeing {
        if let Some((px, py, _)) = pred {
            let (dx, dy) = (x - px, y - py);
            let d = dx.hypot(dy);
            if d != 0.0 {
                mind.flee_dx = dx / d;
                mind.flee_dy = dy / d;
            }
        }
        let intent = Intent { tx: x + mind.flee_dx * speed, ty: y + mind.flee_dy * speed, slow: false };
        return (intent, Mode::Flee);
    }

    // Слой мягкий: видимая еда годится любая, выше слоя или ниже.
    if let Some((tx, ty)) = senses.nearest_plant(x, y, me.pheno.vision2) {
        return (Intent { tx, ty, slow: false }, Mode::Food);
    }

    match mind.target {
        None => pick_random_target(me, mind, rng),
        Some((tx, ty)) => {
            let (dx, dy) = (x - tx, y - ty);
            if dx * dx + dy * dy < step * step {
                pick_random_target(me, mind, rng);
            }
        }
    }
    let (tx, ty) = mind.target.unwrap();
    (Intent { tx, ty, slow: false }, Mode::Wander)
}

/// Поело — сразу новая цель, чтобы не топтаться (даже во время бегства).
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    pick_random_target(me, mind, rng);
}

/// Новая цель блуждания — всегда в домашней полосе. Вне полосы — ближайшая её
/// точка по вертикали: существо возвращается в свой слой. В полосе — случайная
/// точка в пределах зрения; 10 попыток, иначе стоим.
fn pick_random_target(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    let (lo, hi) = (me.pheno.body_lo, me.pheno.body_hi);
    if me.y < lo || me.y > hi {
        mind.target = Some((me.x, me.y.clamp(lo, hi)));
        return;
    }
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
