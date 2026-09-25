//! «Стандартное» поведение: бежит от опасности; иначе оценивает растение,
//! падаль и живую добычу с учётом дороги и времени питания; иначе бродит в своём
//! слое, а оказавшись вне его — возвращается.
//!
//! Решение разбито на `plan`, который говорит ещё и какая ветка сработала:
//! «затаившийся» (`lurker.rs`) ведёт себя так же, но бродит медленно.

use super::strategy::{Intent, Me, Mind};
use crate::config::FLEE_TICKS;
use crate::flock::Circle;
use crate::rng::Rng;
use crate::senses::Senses;

/// Какая ветка решения сработала.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Flee,
    Food,
    Wander,
    /// Back into the flock's circle, at full pace: the circle does not wait for ever.
    Return,
}

/// Below this share of its store a flock member forages anywhere, and keeps foraging until it
/// has `FED_SHARE` again: it does not dart back to its circle after every bite.
pub(crate) const DESPERATE_SHARE: f64 = 0.4;
const FED_SHARE: f64 = 0.7;

/// The circle that limits where this creature takes food: its flock's, unless it forages.
#[inline(always)]
fn feeding_circle(me: &Me, mind: &Mind) -> Option<Circle> {
    me.circle.filter(|_| !mind.social.foraging)
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
    let fullness = me.energy / me.pheno.max_energy;
    if me.circle.is_none() || fullness >= FED_SHARE {
        mind.social.foraging = false;
    } else if fullness < DESPERATE_SHARE {
        mind.social.foraging = true;
    }
    let plant = senses.nearest_plant(me.x, me.y, me.pheno.vision2);
    personal_plant(me, mind, senses, plant);
    if let Some((x, y)) = plant {
        mind.social.observed_food =
            Some(crate::social::Food { x, y, tick: mind.social.tick, observer: me.kinship.id });
    }
    mind.social.outside_since = match me.circle {
        // a guard, a fighter or a forager away from the circle is not straying
        Some(c)
            if !c.holds(me.x, me.y, me.pheno.half)
                && mind.social.territory_guard.is_none()
                && !mind.social.foraging =>
        {
            mind.social.outside_since.or(Some(mind.social.tick))
        }
        _ => None,
    };
    if let Some(f) = mind.social.food
        && (!f.fresh(mind.social.tick)
            || (plant.is_none() && (f.x - me.x).hypot(f.y - me.y) <= me.pheno.size))
    {
        mind.social.food = None;
        mind.social.rejected_food = Some(f);
    }
    let (intent, mode) = plan_inner(me, mind, rng, senses, step);
    if intent.attack.is_some() && intent.tx == me.x && intent.ty == me.y {
        mind.social.activity = crate::social::Activity::Alarm;
        mind.social.rest_until = 0;
        mind.social.course = None;
        return (intent, mode);
    }
    let intent =
        crate::social::adjust(me, mind, intent, mode == Mode::Food, mode == Mode::Flee, mode == Mode::Return);
    (intent, mode)
}

