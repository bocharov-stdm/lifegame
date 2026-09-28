//! The step after the program decided it: a calm walk holds its course and turns smoothly when its
//! program says so (`Action::Smooth`). A flight and a strike are not smoothed.

use super::strategy::{Activity, Intent, Me, Mind};

fn unit(x: f64, y: f64) -> (f64, f64) {
    let d = x.hypot(y);
    if d > 0.0 { (x / d, y / d) } else { (0.0, 0.0) }
}

/// The step as it goes: `feeding` — to food or prey; `fleeing` — a flight.
pub(super) fn adjust(me: &Me, mind: &mut Mind, mut intent: Intent, feeding: bool, fleeing: bool) -> Intent {
    let was_alarm = mind.activity == Activity::Alarm;
    if fleeing {
        mind.activity = Activity::Alarm;
        mind.rest_until = 0;
        mind.course = None;
        return intent;
    }
    if intent.attack.is_some() {
        mind.activity = Activity::Feeding;
        mind.rest_until = 0;
        mind.course = None;
        return intent;
    }
    if was_alarm {
        mind.heading = None;
    }
    mind.activity = if feeding { Activity::Feeding } else { Activity::Travelling };
    let Some(smooth) = mind.stance.smooth.filter(|_| !feeding) else {
        mind.course = None;
        return intent;
    };
    // A calm walk holds its course for its ticks while it goes to the same point.
    let mut dir = unit(intent.tx - me.x, intent.ty - me.y);
    if mind.course_until > mind.tick
        && mind.course_target == Some((intent.tx, intent.ty))
        && let Some(course) = mind.course
        && (intent.tx - me.x).hypot(intent.ty - me.y) > me.pheno.speed
    {
        dir = course;
    } else {
        mind.course = Some(unit(dir.0, dir.1));
        mind.course_target = Some((intent.tx, intent.ty));
        mind.course_until = mind.tick + smooth.ticks;
    }
    let dir = unit(dir.0, dir.1);
    let distance = (intent.tx - me.x).hypot(intent.ty - me.y).min(me.pheno.speed);
    intent.tx = (me.x + dir.0 * distance).clamp(me.pheno.x_lo, me.pheno.x_hi);
    intent.ty = (me.y + dir.1 * distance).clamp(me.pheno.y_lo, me.pheno.y_hi);
    // And turns at most its angle a tick inside its home band.
    let (dx, dy) = (intent.tx - me.x, intent.ty - me.y);
    let distance = dx.hypot(dy);
    let (lo, hi) = me.pheno.band(mind.stance.layer);
    let in_layer = me.y >= lo && me.y <= hi;
    if let Some((hx, hy)) = mind.heading
        && distance > 0.0
        && in_layer
    {
        let old = hy.atan2(hx);
        let delta = (dy.atan2(dx) - old + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let angle = old + delta.clamp(-smooth.turn, smooth.turn);
        intent.tx = me.x + angle.cos() * distance.min(me.pheno.speed);
        intent.ty = (me.y + angle.sin() * distance.min(me.pheno.speed)).clamp(lo, hi);
    }
    intent
}
