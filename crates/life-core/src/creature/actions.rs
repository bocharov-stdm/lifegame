//! The actions a program's block can take (`program::Action`). A deciding action looks at the
//! scene and says where to step, or None when it cannot be done here and now (no prey, no corpse,
//! a hopeless chase): then the next block is tried. A setting only changes how this tick goes
//! (`apply_setting`). An action only chooses a point or a way to stand; `Creature::act` moves and
//! pays.

use super::program::{Action, Block};
use super::scene::{Scene, behind_border};
use super::strategy::{Chase, Intent, Me, Mind};
use crate::config::MIN_PACE;
use crate::rng::Rng;
use crate::senses::{Hunting, Senses, Taste};

/// Which kind of move an action made: the social layer adjusts each differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Flee,
    Food,
    Wander,
    /// Back into the flock's circle, at full pace: the circle does not wait for ever.
    Return,
    /// Resting: it stands, and the social layer shows it resting.
    Rest,
}

/// A setting's effect on this tick (`Action::is_setting`).
pub(super) fn apply_setting(b: &Block, scene: &mut Scene, me: &Me) {
    match b.action {
        Action::EatForeign => scene.stance.foreign = true,
        Action::Rival => scene.stance.rival = b.arg(0),
        Action::Reach => {
            let share = b.arg(0);
            scene.stance.reach = if share >= 1.0 { f64::INFINITY } else { share * me.pheno.height };
        }
        _ => unreachable!("not a setting: {:?}", b.action),
    }
    scene.retaste();
}

/// The move of the block's action, or None when it cannot be done.
#[inline(always)]
pub(super) fn act(
    b: &Block,
    scene: &mut Scene,
    me: &Me,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl Senses,
) -> Option<(Intent, Mode)> {
    match b.action {
        Action::FightBack => fight_back(b.arg(0), scene, me, mind, senses),
        Action::Flee => flee(b, scene, me, mind, senses),
        Action::Hunt => hunt(b, scene, me, mind, senses),
        Action::EatCorpse => {
            let c = scene.corpse(me, mind, senses)?;
            if b.flag(0) && c.score <= scene.plant_score(me, mind, senses) {
                return None;
            }
            mind.social.personal_food = None;
            let (tx, ty) = approach(me, c.x, c.y, me.pheno.size + c.half);
            Some((go(tx, ty, b.arg(1)), Mode::Food))
        }
        Action::EatPlant => {
            let (px, py) = scene.plant(me, mind, senses)?;
            let (tx, ty) = approach(me, px, py, me.pheno.size);
            Some((go(tx, ty, b.arg(0)), Mode::Food))
        }
        Action::FollowReport => {
            // Reports of food elsewhere guide loners (and desperate members); a flock member's
            // reports move the circle instead (`flock::food_goals`).
            if scene.bound.is_some() || !crate::social::follows_reports(me, mind) {
                return None;
            }
            let f = mind.social.food.filter(|f| !behind_border(me, mind, f.x, f.y))?;
            Some((go(f.x, f.y, b.arg(0)), Mode::Wander))
        }
        Action::ReturnToCircle => return_to_circle(me, mind),
        Action::Wander => {
            let pace = b.arg(0).max(MIN_PACE);
            let (tx, ty) = wander(me, mind, rng, me.pheno.speed * pace, b.arg(1) * me.pheno.vision);
            Some((go(tx, ty, pace), Mode::Wander))
        }
        Action::Ambush => Some((stand(me), Mode::Wander)),
        Action::Surface => to_layer_edge(me, mind, me.pheno.body_lo, b.arg(0)),
        Action::Dive => to_layer_edge(me, mind, me.pheno.body_hi, b.arg(0)),
        Action::Rest => rest(b, me, mind),
        Action::Torpor => Some((Intent { torpor: true, ..stand(me) }, Mode::Wander)),
        Action::EatForeign | Action::Rival | Action::Reach => unreachable!("a setting decides nothing"),
    }
}

/// A step towards (x, y) at `pace` of its speed, striking no one.
#[inline(always)]
fn go(tx: f64, ty: f64, pace: f64) -> Intent {
    Intent { pace: pace.max(MIN_PACE), ..Intent::to(tx, ty) }
}

/// Standing where it is.
#[inline(always)]
fn stand(me: &Me) -> Intent {
    Intent::to(me.x, me.y)
}

