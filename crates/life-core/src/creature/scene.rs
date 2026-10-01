//! What a creature perceives each tick before and while its program runs (`strategy::plan`): the
//! enemy that struck it, and — asked only when a block needs them — threats, its plant, the
//! corpse, the prey and what each food is worth. Senses are pure reads, so asking late or not at
//! all changes nothing but the time spent: a program that never hunts never looks for prey. A test
//! writes nothing into its memory: only the block that goes to a plant keeps it (`Mind::personal_food`).
//!
//! What it takes for food depends on the settings its program applied this tick (`Stance`): the
//! other niche's food, how far past its layer. They apply before the deciding blocks; a setting
//! still drops what a test of an earlier setting found for the old terms.

use super::program::{Block, Cond, MODES, Program, Test};
use super::strategy::{Chase, Me, Mind, Stance};
use crate::senses::{CorpseFood, Hunting, Prey, Senses, Taste, Threat};

/// How it picks its plant (`Action::EatPlant`): keep the one it goes to, and else the most
/// profitable one rather than the nearest. The templates keep it and take the nearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlantChoice {
    pub keep: bool,
    pub best: bool,
}

impl PlantChoice {
    /// The choice the tests and the weighing of other food read.
    pub const USUAL: PlantChoice = PlantChoice { keep: true, best: false };

    fn index(self) -> usize {
        usize::from(self.keep) | usize::from(self.best) << 1
    }
}

pub(super) struct Scene {
    /// The enemy that struck it last tick, while still in sight.
    pub struck: Option<Threat>,
    /// The hunt it was on last tick; it goes on only if it hunts that prey again.
    pub chasing: Option<Chase>,
    /// What the settings applied so far set for this tick.
    pub stance: Stance,
    /// The nearest plant in sight, whatever its reach, once asked (`nearest_plant`): a creature
    /// that goes to its kept plant, or decides before any block needs food, never looks.
    nearest_plant: Option<Option<(f64, f64)>>,
    /// The plant it went to when the tick began (`Mind::personal_food`).
    kept_plant: Option<(f64, f64)>,
    /// How far the first threat query looks: the farthest threat test of its program.
    threat_range: f64,
    /// Its most permissive hunt's ratio, for `Cond::PreySeen`; None: it never hunts.
    hunt_ratio: Option<f64>,
    /// Threats found: the radius asked, the nearest threat and the nearest hunter within it.
    threats: Option<(f64, Option<Threat>, Option<Threat>)>,
    /// Prey in sight found: the radius asked and the nearest within it.
    prey_near: Option<(f64, Option<Threat>)>,
    /// Its plant by each way of choosing (`PlantChoice::index`).
    plants: [Option<Option<(f64, f64)>>; 4],
    corpse: Option<Option<CorpseFood>>,
    prey: Option<(Hunting, Option<Prey>)>,
    plant_score: Option<f64>,
}

impl Scene {
    pub fn perceive(me: &Me, mind: &mut Mind, senses: &impl Senses, program: &Program) -> Scene {
        let struck = mind
            .hit
            .filter(|h| mind.tick.saturating_sub(h.tick) <= 1)
            .and_then(|h| senses.visible_enemy(me, h.enemy));
        Scene {
            struck,
            // A chase goes on only while it hunts that prey tick after tick.
            chasing: mind.chase.take(),
            stance: Stance::default(),
            nearest_plant: None,
            kept_plant: mind.personal_food,
            threat_range: program.threat_range() * me.pheno.vision,
            hunt_ratio: program.hunt_ratio(),
            threats: None,
            prey_near: None,
            plants: [None; 4],
            corpse: None,
            prey: None,
            plant_score: None,
        }
    }

    /// The nearest plant in sight, whatever its reach; none for one that digests no plants. Plants
    /// stand still through the moves, so asking late finds what asking first would.
    fn nearest_plant(&mut self, me: &Me, senses: &impl Senses) -> Option<(f64, f64)> {
        *self.nearest_plant.get_or_insert_with(|| {
            if me.pheno.eats_plants() { senses.nearest_plant(me.x, me.y, me.pheno.vision2) } else { None }
        })
    }