fn plan_inner(me: &Me, mind: &mut Mind, rng: &mut Rng, senses: &impl Senses, step: f64) -> (Intent, Mode) {
    let (x, y, speed) = (me.x, me.y, me.pheno.speed);

    // Испуг: чужак, который может съесть, ближе порога. Бежит FLEE_TICKS тиков
    // и тогда, когда тот пропал из виду: пропал — не значит ушёл. Спокойному
    // хватает порога (дальняя угроза его не пугает), а бегущему нужно всё
    // зрение — выбрать, куда бежать. Ответ тот же, а запрос спокойного в
    // девять раз меньше по площади.
    let within = if mind.flee_ticks > 0 { me.pheno.vision } else { me.pheno.flee };
    let threat = mind
        .social
        .hit
        .filter(|h| mind.social.tick.saturating_sub(h.tick) <= 1)
        .and_then(|h| senses.visible_enemy(me, h.enemy))
        .or_else(|| senses.nearest_threat(me, within));
    if let Some(t) = threat {
        mind.social.observed_alarm =
            Some(crate::social::Alarm { enemy: t.id, x: t.x, y: t.y, tick: mind.social.tick });
    }
    if let Some(t) = threat
        && t.gap <= me.pheno.half
        && me.health_share >= me.pheno.retreat
        && me.energy > me.pheno.size * me.pheno.melee_damage_share
    {
        mind.flee_ticks = 0;
        mind.social.shared_flee = false;
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

    // A flock member feeds inside its circle; a chase it already started may lead out of it.
    let bound = feeding_circle(me, mind);
    let inside = |px: f64, py: f64| bound.is_none_or(|c| c.holds(px, py, me.pheno.half));
    let plant = mind.social.personal_food;
    let corpse = senses.best_corpse(me).filter(|c| inside(c.x, c.y));
    let prey = if mind.attack.is_some() || me.energy <= me.pheno.max_energy * 0.9 {
        senses.prey(me, mind.attack).filter(|p| mind.attack == Some(p.id) || inside(p.x, p.y))
    } else {
        None
    };
    let plant_score = plant.map_or(0.0, |(px, py)| {
        me.pheno.plant_energy * me.pheno.plant_bite_yield * me.pheno.plant_efficiency
            / (((px - x).hypot(py - y) - me.pheno.size).max(0.0) / speed.max(0.01)
                + f64::from(crate::plant::PORTIONS))
    });
    let corpse_score = corpse.map_or(0.0, |c| c.score);
    if let Some(p) = prey
        && (mind.attack == Some(p.id) || p.score > plant_score.max(corpse_score))
    {
        mind.social.personal_food = None;
        return (Intent { tx: p.x, ty: p.y, slow: false, attack: Some(p.id) }, Mode::Food);
    }
    if let Some(c) = corpse
        && c.score > plant_score
    {
        mind.social.personal_food = None;
        return (Intent { tx: c.x, ty: c.y, slow: false, attack: None }, Mode::Food);
    }
    if let Some((tx, ty)) = plant {
        return (Intent { tx, ty, slow: false, attack: None }, Mode::Food);
    }

    // Reports of food elsewhere guide loners (and desperate members); a flock member's
    // reports move the circle instead (`flock::food_goals`).
    if bound.is_none()
        && crate::social::follows_reports(me, mind)
        && let Some(f) = mind.social.food
    {
        return (Intent { tx: f.x, ty: f.y, slow: false, attack: None }, Mode::Wander);
    }
    if let Some(c) = me.circle
        && !c.holds(x, y, 0.0)
    {
        let (tx, ty) = c.toward(x, y, 0.7);
        mind.target = None;
        return (Intent { tx, ty, slow: false, attack: None }, Mode::Return);
    }
    let stale = match mind.target {
        None => true,
        Some((tx, ty)) => {
            let (dx, dy) = (x - tx, y - ty);
            dx * dx + dy * dy < step * step || me.circle.is_some_and(|c| !c.holds(tx, ty, 0.0))
        }
    };
    if stale {
        pick_random_target(me, mind, rng);
    }
    let (tx, ty) = mind.target.unwrap();
    (Intent { tx, ty, slow: false, attack: None }, Mode::Wander)
}

/// The plant this creature feeds on: the one it already goes to while it lives, is seen and
/// lies in its feeding circle; else the nearest such one. `nearest` is the nearest visible
/// plant at all: when it lies in the circle it is the answer, and no second query is needed.
fn personal_plant(me: &Me, mind: &mut Mind, senses: &impl Senses, nearest: Option<(f64, f64)>) {
    let bound = feeding_circle(me, mind);
    let inside = |(x, y): (f64, f64)| bound.is_none_or(|c| c.holds(x, y, me.pheno.half));
    let old = mind.social.personal_food.filter(|&(x, y)| {
        (x - me.x).hypot(y - me.y) <= me.pheno.vision
            && inside((x, y))
            && senses.nearest_plant(x, y, 1e-8).is_some()
    });
    mind.social.personal_food = old.or_else(|| match (nearest, bound) {
        (Some(p), _) if inside(p) => Some(p),
        (Some(_), Some(c)) => senses.nearest_plant_in(me.x, me.y, me.pheno.vision2, c, me.pheno.half),
        _ => None,
    });
}

/// Поело — сразу новая цель, чтобы не топтаться (и во время бегства тоже).
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    pick_random_target(me, mind, rng);
}