/// It strikes back the enemy that struck it, else the threat in reach, if that is no more than
/// `ratio` times its size and it can pay for a strike: it stands and strikes. A defence strikes a
/// body of any size once chosen (`combat::defending`).
fn fight_back(
    ratio: f64,
    scene: &mut Scene,
    me: &Me,
    mind: &mut Mind,
    senses: &impl Senses,
) -> Option<(Intent, Mode)> {
    let t = scene
        .struck
        .filter(|t| t.gap <= me.pheno.half)
        .or_else(|| scene.threats(me, senses, me.pheno.half).0)?;
    if t.half >= me.pheno.half * ratio || me.energy <= me.pheno.size * me.pheno.melee_damage_share {
        return None;
    }
    mind.flee_ticks = 0;
    mind.social.shared_flee = false;
    Some((Intent { attack: Some(t.id), ..stand(me) }, Mode::Food))
}

/// Flight from the nearest threat in sight (the enemy that struck it first); out of sight, on its
/// last course for its memory of ticks — out of sight is not gone. A flight starts from whatever
/// its block saw; under way, only a threat nearer than the block's «again» share of sight (the old
/// flight distance) makes it run its whole memory again, a farther one only steers it while the
/// memory runs down.
fn flee(
    b: &Block,
    scene: &mut Scene,
    me: &Me,
    mind: &mut Mind,
    senses: &impl Senses,
) -> Option<(Intent, Mode)> {
    let threat = scene.struck.or_else(|| scene.threats(me, senses, me.pheno.vision).0);
    match threat {
        Some(t) => {
            if mind.flee_ticks == 0 || t.gap < b.arg(2) * me.pheno.vision {
                mind.flee_ticks = b.args[0].into();
            } else {
                mind.flee_ticks -= 1;
            }
            let (dx, dy) = (me.x - t.x, me.y - t.y);
            let d = dx.hypot(dy);
            if d != 0.0 {
                mind.flee_dx = dx / d;
                mind.flee_dy = dy / d;
            } else {
                // Centres coincide: a steady direction without drawing a number.
                let angle = (crate::rng::mix(me.kinship.id) % 360) as f64 * std::f64::consts::PI / 180.0;
                mind.flee_dx = angle.cos();
                mind.flee_dy = angle.sin();
            }
        }
        None if mind.flee_ticks > 0 => mind.flee_ticks -= 1,
        None => return None,
    }
    let speed = me.pheno.speed;
    let intent = go(me.x + mind.flee_dx * speed, me.y + mind.flee_dy * speed, 1.0);
    Some((Intent { burst: b.flag(1), ..intent }, Mode::Flee))
}

/// The hunt of the prey worth most at the block's terms. With «only if it pays more» it leaves prey
/// that pays no more than its plant or the corpse, unless it already strikes it. A chase that does
/// not close in within its patience is given up: other food this tick, and not this prey for the
/// block's «leave alone» ticks.
fn hunt(
    b: &Block,
    scene: &mut Scene,
    me: &Me,
    mind: &mut Mind,
    senses: &impl Senses,
) -> Option<(Intent, Mode)> {
    let (ratio, caution, patience) = (b.arg(0), b.arg(1), b.args[2]);
    let taste = Taste { foreign: scene.stance.foreign, reach: scene.stance.reach };
    let p = scene.prey(me, mind, senses, Hunting { ratio, caution, taste })?;
    if b.flag(3) && mind.attack != Some(p.id) {
        let corpse = scene.corpse(me, mind, senses).map_or(0.0, |c| c.score);
        if p.score <= scene.plant_score(me, mind, senses).max(corpse) {
            return None;
        }
    }
    match chase(me, mind.social.tick, scene.chasing, &p, u64::from(patience)) {
        Some(c) => {
            mind.chase = Some(c);
            mind.social.personal_food = None;
            scene.stance.strike_ratio = ratio;
            let intent = Intent { attack: Some(p.id), burst: b.flag(4), ..Intent::to(p.x, p.y) };
            Some((intent, Mode::Food))
        }
        None => {
            mind.given_up = Some((p.id, mind.social.tick + u64::from(b.args[5])));
            None
        }
    }
}

/// The chase of prey `p` this tick, or None when it is hopeless: the hunter has not closed the gap
/// to its edge by one of its own steps within `patience` ticks. In reach counts as closing.
fn chase(
    me: &Me,
    tick: u64,
    chasing: Option<Chase>,
    p: &crate::senses::Prey,
    patience: u64,
) -> Option<Chase> {
    let gap = ((p.x - me.x).hypot(p.y - me.y) - me.pheno.half - p.half).max(0.0);
    let fresh = Chase { prey: p.id, mark: gap, since: tick };
    match chasing.filter(|c| c.prey == p.id) {
        None => Some(fresh),
        Some(c) if gap <= 0.0 || gap <= c.mark - me.pheno.speed => Some(fresh),
        Some(c) if tick.saturating_sub(c.since) >= patience => None,
        Some(c) => Some(c),
    }
}

