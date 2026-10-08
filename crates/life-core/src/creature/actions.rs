//! The actions a program's block can take (`program::Action`). A deciding action looks at the
//! scene and says where to step, or None when it cannot be done here and now (no prey, no corpse,
//! a hopeless chase): then the next block is tried. A setting only changes how this tick goes
//! (`apply_setting`). An action only chooses a point or a way to stand; `Creature::act` moves and
//! pays.

use super::program::{Action, Block, MODES};
use super::scene::{PlantChoice, Scene};
use super::strategy::{Aid, Chase, Divide, Graze, Heal, Intent, Me, Mind, Shoot, Smooth};
use crate::config::MIN_PACE;
use crate::rng::Rng;
use crate::senses::{Hunting, Senses};

/// Which kind of move an action made: the steering adjusts each differently (`steer::adjust`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Flee,
    Food,
    Wander,
    /// Standing: a rest, an ambush, torpor — no step, and it shows as resting.
    Rest,
    /// Defending its child: it goes for the enemy, not smoothed, in alarm.
    Defend,
}

/// A setting's effect on this tick (`Action::is_setting`); a mode lasts its ticks.
pub(super) fn apply_setting(b: &Block, scene: &mut Scene, me: &Me, mind: &mut Mind) {
    match b.action {
        Action::EatForeign => scene.stance.foreign = true,
        Action::Rival => scene.stance.rival = b.arg(0),
        Action::Reach => {
            let share = b.arg(0);
            scene.stance.reach = if share >= 1.0 { f64::INFINITY } else { share * me.pheno.height };
        }
        Action::Layer => {
            let (a, z) = (b.arg(0), b.arg(1));
            scene.stance.layer = if a <= z { (a, z) } else { (z, a) };
        }
        // the rest leave what it takes for food as it is
        Action::Mode => {
            let k = usize::from(b.args[0]).clamp(1, MODES) - 1;
            mind.modes[k] = mind.tick + u64::from(b.args[1]);
            return;
        }
        Action::Smooth => {
            scene.stance.smooth = Some(Smooth { ticks: b.args[0].into(), turn: b.arg(1) });
            return;
        }
        Action::Divide => {
            scene.stance.divide = Some(Divide { tank: b.arg(0), share: b.arg(1) });
            return;
        }
        Action::Heal => {
            scene.stance.heal = Some(Heal { tank: b.arg(0), calm: b.args[1].into() });
            return;
        }
        Action::Graze => {
            scene.stance.graze = Some(Graze { plants: b.flag(0), corpses: b.flag(1), until: b.arg(2) });
            return;
        }
        Action::Spare => {
            scene.stance.spare = b.arg(0);
            return;
        }
        Action::Shoot => {
            scene.stance.shoot = Some(Shoot { from: b.arg(0), keep: b.arg(1) });
            return;
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
            let c = scene.corpse(me, senses)?;
            if b.flag(0) && c.score <= scene.plant_score(me, senses) {
                return None;
            }
            mind.personal_food = None;
            let (tx, ty) = approach(me, c.x, c.y, me.pheno.size + c.half);
            Some((go(tx, ty, b.arg(1)), Mode::Food))
        }
        Action::EatPlant => {
            let how = PlantChoice { keep: b.flag(1), best: b.flag(2) };
            let (px, py) = if how == PlantChoice::USUAL {
                scene.plant(me, mind, senses)?
            } else {
                let plant = scene.plant_by(me, senses, how)?;
                mind.personal_food = Some(plant);
                plant
            };
            let (tx, ty) = approach(me, px, py, me.pheno.size);
            Some((go(tx, ty, b.arg(0)), Mode::Food))
        }
        Action::Wander => {
            let pace = b.arg(0).max(MIN_PACE);
            let layer = scene.stance.layer;
            let (tx, ty) = wander(me, mind, rng, me.pheno.speed * pace, b.arg(1) * me.pheno.vision, layer);
            Some((go(tx, ty, pace), Mode::Wander))
        }
        Action::Ambush => Some((stand(me), Mode::Rest)),
        Action::Surface => to_layer_edge(me, mind, me.pheno.band(scene.stance.layer).0, b.arg(0)),
        Action::Dive => to_layer_edge(me, mind, me.pheno.band(scene.stance.layer).1, b.arg(0)),
        Action::Rest => rest(b, me, mind, me.pheno.band(scene.stance.layer)),
        Action::Torpor => Some((Intent { torpor: true, ..stand(me) }, Mode::Rest)),
        Action::DefendChild => defend_child(b, scene, me, mind, senses),
        _ => unreachable!("a setting decides nothing: {:?}", b.action),
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
    Some((Intent { attack: Some(t.id), ..stand(me) }, Mode::Food))
}

/// The defence of its child in need (`Action::DefendChild`): go for the enemy and strike it, any
/// size. Not while resting from the last defence, not with a tank at or below the block's share;
/// an episode against one enemy lasts at most the block's ticks, then it rests from defending.
fn defend_child(
    b: &Block,
    scene: &mut Scene,
    me: &Me,
    mind: &mut Mind,
    senses: &impl Senses,
) -> Option<(Intent, Mode)> {
    if mind.tick < mind.aid_cooldown || me.energy <= me.pheno.max_energy * b.arg(1) {
        end_defence(mind);
        return None;
    }
    let within = b.arg(0) * me.pheno.vision;
    let prefer = mind.aid.map(|a| a.victim);
    let Some((victim, enemy)) = senses.child_in_need(me, within, mind.tick, u64::from(b.args[4]), prefer)
    else {
        end_defence(mind);
        return None;
    };
    let started = match mind.aid {
        Some(a) if a.victim == victim && a.enemy == enemy.id => a.started,
        _ => mind.tick,
    };
    if mind.tick.saturating_sub(started) >= u64::from(b.args[2]) {
        end_defence(mind);
        return None;
    }
    mind.aid = Some(Aid { victim, enemy: enemy.id, started, pause: b.args[3] });
    scene.stance.defending = Some(enemy.id);
    Some((Intent { attack: Some(enemy.id), ..Intent::to(enemy.x, enemy.y) }, Mode::Defend))
}

/// A defence under way ends: it rests from defending for its block's pause.
pub(super) fn end_defence(mind: &mut Mind) {
    if let Some(aid) = mind.aid.take() {
        mind.aid_cooldown = mind.tick + u64::from(aid.pause);
    }
}

/// Flight from the nearest threat in sight (the enemy that struck it first); out of sight, on its
/// last course for its memory of ticks — out of sight is not gone. A flight starts from whatever
/// its block saw; under way, only a threat nearer than the block's «again» share of sight (the old
/// flight distance) makes it run its whole memory again, a farther one only steers it while the
/// memory runs down. It runs at the block's pace, its course tilted down or up by the block's tilt.
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
            // straight (100) leaves the course exactly as it is
            if b.args[4] != 100 {
                let (dx, dy) = (mind.flee_dx, mind.flee_dy + b.arg(4));
                let d = dx.hypot(dy);
                if d > 0.0 {
                    (mind.flee_dx, mind.flee_dy) = (dx / d, dy / d);
                }
            }
        }
        None if mind.flee_ticks > 0 => mind.flee_ticks -= 1,
        None => return None,
    }
    let speed = me.pheno.speed;
    let intent = go(me.x + mind.flee_dx * speed, me.y + mind.flee_dy * speed, b.arg(3));
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
    let taste = scene.taste(me);
    let range = b.arg(6) * me.pheno.vision;
    let p = scene.prey(me, mind, senses, Hunting { ratio, caution, taste, range })?;
    if b.flag(3) && mind.attack != Some(p.id) {
        let corpse = scene.corpse(me, senses).map_or(0.0, |c| c.score);
        if p.score <= scene.plant_score(me, senses).max(corpse) {
            return None;
        }
    }
    match chase(me, mind.tick, scene.chasing, &p, u64::from(patience)) {
        Some(c) => {
            mind.chase = Some(c);
            mind.personal_food = None;
            scene.stance.strike_ratio = ratio;
            let intent = Intent { attack: Some(p.id), burst: b.flag(4), ..go(p.x, p.y, b.arg(7)) };
            Some((intent, Mode::Food))
        }
        None => {
            mind.given_up = Some((p.id, mind.tick + u64::from(b.args[5])));
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

/// A rest in its home band: it starts one of the block's length unless in the pause after the
/// last, and goes on with one it started. It ends when the block no longer decides
/// (`strategy::plan`).
fn rest(b: &Block, me: &Me, mind: &mut Mind, (lo, hi): (f64, f64)) -> Option<(Intent, Mode)> {
    let at_home = me.y >= lo && me.y <= hi;
    if !at_home {
        return None;
    }
    if mind.rest_until <= mind.tick {
        if mind.tick < mind.rest_ready {
            return None;
        }
        mind.rest_until = mind.tick + u64::from(b.args[0]);
        mind.rest_ready = mind.rest_until + u64::from(b.args[1]);
    }
    Some((stand(me), Mode::Rest))
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

/// The wander target, a new one once reached (within one `step`) or once its `layer` changed and
/// it lies out of the new home band; a new one lies at most `reach` away in the band. A body that
/// grows narrows its band (the margin is the body) but keeps its layer, and keeps its target —
/// brought within the bounds the grown body reaches, or it would stand at a wall short of it.
fn wander(me: &Me, mind: &mut Mind, rng: &mut Rng, step: f64, reach: f64, layer: (f64, f64)) -> (f64, f64) {
    let band = me.pheno.band(layer);
    let moved = mind.target_layer != layer;
    // A target kept as the body grew is brought within the bounds and the band the grown body has
    // (both margins are the body): a calm walk keeps to the band and would stand short of it.
    let p = &me.pheno;
    let (y_lo, y_hi) = if moved { (p.y_lo, p.y_hi) } else { band };
    mind.target = mind.target.map(|(tx, ty)| (tx.clamp(p.x_lo, p.x_hi), ty.clamp(y_lo, y_hi)));
    let stale = mind.target.is_none_or(|(tx, ty)| {
        let (dx, dy) = (me.x - tx, me.y - ty);
        dx * dx + dy * dy < step * step || (moved && (ty < band.0 || ty > band.1))
    });
    mind.target_layer = layer;
    if stale {
        pick_random_target(me, mind, rng, reach, band);
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

/// It has just eaten: a new target at once, at most `reach` away in the band of `layer`, so it does
/// not tread on the spot (in flight too). The target is `layer`'s: a wander in another layer drops
/// it if it lies out of that layer's band, as it drops any target of a layer it left.
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64, layer: (f64, f64)) {
    mind.target_layer = layer;
    pick_random_target(me, mind, rng, reach, me.pheno.band(layer));
}

/// A new wander target in its home band: outside the band, the nearest band point by depth (it
/// walks back to its layer); inside, a random point from a quarter of `reach` to `reach` away; 10
/// tries, else stand.
fn pick_random_target(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64, (lo, hi): (f64, f64)) {
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
