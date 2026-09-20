//! «Стандартное» поведение существа — исходное: бежит от чужого, который может
//! его съесть; иначе идёт к ближайшему видимому растению; иначе бродит в своём
//! слое, а оказавшись вне его (ушло за едой, убегало) — возвращается.
//!
//! Решение разбито на `plan`, который говорит ещё и какая ветка сработала:
//! «затаившийся» (`lurker.rs`) ведёт себя так же, но бродит медленно.

use super::strategy::{Intent, Me, Mind};
use crate::config::FLEE_TICKS;
use crate::rng::Rng;
use crate::senses::Senses;

/// Какая ветка решения сработала.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Flee,
    Food,
    Wander,
}

#[inline(always)]
pub(crate) fn decide(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl Senses) -> Intent {
    plan(me, mind, rng, senses, me.pheno.speed).0
}

/// Куда идти и почему. `step` — длина шага, если существо будет бродить: по
/// ней решается, дошло ли оно до цели.
#[inline(always)]
pub(super) fn plan(
    me: &Me,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl Senses,
    step: f64,
) -> (Intent, Mode) {
    let (x, y, speed) = (me.x, me.y, me.pheno.speed);

    // Испуг: чужак, который может съесть, ближе порога. Бежит FLEE_TICKS тиков
    // и тогда, когда тот пропал из виду: пропал — не значит ушёл. Спокойному
    // хватает порога (дальняя угроза его не пугает), а бегущему нужно всё
    // зрение — выбрать, куда бежать. Ответ тот же, а запрос спокойного в
    // девять раз меньше по площади.
    let within = if mind.flee_ticks > 0 { me.pheno.vision } else { me.pheno.flee };
    let threat = senses.nearest_threat(me, within);
    if let Some(t) = threat
        && t.gap <= me.pheno.half
        && me.health_share >= me.pheno.retreat
        && me.energy > me.pheno.size * 0.05
    {
        mind.flee_ticks = 0;
        return (Intent { tx: x, ty: y, slow: false, attack: Some(t.id) }, Mode::Food);
    }
    let mut fleeing = false;
    if mind.flee_ticks > 0 {
        fleeing = true;
        mind.flee_ticks -= 1;
    }
    if let Some(t) = threat
        && t.gap < me.pheno.flee
    {
        mind.flee_ticks = FLEE_TICKS;
        fleeing = true;
    }

    if fleeing {
        // прочь от видимой угрозы; не видно — по прежнему вектору
        if let Some(t) = threat {
            let (dx, dy) = (x - t.x, y - t.y);
            let d = dx.hypot(dy);
            if d != 0.0 {
                mind.flee_dx = dx / d;
                mind.flee_dy = dy / d;
            } else {
                // Совпавшие центры: устойчивое направление без расхода RNG.
                let angle = (crate::rng::mix(me.kinship.id) % 360) as f64 * std::f64::consts::PI / 180.0;
                mind.flee_dx = angle.cos();
                mind.flee_dy = angle.sin();
            }
        }
        let intent =
            Intent { tx: x + mind.flee_dx * speed, ty: y + mind.flee_dy * speed, slow: false, attack: None };
        return (intent, Mode::Flee);
    }

    let plant = senses.nearest_plant(x, y, me.pheno.vision2);
    let prey = if mind.attack.is_some() || me.energy <= me.pheno.max_energy * 0.9 {
        senses.prey(me, mind.attack)
    } else {
        None
    };
    let plant_score = plant.map_or(0.0, |(px, py)| {
        me.pheno.plant_energy * me.pheno.plant_efficiency
            / (((px - x).hypot(py - y) - me.pheno.size).max(0.0) / speed.max(0.01) + 1.0)
    });
    if let Some(p) = prey
        && (mind.attack == Some(p.id) || p.score > plant_score)
    {
        return (Intent { tx: p.x, ty: p.y, slow: false, attack: Some(p.id) }, Mode::Food);
    }
    if let Some((tx, ty)) = plant {
        return (Intent { tx, ty, slow: false, attack: None }, Mode::Food);
    }

    if let Some(g) = me.flock_goal {
        let (tx, ty) = if (x - g.x).hypot(y - g.y) > me.pheno.vision { (g.x, g.y) } else { (g.tx, g.ty) };
        return (Intent { tx, ty, slow: false, attack: None }, Mode::Wander);
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
    (Intent { tx, ty, slow: false, attack: None }, Mode::Wander)
}

/// Поело — сразу новая цель, чтобы не топтаться (и во время бегства тоже).
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