/// A rest in its layer (its circle for a member): it starts one of the block's length unless in
/// the pause after the last, and goes on with one it started. It ends when the block no longer
/// decides (`strategy::plan`).
fn rest(b: &Block, me: &Me, mind: &mut Mind) -> Option<(Intent, Mode)> {
    let in_layer = me.y >= me.pheno.body_lo && me.y <= me.pheno.body_hi;
    let at_home = me.circle.map_or(in_layer, |c| c.holds(me.x, me.y, 0.0));
    if !at_home {
        return None;
    }
    let m = &mut mind.social;
    if m.rest_until <= m.tick {
        if m.tick < m.rest_ready {
            return None;
        }
        m.rest_until = m.tick + u64::from(b.args[0]);
        m.rest_ready = m.rest_until + u64::from(b.args[1]);
    }
    Some((stand(me), Mode::Rest))
}

/// Back into its flock's circle when outside it; around a neighbour's border that lies on the way.
fn return_to_circle(me: &Me, mind: &mut Mind) -> Option<(Intent, Mode)> {
    let c = me.circle.filter(|c| !c.holds(me.x, me.y, 0.0))?;
    let (mut tx, mut ty) = c.toward(me.x, me.y, 0.7);
    if behind_border(me, mind, tx, ty)
        && let Some(a) = mind.social.territory_avoid
    {
        // the part of its own circle away from the neighbour
        let away = c.toward(2.0 * c.x - a.x, 2.0 * c.y - a.y, 0.7);
        if !behind_border(me, mind, away.0, away.1) {
            (tx, ty) = away;
        }
    }
    mind.target = None;
    Some((go(tx, ty, 1.0), Mode::Return))
}

/// Up or down its layer to depth `edge`, straight at `pace`; not done once within a unit of it.
fn to_layer_edge(me: &Me, mind: &mut Mind, edge: f64, pace: f64) -> Option<(Intent, Mode)> {
    if (me.y - edge).abs() <= 1.0 {
        return None;
    }
    // a wander afterwards picks a new target from where it came
    mind.target = None;
    Some((go(me.x, edge, pace), Mode::Wander))
}

/// The wander target, a new one once reached (within one `step`), outside the circle or behind a
/// border; a new one lies at most `reach` away.
fn wander(me: &Me, mind: &mut Mind, rng: &mut Rng, step: f64, reach: f64) -> (f64, f64) {
    let stale = match mind.target {
        None => true,
        Some((tx, ty)) => {
            let (dx, dy) = (me.x - tx, me.y - ty);
            dx * dx + dy * dy < step * step
                || me.circle.is_some_and(|c| !c.holds(tx, ty, 0.0))
                || behind_border(me, mind, tx, ty)
        }
    };
    if stale {
        // a few more draws when the target falls behind a border
        for _ in 0..4 {
            pick_random_target(me, mind, rng, reach);
            if mind.target.is_none_or(|(tx, ty)| !behind_border(me, mind, tx, ty)) {
                break;
            }
        }
    }
    mind.target.unwrap()
}

/// Where to stand to eat food at (fx, fy) that is reached within `reach` of the centre: on the
/// line to it, `EAT_STOP_SHARE` of the reach away, so the food lies beside the body. Already
/// that close — stay.
fn approach(me: &Me, fx: f64, fy: f64, reach: f64) -> (f64, f64) {
    let (dx, dy) = (me.x - fx, me.y - fy);
    let d = dx.hypot(dy);
    let stop = reach * crate::config::EAT_STOP_SHARE;
    if d <= stop {
        return (me.x, me.y);
    }
    (fx + dx / d * stop, fy + dy / d * stop)
}

/// It has just eaten: a new target at once, at most `reach` away, so it does not tread on the spot
/// (in flight too).
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64) {
    pick_random_target(me, mind, rng, reach);
}

/// A new wander target. A flock member wanders inside its circle. Anyone else stays in its
/// home band: outside the band, the nearest band point by depth (it walks back to its layer);
/// inside, a random point from a quarter of `reach` to `reach` away; 10 tries, else stand.
fn pick_random_target(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64) {
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
    // a layer collapsed into a line: a random point never falls on it, and the creature would
    // stand like a post, so it walks only along the line
    let flat = lo == hi;
    for _ in 0..10 {
        let angle = rng.uniform(0.0, std::f64::consts::TAU);
        let dist = rng.uniform(reach * 0.25, reach);
        let tx = me.x + angle.cos() * dist;
        let ty = if flat { lo } else { me.y + angle.sin() * dist };
        if me.pheno.x_lo <= tx && tx <= me.pheno.x_hi && lo <= ty && ty <= hi {
            mind.target = Some((tx, ty));
            return;
        }
    }
    mind.target = Some((me.x, me.y));
}