/// A new wander target. A flock member wanders inside its circle. Anyone else stays in its
/// home band: outside the band, the nearest band point by depth (it walks back to its layer);
/// inside, a random point within sight; 10 tries, else stand.
fn pick_random_target(me: &Me, mind: &mut Mind, rng: &mut Rng) {
    if let Some(c) = me.circle {
        let angle = rng.uniform(0.0, std::f64::consts::TAU);
        let d = c.radius * 0.8 * rng.random().sqrt();
        mind.target = Some((
            (c.x + angle.cos() * d).clamp(me.pheno.x_lo, me.pheno.x_hi),
            (c.y + angle.sin() * d).clamp(me.pheno.y_lo, me.pheno.y_hi),
        ));
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature::Creature;
    use crate::genome::creature::Gene;
    use crate::senses::{CorpseFood, Prey, Threat};
    use crate::{CreatureGenome, Rules, Space};

    struct FoodSense {
        prey: Option<Prey>,
    }

    impl Senses for FoodSense {
        fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
            Some((1010.0, 1000.0))
        }

        fn best_corpse(&self, _: &Me) -> Option<CorpseFood> {
            Some(CorpseFood { owner: 2, x: 990.0, y: 1000.0, score: 3.0 })
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, _: &Me, _: Option<u64>) -> Option<Prey> {
            self.prey
        }
    }

    /// One plant east of the circle, within sight.
    struct PlantOutside;

    impl Senses for PlantOutside {
        fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
            ((1300.0 - x).powi(2) + (1000.0 - y).powi(2) <= r2).then_some((1300.0, 1000.0))
        }

        fn best_corpse(&self, _: &Me) -> Option<CorpseFood> {
            None
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, _: &Me, _: Option<u64>) -> Option<Prey> {
            None
        }
    }

    #[test]
    fn a_hungry_member_forages_outside_its_circle_until_it_is_fed() {
        let v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE,
            Some(1000.0),
            Some(1000.0),
            Some(30.0),
            Rng::new(1),
        );
        let circle = Circle { x: 1000.0, y: 1000.0, radius: 150.0 };
        let mut mind = Mind::default();
        let mut rng = Rng::new(3);
        for (share, forages) in [(0.8, false), (0.3, true), (0.6, true), (0.75, false), (0.5, false)] {
            let me = Me {
                x: v.x,
                y: v.y,
                energy: v.pheno.max_energy * share,
                kinship: v.kinship(),
                flock: v.flock,
                circle: Some(circle),
                pheno: &v.pheno,
                health_share: 1.0,
            };
            let (_, mode) = plan(&me, &mut mind, &mut rng, &PlantOutside, v.pheno.speed);
            assert_eq!(mind.social.foraging, forages, "fullness {share}");
            assert_eq!(
                mode == Mode::Food,
                forages,
                "fullness {share}: the plant outside is taken only when foraging"
            );
        }
    }

    #[test]
    fn плотоядный_идёт_к_падали_травоядный_к_растению_начатый_бой_сохраняется() {
        for (carnivory, target) in [(100.0, 990.0), (0.0, 1010.0)] {
            let v = Creature::new(
                &Space::default(),
                &Rules::default(),
                CreatureGenome::BASE.with(Gene::Carnivory, carnivory),
                Some(1000.0),
                Some(1000.0),
                Some(30.0),
                Rng::new(1),
            );
            let me = Me {
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                flock: v.flock,
                circle: None,
                pheno: &v.pheno,
                health_share: 1.0,
            };
            let mut mind = Mind::default();
            let mut rng = Rng::new(3);
            let (intent, mode) = plan(&me, &mut mind, &mut rng, &FoodSense { prey: None }, v.pheno.speed);
            assert_eq!(mode, Mode::Food);
            assert_eq!(intent.tx, target);

            mind.attack = Some(9);
            let (fight, _) = plan(
                &me,
                &mut mind,
                &mut rng,
                &FoodSense { prey: Some(Prey { id: 9, x: 1020.0, y: 1000.0, score: 0.1 }) },
                v.pheno.speed,
            );
            assert_eq!(fight.attack, Some(9));
        }
    }
}
