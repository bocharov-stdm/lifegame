//! How a creature decides its step. The step splits in two: the strategy **decides** where to go
//! (`decide`), the creature **acts** (`Creature::act`: a step of its pace, clamped to the world,
//! the upkeep, death). The strategy sees only itself (`Me`), its program, its memory (`Mind`), its
//! generator and its senses — it cannot move, feed or divide the creature. So the decisions run in
//! parallel (`par.rs`).
//!
//! What it decides is its behaviour program for its stage of life (`program/`): each tick it
//! perceives the scene (`scene.rs`); the settings whose tests hold set how this tick goes,
//! wherever they stand; then its deciding blocks run in order and the first whose action can be
//! done decides (`actions.rs`); a calm step is smoothed and a parent covering its child goes for
//! the enemy (`steer::adjust`). What the settings set (`Stance`) stays in its memory for the
//! world's eating and fighting phases. The
//! `strategy` gene names the template a founder's programs start from: after that they are
//! inherited, drift and mutate on their own.

use super::actions::{self, Mode};
use super::program::{Action, Block, MODES, Program};
use super::scene::Scene;
use super::{Kinship, Phenotype};
use crate::genome::Variant;
use crate::rng::Rng;
use crate::senses::Senses;

/// The template a founder's programs start from (`Program::template`). A child keeps its parent's
/// template name, while its programs are inherited and mutate.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Strategy {
    /// Flees from dangerous strangers; else goes for the food worth most; else wanders its layer.
    #[default]
    Standard,
    /// The same, but with no food in sight it wanders at a third of its speed, and cheaper.
    Lurker,
}

impl Strategy {
    /// Every variant in the order of `VARIANTS`.
    pub const ALL: [Strategy; 2] = [Strategy::Standard, Strategy::Lurker];

    /// The strategy of a gene value — a variant's number.
    #[inline]
    pub fn from_gene(value: f64) -> Strategy {
        debug_assert!(
            value >= 0.0 && value.fract() == 0.0 && (value as usize) < Self::ALL.len(),
            "strategy gene out of its variants: {value}"
        );
        Self::ALL.get(value as usize).copied().unwrap_or_default()
    }
}

/// The strategy gene's variants, in the order of `Strategy`. Append only.
pub const VARIANTS: [Variant; 2] = [
    Variant {
        key: "standard",
        label: "стандартный",
        about: "Программа основателей: бежит от опасных, охотится и ест, что выгоднее, иначе бродит в своём слое.",
    },
    Variant {
        key: "lurker",
        label: "затаившийся",
        about: "Как стандартный, но пока не видит еды, бродит втрое медленнее — и тратит меньше.",
    },
];

/// What its program set for this tick (`Action::is_setting`), the size limit of the target it
/// chose and the food its deciding block went for: the world's eating, fighting and dividing
/// phases read it after the moves, the healing and the kinship of the next tick before them.
/// Without a setting its default is «nothing»: the whole depth, no smoothing, no division, no
/// healing, no eating on the move, no children spared, no shots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stance {
    /// It eats the other niche's food too (`Action::EatForeign`).
    pub foreign: bool,
    /// It strikes a stranger this many times smaller eating the same food beside it; 0: none.
    pub rival: f64,
    /// How far past its layer it goes for food, y; infinite: anywhere (`Action::Reach`).
    pub reach: f64,
    /// The prey it chose to strike must be this many times smaller (its hunt's ratio); 0: it
    /// chose none to hunt.
    pub strike_ratio: f64,
    /// Its layer, shares of the world's depth, top and bottom (`Action::Layer`,
    /// `Phenotype::band`).
    pub layer: (f64, f64),
    /// A calm walk holds its course and turns smoothly (`Action::Smooth`).
    pub smooth: Option<Smooth>,
    /// It divides (`Action::Divide`).
    pub divide: Option<Divide>,
    /// It heals from the next tick (`Action::Heal`).
    pub heal: Option<Heal>,
    /// It eats on the move (`Action::Graze`).
    pub graze: Option<Graze>,
    /// It knows its child until the child has grown to this share (`Action::Spare`); 0: never.
    pub spare: f64,
    /// It shoots (`Action::Shoot`).
    pub shoot: Option<Shoot>,
    /// What its deciding block went for this tick: it eats that food on contact whether it eats on
    /// the move or not.
    pub goes_for: Option<Food>,
    /// The enemy of its child it defends this tick: it strikes it whatever its size.
    pub defending: Option<u64>,
    /// What the others read of it after this move (`Menace`).
    pub menace: Menace,
    /// Set by a move (`plan`): before its first move the world reads its program's shape instead
    /// (`Creature::menace`).
    pub moved: bool,
}

