//! How a creature decides its step. The step splits in two: the strategy **decides** where to go
//! (`decide`), the creature **acts** (`Creature::act`: a step of its pace, clamped to the world,
//! the upkeep, death). The strategy sees only itself (`Me`), its program, its memory (`Mind`), its
//! generator and its senses — it cannot move, feed or divide the creature. The parallel tick will
//! need it: decisions can be taken at once.
//!
//! What it decides is its behaviour program for its stage of life (`program.rs`): each tick it
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
    /// The layer (`Stance::layer`) its wander target was kept in: a target out of a changed layer is
    /// dropped; one a growing body's narrower band left past its margin is brought within the band.
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
    let band = me.pheno.band(mind.stance.layer);
    actions::after_eating(me, mind, rng, reach, band);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CHASE_GIVE_UP_TICKS, CHASE_PATIENCE};
    use crate::creature::{Creature, Diet, Programs};
    use crate::genome::creature::Gene;
    use crate::senses::{CorpseFood, Hunting, Prey, Taste, Threat};
    use crate::{CreatureGenome, Rules, Space};

    /// A plant 100 to the east and a corpse (radius 10) 100 to the west; like the world's senses,
    /// a corpse or prey only for those that eat meat at the terms asked (own or foreign food).
    struct FoodSense {
        prey: Option<Prey>,
    }

    impl Senses for FoodSense {
        fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
            Some((1100.0, 1000.0))
        }

        fn best_corpse(&self, me: &Me, taste: Taste) -> Option<CorpseFood> {
            use crate::corpse::Stage;
            let eats_corpses = [Stage::Fresh, Stage::Rot]
                .into_iter()
                .any(|stage| me.pheno.corpse_efficiency(stage, taste.foreign) > 0.0);
            eats_corpses.then_some(CorpseFood { owner: 2, x: 900.0, y: 1000.0, half: 10.0, score: 3.0 })
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, hunt: Hunting) -> Option<Prey> {
            self.prey.filter(|_| me.pheno.hunts_now(hunt.taste.foreign))
        }
    }

    #[test]
    fn мясоед_идёт_к_падали_травоядный_к_растению_оба_встают_у_края() {
        use crate::config::EAT_STOP_SHARE;
        let (size, half) = (40.0, 10.0);
        for (diet, target) in [
            (Diet::Carnivore, 900.0 + (size + half) * EAT_STOP_SHARE),
            (Diet::Scavenger, 900.0 + (size + half) * EAT_STOP_SHARE),
            (Diet::Herbivore, 1100.0 - size * EAT_STOP_SHARE),
        ] {
            let v = Creature::new(
                &Space::default(),
                &Rules::default(),
                CreatureGenome::BASE.with(Gene::Diet, diet as usize as f64),
                Some(1000.0),
                Some(1000.0),
                Some(30.0),
                Rng::new(1),
            );
            let me = Me {
                age: 0.0,
                winded: false,
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                pheno: &v.pheno,
                health_share: 1.0,
                health: v.pheno.size,
            };
            let mut mind = Mind::default();
            let mut rng = Rng::new(3);
            let (intent, mode) =
                plan(&me, &Program::STANDARD, &mut mind, &mut rng, &FoodSense { prey: None });
            assert_eq!(mode, Mode::Food, "{diet:?}");
            assert!((intent.tx - target).abs() < 1e-9, "{diet:?}: stops at the edge of reach, {}", intent.tx);

            mind.attack = Some(9);
            let (fight, _) = plan(
                &me,
                &Program::STANDARD,
                &mut mind,
                &mut rng,
                &FoodSense { prey: Some(Prey { id: 9, x: 1020.0, y: 1000.0, half: 10.0, score: 0.1 }) },
            );
            // fed, only the carnivore has fresh meat for its own food
            let hunts = diet == Diet::Carnivore;
            assert_eq!(
                fight.attack.is_some(),
                hunts,
                "{diet:?}: a started hunt goes on only on its own food"
            );
        }
    }

    /// Prey `gap` beyond the hunter's reach, straight east; none once the hunter gave it up.
    struct Ahead {
        gap: f64,
    }

    impl Senses for Ahead {
        fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
            None
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, me: &Me, _: Option<u64>, avoid: Option<u64>, _: Hunting) -> Option<Prey> {
            let x = me.x + me.pheno.half + 10.0 + self.gap;
            (avoid != Some(9)).then_some(Prey { id: 9, x, y: me.y, half: 10.0, score: 1.0 })
        }
    }

    /// A hunter that does not close in on its prey within `CHASE_PATIENCE` ticks gives it up and
    /// does not choose it for `CHASE_GIVE_UP_TICKS`; one that closes a step now and then goes on.
    #[test]
    fn a_hopeless_chase_is_given_up() {
        let v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64),
            Some(1000.0),
            Some(1000.0),
            Some(30.0),
            Rng::new(1),
        );
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.pheno.size,
        };
        let hunt = |mind: &mut Mind, gap: f64| {
            mind.tick += 1;
            let (intent, _) = plan(&me, &Program::STANDARD, mind, &mut Rng::new(3), &Ahead { gap });
            mind.attack = intent.attack;
            intent.attack
        };
        // it keeps its distance: the chase lasts `CHASE_PATIENCE` ticks
        let mut mind = Mind::default();
        for t in 0..CHASE_PATIENCE {
            assert_eq!(hunt(&mut mind, 100.0), Some(9), "tick {t}");
        }
        assert_eq!(hunt(&mut mind, 100.0), None, "given up");
        let until = mind.given_up.expect("remembered").1;
        assert_eq!(until, mind.tick + CHASE_GIVE_UP_TICKS);
        assert_eq!(hunt(&mut mind, 100.0), None, "not chosen again");
        mind.tick = until - 1;
        assert_eq!(hunt(&mut mind, 100.0), Some(9), "chosen again later");

        // it closes a step every few ticks: the chase goes on
        let mut mind = Mind::default();
        let mut gap = 300.0;
        for t in 0..3 * CHASE_PATIENCE {
            if t % 10 == 0 {
                gap -= v.pheno.speed;
            }
            assert_eq!(hunt(&mut mind, gap), Some(9), "tick {t}");
        }
    }

    /// A prey given up between two asks at the same terms is not handed out again: the second ask
    /// looks anew, past it.
    #[test]
    fn prey_given_up_is_not_found_again_within_the_tick() {
        let v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64),
            Some(1000.0),
            Some(1000.0),
            Some(30.0),
            Rng::new(1),
        );
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.pheno.size,
        };
        let senses = Ahead { gap: 100.0 };
        let mut mind = Mind::default();
        let mut scene = Scene::perceive(&me, &mut mind, &senses, &Program::STANDARD);
        let terms = Hunting { ratio: 1.5, caution: 0.0, taste: scene.taste(&me), range: me.pheno.vision };
        assert_eq!(scene.prey(&me, &mind, &senses, terms).map(|p| p.id), Some(9));
        mind.given_up = Some((9, mind.tick + CHASE_GIVE_UP_TICKS));
        assert_eq!(scene.prey(&me, &mind, &senses, terms).map(|p| p.id), None);
    }

    /// Food within the stop distance: it stays and eats, beside the food, not on it.
    #[test]
    fn у_еды_стоит_а_не_залезает_на_неё() {
        let v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE,
            Some(1080.0),
            Some(1000.0),
            Some(30.0),
            Rng::new(1),
        );
        let me = Me {
            age: 0.0,
            winded: false,
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.pheno.size,
        };
        let (intent, mode) =
            plan(&me, &Program::STANDARD, &mut Mind::default(), &mut Rng::new(3), &FoodSense { prey: None });
        assert_eq!(mode, Mode::Food);
        assert_eq!((intent.tx, intent.ty), (1080.0, 1000.0), "20 from the plant: already within reach");
    }

    use crate::creature::program::{Block, Cond, JUVENILE, Test};
    use crate::senses::{Blind, senses_from};

    /// A base creature at (1000, y) with `energy` of its store.
    fn body(genome: CreatureGenome, y: Option<f64>, energy: f64) -> Creature {
        let v =
            Creature::new(&Space::default(), &Rules::default(), genome, Some(1000.0), y, None, Rng::new(1));
        let energy = v.pheno.max_energy * energy;
        Creature { energy, ..v }
    }

    fn run(v: &Creature, program: &Program, mind: &mut Mind, senses: &impl Senses) -> (Intent, Mode) {
        plan(&v.me(), program, mind, &mut Rng::new(3), senses)
    }

    #[test]
    fn an_ambusher_stands_a_diver_goes_to_its_layer_edge_a_torpid_one_sleeps() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let ambush = Program::of(&[Block::does(Action::Ambush)]);
        let mut mind = Mind::default();
        let (intent, _) = run(&v, &ambush, &mut mind, &Blind);
        assert_eq!((intent.tx, intent.ty, intent.torpor), (v.x, v.y, false), "it stands awake");
        assert_eq!(mind.fired, Some(0));
        let torpor = Program::of(&[Block::does(Action::Torpor)]);
        let (intent, _) = run(&v, &torpor, &mut Mind::default(), &Blind);
        assert!(intent.torpor && (intent.tx, intent.ty) == (v.x, v.y), "it stands torpid");

        for (action, down) in [(Action::Surface, false), (Action::Dive, true)] {
            let program = Program::of(&[Block::does(action).with(0, 50), Block::does(Action::Wander)]);
            let mut mind = Mind::default();
            let (intent, _) = run(&v, &program, &mut mind, &Blind);
            assert_eq!(mind.fired, Some(0), "{action:?}");
            assert_eq!(intent.pace, 0.5, "{action:?}: at its block's pace");
            assert!((intent.tx - v.x).abs() < 1e-9 && (intent.ty > v.y) == down, "{action:?}: {intent:?}");
            // at the edge of its band (no layer setting: the whole depth) it is done: the next block
            // decides
            let (lo, hi) = v.pheno.band((0.0, 1.0));
            let edge = if down { hi } else { lo };
            let there = body(CreatureGenome::BASE, Some(edge), 0.5);
            let mut mind = Mind::default();
            let (intent, _) = run(&there, &program, &mut mind, &Blind);
            assert_eq!(mind.fired, Some(1), "{action:?} at the edge");
            assert_eq!(intent.pace, 1.0);
        }
    }

    #[test]
    fn a_test_reads_its_threshold_and_can_be_negated() {
        let hungry = Test::at(Cond::Fullness, 50).not();
        for (negate, share, fired) in [(false, 0.3, 1), (false, 0.7, 0), (true, 0.3, 0), (true, 0.7, 1)] {
            let test = if negate { hungry } else { hungry.not() };
            let program = Program::of(&[Block::when(test, Action::Ambush), Block::does(Action::Wander)]);
            let v = body(CreatureGenome::BASE, Some(1000.0), share);
            let mut mind = Mind::default();
            run(&v, &program, &mut mind, &Blind);
            assert_eq!(mind.fired, Some(fired), "{} at fullness {share}", test.label());
        }
        // depth: the world is 4000 deep, it stands at 1000 = 25%
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        for (param, fired) in [(20, 0), (30, 1)] {
            let program = Program::of(&[
                Block::when(Test::at(Cond::Depth, param), Action::Ambush),
                Block::does(Action::Wander),
            ]);
            let mut mind = Mind::default();
            run(&v, &program, &mut mind, &Blind);
            assert_eq!(mind.fired, Some(fired), "deeper than {param}%");
        }
    }

    /// Nothing to do: no block decides, and it stands.
    #[test]
    fn when_no_block_decides_it_stands() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let program = Program::of(&[Block::does(Action::EatPlant), Block::does(Action::Hunt)]);
        let mut mind = Mind::default();
        let (intent, _) = run(&v, &program, &mut mind, &Blind);
        assert_eq!(mind.fired, None);
        assert_eq!((intent.tx, intent.ty), (v.x, v.y));
    }

    /// Settings apply and the program goes on; what they set stays in its memory for the world's
    /// eating and fighting phases, and the window sees which applied. Of one kind the first that
    /// applies wins.
    #[test]
    fn settings_apply_and_the_program_goes_on() {
        let program = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 30).not(), Action::EatForeign),
            Block::when(Test::at(Cond::Fullness, 10).not(), Action::Rival),
            Block::does(Action::Rival).with(0, 250),
            Block::does(Action::Reach).with(0, 10),
            Block::does(Action::Wander),
        ]);
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.2);
        let mut mind = Mind::default();
        run(&v, &program, &mut mind, &Blind);
        assert_eq!(mind.fired, Some(4), "a setting does not decide");
        assert_eq!(mind.applied, 0b1101, "the first rival's test failed: the second applied");
        assert!(mind.stance.foreign);
        assert_eq!(mind.stance.rival, 2.5);
        assert!((mind.stance.reach - 0.1 * v.pheno.height).abs() < 1e-9);
        let starving = body(CreatureGenome::BASE, Some(1000.0), 0.05);
        let mut mind = Mind::default();
        run(&starving, &program, &mut mind, &Blind);
        assert_eq!(mind.applied, 0b1011, "the first rival that applies wins, the later is not lit");
        assert_eq!(mind.stance.rival, 1.5);
        // a new tick starts from no settings
        run(&v, &Program::of(&[Block::does(Action::Wander)]), &mut mind, &Blind);
        assert!(mind.stance.moved);
        assert_eq!(Stance { moved: false, ..mind.stance }, Stance::default());
    }

    /// The others read a creature by the blocks whose tests held on its last move — the hunt, the
    /// fight-back with its health threshold, the defence — whether or not the action could be done;
    /// a block behind a test that fails this tick is no threat, nor a hunt that could take no prey
    /// (a full tank, fresh meat not its food). Before its first move: by the shape.
    #[test]
    fn others_read_the_blocks_whose_tests_held() {
        let program = Program::of(&[
            Block::when(Test::at(Cond::Health, 50), Action::FightBack),
            Block::when(Test::at(Cond::Fullness, 30).not(), Action::Hunt),
            Block::when(Test::at(Cond::Fullness, 30).not(), Action::DefendChild),
            Block::does(Action::Wander),
        ]);
        let shape = Menace { hunt: 150, fight: 150, fight_health: 50, defends: true };
        assert_eq!(Menace::of(&program), shape);
        assert_eq!((shape.hunt(), shape.fight()), (Some(1.5), Some((1.5, 0.5))));
        let carnivore = CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64);
        let mut fed = body(carnivore, Some(1000.0), 0.5);
        fed.programs = Programs::both(program);
        assert_eq!(fed.menace(), shape, "before its first move: by the shape");
        let mut mind = Mind::default();
        run(&fed, &program, &mut mind, &Blind);
        let fight_only = Menace { hunt: 0, defends: false, ..shape };
        assert_eq!(mind.stance.menace, fight_only, "fed: its hunt and defence blocks were not reached");
        fed.mind = mind;
        assert_eq!(fed.menace(), fight_only, "after a move: by the move");
        let hungry = body(carnivore, Some(1000.0), 0.2);
        let mut mind = Mind::default();
        run(&hungry, &program, &mut mind, &Blind);
        assert_eq!(mind.stance.menace, shape, "hungry: they were tried, though nothing was in sight");
        assert_eq!(mind.fired, Some(3));
        // a hunt that could take nothing is no threat: a grazer's, a full hunter's
        let grazer = body(CreatureGenome::BASE, Some(1000.0), 0.2);
        let mut mind = Mind::default();
        run(&grazer, &program, &mut mind, &Blind);
        assert_eq!(mind.stance.menace, Menace { hunt: 0, ..shape }, "fresh meat is not a grazer's food");
        let hunter = Program::of(&[Block::does(Action::Hunt), Block::does(Action::Wander)]);
        let full = body(carnivore, Some(1000.0), 1.0);
        let mut mind = Mind::default();
        run(&full, &hunter, &mut mind, &Blind);
        assert_eq!(mind.stance.menace.hunt(), None, "a full tank takes no prey");
        let mut mind = Mind::default();
        run(&hungry, &hunter, &mut mind, &Blind);
        assert_eq!(mind.stance.menace.hunt(), Some(1.5));
    }

    /// A flight is the flight block's decision, not its memory: with «бежать ещё 0» it still flees
    /// (no strikes, a burst) while the threat is in sight.
    #[test]
    fn a_flight_without_a_memory_is_still_a_flight() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let threat = Threat { id: 7, x: 1030.0, y: 1000.0, gap: 5.0, half: 40.0 };
        let senses = senses_from(|_, _, _| None).with_threat(threat);
        let flight = Program::of(&[
            Block::when(Test::at(Cond::ThreatNear, 50), Action::Flee).with(0, 0),
            Block::does(Action::Wander),
        ]);
        let mut mind = Mind::default();
        let (_, mode) = run(&v, &flight, &mut mind, &senses);
        assert_eq!((mode, mind.flight, mind.flee_ticks), (Mode::Flee, true, 0));
        run(&v, &Program::of(&[Block::does(Action::Wander)]), &mut mind, &senses);
        assert!(!mind.flight, "another decision ends it");
    }

    /// A mode setting switches its mode on for its ticks, a later block sees it this very tick, it
    /// expires, and a time of 0 switches it off; both tracks share the modes.
    #[test]
    fn modes_are_switched_on_seen_and_expire() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let program = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 40), Action::Mode).with(0, 2).with(1, 5),
            Block::when(Test::at(Cond::Mode, 2), Action::Ambush),
            Block::does(Action::Wander),
        ]);
        let mut mind = Mind { tick: 100, ..Mind::default() };
        run(&v, &program, &mut mind, &Blind);
        assert_eq!((mind.fired, mind.modes), (Some(1), [0, 105, 0, 0]), "on this tick, seen at once");
        let hungry = body(CreatureGenome::BASE, Some(1000.0), 0.2);
        for (tick, fired) in [(104, 1), (105, 2)] {
            mind.tick = tick;
            run(&hungry, &program, &mut mind, &Blind);
            assert_eq!(mind.fired, Some(fired), "tick {tick}: the mode lasts 5 ticks");
        }
        let off =
            Program::of(&[Block::does(Action::Mode).with(0, 2).with(1, 0), Block::does(Action::Wander)]);
        mind.modes[1] = 500;
        run(&v, &off, &mut mind, &Blind);
        assert_eq!(mind.modes[1], mind.tick, "0 ticks: off");
        // two modes of different numbers apply in one tick
        let both = Program::of(&[
            Block::does(Action::Mode).with(0, 1),
            Block::does(Action::Mode).with(0, 3).with(1, 7),
            Block::does(Action::Mode).with(0, 1).with(1, 500),
            Block::does(Action::Wander),
        ]);
        let mut mind = Mind { tick: 10, ..Mind::default() };
        run(&v, &both, &mut mind, &Blind);
        assert_eq!((mind.modes, mind.applied), ([70, 0, 17, 0], 0b011), "the second mode 1 is skipped");
    }

    /// The body, age and place tests read what they say.
    #[test]
    fn body_age_and_place_tests_read_themselves() {
        let fires = |v: &Creature, test: Test| {
            let program = Program::of(&[Block::when(test, Action::Ambush), Block::does(Action::Wander)]);
            let mut mind = Mind::default();
            run(v, &program, &mut mind, &Blind);
            mind.fired == Some(0)
        };
        let mut v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        v.age = v.pheno.lifespan * 0.75;
        assert!(fires(&v, Test::at(Cond::Age, 70)) && !fires(&v, Test::at(Cond::Age, 80)));
        assert!(!fires(&v, Test::is(Cond::Winded)));
        v.winded = 3;
        assert!(fires(&v, Test::is(Cond::Winded)));
        // the thermocline: warm to 15% of the depth, cold from 45%
        let deep = body(CreatureGenome::BASE, Some(v.pheno.height * 0.6), 0.5);
        let shallow = body(CreatureGenome::BASE, Some(v.pheno.height * 0.05), 0.5);
        assert!(fires(&deep, Test::at(Cond::Cold, 100)) && !fires(&shallow, Test::at(Cond::Cold, 1)));
        // a layer setting from 5% to 50% of the depth: the tests after it see its band
        let layered = |v: &Creature, test: Test| {
            let program = Program::of(&[
                Block::does(Action::Layer).with(1, 50),
                Block::when(test, Action::Ambush),
                Block::does(Action::Wander),
            ]);
            let mut mind = Mind::default();
            run(v, &program, &mut mind, &Blind);
            mind.fired == Some(1)
        };
        let (lo, hi) = v.pheno.band((0.05, 0.5));
        for (y, above, inside, below) in
            [(lo - 5.0, true, false, false), (lo + 5.0, false, true, false), (hi + 5.0, false, false, true)]
        {
            let w = body(CreatureGenome::BASE, Some(y), 0.5);
            assert_eq!(layered(&w, Test::is(Cond::AboveLayer)), above, "y {y}");
            assert_eq!(layered(&w, Test::is(Cond::InLayer)), inside, "y {y}");
            assert_eq!(layered(&w, Test::is(Cond::BelowLayer)), below, "y {y}");
        }
        let low = body(CreatureGenome::BASE, Some(hi + 5.0), 0.5);
        assert!(layered(&low, Test::is(Cond::InLayer).not()), "a negated test");
        assert!(fires(&low, Test::is(Cond::InLayer)), "without the setting the whole depth is its layer");
    }

    /// Three tests: the block fires only when all hold.
    #[test]
    fn a_block_fires_only_when_all_three_tests_hold() {
        let program = |third: Test| {
            Program::of(&[
                Block::when3(Test::at(Cond::Fullness, 30), Test::at(Cond::Health, 50), third, Action::Ambush),
                Block::does(Action::Wander),
            ])
        };
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        for (third, fired) in
            [(Test::ALWAYS, 0), (Test::at(Cond::Depth, 90), 1), (Test::is(Cond::Winded).not(), 0)]
        {
            let mut mind = Mind::default();
            run(&v, &program(third), &mut mind, &Blind);
            assert_eq!(mind.fired, Some(fired), "{}", third.label());
        }
    }

    /// Food and prey in sight: a plant, a corpse, and prey its hunt would take within the test's
    /// share of sight; a program without a hunt sees no prey.
    #[test]
    fn food_and_prey_tests_see_what_the_senses_show() {
        struct Around {
            prey_gap: f64,
        }
        impl Senses for Around {
            fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
                (100.0 * 100.0 < r2).then_some((x + 100.0, y))
            }
            fn best_corpse(&self, me: &Me, taste: Taste) -> Option<CorpseFood> {
                use crate::corpse::Stage;
                let eats = [Stage::Fresh, Stage::Rot]
                    .into_iter()
                    .any(|stage| me.pheno.corpse_efficiency(stage, taste.foreign) > 0.0);
                eats.then_some(CorpseFood { owner: 2, x: me.x - 120.0, y: me.y, half: 10.0, score: 1.0 })
            }
            fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
                None
            }
            fn nearest_prey(&self, me: &Me, ratio: f64, taste: Taste, within: f64) -> Option<Threat> {
                let t = Threat { id: 4, x: me.x + 50.0, y: me.y, gap: self.prey_gap, half: 5.0 };
                (me.pheno.hunts_now(taste.foreign) && ratio == 1.5 && t.gap < within).then_some(t)
            }
        }
        let fires = |v: &Creature, test: Test, hunts: bool, gap: f64| {
            let tail = if hunts { Action::Hunt } else { Action::Ambush };
            let program = Program::of(&[
                Block::when(test, Action::Ambush),
                Block::does(tail),
                Block::does(Action::Wander),
            ]);
            let mut mind = Mind::default();
            run(v, &program, &mut mind, &Around { prey_gap: gap });
            mind.fired == Some(0)
        };
        let grazer = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let carnivore =
            body(CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64), Some(1000.0), 0.5);
        assert!(fires(&grazer, Test::is(Cond::PlantSeen), false, 0.0));
        assert!(!fires(&grazer, Test::is(Cond::CorpseSeen), false, 0.0));
        assert!(fires(&carnivore, Test::is(Cond::CorpseSeen), false, 0.0));
        let near = carnivore.pheno.vision * 0.2;
        assert!(fires(&carnivore, Test::at(Cond::PreySeen, 30), true, near));
        assert!(!fires(&carnivore, Test::at(Cond::PreySeen, 10), true, near), "farther than the test looks");
        assert!(!fires(&carnivore, Test::at(Cond::PreySeen, 30), false, near), "no hunt block, no prey");
        assert!(!fires(&grazer, Test::at(Cond::PreySeen, 30), true, near), "a grazer takes no prey");
    }

    /// A flight at its block's pace, tilted down or up; straight by default.
    #[test]
    fn a_flight_keeps_its_pace_and_tilt() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let threat = Threat { id: 7, x: 1030.0, y: 1000.0, gap: 5.0, half: 40.0 };
        let senses = senses_from(|_, _, _| None).with_threat(threat);
        let flee = |tilt: u16, pace: u16| {
            let program = Program::of(&[
                Block::when(Test::at(Cond::ThreatNear, 50), Action::Flee).with(3, pace).with(4, tilt),
                Block::does(Action::Wander),
            ]);
            let mut mind = Mind::default();
            run(&v, &program, &mut mind, &senses).0
        };
        let straight = flee(100, 100);
        assert_eq!((straight.ty, straight.pace), (1000.0, 1.0), "straight west at full speed");
        let down = flee(150, 40);
        assert!(down.ty > 1000.0 && down.tx < 1000.0 && down.pace == 0.4, "{down:?}");
        let (dx, dy) = (down.tx - 1000.0, down.ty - 1000.0);
        assert!((dy / -dx - 0.5).abs() < 1e-9, "down by half a unit a unit away");
        assert!(flee(50, 100).ty < 1000.0, "up");
    }

    /// Its child in need: `child`, struck by `enemy`; it records how near it asked.
    struct Needs {
        child: Option<(u64, Threat)>,
        asked: std::cell::Cell<f64>,
    }

    impl Senses for Needs {
        fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
            None
        }
        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }
        fn child_in_need(
            &self,
            _: &Me,
            within: f64,
            _: u64,
            window: u64,
            _: Option<u64>,
        ) -> Option<(u64, Threat)> {
            assert_eq!(window, 30, "the block's window");
            self.asked.set(within);
            self.child
        }
    }

    /// «Защищать детёныша»: it goes for its child's enemy and strikes it whatever its size, only
    /// with a tank above the block's share, for at most the block's ticks, then rests from it; an
    /// episode another block interrupts ends with the pause too.
    #[test]
    fn a_parent_defends_its_child_within_its_blocks_limits() {
        let enemy = Threat { id: 9, x: 1200.0, y: 1000.0, gap: 150.0, half: 50.0 };
        let needs = Needs { child: Some((5, enemy)), asked: std::cell::Cell::new(0.0) };
        let program = Program::of(&[Block::does(Action::DefendChild), Block::does(Action::Wander)]);
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.8);
        let mut mind = Mind { tick: 100, ..Mind::default() };
        let (intent, mode) = run(&v, &program, &mut mind, &needs);
        assert_eq!((mind.fired, mode, intent.attack), (Some(0), Mode::Defend, Some(9)));
        assert_eq!((intent.tx, intent.ty, mind.stance.defending), (1200.0, 1000.0, Some(9)));
        assert_eq!(mind.activity, Activity::Alarm);
        assert!((needs.asked.get() - 0.5 * v.pheno.vision).abs() < 1e-9, "a child within half its sight");
        assert_eq!(mind.aid.map(|a| (a.victim, a.enemy, a.started)), Some((5, 9, 100)));
        // the episode goes on for 90 ticks, then ends with a pause of 60
        mind.tick = 189;
        run(&v, &program, &mut mind, &needs);
        assert_eq!((mind.fired, mind.aid.map(|a| a.started)), (Some(0), Some(100)));
        mind.tick = 190;
        run(&v, &program, &mut mind, &needs);
        assert_eq!((mind.fired, mind.aid, mind.aid_cooldown), (Some(1), None, 250));
        mind.tick = 249;
        run(&v, &program, &mut mind, &needs);
        assert_eq!(mind.fired, Some(1), "resting from defending");
        mind.tick = 250;
        run(&v, &program, &mut mind, &needs);
        assert_eq!(mind.fired, Some(0), "a new episode");
        // a poor tank does not defend
        let poor = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let mut mind = Mind::default();
        run(&poor, &program, &mut mind, &needs);
        assert_eq!(mind.fired, Some(1));
        // another block decides first: the episode ends with its pause
        let hungry = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 90).not(), Action::Ambush),
            Block::does(Action::DefendChild),
        ]);
        let mut mind = Mind { tick: 10, ..Mind::default() };
        run(&body(CreatureGenome::BASE, Some(1000.0), 0.95), &hungry, &mut mind, &needs);
        assert!(mind.aid.is_some());
        run(&v, &hungry, &mut mind, &needs);
        assert_eq!((mind.fired, mind.aid, mind.aid_cooldown), (Some(0), None, 70));
        // no child in need: nothing to do
        let calm = Needs { child: None, asked: std::cell::Cell::new(0.0) };
        let mut mind = Mind::default();
        run(&v, &program, &mut mind, &calm);
        assert_eq!(mind.fired, Some(1));
    }

    /// A hunt looks for prey no farther than its share of sight and chases at its pace.
    #[test]
    fn a_hunt_looks_as_far_and_chases_as_fast_as_its_block_says() {
        struct Asks(std::cell::Cell<f64>);
        impl Senses for Asks {
            fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
                None
            }
            fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
                None
            }
            fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, hunt: Hunting) -> Option<Prey> {
                self.0.set(hunt.range);
                Some(Prey { id: 9, x: me.x + 200.0, y: me.y, half: 5.0, score: 1.0 })
            }
        }
        let v =
            body(CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64), Some(1000.0), 0.5);
        let program =
            Program::of(&[Block::does(Action::Hunt).with(6, 40).with(7, 60), Block::does(Action::Wander)]);
        let asks = Asks(std::cell::Cell::new(0.0));
        let mut mind = Mind::default();
        let (intent, _) = run(&v, &program, &mut mind, &asks);
        assert!((asks.0.get() - 0.4 * v.pheno.vision).abs() < 1e-9);
        assert_eq!((intent.attack, intent.pace), (Some(9), 0.6));
    }

    /// A hunter that reached a prey standing still stands on its centre: it feeds, it does not
    /// fight back (a defence strikes any size, a hunt keeps to its ratio); one that fights back
    /// standing is in alarm.
    #[test]
    fn a_hunter_on_its_prey_feeds_and_one_fighting_back_is_in_alarm() {
        struct Under;
        impl Senses for Under {
            fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
                None
            }
            fn nearest_threat(&self, me: &Me, _: f64) -> Option<Threat> {
                Some(Threat { id: 9, x: me.x, y: me.y, gap: 0.0, half: 5.0 })
            }
            fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, _: Hunting) -> Option<Prey> {
                Some(Prey { id: 9, x: me.x, y: me.y, half: 5.0, score: 1.0 })
            }
        }
        let v =
            body(CreatureGenome::BASE.with(Gene::Diet, Diet::Carnivore as usize as f64), Some(1000.0), 0.5);
        for (action, activity) in [(Action::Hunt, Activity::Feeding), (Action::FightBack, Activity::Alarm)] {
            let program = Program::of(&[Block::does(action), Block::does(Action::Wander)]);
            let mut mind = Mind::default();
            let (intent, _) = run(&v, &program, &mut mind, &Under);
            assert_eq!((intent.attack, intent.tx, intent.ty), (Some(9), v.x, v.y), "{action:?}");
            assert_eq!(mind.activity, activity, "{action:?}");
        }
    }

    /// The plant choice: the usual keeps the one it goes to and else takes the nearest; without
    /// keeping it takes the nearest at once; «most profitable» asks the senses for the best.
    #[test]
    fn a_plant_block_chooses_as_its_flags_say() {
        struct Two;
        impl Senses for Two {
            fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
                // two plants: (1100, 1000) and (1200, 1000); the query at a plant finds it
                [(1100.0, 1000.0), (1200.0, 1000.0)]
                    .into_iter()
                    .filter(|&(px, py)| (px - x).powi(2) + (py - y).powi(2) < r2)
                    .min_by(|a, b| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
            }
            fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
                None
            }
            fn best_plant(&self, _: &Me, _: Taste) -> Option<(f64, f64)> {
                Some((1200.0, 1000.0))
            }
        }
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let goes = |keep: u16, best: u16, kept: Option<(f64, f64)>| {
            let program = Program::of(&[Block::does(Action::EatPlant).with(1, keep).with(2, best)]);
            let mut mind = Mind { personal_food: kept, ..Mind::default() };
            let (intent, _) = run(&v, &program, &mut mind, &Two);
            (intent.tx > 1150.0, mind.personal_food)
        };
        let far = Some((1200.0, 1000.0));
        assert_eq!(goes(1, 0, far), (true, far), "keeps the one it goes to");
        assert!(!goes(1, 0, None).0, "else the nearest");
        assert_eq!(goes(0, 0, far), (false, Some((1100.0, 1000.0))), "without keeping: the nearest");
        assert_eq!(goes(0, 1, None), (true, far), "the most profitable");
        assert!(!goes(1, 1, Some((1100.0, 1000.0))).0, "keeping comes first");
    }

    /// A setting applies wherever it stands, behind the deciding block too; the window learns which
    /// deciding blocks were tried and could not act.
    #[test]
    fn settings_apply_wherever_they_stand() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let program = Program::of(&[
            Block::does(Action::Hunt),
            Block::when(Test::at(Cond::Fullness, 90), Action::Rest),
            Block::does(Action::Wander),
            Block::does(Action::EatForeign),
            Block::does(Action::Reach).with(0, 20),
        ]);
        let mut mind = Mind::default();
        run(&v, &program, &mut mind, &Blind);
        assert_eq!(mind.fired, Some(2));
        assert_eq!(mind.applied, 0b11000, "the settings after the deciding block applied");
        assert_eq!(mind.tried, 0b101, "the hunt found nothing, the rest's test failed");
        assert!(mind.stance.foreign && (mind.stance.reach - 0.2 * v.pheno.height).abs() < 1e-9);
    }

    /// Under way, only a threat nearer than the flight's «again» share of sight renews it; a farther
    /// one steers it while its memory runs down, then it stops — the old flight distance. (The
    /// templates flee under their alarm mode; here the flight is kept by its own memory.)
    #[test]
    fn a_far_threat_steers_a_flight_but_does_not_renew_it() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let vision = v.pheno.vision;
        let program =
            Program::of(&[Block::when(Test::is(Cond::Fleeing), Action::Flee), Block::does(Action::Wander)]);
        let far = Threat { id: 7, x: 1000.0 + vision * 0.6, y: 1000.0, gap: vision * 0.5, half: 40.0 };
        let senses = senses_from(|_, _, _| None).with_threat(far);
        let mut mind = Mind { flee_ticks: 3, ..Mind::default() };
        for left in [2, 1, 0] {
            let (intent, mode) = run(&v, &program, &mut mind, &senses);
            assert_eq!((mode, mind.flee_ticks), (Mode::Flee, left));
            assert!(intent.tx < v.x, "away from the far threat: {intent:?}");
        }
        let (_, mode) = run(&v, &program, &mut mind, &senses);
        assert_ne!(mode, Mode::Flee, "its memory ran out: a threat that far no longer scares it");
        let near = Threat { gap: vision * 0.2, ..far };
        let mut mind = Mind { flee_ticks: 3, ..Mind::default() };
        run(&v, &program, &mut mind, &senses_from(|_, _, _| None).with_threat(near));
        assert_eq!(mind.flee_ticks, crate::config::FLEE_TICKS, "a near one renews it");
    }

    /// «Struck» looks back as many ticks as its threshold, at an enemy still in sight.
    #[test]
    fn struck_looks_back_its_window() {
        struct Sees;
        impl Senses for Sees {
            fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
                None
            }
            fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
                None
            }
            fn visible_enemy(&self, _: &Me, id: u64) -> Option<Threat> {
                Some(Threat { id, x: 1300.0, y: 1000.0, gap: 280.0, half: 20.0 })
            }
        }
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        for (window, ago, fired) in [(1, 1, 0), (1, 4, 1), (5, 4, 0), (5, 6, 1)] {
            let program = Program::of(&[
                Block::when(Test::at(Cond::Struck, window), Action::Ambush),
                Block::does(Action::Wander),
            ]);
            let mut mind = Mind {
                tick: 100,
                hit: Some(Sighting { enemy: 9, x: 1300.0, y: 1000.0, tick: 100 - ago }),
                ..Default::default()
            };
            run(&v, &program, &mut mind, &Sees);
            assert_eq!(mind.fired, Some(fired), "window {window}, struck {ago} ticks ago");
        }
    }

    /// A threat close by: the standard program flees; a program without a flight block goes on
    /// with its business and keeps no flight in its memory.
    #[test]
    fn only_a_program_with_a_flight_block_flees() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let threat = Threat { id: 7, x: 1030.0, y: 1000.0, gap: 5.0, half: 40.0 };
        let senses = senses_from(|_, _, _| None).with_threat(threat);
        let mut mind = Mind::default();
        let (intent, mode) = run(&v, &Program::STANDARD, &mut mind, &senses);
        assert_eq!(mode, Mode::Flee);
        assert!(intent.tx < v.x && intent.burst, "away from the threat, with a burst: {intent:?}");
        assert_eq!(mind.flee_ticks, crate::config::FLEE_TICKS);
        let reckless = Program::of(&[Block::does(Action::Wander)]);
        let mut mind = Mind { flee_ticks: 5, ..Mind::default() };
        let (_, mode) = run(&v, &reckless, &mut mind, &senses);
        assert_eq!((mode, mind.flee_ticks), (Mode::Wander, 0));
        // it still sees the threat: a program may react to it in its own way
        let hide = Program::of(&[
            Block::when(Test::at(Cond::ThreatNear, 10), Action::Dive),
            Block::does(Action::Wander),
        ]);
        let mut mind = Mind::default();
        let (intent, _) = run(&v, &hide, &mut mind, &senses);
        assert_eq!(mind.fired, Some(0));
        assert!(intent.ty > v.y, "down, away from the danger: {intent:?}");
        // a threat farther than the test's share of sight does not count
        let far = Threat { gap: v.pheno.vision * 0.2, ..threat };
        let mut mind = Mind::default();
        run(&v, &hide, &mut mind, &senses_from(|_, _, _| None).with_threat(far));
        assert_eq!(mind.fired, Some(1));
    }

    /// A weak creature does not fight back where its program asks for health; a program without
    /// that test fights back at any health, but not an enemy past its block's ratio.
    #[test]
    fn fighting_back_follows_the_block() {
        let mut v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        v.health = v.max_health() * 0.3;
        let close = |half: f64| {
            senses_from(|_, _, _| None).with_threat(Threat {
                id: 5,
                x: 1000.0 + 20.0 + half - 1.0,
                y: 1000.0,
                gap: 5.0,
                half,
            })
        };
        let mut mind = Mind::default();
        let (intent, _) = run(&v, &Program::STANDARD, &mut mind, &close(20.0));
        assert_eq!(intent.attack, None, "below the template's 50% health it runs");
        let fierce = Program::of(&[Block::does(Action::FightBack).with(0, 300), Block::does(Action::Wander)]);
        for (half, fights) in [(40.0, true), (60.0, false)] {
            let mut mind = Mind::default();
            let (intent, _) = run(&v, &fierce, &mut mind, &close(half));
            assert_eq!(intent.attack == Some(5), fights, "an enemy of radius {half} against 20 × 3");
        }
    }

    /// An ambusher that sees no food stands, and its energy only goes down: no block makes energy.
    /// A torpid one loses less a tick, but loses.
    #[test]
    fn standing_still_or_torpid_only_loses_energy() {
        let mut costs = Vec::new();
        for action in [Action::Ambush, Action::Torpor] {
            let mut v = body(CreatureGenome::BASE, Some(1000.0), 0.9);
            v.programs = Programs::both(Program::of(&[Block::does(action)]));
            let (x, y) = (v.x, v.y);
            let mut energy = v.energy;
            for t in 0..300 {
                v.step(&Blind);
                assert!(v.energy < energy, "{action:?}, tick {t}: {} after {energy}", v.energy);
                assert_eq!((v.x, v.y), (x, y), "{action:?}, tick {t}: it stands");
                assert_eq!(v.torpid, action == Action::Torpor);
                energy = v.energy;
            }
            costs.push(v.pheno.max_energy * 0.9 - v.energy);
        }
        assert!(costs[1] < costs[0] * 0.5, "torpor saves: {costs:?}");
    }

    /// A growing creature lives by its juvenile program, a grown one by its adult program.
    #[test]
    fn a_creature_lives_by_the_program_of_its_stage() {
        let mut v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        v.programs =
            [Program::of(&[Block::does(Action::Ambush)]), Program::of(&[Block::does(Action::Wander)])].into();
        assert_eq!(v.stage(), ADULT_STAGE);
        let x = v.x;
        v.step(&Blind);
        assert_ne!((v.x, v.y), (x, 1000.0), "grown: it wanders");
        let mut young = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        young.pheno = Phenotype::at_size(&young.genome, &Rules::default(), &Space::default(), 20.0);
        young.programs = v.programs.clone();
        assert_eq!(young.stage(), JUVENILE);
        let (x, y) = (young.x, young.y);
        young.step(&Blind);
        assert_eq!((young.x, young.y), (x, y), "growing: it stands, as its juvenile program says");
    }

    const ADULT_STAGE: usize = crate::creature::program::ADULT;

    /// Plants, a corpse, prey and a threat around, every tick.
    struct Everything {
        threat: bool,
    }

    impl Senses for Everything {
        fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
            let p = (x + 150.0, y - 40.0);
            ((p.0 - x).powi(2) + (p.1 - y).powi(2) < r2).then_some(p)
        }

        fn best_corpse(&self, me: &Me, _: Taste) -> Option<CorpseFood> {
            let eats = me.pheno.meat_efficiency > 0.0 || me.pheno.rot_efficiency > 0.0;
            eats.then_some(CorpseFood { owner: 2, x: me.x - 120.0, y: me.y + 30.0, half: 10.0, score: 1.5 })
        }

        fn nearest_threat(&self, me: &Me, within: f64) -> Option<Threat> {
            let t = Threat { id: 3, x: me.x + 60.0, y: me.y, gap: 60.0 - me.pheno.half - 20.0, half: 20.0 };
            (self.threat && t.gap < within).then_some(t)
        }

        fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, hunt: Hunting) -> Option<Prey> {
            me.pheno.hunts_now(hunt.taste.foreign).then_some(Prey {
                id: 4,
                x: me.x,
                y: me.y + 80.0,
                half: 8.0,
                score: 2.0,
            })
        }
    }

    /// Thousands of random programs on creatures of every diet and stage, among food, prey and a
    /// threat: none panics, every step stays finite and inside the world, the block that decided
    /// exists and is no setting.
    #[test]
    fn random_programs_never_break_a_creature() {
        let mut rng = Rng::new(2024);
        for case in 0..2000 {
            let mut programs = [Program::STANDARD, Program::LURKER];
            for p in &mut programs {
                for _ in 0..rng.randint(1, 40) {
                    p.drift(rng.uniform(0.0, 3.0), &mut rng);
                    p.mutate(1.0, &mut rng);
                }
            }
            let diet = rng.randint(0, 3) as f64;
            let size = if case % 3 == 0 { 80.0 } else { 40.0 };
            let genome = CreatureGenome::BASE.with(Gene::Diet, diet).with(Gene::Size, size);
            let mut v = body(genome, Some(rng.uniform(300.0, 3700.0)), rng.uniform(0.05, 1.0));
            v.programs = programs.into();
            let senses = Everything { threat: case % 2 == 0 };
            for _ in 0..20 {
                v.step(&senses);
                if !v.alive {
                    break;
                }
                assert!(v.x.is_finite() && v.y.is_finite() && v.energy.is_finite(), "case {case}");
                assert!((v.pheno.x_lo..=v.pheno.x_hi).contains(&v.x), "case {case}: x {}", v.x);
                assert!((v.pheno.y_lo..=v.pheno.y_hi).contains(&v.y), "case {case}: y {}", v.y);
                let blocks = v.program().blocks();
                assert!(v.mind.fired.is_none_or(|i| !blocks[usize::from(i)].action.is_setting()));
            }
        }
    }

    #[test]
    fn варианты_совпадают_со_стратегиями() {
        assert_eq!(VARIANTS.len(), Strategy::ALL.len());
        for (i, s) in Strategy::ALL.iter().enumerate() {
            assert_eq!(*s as usize, i);
            assert_eq!(Strategy::from_gene(i as f64), *s);
        }
    }
}