    /// A setting changed what it takes for food: what was found for the old one goes.
    pub fn retaste(&mut self) {
        (self.plants, self.corpse, self.prey, self.plant_score, self.prey_near) =
            ([None; 4], None, None, None, None);
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

    /// The nearest stranger its hunts would take closer than `within` to its body's edge
    /// (`Cond::PreySeen`); none for a program that never hunts.
    fn prey_near(&mut self, me: &Me, senses: &impl Senses, within: f64) -> Option<Threat> {
        let ratio = self.hunt_ratio?;
        let found = match self.prey_near {
            Some((r, found)) if within <= r => found,
            _ => {
                let found = senses.nearest_prey(me, ratio, self.taste(me), within);
                self.prey_near = Some((within, found));
                found
            }
        };
        found.filter(|p| p.gap < within)
    }

    /// The nearest threat it has found this tick, if any: it reports it to its neighbours.
    pub fn found_threat(&self) -> Option<Threat> {
        self.struck.or(self.threats.and_then(|t| t.1))
    }

    /// What it takes for food as the settings applied so far set it.
    pub fn taste(&self, me: &Me) -> Taste {
        Taste {
            foreign: self.stance.foreign,
            reach: self.stance.reach,
            layer: me.pheno.layer(self.stance.layer),
        }
    }

    /// Its plant the usual way (`PlantChoice::USUAL`): the one it already goes to while it lives,
    /// is seen and lies within its reach; else the nearest such one. It remembers the choice
    /// (`Mind::personal_food`).
    pub fn plant(&mut self, me: &Me, mind: &mut Mind, senses: &impl Senses) -> Option<(f64, f64)> {
        let plant = self.plant_by(me, senses, PlantChoice::USUAL);
        mind.personal_food = plant;
        plant
    }

    /// Its plant chosen `how`: the one it went to when the tick began while it lives, is seen and
    /// lies within its reach (if it keeps it); else the most profitable or the nearest such one.
    pub fn plant_by(&mut self, me: &Me, senses: &impl Senses, how: PlantChoice) -> Option<(f64, f64)> {
        if let Some(plant) = self.plants[how.index()] {
            return plant;
        }
        let taste = self.taste(me);
        let inside = |(_, y): (f64, f64)| taste.admits(y);
        // a diet that digests no plants neither eats nor goes for one, the plant it went to while
        // young included
        let old = self.kept_plant.filter(|&(x, y)| {
            how.keep
                && me.pheno.eats_plants()
                && (x - me.x).hypot(y - me.y) <= me.pheno.vision
                && inside((x, y))
                && senses.nearest_plant(x, y, 1e-8).is_some()
        });
        let plant = old.or_else(|| match self.nearest_plant(me, senses) {
            // no plant in sight, or it eats none
            None => None,
            Some(_) if how.best => senses.best_plant(me, taste),
            Some(p) if inside(p) => Some(p),
            Some(_) => senses.nearest_plant_where(me.x, me.y, me.pheno.vision2, |x, y| inside((x, y))),
        });
        self.plants[how.index()] = Some(plant);
        plant
    }

    /// The best corpse in sight it eats.
    pub fn corpse(&mut self, me: &Me, senses: &impl Senses) -> Option<CorpseFood> {
        if self.corpse.is_none() {
            self.corpse = Some(senses.best_corpse(me, self.taste(me)));
        }
        self.corpse.flatten()
    }

    /// The prey worth hunting at these terms: what the meat adds to the tank less the expected
    /// strikes (`Prey::of`); a full tank takes nothing. The one it gave up chasing is no candidate
    /// for a while.
    pub fn prey(&mut self, me: &Me, mind: &Mind, senses: &impl Senses, hunting: Hunting) -> Option<Prey> {
        if let Some((terms, found)) = self.prey
            && terms == hunting
        {
            return found;
        }
        let avoid = mind.given_up.filter(|&(_, until)| mind.tick < until).map(|(id, _)| id);
        let found = if me.energy < me.pheno.max_energy {
            senses.prey(me, mind.attack, avoid, hunting).filter(|p| p.score > 0.0)
        } else {
            None
        };
        self.prey = Some((hunting, found));
        found
    }

    /// What the portions left of its usual plant add to the tank, per tick of the way and the
    /// meal; the weighing does not choose the plant.
    pub fn plant_score(&mut self, me: &Me, senses: &impl Senses) -> f64 {
        if let Some(score) = self.plant_score {
            return score;
        }
        let score = self
            .plant_by(me, senses, PlantChoice::USUAL)
            .map_or(0.0, |(px, py)| crate::senses::plant_worth(me, px, py, senses.plant_portions(px, py)));
        self.plant_score = Some(score);
        score
    }

    /// Whether all the block's tests hold, tested in order until one fails.
    pub fn holds(&mut self, b: &Block, me: &Me, mind: &mut Mind, senses: &impl Senses) -> bool {
        b.when.iter().all(|&t| self.test(t, me, mind, senses))
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
                .hit
                .filter(|h| mind.tick.saturating_sub(h.tick) <= u64::from(t.param))
                .is_some_and(|h| senses.visible_enemy(me, h.enemy).is_some()),
            Cond::Fleeing => mind.flee_ticks > 0,
            Cond::Resting => mind.rest_until > mind.tick,
            Cond::FoodSeen => {
                self.plant_by(me, senses, PlantChoice::USUAL).is_some() || self.corpse(me, senses).is_some()
            }
            Cond::PreySeen => self.prey_near(me, senses, value * me.pheno.vision).is_some(),
            Cond::PlantSeen => self.plant_by(me, senses, PlantChoice::USUAL).is_some(),
            Cond::CorpseSeen => self.corpse(me, senses).is_some(),
            Cond::Age => me.age >= value * me.pheno.lifespan,
            Cond::Winded => me.winded,
            Cond::Cold => me.pheno.coldness(me.y) >= value,
            Cond::AboveLayer => me.y < me.pheno.band(self.stance.layer).0,
            Cond::BelowLayer => me.y > me.pheno.band(self.stance.layer).1,
            Cond::InLayer => {
                let (lo, hi) = me.pheno.band(self.stance.layer);
                me.y >= lo && me.y <= hi
            }
            Cond::Mode => {
                let k = usize::from(t.param).clamp(1, MODES) - 1;
                mind.modes[k] > mind.tick
            }
        };
        holds != t.negate
    }
}