impl Default for Stance {
    fn default() -> Self {
        Stance {
            foreign: false,
            rival: 0.0,
            reach: f64::INFINITY,
            strike_ratio: 0.0,
            layer: (0.0, 1.0),
            smooth: None,
            divide: None,
            heal: None,
            graze: None,
            spare: 0.0,
            shoot: None,
            goes_for: None,
            defending: None,
            menace: Menace::NONE,
            moved: false,
        }
    }
}

impl Stance {
    /// Whether it eats `food` it touches now, with `fullness` of its tank.
    #[inline]
    pub fn eats(&self, food: Food, fullness: f64) -> bool {
        self.goes_for == Some(food)
            || self.graze.is_some_and(|g| {
                fullness <= g.until
                    && match food {
                        Food::Plant => g.plants,
                        Food::Corpse => g.corpses,
                    }
            })
    }
}

/// Food a creature eats on contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Food {
    Plant,
    Corpse,
}

/// `Action::Smooth`: a calm course held this many ticks, a turn of at most this many radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smooth {
    pub ticks: u64,
    pub turn: f64,
}

/// `Action::Divide`: from this share of the tank, giving the child this share of its energy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divide {
    pub tank: f64,
    pub share: f64,
}

/// `Action::Heal`: while the tank holds more than this share and nobody struck it this many ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heal {
    pub tank: f64,
    pub calm: u32,
}

/// `Action::Graze`: which food it eats on the move, while its tank is no fuller than `until`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graze {
    pub plants: bool,
    pub corpses: bool,
    pub until: f64,
}

/// `Action::Shoot`: from this share of its range, keeping this share of its tank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shoot {
    pub from: f64,
    pub keep: f64,
}

/// What the others read of a creature, raw as its blocks keep it: how many times smaller its prey
/// must be (`hunt`, hundredths; 0: it does not hunt), its defence — the enemy's size ratio its
/// fight-back block still fights (`fight`, hundredths; 0: none) and the health share down to which
/// (`fight_health`, %; 0: no health test) — and whether it defends its children. Taken from the
/// blocks whose tests held on its last move, the one that decided and those before it, whether
/// or not their action could be done — a hunt only while it could take prey at all (not full, and
/// fresh meat its food this tick): a hunter whose hunt block stands behind a rest or a flight this
/// tick is not feared, nor a sated one, a parent whose defence block no test lets through covers
/// nobody, and a block behind a condition that never holds is no bluff. Before its first move —
/// from its program's shape (`Menace::of`, `Creature::menace`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Menace {
    hunt: u16,
    fight: u16,
    fight_health: u16,
    pub defends: bool,
}

impl Menace {
    pub const NONE: Menace = Menace { hunt: 0, fight: 0, fight_health: 0, defends: false };

    /// By the shape of a program: its most permissive live hunt, its first live fight-back block,
    /// a live defence (`Program::hunt_ratio`, `defence`, `defends`).
    pub fn of(program: &Program) -> Menace {
        let (hunt, (fight, fight_health)) = program.menace_raw();
        Menace { hunt, fight, fight_health, defends: program.defends() }
    }

    /// How many times smaller its prey must be; None: it does not hunt.
    pub fn hunt(&self) -> Option<f64> {
        (self.hunt > 0).then(|| f64::from(self.hunt) / 100.0)
    }

    /// The enemy's size ratio it still fights back and the health share down to which (0: no
    /// health test); None: it does not fight back.
    pub fn fight(&self) -> Option<(f64, f64)> {
        (self.fight > 0).then(|| (f64::from(self.fight) / 100.0, f64::from(self.fight_health) / 100.0))
    }

    /// A block whose tests held this tick, in program order: the most permissive hunt (when
    /// `can_hunt`), the first fight-back with its health threshold, any defence — as
    /// `Summary::of` reads the shape.
    fn note(&mut self, b: &Block, can_hunt: bool) {
        match b.action {
            Action::Hunt if can_hunt && (self.hunt == 0 || b.args[0] < self.hunt) => self.hunt = b.args[0],
            Action::FightBack if self.fight == 0 => {
                self.fight = b.args[0];
                self.fight_health = b.fight_health();
            }
            Action::DefendChild => self.defends = true,
            _ => {}
        }
    }
}

/// What a creature is doing, as the report counts it. `Alarm`: it fights — stands striking back or
/// covers its child — and a defence strikes a body of any size (`combat::defending`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    Feeding,
    Resting,
    #[default]
    Travelling,
    Alarm,
}

impl Activity {
    pub const ALL: [Self; 4] = [Self::Feeding, Self::Resting, Self::Travelling, Self::Alarm];

