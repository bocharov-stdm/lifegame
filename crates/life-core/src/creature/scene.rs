//! What a creature perceives each tick before and while its program runs (`strategy::plan`): the
//! bookkeeping every creature does whatever it decides (its flock foraging flag, the food it
//! reports, the report it drops, the enemy that struck it), and — asked only when a block needs
//! them — threats, its plant, the corpse, the prey and what each food is worth. Senses are pure
//! reads, so asking late or not at all changes nothing but the time spent: a program that never
//! hunts never looks for prey.
//!
//! What it takes for food depends on the settings its program applied this tick (`Stance`): the
//! other niche's food, how far past its layer. They apply before the deciding blocks; a setting
//! still drops what a test of an earlier setting found for the old terms.

use super::program::{Cond, Test};
use super::strategy::{Chase, Me, Mind, Stance};
use crate::flock::Circle;
use crate::senses::{CorpseFood, Hunting, Prey, Senses, Taste, Threat};

/// A flock member below its inherited `forage` share of its store takes food anywhere, and keeps
/// foraging until it has this many times as much: it does not dart back to its circle after
/// every bite. At the base gene (40%) that is 70%, the former fixed thresholds.
pub(crate) const FORAGE_FED: f64 = 1.75;

pub(super) struct Scene {
    /// The enemy that struck it last tick, while still in sight.
    pub struck: Option<Threat>,
    /// The hunt it was on last tick; it goes on only if it hunts that prey again.
    pub chasing: Option<Chase>,
    /// Where it takes food: its flock's circle, unless it forages.
    pub bound: Option<Circle>,
    /// What the settings applied so far set for this tick.
    pub stance: Stance,
    /// The nearest plant in sight, whatever its reach (the preamble's query).
    nearest_plant: Option<(f64, f64)>,
    /// How far the first threat query looks: the farthest threat test of its program.
    threat_range: f64,
    /// Threats found: the radius asked, the nearest threat and the nearest hunter within it.
    threats: Option<(f64, Option<Threat>, Option<Threat>)>,
    plant: Option<Option<(f64, f64)>>,
    corpse: Option<Option<CorpseFood>>,
    prey: Option<(Hunting, Option<Prey>)>,
    plant_score: Option<f64>,
}

