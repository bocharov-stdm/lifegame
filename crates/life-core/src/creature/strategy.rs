//! How a creature decides its step. The step splits in two: the strategy **decides** where to go
//! (`decide`), the creature **acts** (`Creature::act`: a step of its pace, clamped to the world,
//! the upkeep, death). The strategy sees only itself (`Me`), its program, its memory (`Mind`), its
//! generator and its senses — it cannot move, feed or divide the creature. The parallel tick will
//! need it: decisions can be taken at once.
//!
//! What it decides is its behaviour program for its stage of life (`program.rs`): each tick it
//! perceives the scene (`scene.rs`); the settings whose tests hold set how this tick goes,
//! wherever they stand; then its deciding blocks run in order and the first whose action can be
//! done decides (`actions.rs`); the social layer adjusts the step (`social::adjust`). What the
//! settings set (`Stance`) stays in its memory for the world's eating and fighting phases. The
//! `strategy` gene names the template a founder's programs start from: after that they are
//! inherited, drift and mutate on their own.

use super::actions::{self, Mode};
use super::program::{Action, Program};
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

/// What its program set for this tick (`Action::is_setting`), and the size limit of the target it
/// chose: the world's eating and fighting phases read it after the moves.
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
}

impl Default for Stance {
    fn default() -> Self {
        Stance { foreign: false, rival: 0.0, reach: f64::INFINITY, strike_ratio: 0.0 }
    }
}

/// A creature's memory between ticks, the same for every program: new state goes here (the
/// struct stays `Copy`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mind {
    pub social: crate::social::Memory,
    /// The target it chose to strike this tick.
    pub attack: Option<u64>,
    /// Ticks it still runs after losing its threat from sight (`Action::Flee`).
    pub flee_ticks: u32,
    /// The last course of its flight (a unit vector): it runs on it when the threat is out of sight.
    pub flee_dx: f64,
    pub flee_dy: f64,
    /// The wander target. None until the first choice: else a newborn would later walk to its
    /// birthplace.
    pub target: Option<(f64, f64)>,
    /// The hunt it is on: does it close in within its patience?
    pub chase: Option<Chase>,
    /// The prey it gave up chasing, and until what tick it does not choose it again.
    pub given_up: Option<(u64, u64)>,
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
    pub flock: u64,
    /// The flock's circle: food is taken inside it.
    pub circle: Option<crate::flock::Circle>,
    pub pheno: &'a Phenotype,
    pub health_share: f64,
    /// Health now: a hunter weighs the strikes it expects against it.
    pub health: f64,
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
/// decides, the social layer.
#[inline(always)]
pub(super) fn plan(
    me: &Me,
    program: &Program,
    mind: &mut Mind,
    rng: &mut Rng,
    senses: &impl Senses,
) -> (Intent, Mode) {
    let mut scene = Scene::perceive(me, mind, senses, program.threat_range());
    let blocks = program.blocks();
    // The settings first, wherever they stand: a swap cannot hide one behind a deciding block.
    let mut applied = 0_u32;
    for (i, block) in blocks.iter().enumerate().filter(|(_, b)| b.action.is_setting()) {
        if scene.test(block.when[0], me, mind, senses) && scene.test(block.when[1], me, mind, senses) {
            actions::apply_setting(block, &mut scene, me);
            applied |= 1 << i;
        }
    }
    let (mut decided, mut tried) = (None, 0_u32);
    for (i, block) in blocks.iter().enumerate().filter(|(_, b)| !b.action.is_setting()) {
        if !(scene.test(block.when[0], me, mind, senses) && scene.test(block.when[1], me, mind, senses)) {
            continue;
        }
        tried |= 1 << i;
        if let Some(done) = actions::act(block, &mut scene, me, mind, rng, senses) {
            decided = Some((i, block.action, done));
            break;
        }
    }
    mind.stance = scene.stance;
    (mind.applied, mind.tried) = (applied, tried);
    if let Some(t) = scene.found_threat() {
        mind.social.observed_alarm =
            Some(crate::social::Alarm { enemy: t.id, x: t.x, y: t.y, tick: mind.social.tick });
    }
    // Nothing decided: it stands.
    let (action, (intent, mode)) = match decided {
        Some((_, action, done)) => (Some(action), done),
        None => (None, (Intent::to(me.x, me.y), Mode::Wander)),
    };
    mind.fired = decided.map(|(i, ..)| i as u8);
    // A flight lasts while it chooses to flee, a rest while it chooses to rest.
    if action != Some(Action::Flee) {
        mind.flee_ticks = 0;
    }
    if action != Some(Action::Rest) {
        mind.social.rest_until = 0;
    }
    if intent.attack.is_some() && intent.tx == me.x && intent.ty == me.y {
        mind.social.activity = crate::social::Activity::Alarm;
        mind.social.course = None;
        return (intent, mode);
    }
    let intent =
        crate::social::adjust(me, mind, intent, mode == Mode::Food, mode == Mode::Flee, mode == Mode::Return);
    if mode == Mode::Rest && mind.social.activity != crate::social::Activity::Alarm {
        mind.social.activity = crate::social::Activity::Resting;
        mind.social.course = None;
    }
    (intent, mode)
}