    /// Game UI and the chronicle.
    pub fn label(self) -> &'static str {
        match self {
            Self::Feeding => "кормятся",
            Self::Resting => "отдыхают",
            Self::Travelling => "переходят",
            Self::Alarm => "тревога",
        }
    }
}

/// Somebody seen at a place and a tick: the enemy that struck it, the threat it found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighting {
    pub enemy: u64,
    pub x: f64,
    pub y: f64,
    pub tick: u64,
}

/// A parent defending its child (`Action::DefendChild`): whom, against whom, since when, and the
/// pause its block rests from defending after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aid {
    pub victim: u64,
    pub enemy: u64,
    pub started: u64,
    pub pause: u16,
}

/// A creature's memory between ticks, the same for every program: new state goes here (the
/// struct stays `Copy`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mind {
    /// The world's tick as this creature last lived it.
    pub tick: u64,
    pub activity: Activity,
    /// The enemy that struck it last.
    pub hit: Option<Sighting>,
    /// The last threat it met: found by its program, or the one that struck it. A parent covers a
    /// young child by it.
    pub alarm: Option<Sighting>,
    /// When it last shot.
    pub last_shot: u64,
    /// The rest it is in, until what tick, and when the next may start (`Action::Rest`).
    pub rest_until: u64,
    pub rest_ready: u64,
    /// The direction of its last step, and the course a calm walk holds (`steer::adjust`).
    pub heading: Option<(f64, f64)>,
    pub course: Option<(f64, f64)>,
    pub course_target: Option<(f64, f64)>,
    pub course_until: u64,
    /// The plant it goes to: it keeps it while it lives and is seen.
    pub personal_food: Option<(f64, f64)>,
    /// The child it defends, and from when it may defend one again (`Action::DefendChild`).
    pub aid: Option<Aid>,
    pub aid_cooldown: u64,
    /// The target it chose to strike this tick.
    pub attack: Option<u64>,
    /// Its flight block decided this tick (`Creature::fleeing`): it strikes nobody, and a burst
    /// goes the whole step. Not its memory: that is `flee_ticks`.
    pub flight: bool,
    /// Ticks it still runs after losing its threat from sight (`Action::Flee`, `Cond::Fleeing`).
    pub flee_ticks: u32,
    /// The last course of its flight (a unit vector): it runs on it when the threat is out of sight.
    pub flee_dx: f64,
    pub flee_dy: f64,
    /// The wander target. None until the first choice: else a newborn would later walk to its
    /// birthplace.
    pub target: Option<(f64, f64)>,
    /// The layer (`Stance::layer`) its wander target was picked or kept in, by a wander or after
    /// eating: a target out of a changed layer is dropped; one a growing body's narrower band left
    /// past its margin is brought within the band.
    pub target_layer: (f64, f64),
    /// Until what share of their size it knows its children, as its last tick's «щадить детей» set
    /// it (`Stance::spare`). Copied as the tick begins (`World::step`), so the herd's snapshot, the
    /// decisions and combat all read the last tick's, as healing does.
    pub knew_until: f64,
    /// The hunt it is on: does it close in within its patience?
    pub chase: Option<Chase>,
    /// The prey it gave up chasing, and until what tick it does not choose it again.
    pub given_up: Option<(u64, u64)>,
    /// Until what tick each of its modes is on (`Action::Mode`, `Cond::Mode`); both of its
    /// programs read the same modes.
    pub modes: [u64; MODES],
    /// What its program set this tick.
    pub stance: Stance,
    /// The block of its current program that decided this tick (None: none did), the settings
    /// that applied and the deciding blocks whose tests held (the one that decided, and those whose
    /// action could not be done), a bit a block. Only the window reads them.
    pub fired: Option<u8>,
    pub applied: u32,
    pub tried: u32,
    /// The stage (`JUVENILE`, `ADULT`) of the program that decided this tick: `fired`, `applied` and
    /// `tried` are that program's. On the tick it grows up it is the juvenile one, while
    /// `Creature::stage` is already adult. Only the window reads it.
    pub decided_by: u8,
}

/// A hunt under way: the prey, the gap to its edge the hunter last closed to, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chase {
    pub prey: u64,
    pub mark: f64,
    pub since: u64,
}

/// What the strategy knows about itself.
pub struct Me<'a> {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    /// Its number and parent: the senses do not show family as a threat.
    pub kinship: Kinship,
    pub pheno: &'a Phenotype,
    pub health_share: f64,
    /// Health now: a hunter weighs the strikes it expects against it.
    pub health: f64,
    /// Its age, ticks (`Cond::Age`).
    pub age: f64,
    /// Winded after a burst (`Cond::Winded`).
    pub winded: bool,
}