impl Scene {
    pub fn perceive(me: &Me, mind: &mut Mind, senses: &impl Senses, threat_range: f64) -> Scene {
        let fullness = me.energy / me.pheno.max_energy;
        if me.circle.is_none() || fullness >= (me.pheno.forage * FORAGE_FED).min(1.0) {
            mind.social.foraging = false;
        } else if fullness < me.pheno.forage {
            mind.social.foraging = true;
        }
        // A creature that does not digest plants neither looks for them nor reports them; it
        // reports the corpse it would eat instead.
        let plant =
            if me.pheno.eats_plants() { senses.nearest_plant(me.x, me.y, me.pheno.vision2) } else { None };
        let seen = plant.or_else(|| {
            (!me.pheno.eats_plants())
                .then(|| senses.best_corpse(me, Taste::OWN).map(|c| (c.x, c.y)))
                .flatten()
        });
        if let Some((x, y)) = seen {
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
        let struck = mind
            .social
            .hit
            .filter(|h| mind.social.tick.saturating_sub(h.tick) <= 1)
            .and_then(|h| senses.visible_enemy(me, h.enemy));
        Scene {
            struck,
            // A chase goes on only while it hunts that prey tick after tick.
            chasing: mind.chase.take(),
            bound: feeding_circle(me, mind),
            stance: Stance::default(),
            nearest_plant: plant,
            threat_range: threat_range * me.pheno.vision,
            threats: None,
            plant: None,
            corpse: None,
            prey: None,
            plant_score: None,
        }
    }

    /// A setting changed what it takes for food: what was found for the old one goes.
    pub fn retaste(&mut self) {
        (self.plant, self.corpse, self.prey, self.plant_score) = (None, None, None, None);
    }

    /// The nearest threat and the nearest hunting threat closer than `within` to their bodies'
    /// edge. The first query covers the farthest threat test of the program, so the tests after it
    /// cost nothing; a farther one (a flight looks with all its sight) asks again.
    pub fn threats(
        &mut self,
        me: &Me,
        senses: &impl Senses,
        within: f64,
    ) -> (Option<Threat>, Option<Threat>) {
        let (any, hunter) = match self.threats {
            Some((r, any, hunter)) if within <= r => (any, hunter),
            _ => {
                let r = within.max(self.threat_range);
                let (any, hunter) = senses.threats_near(me, r);
                self.threats = Some((r, any, hunter));
                (any, hunter)
            }
        };
        (any.filter(|t| t.gap < within), hunter.filter(|t| t.gap < within))
    }

    /// The nearest threat it has found this tick, if any: it reports it to its neighbours.
    pub fn found_threat(&self) -> Option<Threat> {
        self.struck.or(self.threats.and_then(|t| t.1))
    }

    /// Whether a point is where it takes food now: inside its feeding circle, not behind a border.
    #[inline(always)]
    fn inside(&self, me: &Me, mind: &Mind, x: f64, y: f64) -> bool {
        self.bound.is_none_or(|c| c.holds(x, y, me.pheno.half)) && !behind_border(me, mind, x, y)
    }

    fn taste(&self) -> Taste {
        Taste { foreign: self.stance.foreign, reach: self.stance.reach }
    }

    /// Its plant: the one it already goes to while it lives, is seen, lies in its feeding circle
    /// and within its reach; else the nearest such one (`Mind::social.personal_food`).
    pub fn plant(&mut self, me: &Me, mind: &mut Mind, senses: &impl Senses) -> Option<(f64, f64)> {
        if self.plant.is_none() {
            let reach = self.stance.reach;
            let bound = self.bound;
            let inside = |mind: &Mind, (x, y): (f64, f64)| {
                bound.is_none_or(|c| c.holds(x, y, me.pheno.half))
                    && !behind_border(me, mind, x, y)
                    && me.pheno.within_reach(y, reach)
            };
            let old = mind.social.personal_food.filter(|&(x, y)| {
                (x - me.x).hypot(y - me.y) <= me.pheno.vision
                    && inside(mind, (x, y))
                    && senses.nearest_plant(x, y, 1e-8).is_some()
            });
            let plant = old.or_else(|| match self.nearest_plant {
                Some(p) if inside(mind, p) => Some(p),
                Some(_) => {
                    senses.nearest_plant_where(me.x, me.y, me.pheno.vision2, |x, y| inside(mind, (x, y)))
                }
                None => None,
            });
            mind.social.personal_food = plant;
            self.plant = Some(plant);
        }
        self.plant.flatten()
    }

    /// The best corpse in sight it eats, where it takes food.
    pub fn corpse(&mut self, me: &Me, mind: &Mind, senses: &impl Senses) -> Option<CorpseFood> {
        if self.corpse.is_none() {
            self.corpse =
                Some(senses.best_corpse(me, self.taste()).filter(|c| self.inside(me, mind, c.x, c.y)));
        }
        self.corpse.flatten()
    }

    /// The prey worth hunting at these terms: what the meat adds to the tank less the expected
    /// strikes (`Prey::of`); a full tank takes nothing. The prey it strikes already may lead out
    /// of its circle; the one it gave up chasing is no candidate for a while.
    pub fn prey(&mut self, me: &Me, mind: &Mind, senses: &impl Senses, hunting: Hunting) -> Option<Prey> {
        if let Some((terms, found)) = self.prey
            && terms == hunting
        {
            return found;
        }
        let avoid = mind.given_up.filter(|&(_, until)| mind.social.tick < until).map(|(id, _)| id);
        let found = if me.energy < me.pheno.max_energy {
            senses.prey(me, mind.attack, avoid, hunting).filter(|p| {
                p.score > 0.0
                    && (mind.attack == Some(p.id) || self.inside(me, mind, p.x, p.y))
                    && !behind_border(me, mind, p.x, p.y)
            })
        } else {
            None
        };
        self.prey = Some((hunting, found));
        found
    }

    /// What the plant's portions left add to the tank, per tick of the way and the meal.
    pub fn plant_score(&mut self, me: &Me, mind: &mut Mind, senses: &impl Senses) -> f64 {
        if let Some(score) = self.plant_score {
            return score;
        }
        let score = self.plant(me, mind, senses).map_or(0.0, |(px, py)| {
            let portions = f64::from(senses.plant_portions(px, py));
            let bite = me.pheno.plant_energy * me.pheno.plant_bite_yield * me.pheno.plant_efficiency
                / f64::from(crate::plant::PORTIONS);
            let room = (me.pheno.max_energy - me.energy).max(0.0);
            (bite * portions).min(room)
                / (((px - me.x).hypot(py - me.y) - me.pheno.size).max(0.0) / me.pheno.speed.max(0.01)
                    + portions)
                    .max(1.0)
        });
        self.plant_score = Some(score);
        score
    }

    /// Whether a block's test holds.
    pub fn test(&mut self, t: Test, me: &Me, mind: &mut Mind, senses: &impl Senses) -> bool {
        let value = t.value();
        let holds = match t.cond {
            Cond::Always => true,
            Cond::Fullness => me.energy / me.pheno.max_energy >= value,
            Cond::Health => me.health_share >= value,
            Cond::Depth => me.y >= value * me.pheno.height,
            Cond::ThreatNear => self.threats(me, senses, value * me.pheno.vision).0.is_some(),
            Cond::HunterNear => self.threats(me, senses, value * me.pheno.vision).1.is_some(),
            // the preamble has the last tick's enemy already; a longer window asks the senses
            Cond::Struck if t.param <= 1 => self.struck.is_some(),
            Cond::Struck => mind
                .social
                .hit
                .filter(|h| mind.social.tick.saturating_sub(h.tick) <= u64::from(t.param))
                .is_some_and(|h| senses.visible_enemy(me, h.enemy).is_some()),
            Cond::Fleeing => mind.flee_ticks > 0,
            Cond::Resting => mind.social.rest_until > mind.social.tick,
            Cond::FoodSeen => {
                self.plant(me, mind, senses).is_some() || self.corpse(me, mind, senses).is_some()
            }
        };
        holds != t.negate
    }
}

/// Whether a point lies behind the border this creature respects now (the circle it walks
/// around, `territory::steer`): going there is futile, so it is no target for food, a return or
/// a wander. A starving creature ignores borders; a member inside its own circle may use all of
/// it, as `steer` lets it be there. A moderate border is respected only in sight of a member, so
/// what lies behind it becomes a target again once no member is seen (the border stays leaky).
#[inline(always)]
pub(super) fn behind_border(me: &Me, mind: &Mind, x: f64, y: f64) -> bool {
    if me.energy < me.pheno.max_energy * crate::territory::STARVING_SHARE {
        return false;
    }
    let Some(a) = mind.social.territory_avoid.filter(|a| a.flock != me.flock) else { return false };
    if me.circle.is_some_and(|c| c.holds(me.x, me.y, 0.0) && c.holds(x, y, 0.0)) {
        return false;
    }
    let r = a.radius + me.pheno.half + 4.0;
    (x - a.x).powi(2) + (y - a.y).powi(2) < r * r
}

/// The circle that limits where this creature takes food: its flock's, unless it forages.
#[inline(always)]
fn feeding_circle(me: &Me, mind: &Mind) -> Option<Circle> {
    me.circle.filter(|_| !mind.social.foraging)
}