/// It has just eaten (`me` with its new energy); its next wander target lies at most `reach` away.
#[inline(always)]
pub(crate) fn after_eating(me: &Me, mind: &mut Mind, rng: &mut Rng, reach: f64) {
    actions::after_eating(me, mind, rng, reach);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CHASE_GIVE_UP_TICKS, CHASE_PATIENCE};
    use crate::creature::{Creature, Diet, Programs};
    use crate::flock::Circle;
    use crate::genome::creature::Gene;
    use crate::senses::{CorpseFood, Hunting, Prey, Taste, Threat};
    use crate::{CreatureGenome, Rules, Space};

    /// A plant 100 to the east and a corpse (radius 10) 100 to the west; like the world's senses,
    /// a corpse or prey only for those that eat meat.
    struct FoodSense {
        prey: Option<Prey>,
    }

    impl Senses for FoodSense {
        fn nearest_plant(&self, _: f64, _: f64, _: f64) -> Option<(f64, f64)> {
            Some((1100.0, 1000.0))
        }

        fn best_corpse(&self, me: &Me, _: Taste) -> Option<CorpseFood> {
            let eats_corpses = me.pheno.meat_efficiency > 0.0 || me.pheno.rot_efficiency > 0.0;
            eats_corpses.then_some(CorpseFood { owner: 2, x: 900.0, y: 1000.0, half: 10.0, score: 3.0 })
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, _: Hunting) -> Option<Prey> {
            self.prey.filter(|_| me.pheno.hunts())
        }
    }

    /// One plant east of the circle, within sight.
    struct PlantOutside;

    impl Senses for PlantOutside {
        fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
            ((1300.0 - x).powi(2) + (1000.0 - y).powi(2) <= r2).then_some((1300.0, 1000.0))
        }

        fn best_corpse(&self, _: &Me, _: Taste) -> Option<CorpseFood> {
            None
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }

        fn prey(&self, _: &Me, _: Option<u64>, _: Option<u64>, _: Hunting) -> Option<Prey> {
            None
        }
    }

    /// Two plants: a near one at (1100, 1000) and a farther one at (800, 1000).
    struct TwoPlants;

    impl Senses for TwoPlants {
        fn nearest_plant(&self, x: f64, y: f64, r2: f64) -> Option<(f64, f64)> {
            self.nearest_plant_where(x, y, r2, |_, _| true)
        }

        fn nearest_plant_where(
            &self,
            x: f64,
            y: f64,
            r2: f64,
            keep: impl Fn(f64, f64) -> bool,
        ) -> Option<(f64, f64)> {
            [(1100.0, 1000.0), (800.0, 1000.0)]
                .into_iter()
                .filter(|&(px, py)| keep(px, py) && (px - x).powi(2) + (py - y).powi(2) < r2)
                .min_by(|a, b| ((a.0 - x).abs()).total_cmp(&(b.0 - x).abs()))
        }

        fn nearest_threat(&self, _: &Me, _: f64) -> Option<Threat> {
            None
        }
    }

    #[test]
    fn food_behind_a_respected_border_is_no_target_unless_starving() {
        let v = Creature::new(
            &Space::default(),
            &Rules::default(),
            CreatureGenome::BASE,
            Some(1000.0),
            Some(1000.0),
            None,
            Rng::new(1),
        );
        for (share, want) in [(0.5, 800.0), (0.2, 1100.0)] {
            let me = Me {
                x: v.x,
                y: v.y,
                energy: v.pheno.max_energy * share,
                kinship: v.kinship(),
                flock: v.flock,
                circle: None,
                pheno: &v.pheno,
                health_share: 1.0,
                health: v.pheno.size,
            };
            let mut mind = Mind::default();
            // the near plant lies in a neighbour's area it walks around
            mind.social.territory_avoid =
                Some(crate::territory::Area { flock: 99, x: 1150.0, y: 1000.0, radius: 80.0 });
            let (intent, mode) = plan(&me, &Program::STANDARD, &mut mind, &mut Rng::new(3), &TwoPlants);
            assert_eq!(mode, Mode::Food, "fullness {share}");
            // it walks up to the edge of its reach of that plant
            let stop = want + (v.x - want).signum() * v.pheno.size * crate::config::EAT_STOP_SHARE;
            assert!(
                (intent.tx - stop).abs() < 1e-9,
                "fullness {share}: which plant it goes for, {}",
                intent.tx
            );
        }
    }

    #[test]
    fn how_hungry_a_member_leaves_its_circle_is_inherited() {
        for (forage, leaves) in [(20.0, false), (60.0, true)] {
            let genome = CreatureGenome::BASE.with(Gene::Forage, forage);
            let v = Creature::new(
                &Space::default(),
                &Rules::default(),
                genome,
                Some(1000.0),
                Some(1000.0),
                None,
                Rng::new(1),
            );
            let me = Me {
                x: v.x,
                y: v.y,
                energy: v.pheno.max_energy * 0.3,
                kinship: v.kinship(),
                flock: v.flock,
                circle: Some(Circle { x: 1000.0, y: 1000.0, radius: 150.0 }),
                pheno: &v.pheno,
                health_share: 1.0,
                health: v.pheno.size,
            };
            let mut mind = Mind::default();
            let (_, mode) = plan(&me, &Program::STANDARD, &mut mind, &mut Rng::new(3), &PlantOutside);
            assert_eq!(mind.social.foraging, leaves, "forage {forage}% at 30% of the store");
            assert_eq!(mode == Mode::Food, leaves, "forage {forage}%: the plant outside");
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
                health: v.pheno.size,
            };
            let (_, mode) = plan(&me, &Program::STANDARD, &mut mind, &mut rng, &PlantOutside);
            assert_eq!(mind.social.foraging, forages, "fullness {share}");
            assert_eq!(
                mode == Mode::Food,
                forages,
                "fullness {share}: the plant outside is taken only when foraging"
            );
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
                x: v.x,
                y: v.y,
                energy: v.energy,
                kinship: v.kinship(),
                flock: v.flock,
                circle: None,
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
            let hunts = diet != Diet::Herbivore;
            assert_eq!(
                fight.attack.is_some(),
                hunts,
                "{diet:?}: a started hunt goes on, a herbivore has none"
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
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
            pheno: &v.pheno,
            health_share: 1.0,
            health: v.pheno.size,
        };
        let hunt = |mind: &mut Mind, gap: f64| {
            mind.social.tick += 1;
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
        assert_eq!(until, mind.social.tick + CHASE_GIVE_UP_TICKS);
        assert_eq!(hunt(&mut mind, 100.0), None, "not chosen again");
        mind.social.tick = until - 1;
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
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
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

    fn me_of(v: &Creature) -> Me<'_> {
        Me {
            x: v.x,
            y: v.y,
            energy: v.energy,
            kinship: v.kinship(),
            flock: v.flock,
            circle: None,
            pheno: &v.pheno,
            health_share: v.health / v.max_health(),
            health: v.health,
        }
    }

    fn run(v: &Creature, program: &Program, mind: &mut Mind, senses: &impl Senses) -> (Intent, Mode) {
        plan(&me_of(v), program, mind, &mut Rng::new(3), senses)
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
            // at the edge it is done: the next block decides
            let edge = if down { v.pheno.body_hi } else { v.pheno.body_lo };
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
    /// eating and fighting phases, and the window sees which applied.
    #[test]
    fn settings_apply_and_the_program_goes_on() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.2);
        let program = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 30).not(), Action::EatForeign),
            Block::when(Test::at(Cond::Fullness, 10).not(), Action::Rival),
            Block::does(Action::Rival).with(0, 250),
            Block::does(Action::Reach).with(0, 10),
            Block::does(Action::Wander),
        ]);
        let mut mind = Mind::default();
        run(&v, &program, &mut mind, &Blind);
        assert_eq!(mind.fired, Some(4), "a setting does not decide");
        assert_eq!(mind.applied, 0b1101, "which settings applied");
        assert!(mind.stance.foreign);
        assert_eq!(mind.stance.rival, 2.5, "the last rival setting counts");
        assert!((mind.stance.reach - 0.1 * v.pheno.height).abs() < 1e-9);
        // a new tick starts from no settings
        run(&v, &Program::of(&[Block::does(Action::Wander)]), &mut mind, &Blind);
        assert_eq!(mind.stance, Stance::default());
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
    /// one steers it while its memory runs down, then it stops — the old flight distance.
    #[test]
    fn a_far_threat_steers_a_flight_but_does_not_renew_it() {
        let v = body(CreatureGenome::BASE, Some(1000.0), 0.5);
        let vision = v.pheno.vision;
        let far = Threat { id: 7, x: 1000.0 + vision * 0.6, y: 1000.0, gap: vision * 0.5, half: 40.0 };
        let senses = senses_from(|_, _, _| None).with_threat(far);
        let mut mind = Mind { flee_ticks: 3, ..Mind::default() };
        for left in [2, 1, 0] {
            let (intent, mode) = run(&v, &Program::STANDARD, &mut mind, &senses);
            assert_eq!((mode, mind.flee_ticks), (Mode::Flee, left));
            assert!(intent.tx < v.x, "away from the far threat: {intent:?}");
        }
        let (_, mode) = run(&v, &Program::STANDARD, &mut mind, &senses);
        assert_ne!(mode, Mode::Flee, "its memory ran out: a threat that far no longer scares it");
        let near = Threat { gap: vision * 0.2, ..far };
        let mut mind = Mind { flee_ticks: 3, ..Mind::default() };
        run(&v, &Program::STANDARD, &mut mind, &senses_from(|_, _, _| None).with_threat(near));
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
            let mut mind = Mind::default();
            mind.social.tick = 100;
            mind.social.hit = Some(crate::social::Alarm { enemy: 9, x: 1300.0, y: 1000.0, tick: 100 - ago });
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

        fn prey(&self, me: &Me, _: Option<u64>, _: Option<u64>, _: Hunting) -> Option<Prey> {
            me.pheno.hunts().then_some(Prey { id: 4, x: me.x, y: me.y + 80.0, half: 8.0, score: 2.0 })
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