/// The decision of a tick: the point to step towards. The step goes no farther than `pace` of its
/// speed in its direction; a point closer than a step is not overshot, the creature stops on it.
/// So a flight is a point exactly a step away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub tx: f64,
    pub ty: f64,
    /// The share of its speed it goes at (`MIN_PACE` to 1); a slow step is paid as taken.
    pub pace: f64,
    pub attack: Option<u64>,
    /// A burst in a chase or in flight, as far as its muscles go (the `burst` gene).
    pub burst: bool,
    /// It stands torpid (`Action::Torpor`).
    pub torpor: bool,
}

impl Intent {
    /// Towards (tx, ty) at full speed, striking no one.
    #[inline(always)]
    pub fn to(tx: f64, ty: f64) -> Intent {
        Intent { tx, ty, pace: 1.0, attack: None, burst: false, torpor: false }
    }
}

/// Where to go this tick.
#[inline(always)]
pub(crate) fn decide(
    me: &Me,
    program: &Program,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl Senses,
) -> Intent {
    plan(me, program, mind, rng, senses).0
}

/// Where to go and which kind of move it is: the scene, the settings, the first block that
/// decides, the steering.
#[inline(always)]
pub(super) fn plan(
    me: &Me,
    program: &Program,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl Senses,
) -> (Intent, Mode) {
    let mut scene = Scene::perceive(me, mind, senses, program);
    let blocks = program.blocks();
    // The settings first, wherever they stand: a swap cannot hide one behind a deciding block. Of
    // one kind the first that applies wins; the later ones are not even tested.
    let (mut applied, mut kinds) = (0_u32, 0_u64);
    for (i, block) in blocks.iter().enumerate().filter(|(_, b)| b.action.is_setting()) {
        let kind = block.setting_kind();
        if kinds & kind == 0 && scene.holds(block, me, mind, senses) {
            actions::apply_setting(block, &mut scene, me, mind);
            applied |= 1 << i;
            kinds |= kind;
        }
    }
    // a hunt is feared only while it could take prey: `Scene::prey` and the senses take none
    let can_hunt = me.energy < me.pheno.max_energy && me.pheno.hunts_now(scene.stance.foreign);
    let (mut decided, mut tried) = (None, 0_u32);
    for (i, block) in blocks.iter().enumerate().filter(|(_, b)| !b.action.is_setting()) {
        if !scene.holds(block, me, mind, senses) {
            continue;
        }
        tried |= 1 << i;
        // its tests held: the others read of it what this block would do
        scene.stance.menace.note(block, can_hunt);
        if let Some(done) = actions::act(block, &mut scene, me, mind, rng, senses) {
            decided = Some((i, block.action, done));
            break;
        }
    }
    (mind.applied, mind.tried) = (applied, tried);
    if let Some(t) = scene.found_threat() {
        mind.alarm = Some(Sighting { enemy: t.id, x: t.x, y: t.y, tick: mind.tick });
    }
    // Nothing decided: it stands.
    let (action, (intent, mode)) = match decided {
        Some((_, action, done)) => (Some(action), done),
        None => (None, (Intent::to(me.x, me.y), Mode::Wander)),
    };
    // the food its block went for it eats on contact, on the move or not
    scene.stance.goes_for = match action {
        Some(Action::EatPlant) => Some(Food::Plant),
        Some(Action::EatCorpse) => Some(Food::Corpse),
        _ => None,
    };
    scene.stance.moved = true;
    mind.stance = scene.stance;
    mind.fired = decided.map(|(i, ..)| i as u8);
    mind.flight = action == Some(Action::Flee);
    // A flight lasts while it chooses to flee, a rest while it chooses to rest, a defence of its
    // child while it chooses to defend it (then it pauses).
    if action != Some(Action::Flee) {
        mind.flee_ticks = 0;
    }
    if action != Some(Action::Rest) {
        mind.rest_until = 0;
    }
    if action != Some(Action::DefendChild) {
        actions::end_defence(mind);
    }
    // a fight back, by its block, not by standing: a hunter that reached a prey standing still
    // stands on it too, and is no defender (`combat::defending`)
    if mode == Mode::Defend || action == Some(Action::FightBack) {
        mind.activity = Activity::Alarm;
        mind.course = None;
        return (intent, mode);
    }
    let intent = super::steer::adjust(me, mind, intent, mode == Mode::Food, mode == Mode::Flee);
    if mode == Mode::Rest && mind.activity != Activity::Alarm {
        mind.activity = Activity::Resting;
        mind.course = None;
    }
    (intent, mode)
}

/// It has just eaten (`me` with its new energy); its next wander target lies at most `reach` away,
/// in the band of this tick's layer.
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64) {
    let layer = mind.stance.layer;
    actions::after_eating(me, mind, rng, reach, layer);
}

#[cfg(test)]
mod tests;
