//! Behaviour programs: a creature's behaviour is an ordered list of blocks «if TESTS → ACTION», up
//! to three tests a block. Each tick the **settings** (`Action::is_setting`) whose tests hold apply
//! first, wherever they stand — eat the other niche's food too, strike rivals at food, go no
//! farther from the layer for food, switch a mode on — the first of each kind that applies wins
//! (`Block::setting_kind`), and a test sees what the settings above it set; then the first deciding
//! block whose tests hold and whose action can be done decides the step (`strategy::plan`); an
//! action that cannot be done (no prey, no corpse in sight, a hopeless chase) falls through to the
//! next block. When nothing decides, the creature stands. Modes (`Action::Mode`, `Cond::Mode`) are
//! its memory: a setting switches one on for a while, and any block may test it.
//!
//! Every number of behaviour lives in the blocks, never in the world or the genome: thresholds of
//! the tests (`Cond::param`), and each action's parameters (`Action::params`: how much smaller its
//! prey, how careful, how patient, how long it runs, how fast and how far it wanders, whether it
//! bursts). A creature has two programs (`Creature::programs`): the juvenile one while it grows to
//! its size gene, the adult one after.
//!
//! The programs are inherited apart from the gene table: a child copies its parent's; unless it is
//! an exact copy, a third of the numbers drift, like its genes (`Program::drift`, the rule
//! `program_drift` × the gene `program_mutability`), and with the rule `program_mutation` × the
//! same gene one mutation changes each — a number, a test, a negation, an action, the order, a
//! copy, a deletion, a new block, a block switched off or on, a mode and its test together (a
//! pair), a block of the other track copied in (a transfer; `Program::mutate_with`). Founders
//! start from the template of their `strategy` gene (`Program::template`), which carries the
//! values of the behaviour genes the programs replaced.
//!
//! A program is behaviour: it costs no upkeep and creates no energy — an action only chooses where
//! to step or how to stand; `Creature::act` pays. Its catch is its consequences: a program that
//! never flees is eaten, one that never looks for food starves.
//!
//! **`Cond`, `Action` and their parameter tables are append-only**, like the gene tables: their
//! numbers are drawn by the mutation and hashed by the golden test. Labels are game UI (Russian).

use super::strategy::Strategy;
use crate::config::{
    CHASE_GIVE_UP_TICKS, CHASE_PATIENCE, FLEE_SIGHT_SHARE, FLEE_TICKS, PROGRAM_DRIFT_SHARE,
    PROGRAM_NUDGE_POINTS,
};
use crate::rng::Rng;
use std::sync::Arc;

/// At most this many blocks: a duplicate or an insertion into a full program does nothing. The
/// window's path masks (`Mind::applied`, `Mind::tried`) have a bit a block.
pub const MAX_BLOCKS: usize = 32;
/// At most this many parameters an action reads.
pub const MAX_ARGS: usize = 8;
/// Tests a block has; «always» fills the unused ones.
pub const TESTS: usize = 3;
/// Numbers a block has: its tests' thresholds, then its action's parameters (`Block::spec`).
const SLOTS: usize = TESTS + MAX_ARGS;
/// Modes a creature can switch on (`Action::Mode`, `Cond::Mode`), numbered from 1.
pub const MODES: usize = 4;

/// How a parameter's raw number reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Percent: the raw number.
    Percent,
    /// Percent of its sight.
    Sight,
    /// A ratio: raw / 100 (150 = 1.5 times).
    Ratio,
    Ticks,
    /// Yes or no: 1 or 0; a mutation flips it, the drift leaves it.
    Flag,
    /// A number among a few (a mode): the drift leaves it, a mutation picks another.
    Index,
    /// An angle in hundredths of a radian (120 = 1.2 rad), shown in degrees.
    Angle,
    /// A tilt of a course, raw 0‒200: 100 is straight, above it down by (raw − 100)%, below up.
    Tilt,
}

/// A number a test or an action reads: what it is, its range and base, and how far a mutation
/// moves it (gauss sigma in raw units; a flag flips).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// Game UI: «добыча мельче в».
    pub label: &'static str,
    pub unit: Unit,
    pub lo: u16,
    pub hi: u16,
    pub base: u16,
    pub nudge: f64,
}

impl ParamSpec {
    const fn percent(label: &'static str, lo: u16, hi: u16, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Percent, lo, hi, base, nudge: PROGRAM_NUDGE_POINTS }
    }

    const fn sight(label: &'static str, lo: u16, hi: u16, base: u16, nudge: f64) -> ParamSpec {
        ParamSpec { label, unit: Unit::Sight, lo, hi, base, nudge }
    }

    const fn ratio(label: &'static str, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Ratio, lo: 100, hi: 500, base, nudge: 20.0 }
    }

    /// A time: a mutation moves it by a quarter of its base.
    const fn ticks(label: &'static str, lo: u16, hi: u16, base: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Ticks, lo, hi, base, nudge: base as f64 * 0.25 }
    }

    const fn flag(label: &'static str, base: bool) -> ParamSpec {
        ParamSpec { label, unit: Unit::Flag, lo: 0, hi: 1, base: base as u16, nudge: 0.0 }
    }

    const fn index(label: &'static str, hi: u16) -> ParamSpec {
        ParamSpec { label, unit: Unit::Index, lo: 1, hi, base: 1, nudge: 0.0 }
    }

    /// Straight by base; a mutation tilts it by 10 points.
    const fn tilt(label: &'static str) -> ParamSpec {
        ParamSpec { label, unit: Unit::Tilt, lo: 0, hi: 200, base: 100, nudge: PROGRAM_NUDGE_POINTS }
    }

    /// The value as the engine reads it: a share for percents (0.5), times for ratios (1.5),
    /// ticks, 1 or 0 for a flag, the number of an index, radians, a tilt's vertical share
    /// (+0.3 down, −0.3 up).
    pub fn value(&self, raw: u16) -> f64 {
        match self.unit {
            Unit::Percent | Unit::Sight | Unit::Ratio | Unit::Angle => f64::from(raw) / 100.0,
            Unit::Ticks | Unit::Flag | Unit::Index => f64::from(raw),
            Unit::Tilt => (f64::from(raw) - 100.0) / 100.0,
        }
    }

    /// Game UI: the amount alone, «30%», «33% зрения», «1,5 раза», «31 тик», «69°», «вниз 30%».
    pub fn amount(&self, raw: u16) -> String {
        match self.unit {
            Unit::Percent => format!("{raw}%"),
            Unit::Sight => format!("{raw}% зрения"),
            Unit::Ratio => format!("{} раза", format!("{:.1}", f64::from(raw) / 100.0).replace('.', ",")),
            Unit::Ticks => format!("{raw} {}", ticks_word(raw)),
            Unit::Flag => (if raw != 0 { "да" } else { "нет" }).to_string(),
            Unit::Index => raw.to_string(),
            Unit::Angle => format!("{:.0}°", f64::from(raw) / 100.0 * 180.0 / std::f64::consts::PI),
            Unit::Tilt if raw > 100 => format!("вниз {}%", raw - 100),
            Unit::Tilt if raw < 100 => format!("вверх {}%", 100 - raw),
            Unit::Tilt => "прямо".to_string(),
        }
    }

    /// Game UI: «добыча мельче в 1,5 раза», «рывком», «не рывком», «вниз 30%».
    pub fn show(&self, raw: u16) -> String {
        match self.unit {
            Unit::Flag if raw != 0 => self.label.to_string(),
            Unit::Flag => format!("не {}", self.label),
            Unit::Tilt => self.amount(raw),
            _ => format!("{} {}", self.label, self.amount(raw)),
        }
    }

    /// Whether the drift moves it: every number but a flag and an index.
    const fn drifts(&self) -> bool {
        !matches!(self.unit, Unit::Flag | Unit::Index)
    }

    /// The number moved by gauss(0, `sigma`), held in range.
    fn moved(&self, raw: u16, sigma: f64, rng: &mut Rng) -> u16 {
        let moved = f64::from(raw) + rng.gauss(0.0, sigma);
        moved.round().clamp(f64::from(self.lo), f64::from(self.hi)) as u16
    }

    /// A mutation's new value: a flag flips, an index becomes another one; a number moves by
    /// gauss(0, `nudge`), held in range.
    fn nudged(&self, raw: u16, rng: &mut Rng) -> u16 {
        match self.unit {
            Unit::Flag => u16::from(raw == 0),
            Unit::Index => {
                let other = rng.randint(i64::from(self.lo), i64::from(self.hi) - 1) as u16;
                if other >= raw { other + 1 } else { other }
            }
            _ => self.moved(raw, self.nudge, rng),
        }
    }

    /// A random value in range: a new test's threshold.
    fn random(&self, rng: &mut Rng) -> u16 {
        rng.randint(i64::from(self.lo), i64::from(self.hi)) as u16
    }
}

/// «1 тик», «3 тика», «30 тиков». Game UI.
pub fn ticks_word(n: u16) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=14) => "тиков",
        (1, _) => "тик",
        (2..=4, _) => "тика",
        _ => "тиков",
    }
}

/// What a test checks. A test with `negate` checks the opposite.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cond {
    Always,
    /// Fullness at least the threshold.
    Fullness,
    /// Health at least the threshold of its maximum.
    Health,
    /// At least the threshold of the world's depth down.
    Depth,
    /// A stranger that could eat it, closer to its body's edge than the threshold of its sight.
    ThreatNear,
    /// The same, but one hunting somebody now (it chose a target on its last move).
    HunterNear,
    /// Struck within the threshold's ticks by an enemy still in sight.
    Struck,
    /// Still running from a threat it lost sight of (the flight's memory, `Action::Flee`).
    Fleeing,
    /// In a rest it started (`Action::Rest`).
    Resting,
    /// A plant or a corpse it eats is in sight.
    FoodSeen,
    /// A stranger its hunts would take (by its most permissive hunt's ratio, the food it takes this
    /// tick) closer to its body's edge than the threshold of its sight.
    PreySeen,
    /// A plant it eats is in sight, within its reach.
    PlantSeen,
    /// A corpse it eats is in sight (or smelt), within its reach.
    CorpseSeen,
    /// At least the threshold of its lifespan old.
    Age,
    /// Winded after a burst (`Creature::winded`).
    Winded,
    /// The water where it stands at least the threshold cold (`Phenotype::coldness`).
    Cold,
    /// Above its layer.
    AboveLayer,
    /// Below its layer.
    BelowLayer,
    /// Inside its layer.
    InLayer,
    /// Its mode of the threshold's number is on (`Action::Mode`).
    Mode,
}

/// The flight distance of the templates, % of sight (`FLEE_SIGHT_SHARE`).
const FLEE_PCT: u16 = (FLEE_SIGHT_SHARE * 100.0 + 0.5) as u16;
/// How far the templates pick a wander target, % of sight: from a quarter of it (the old 0.5‒2
/// of sight).
const WANDER_REACH_PCT: u16 = 200;

const FULLNESS: ParamSpec = ParamSpec::percent("сытость", 0, 100, 50);
const HEALTH: ParamSpec = ParamSpec::percent("здоровье", 0, 100, 50);
const DEPTH: ParamSpec = ParamSpec::percent("глубже", 0, 100, 50);
const THREAT: ParamSpec = ParamSpec::sight("угроза ближе", 0, 100, FLEE_PCT, PROGRAM_NUDGE_POINTS);
const HUNTER: ParamSpec = ParamSpec::sight("охотник ближе", 0, 100, FLEE_PCT, PROGRAM_NUDGE_POINTS);
/// The old fixed window: struck on the last tick.
const STRUCK: ParamSpec = ParamSpec { label: "за", unit: Unit::Ticks, lo: 1, hi: 100, base: 1, nudge: 3.0 };
const PREY: ParamSpec = ParamSpec::sight("добыча ближе", 0, 100, 100, PROGRAM_NUDGE_POINTS);
/// Old age sets in at 70% of the lifespan (`phenotype::vigour`).
const AGE: ParamSpec = ParamSpec::percent("возраст", 0, 100, 70);
const COLD: ParamSpec = ParamSpec::percent("холод", 0, 100, 50);
const MODE: ParamSpec = ParamSpec::index("режим", MODES as u16);

impl Cond {
    pub const ALL: [Cond; 20] = [
        Cond::Always,
        Cond::Fullness,
        Cond::Health,
        Cond::Depth,
        Cond::ThreatNear,
        Cond::HunterNear,
        Cond::Struck,
        Cond::Fleeing,
        Cond::Resting,
        Cond::FoodSeen,
        Cond::PreySeen,
        Cond::PlantSeen,
        Cond::CorpseSeen,
        Cond::Age,
        Cond::Winded,
        Cond::Cold,
        Cond::AboveLayer,
        Cond::BelowLayer,
        Cond::InLayer,
        Cond::Mode,
    ];

    /// The threshold a test reads, if it has one.
    pub const fn param(self) -> Option<ParamSpec> {
        match self {
            Cond::Fullness => Some(FULLNESS),
            Cond::Health => Some(HEALTH),
            Cond::Depth => Some(DEPTH),
            Cond::ThreatNear => Some(THREAT),
            Cond::HunterNear => Some(HUNTER),
            Cond::Struck => Some(STRUCK),
            Cond::PreySeen => Some(PREY),
            Cond::Age => Some(AGE),
            Cond::Cold => Some(COLD),
            Cond::Mode => Some(MODE),
            _ => None,
        }
    }

    /// The label of the test as it holds, and as it holds negated (the threshold follows). Game UI.
    pub const fn labels(self) -> (&'static str, &'static str) {
        match self {
            Cond::Always => ("всегда", "никогда"),
            Cond::Fullness => ("сытость ≥", "сытость <"),
            Cond::Health => ("здоровье ≥", "здоровье <"),
            Cond::Depth => ("глубже", "выше"),
            Cond::ThreatNear => ("угроза ближе", "угрозы нет ближе"),
            Cond::HunterNear => ("охотник ближе", "охотника нет ближе"),
            Cond::Struck => ("его ударили за", "его не били за"),
            Cond::Fleeing => ("ещё убегает", "не убегает"),
            Cond::Resting => ("отдыхает", "не отдыхает"),
            Cond::FoodSeen => ("видит еду", "не видит еды"),
            Cond::PreySeen => ("добыча ближе", "добычи нет ближе"),
            Cond::PlantSeen => ("видит растение", "не видит растения"),
            Cond::CorpseSeen => ("чует падаль", "не чует падали"),
            Cond::Age => ("возраст ≥", "возраст <"),
            Cond::Winded => ("запыхался", "не запыхался"),
            Cond::Cold => ("холод ≥", "холод <"),
            Cond::AboveLayer => ("выше своего слоя", "не выше своего слоя"),
            Cond::BelowLayer => ("ниже своего слоя", "не ниже своего слоя"),
            Cond::InLayer => ("в своём слое", "вне своего слоя"),
            Cond::Mode => ("включён режим", "выключен режим"),
        }
    }

    /// Game UI.
    pub const fn about(self) -> &'static str {
        match self {
            Cond::Always => "Срабатывает всегда; перевёрнутое — никогда, блок выключен.",
            Cond::Fullness => "Доля полного бака, %.",
            Cond::Health => "Доля полного здоровья, %.",
            Cond::Depth => "Глубина, где оно сейчас, в процентах глубины мира.",
            Cond::ThreatNear => {
                "Тот, кто может его съесть, ближе этой доли зрения (до края его тела) — охотится он или нет."
            }
            Cond::HunterNear => "То же, но только тот, кто сейчас на кого-то охотится.",
            Cond::Struck => "За столько последних тиков его ударил тот, кого оно ещё видит.",
            Cond::Fleeing => {
                "Бежит от угрозы, которую уже потеряло из виду: память бегства ещё не кончилась."
            }
            Cond::Resting => "Уже отдыхает: отдых начат и не кончился.",
            Cond::FoodSeen => "Видит растение или падаль, которую ест.",
            Cond::PreySeen => {
                "Чужак, которого взял бы его самый смелый блок охоты, ближе этой доли зрения (до края \
                 тела). Без блока охоты или без вкуса к свежему мясу — никогда."
            }
            Cond::PlantSeen => "Видит растение, которое ест, в пределах своего выхода за слой.",
            Cond::CorpseSeen => "Видит или чует падаль, которую ест, в пределах своего выхода за слой.",
            Cond::Age => "Прожил эту долю своей жизни, %. Старость начинается с 70%.",
            Cond::Winded => "Запыхался после рывка и пока не может рвануть снова.",
            Cond::Cold => {
                "Насколько холодна вода там, где оно сейчас: 0% — тёплая вода наверху, 100% — холод \
                 глубины."
            }
            Cond::AboveLayer => "Выше верхнего края своего слоя.",
            Cond::BelowLayer => "Ниже нижнего края своего слоя.",
            Cond::InLayer => "Внутри своего слоя.",
            Cond::Mode => {
                "Режим с этим номером включён установкой «включить режим» и ещё не истёк: память \
                 существа о том, что оно начало."
            }
        }
    }
}

/// What a block does when its tests hold. A deciding action has its own guard: one that cannot be
/// done (no prey, no corpse in sight, the chase is hopeless) falls through to the next block. A
/// setting always applies and decides nothing.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Strike the enemy that struck it, or the threat in reach, if not too big and it can pay.
    FightBack,
    /// Run from the nearest threat in sight; with none in sight, on for its memory of ticks.
    Flee,
    /// Chase the prey worth most and strike it; a chase that does not close in is given up.
    Hunt,
    /// Go to the best corpse in sight and eat.
    EatCorpse,
    /// Go to its plant and eat.
    EatPlant,
    /// Wander in its layer.
    Wander,
    /// Stand still: no step to pay for.
    Ambush,
    /// Up to the top of its layer (not done once there).
    Surface,
    /// Down to the bottom of its layer (not done once there).
    Dive,
    /// Stand resting in its layer for a while, then not again for a pause.
    Rest,
    /// Stand torpid: a share of the standing upkeep (`TORPOR_UPKEEP`), eating nothing.
    Torpor,
    /// Setting: this tick it eats the other niche's food too (a scavenger fresh meat, a carnivore
    /// rot and bones); without it only its own (`DIET_OWN`).
    EatForeign,
    /// Setting: this tick it strikes a stranger this many times smaller eating the same food beside it.
    Rival,
    /// Setting: this tick it goes for food no farther than this share of the depth past its layer.
    Reach,
    /// Setting: switches its mode of this number on for this many ticks from now (0: off). The
    /// modes are its memory (`Cond::Mode`).
    Mode,
    /// Setting: its layer this tick, from and to these shares of the world's depth (swapped when
    /// reversed); without it the whole depth. Where it wanders, rests and walks back to.
    Layer,
    /// Setting: a calm walk holds its course this many ticks while it goes to the same point, and
    /// turns at most this angle a tick inside its layer; without it every step goes straight.
    Smooth,
    /// Setting: this tick it divides from this share of its tank, giving the child this share of
    /// its energy; without it it never divides.
    Divide,
    /// Setting: from the next tick it heals (`config::HEAL_SHARE` of its health a tick, paid from
    /// the tank) while its tank holds more than this share and nobody struck it this many ticks;
    /// without it it never heals.
    Heal,
    /// Setting: this tick it eats on the move — plants, corpses — whatever of its food it touches,
    /// while its tank is no fuller than this share; without it it eats only what its deciding
    /// block goes for.
    Graze,
    /// Setting: it knows its child — neither strikes nor fears it — until the child has grown to
    /// this share of its adult size; without it not at all.
    Spare,
    /// Setting: this tick it shoots its target from this share of its range, keeping this share
    /// of its tank; without it it never shoots.
    Shoot,
    /// Goes for the enemy of its child in need — struck lately, or while young afraid of a threat —
    /// and strikes it whatever its size (`Stance::defending`), while the child is this near, its
    /// tank fuller than this share, for at most this many ticks, then not again for a pause.
    DefendChild,
}

const PACE: ParamSpec = ParamSpec::percent("темп", 10, 100, 100);
const BURST: ParamSpec = ParamSpec::flag("рывком", true);
const BEST: ParamSpec = ParamSpec::flag("только если выгоднее", true);
const FIGHT_PARAMS: [ParamSpec; 1] = [ParamSpec::ratio("враг крупнее не более чем в", 150)];
const FLEE_PARAMS: [ParamSpec; 5] = [
    ParamSpec::ticks("бежать ещё", 0, 600, FLEE_TICKS as u16),
    BURST,
    // under way, only a threat this near makes it run its whole memory again (the old flight
    // distance); a farther one only steers it
    ParamSpec::sight("снова пугается угрозы ближе", 0, 100, FLEE_PCT, PROGRAM_NUDGE_POINTS),
    PACE,
    // straight away from the threat by base; a tilt turns the flight down or up
    ParamSpec::tilt("уклон"),
];
const HUNT_PARAMS: [ParamSpec; 8] = [
    ParamSpec::ratio("добыча мельче в", 150),
    // the weight of the strikes it expects: 100% is the old base caution, 0 ignores them
    ParamSpec {
        label: "осторожность", unit: Unit::Percent, lo: 0, hi: 400, base: 100, nudge: 20.0
    },
    ParamSpec::ticks("терпение", 5, 300, CHASE_PATIENCE as u16),
    BEST,
    BURST,
    ParamSpec::ticks("брошенную не трогать", 0, 3000, CHASE_GIVE_UP_TICKS as u16),
    ParamSpec::sight("добыча не дальше", 10, 100, 100, PROGRAM_NUDGE_POINTS),
    ParamSpec::percent("темп погони", 10, 100, 100),
];
const CORPSE_PARAMS: [ParamSpec; 2] = [BEST, PACE];
/// The old plant choice: the plant it goes to while it lives and is seen, else the nearest.
const PLANT_PARAMS: [ParamSpec; 3] =
    [PACE, ParamSpec::flag("держится выбранного", true), ParamSpec::flag("самое выгодное", false)];
const PACE_PARAMS: [ParamSpec; 1] = [PACE];
const WANDER_PARAMS: [ParamSpec; 2] = [PACE, ParamSpec::sight("цели до", 20, 500, WANDER_REACH_PCT, 25.0)];
const REST_PARAMS: [ParamSpec; 2] =
    [ParamSpec::ticks("отдых", 1, 600, 90), ParamSpec::ticks("пауза", 0, 1000, 180)];
const RIVAL_PARAMS: [ParamSpec; 1] = [ParamSpec::ratio("соперник мельче в", 150)];
const REACH_PARAMS: [ParamSpec; 1] = [ParamSpec::percent("за слой не дальше", 0, 100, 100)];
const MODE_PARAMS: [ParamSpec; 2] = [MODE, ParamSpec::ticks("на", 0, 600, 60)];
/// The old genes' bases: the layer 5‒100% (`min_y`, `max_y`), a division from 70% of the tank
/// giving 40% (`repro_threshold`, `repro_share`), shots from half the range keeping half the tank
/// (`fire_preference`, `fire_reserve`), the children known until grown (`care` 50%: twice it).
const LAYER_PARAMS: [ParamSpec; 2] =
    [ParamSpec::percent("сверху", 0, 100, 5), ParamSpec::percent("снизу", 0, 100, 100)];
/// The old social layer's smoothing: a course held 30 ticks, a turn of at most 1.2 rad a tick.
const SMOOTH_PARAMS: [ParamSpec; 2] = [
    ParamSpec::ticks("держит курс", 0, 300, 30),
    ParamSpec { label: "поворот до", unit: Unit::Angle, lo: 0, hi: 314, base: 120, nudge: 15.0 },
];
/// A child gets at least 1% of the tank: one of nothing would be born dead.
const DIVIDE_PARAMS: [ParamSpec; 2] =
    [ParamSpec::percent("с бака", 0, 100, 70), ParamSpec::percent("потомку", 1, 100, 40)];
const HEAL_PARAMS: [ParamSpec; 2] =
    [ParamSpec::percent("при баке больше", 0, 100, 50), ParamSpec::ticks("без ударов", 0, 600, 60)];
const GRAZE_PARAMS: [ParamSpec; 3] = [
    ParamSpec::flag("растения", true),
    ParamSpec::flag("падаль", true),
    ParamSpec::percent("пока сытость не больше", 0, 100, 100),
];
const SPARE_PARAMS: [ParamSpec; 1] = [ParamSpec::percent("пока ребёнок не вырос до", 0, 100, 100)];
const SHOOT_PARAMS: [ParamSpec; 2] =
    [ParamSpec::percent("с доли дальности", 0, 100, 50), ParamSpec::percent("оставляя бак", 0, 100, 50)];
/// The old parent's cover: a child within half its sight (the `care` gene's base), a tank more
/// than half full, 90 ticks at most, then 60 of rest; a child struck within 30 ticks.
const DEFEND_PARAMS: [ParamSpec; 5] = [
    ParamSpec::sight("ребёнок ближе", 0, 100, 50, PROGRAM_NUDGE_POINTS),
    ParamSpec::percent("при баке больше", 0, 100, 50),
    ParamSpec::ticks("не дольше", 1, 600, 90),
    ParamSpec::ticks("потом пауза", 0, 1000, 60),
    ParamSpec::ticks("ребёнка ударили за", 1, 300, 30),
];
const NO_PARAMS: [ParamSpec; 0] = [];

impl Action {
    pub const ALL: [Action; 23] = [
        Action::FightBack,
        Action::Flee,
        Action::Hunt,
        Action::EatCorpse,
        Action::EatPlant,
        Action::Wander,
        Action::Ambush,
        Action::Surface,
        Action::Dive,
        Action::Rest,
        Action::Torpor,
        Action::EatForeign,
        Action::Rival,
        Action::Reach,
        Action::Mode,
        Action::Layer,
        Action::Smooth,
        Action::Divide,
        Action::Heal,
        Action::Graze,
        Action::Spare,
        Action::Shoot,
        Action::DefendChild,
    ];

    /// The parameters the action reads, in the order of `Block::args`.
    pub const fn params(self) -> &'static [ParamSpec] {
        match self {
            Action::FightBack => &FIGHT_PARAMS,
            Action::Flee => &FLEE_PARAMS,
            Action::Hunt => &HUNT_PARAMS,
            Action::EatCorpse => &CORPSE_PARAMS,
            Action::EatPlant => &PLANT_PARAMS,
            Action::Surface | Action::Dive => &PACE_PARAMS,
            Action::Wander => &WANDER_PARAMS,
            Action::Rest => &REST_PARAMS,
            Action::Rival => &RIVAL_PARAMS,
            Action::Reach => &REACH_PARAMS,
            Action::Mode => &MODE_PARAMS,
            Action::Layer => &LAYER_PARAMS,
            Action::Smooth => &SMOOTH_PARAMS,
            Action::Divide => &DIVIDE_PARAMS,
            Action::Heal => &HEAL_PARAMS,
            Action::Graze => &GRAZE_PARAMS,
            Action::Spare => &SPARE_PARAMS,
            Action::Shoot => &SHOOT_PARAMS,
            Action::DefendChild => &DEFEND_PARAMS,
            Action::Ambush | Action::Torpor | Action::EatForeign => &NO_PARAMS,
        }
    }

    /// The parameters at their bases.
    pub const fn base_args(self) -> [u16; MAX_ARGS] {
        let params = self.params();
        let mut args = [0; MAX_ARGS];
        let mut i = 0;
        while i < params.len() {
            args[i] = params[i].base;
            i += 1;
        }
        args
    }

    /// The parameters for this action taken over from a block of `old`: a parameter of the same
    /// label and unit (the pace, a burst, «only if it pays more») keeps its number, the rest start
    /// at their bases.
    pub fn args_from(self, old: Action, old_args: &[u16; MAX_ARGS]) -> [u16; MAX_ARGS] {
        let mut args = self.base_args();
        for (i, p) in self.params().iter().enumerate() {
            if let Some(j) = old.params().iter().position(|q| q.label == p.label && q.unit == p.unit) {
                args[i] = old_args[j];
            }
        }
        args
    }

    /// A setting does not decide the step: it applies before the deciding blocks, wherever it
    /// stands.
    pub const fn is_setting(self) -> bool {
        matches!(
            self,
            Action::EatForeign
                | Action::Rival
                | Action::Reach
                | Action::Mode
                | Action::Layer
                | Action::Smooth
                | Action::Divide
                | Action::Heal
                | Action::Graze
                | Action::Spare
                | Action::Shoot
        )
    }

    /// Whether the action is always done when reached: the deciding blocks after an unconditional
    /// one of these are never reached.
    pub const fn never_fails(self) -> bool {
        matches!(self, Action::Wander | Action::Ambush | Action::Torpor)
    }

    /// Game UI.
    pub const fn label(self) -> &'static str {
        match self {
            Action::FightBack => "дать сдачи",
            Action::Flee => "убегать",
            Action::Hunt => "охотиться",
            Action::EatCorpse => "к падали",
            Action::EatPlant => "к растению",
            Action::Wander => "бродить",
            Action::Ambush => "замереть",
            Action::Surface => "к верху слоя",
            Action::Dive => "ко дну слоя",
            Action::Rest => "отдыхать",
            Action::Torpor => "оцепенеть",
            Action::EatForeign => "есть и чужую пищу",
            Action::Rival => "гнать соперников у еды",
            Action::Reach => "за едой из слоя",
            Action::Mode => "включить режим",
            Action::Layer => "слой",
            Action::Smooth => "плавный ход",
            Action::Divide => "делиться",
            Action::Heal => "лечиться",
            Action::Graze => "есть на ходу",
            Action::Spare => "щадить детей",
            Action::Shoot => "стрелять",
            Action::DefendChild => "защищать детёныша",
        }
    }

    /// Game UI.
    pub const fn about(self) -> &'static str {
        match self {
            Action::FightBack => {
                "Бьёт того, кто его ударил, или угрозу, которая уже рядом, если она не слишком велика и \
                 хватает энергии на удар. Охотник знает это правило жертвы по её последнему ходу — пока \
                 условие блока выполнено — и боится её ударов, только если сам в него попадает."
            }
            Action::Flee => {
                "Бежит от ближайшей угрозы в поле зрения; потеряв её из виду, бежит ещё столько тиков. \
                 На бегу память бегства обновляет только угроза ближе порога «снова пугается», дальняя \
                 лишь задаёт направление. Рывком — быстрее, насколько позволяют мышцы (ген «рывок»). \
                 Ход — доля скорости; уклон уводит бегство вниз или вверх."
            }
            Action::Hunt => {
                "Гонится за самой выгодной добычей, которая мельче его в столько раз и не дальше этой \
                 доли зрения, и бьёт её. Осторожность — насколько боится ответных ударов (100% — \
                 обычная, 0 — не боится); погоню, которая за «терпение» тиков не сократила разрыв, \
                 бросает и столько тиков эту добычу не трогает. «Только если выгоднее» — не охотится, \
                 когда растение или падаль дают не меньше. Ход погони — доля скорости. Добыча его \
                 боится, если на последнем ходу условие блока выполнилось и он мог охотиться: не сыт \
                 и свежее мясо ему сейчас пища. Если охоту заслонил блок выше (бегство, отдых) — нет."
            }
            Action::EatCorpse => {
                "Идёт к лучшей видимой падали, которую ест, и ест. «Только если выгоднее» — когда \
                 растение даёт не больше."
            }
            Action::EatPlant => {
                "Идёт к растению и ест. «Держится выбранного» — к тому, к которому уже шло, пока оно \
                 цело и видно; «самое выгодное» — иначе к тому, что даст больше за тик пути и еды, а \
                 не к ближайшему."
            }
            Action::Wander => {
                "Бродит по своему слою глубины, выбирая новые точки от четверти до «цели до» своего \
                 зрения."
            }
            Action::Ambush => "Стоит на месте: за ход не платит, только за тело, глаза и мышцы рывка.",
            Action::Surface => "Поднимается к верхнему краю своего слоя; наверху не срабатывает.",
            Action::Dive => "Опускается к нижнему краю своего слоя; внизу не срабатывает.",
            Action::Rest => {
                "Стоит отдыхая в своём слое не дольше «отдыха» тиков; потом «паузу» тиков не отдыхает. \
                 Отдых прерывается, как только блок не сработал."
            }
            Action::Torpor => {
                "Замирает в оцепенении: тратит 30% расхода стоя. Не ест, даже то, что рядом, и не \
                 бежит, пока программа не решит иначе."
            }
            Action::EatForeign => {
                "Установка на этот тик: ест и чужую пищу, если её усваивает: травоядный и падальщик — \
                 свежее мясо, мясоед — гниль. Без неё — только свою."
            }
            Action::Rival => {
                "Установка на этот тик: бьёт чужака, который мельче в столько раз и ест ту же еду рядом."
            }
            Action::Reach => {
                "Установка на этот тик: за едой, которую видит, выходит из своего слоя не дальше этой \
                 доли глубины мира. Без неё — куда угодно."
            }
            Action::Mode => {
                "Установка: включает режим с этим номером на столько тиков вперёд (0 — выключает). \
                 Режим — память: любой блок может проверить, включён ли он."
            }
            Action::Layer => {
                "Установка: слой глубины на этот тик, в процентах глубины мира. В нём бродит, \
                 отдыхает и в него возвращается; за видимой едой выходит. Без неё — вся глубина."
            }
            Action::Smooth => {
                "Установка: спокойный шаг держит курс столько тиков, пока цель та же, и поворачивает \
                 не больше этого угла за тик. Без неё каждый шаг идёт прямо к цели."
            }
            Action::Divide => {
                "Установка: делится, когда бак полон на эту долю, и отдаёт потомку эту долю энергии. \
                 Делится только выросший и не чаще раза в 30 тиков, так что в детской дорожке не \
                 срабатывает. Без неё не делится вовсе."
            }
            Action::Heal => {
                "Установка: со следующего тика лечится (0,2% здоровья за тик, платит из бака), пока \
                 бак полнее этой доли и его столько тиков не били. Без неё не лечится."
            }
            Action::Graze => {
                "Установка: на этот тик ест на ходу своё — растения, падаль, — к чему прикоснётся, \
                 пока бак не полнее этой доли. Без неё ест только то, к чему идёт решающий блок."
            }
            Action::Spare => {
                "Установка: узнаёт своих детей — не бьёт их и не боится, — пока ребёнок не вырос до \
                 этой доли своего взрослого размера. Без неё не узнаёт вовсе."
            }
            Action::Shoot => {
                "Установка: на этот тик стреляет в свою цель, когда та ближе этой доли дальности \
                 выстрела, и оставляет в баке эту долю. Без неё не стреляет."
            }
            Action::DefendChild => {
                "Идёт на врага своего ребёнка и бьёт его, какого бы размера тот ни был: ребёнка \
                 недавно ударили или, пока он мал, его напугала угроза. Только пока ребёнок ближе \
                 этой доли зрения, бак полнее этой доли, не дольше столько тиков — потом пауза. \
                 Охотники считают его защитником детей, пока условие блока выполнено на его \
                 последнем ходу."
            }
        }
    }
}

/// One check of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Test {
    pub cond: Cond,
    pub negate: bool,
    /// The threshold of a test that has one (`Cond::param`), raw.
    pub param: u16,
}

impl Test {
    pub const ALWAYS: Test = Test::is(Cond::Always);

    /// The test at its base threshold, if it has one.
    pub const fn is(cond: Cond) -> Test {
        let param = match cond.param() {
            Some(p) => p.base,
            None => 0,
        };
        Test { cond, negate: false, param }
    }

    pub const fn at(cond: Cond, param: u16) -> Test {
        Test { cond, negate: false, param }
    }

    pub const fn not(self) -> Test {
        Test { negate: !self.negate, ..self }
    }

    /// Whether it always holds.
    pub const fn always(self) -> bool {
        matches!(self.cond, Cond::Always) && !self.negate
    }

    /// Whether it never holds: a negated «always» switches its block off.
    pub const fn never(self) -> bool {
        matches!(self.cond, Cond::Always) && self.negate
    }

    /// The threshold as the engine reads it (a share for percents, ticks for a time).
    pub fn value(self) -> f64 {
        self.cond.param().map_or(0.0, |p| p.value(self.param))
    }

    /// Game UI: «сытость < 40%», «угроза ближе 33% зрения», «его ударили за 1 тик».
    pub fn label(self) -> String {
        let (yes, no) = self.cond.labels();
        let text = if self.negate { no } else { yes };
        match self.cond.param() {
            Some(p) => format!("{text} {}", p.amount(self.param)),
            None => text.to_string(),
        }
    }
}

/// «If all its tests hold → the action with its parameters».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub when: [Test; TESTS],
    pub action: Action,
    /// The action's parameters, raw, in the order of `Action::params`; the rest are zero.
    pub args: [u16; MAX_ARGS],
}

/// What fills a program past its end: kept the same, so programs of the same blocks are equal.
const FILLER: Block = Block::does(Action::Wander);

impl Block {
    /// Unconditional, the parameters at their bases.
    pub const fn does(action: Action) -> Block {
        Block::when3(Test::ALWAYS, Test::ALWAYS, Test::ALWAYS, action)
    }

    pub const fn when(test: Test, action: Action) -> Block {
        Block::when3(test, Test::ALWAYS, Test::ALWAYS, action)
    }

    pub const fn when2(a: Test, b: Test, action: Action) -> Block {
        Block::when3(a, b, Test::ALWAYS, action)
    }

    pub const fn when3(a: Test, b: Test, c: Test, action: Action) -> Block {
        Block { when: [a, b, c], action, args: action.base_args() }
    }

    /// A setting's kind as a bit: of the settings of one kind whose tests hold, only the first
    /// applies. Each mode is a kind of its own, so one tick can switch several on.
    pub const fn setting_kind(&self) -> u64 {
        match self.action {
            Action::Mode => 1 << (48 + self.args[0]),
            a => 1 << a as u8,
        }
    }

    /// The same block with parameter `i` set, raw.
    pub const fn with(mut self, i: usize, raw: u16) -> Block {
        self.args[i] = raw;
        self
    }

    /// Parameter `i` as the engine reads it.
    pub fn arg(&self, i: usize) -> f64 {
        self.action.params()[i].value(self.args[i])
    }

    /// Whether a flag parameter is on.
    pub fn flag(&self, i: usize) -> bool {
        self.args[i] != 0
    }

    /// Switched off: a test never holds.
    pub const fn off(&self) -> bool {
        self.when[0].never() || self.when[1].never() || self.when[2].never()
    }

    /// Whether it decides every time it is reached.
    pub const fn always_fires(&self) -> bool {
        self.unconditional() && self.action.never_fails()
    }

    /// Its tests as one line: «сытость < 40% и его ударили за 1 тик»; «всегда» when there are none.
    pub fn condition_label(&self) -> String {
        let tests: Vec<String> = self.when.iter().filter(|t| !t.always()).map(|t| t.label()).collect();
        if tests.is_empty() { Test::ALWAYS.label() } else { tests.join(" и ") }
    }

    /// Its parameters as text: «добыча мельче в 1,5 раза, осторожность 100%». Game UI.
    pub fn args_label(&self) -> String {
        let params = self.action.params();
        params.iter().zip(self.args).map(|(p, raw)| p.show(raw)).collect::<Vec<_>>().join(", ")
    }

    /// Everything about the block packed into numbers: for digests and counting programs. A test
    /// takes 18 bits (condition 5, negation 1, threshold 12), the action 5, a parameter 12.
    pub fn code(&self) -> [u64; 3] {
        let test = |t: Test| u64::from(t.cond as u8) | u64::from(t.negate) << 5 | u64::from(t.param) << 6;
        let head = test(self.when[0])
            | test(self.when[1]) << 18
            | test(self.when[2]) << 36
            | u64::from(self.action as u8) << 54;
        let mut args = [0; 2];
        for (i, a) in self.args.iter().enumerate() {
            args[i / 5] |= u64::from(*a) << (12 * (i % 5));
        }
        [head, args[0], args[1]]
    }

    /// The block without its numbers — its tests' conditions and negations, its action, its flags
    /// and its indices (a mode's number): what stays while the numbers drift. The report groups
    /// programs by it.
    pub fn shape(&self) -> u64 {
        let test = |t: Test| {
            let index = if t.cond.param().is_some_and(|p| p.unit == Unit::Index) { t.param } else { 0 };
            u64::from(t.cond as u8) | u64::from(t.negate) << 5 | u64::from(index) << 6
        };
        let mut fixed = 0_u64;
        for (i, p) in self.action.params().iter().enumerate() {
            if !p.drifts() {
                fixed |= u64::from(self.args[i]) << (3 * i);
            }
        }
        test(self.when[0])
            | test(self.when[1]) << 9
            | test(self.when[2]) << 18
            | u64::from(self.action as u8) << 27
            | fixed << 32
    }

    /// Whether every test is «always»: it applies (a setting) or is tried (a deciding block) each
    /// time it is reached.
    pub const fn unconditional(&self) -> bool {
        self.when[0].always() && self.when[1].always() && self.when[2].always()
    }

    /// The spec of the number in `slot`: a test's threshold (slots 0‒2) or a parameter (3‒); None
    /// for a test without a threshold or past the action's parameters.
    fn spec(&self, slot: usize) -> Option<ParamSpec> {
        if slot < TESTS {
            self.when[slot].cond.param()
        } else {
            self.action.params().get(slot - TESTS).copied()
        }
    }

    /// The numbers of the block the drift moves: each test's threshold and each parameter but the
    /// flags and indices, as (slot, its spec); slots 0‒2 are the tests, 3‒ the parameters.
    fn numbers(&self) -> impl Iterator<Item = (usize, ParamSpec)> + '_ {
        (0..SLOTS).filter_map(|s| self.spec(s).filter(ParamSpec::drifts).map(|p| (s, p)))
    }

    /// The health share down to which a fight-back block fights: its highest «health ≥» test
    /// (0 without one). `Summary::of` reads the shape by it, `Menace` a creature's last move.
    pub const fn fight_health(&self) -> u16 {
        let mut health = 0;
        let mut t = 0;
        while t < TESTS {
            let test = self.when[t];
            if !test.negate && matches!(test.cond, Cond::Health) && test.param > health {
                health = test.param;
            }
            t += 1;
        }
        health
    }

    fn number(&mut self, slot: usize) -> &mut u16 {
        if slot < TESTS { &mut self.when[slot].param } else { &mut self.args[slot - TESTS] }
    }

    fn get(&self, slot: usize) -> u16 {
        if slot < TESTS { self.when[slot].param } else { self.args[slot - TESTS] }
    }
}

/// What the rest of the world reads of a program every tick, worked out when the program is made
/// or changed. Raw numbers, as the blocks keep them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Summary {
    /// Blocks up to the first deciding one that always fires.
    reachable: u8,
    /// The blocks that may act, a bit a block (`Program::live`).
    live: u32,
    /// The farthest a threat test looks, % of sight.
    threat: u16,
    /// Its most permissive hunt's ratio; 0: it never hunts. Its own `Cond::PreySeen` reads it;
    /// the others read what its blocks did on its last move (`Menace`), the shape only before it.
    hunt: u16,
    /// Its first fight-back block's ratio (0: it never fights back) and health threshold.
    fight: u16,
    fight_health: u16,
    /// How far its first wander looks for a target, % of sight.
    wander: u16,
    /// Its home layer, % of the depth: its first live unconditional layer setting (a founder is
    /// placed there); the whole depth without one.
    layer: (u16, u16),
    /// It has a live shooting setting.
    shoots: bool,
    /// It has a live block defending its children.
    defends: bool,
}

impl Summary {
    const fn of(blocks: &[Block; MAX_BLOCKS], len: usize) -> Summary {
        let mut reachable = len;
        let mut i = 0;
        while i < len {
            if blocks[i].always_fires() {
                reachable = i + 1;
                break;
            }
            i += 1;
        }
        // live: not switched off, and a deciding block reached or a setting that no earlier
        // unconditional one of its kind hides (only the first of a kind whose tests hold applies)
        let (mut live, mut hidden) = (0_u32, 0_u64);
        let mut i = 0;
        while i < len {
            let b = &blocks[i];
            if b.off() {
                // switched off
            } else if b.action.is_setting() {
                let kind = b.setting_kind();
                if hidden & kind == 0 {
                    live |= 1 << i;
                    if b.unconditional() {
                        hidden |= kind;
                    }
                }
            } else if i < reachable {
                live |= 1 << i;
            }
            i += 1;
        }
        let mut s = Summary {
            reachable: reachable as u8,
            live,
            threat: 0,
            hunt: 0,
            fight: 0,
            fight_health: 0,
            wander: 0,
            layer: (0, 100),
            shoots: false,
            defends: false,
        };
        let (mut fights, mut wanders, mut layered) = (false, false, false);
        i = 0;
        while i < len {
            let b = &blocks[i];
            if live & (1 << i) != 0 {
                let mut t = 0;
                while t < TESTS {
                    let test = b.when[t];
                    if !test.negate
                        && matches!(test.cond, Cond::ThreatNear | Cond::HunterNear)
                        && test.param > s.threat
                    {
                        s.threat = test.param;
                    }
                    t += 1;
                }
                match b.action {
                    Action::Hunt if s.hunt == 0 || b.args[0] < s.hunt => s.hunt = b.args[0],
                    Action::FightBack if !fights => {
                        fights = true;
                        s.fight = b.args[0];
                        s.fight_health = b.fight_health();
                    }
                    Action::Wander if !wanders => {
                        wanders = true;
                        s.wander = b.args[1];
                    }
                    Action::Layer if !layered && b.unconditional() => {
                        layered = true;
                        let (a, z) = (b.args[0], b.args[1]);
                        s.layer = if a <= z { (a, z) } else { (z, a) };
                    }
                    Action::Shoot => s.shoots = true,
                    Action::DefendChild => s.defends = true,
                    _ => {}
                }
            }
            i += 1;
        }
        if !wanders {
            s.wander = WANDER_REACH_PCT;
        }
        s
    }
}

/// A creature's behaviour in one stage of its life: its blocks in order, and how many mutations
/// changed it since its template (the drift of its numbers aside).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Program {
    blocks: [Block; MAX_BLOCKS],
    len: u8,
    /// Mutations since the template that changed something, for showing.
    pub changes: u16,
    summary: Summary,
}

/// The stages of life a creature has a program for: `Creature::programs[stage]`.
pub const JUVENILE: usize = 0;
pub const ADULT: usize = 1;

/// The founders' modes (`Action::Mode`, `Cond::Mode`): their memory of an alarm, of a full tank
/// and of hunger. The founders remember through modes rather than the special tests («ещё
/// убегает», «отдыхает»), so every line starts with a working memory a mutation can rebuild.
pub const ALARM_MODE: u16 = 1;
pub const FULL_MODE: u16 = 2;
pub const HUNGRY_MODE: u16 = 3;
/// The founders' alarm lasts the tick it is raised and the old flight's memory after it, so the
/// template runs as long as the old strategy did.
const ALARM_TICKS: u16 = FLEE_TICKS as u16 + 1;

/// The founders' behaviour — the old hand-written strategy with the base values of the behaviour
/// genes the programs replaced — wandering at `pace` % of its speed.
const fn founders(pace: u16) -> Program {
    // the old bravery 50%: a calm stranger may come half as near as a hunting one
    let calm = FLEE_PCT / 2;
    Program::of(&[
        // what the genes and the world did until `life-behavior/14`: the layer 5‒100%, the social
        // layer's smoothing, the division, the healing, eating whatever it touches, the children
        // spared until grown
        Block::does(Action::Layer),
        Block::does(Action::Smooth),
        Block::does(Action::Divide),
        Block::does(Action::Heal),
        Block::does(Action::Graze),
        Block::does(Action::Spare),
        // hunger remembered 60 ticks: below the old picky and rivalry (30%) it eats the other
        // niche's food and fights for its own
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::Mode).with(0, HUNGRY_MODE),
        Block::when(Test::at(Cond::Mode, HUNGRY_MODE), Action::EatForeign),
        Block::when(Test::at(Cond::Mode, HUNGRY_MODE), Action::Rival),
        // an alarm remembered the old flight's memory: a hunter within the flight distance, a calm
        // stranger within half of it
        Block::when(Test::at(Cond::HunterNear, FLEE_PCT), Action::Mode)
            .with(0, ALARM_MODE)
            .with(1, ALARM_TICKS),
        Block::when(Test::at(Cond::ThreatNear, calm), Action::Mode).with(0, ALARM_MODE).with(1, ALARM_TICKS),
        // a full tank remembered 200 ticks: the old rest gene, from 95% fullness
        Block::when(Test::at(Cond::Fullness, 95), Action::Mode).with(0, FULL_MODE).with(1, 200),
        // the old bravery 50%: it defends itself down to half its health
        Block::when(Test::at(Cond::Health, 50), Action::FightBack),
        Block::when(Test::at(Cond::Mode, ALARM_MODE), Action::Flee),
        // the old parent's cover, while it stood firm: 10 points above its fight-back threshold
        Block::when(Test::at(Cond::Health, 60), Action::DefendChild),
        Block::does(Action::Hunt),
        // the rest is given up 10 points below where it began
        Block::when2(Test::at(Cond::Mode, FULL_MODE), Test::at(Cond::Fullness, 85), Action::Rest),
        Block::does(Action::EatCorpse),
        Block::does(Action::EatPlant),
        Block::does(Action::Wander).with(0, pace),
    ])
}

impl Program {
    /// The standard behaviour: wandering at full speed.
    pub const STANDARD: Program = founders(100);

    /// The lurker: the same, but it wanders at a third of its speed.
    pub const LURKER: Program = founders(33);

    /// A program of these blocks; panics on none or more than `MAX_BLOCKS`.
    pub const fn of(list: &[Block]) -> Program {
        assert!(!list.is_empty() && list.len() <= MAX_BLOCKS, "a program has 1 to MAX_BLOCKS blocks");
        let mut blocks = [FILLER; MAX_BLOCKS];
        let mut i = 0;
        while i < list.len() {
            blocks[i] = list[i];
            i += 1;
        }
        Program::made(blocks, list.len())
    }

    const fn made(blocks: [Block; MAX_BLOCKS], len: usize) -> Program {
        Program { blocks, len: len as u8, changes: 0, summary: Summary::of(&blocks, len) }
    }

    /// The founders' program of a strategy.
    pub const fn template(strategy: Strategy) -> Program {
        match strategy {
            Strategy::Standard => Program::STANDARD,
            Strategy::Lurker => Program::LURKER,
        }
    }

    /// A founder's program: its strategy's template in the layer `layer` (% of the depth), with a
    /// shooting setting for a shooter (after the other settings).
    pub fn founder(strategy: Strategy, layer: (u16, u16), shoots: bool) -> Program {
        let mut p = Program::template(strategy)
            .tuned(Action::Layer, |b| b.args[..2].copy_from_slice(&[layer.0, layer.1]));
        if shoots {
            let at = p.blocks().iter().take_while(|b| b.action.is_setting()).count();
            p.insert(at, Block::does(Action::Shoot));
            p.summary = Summary::of(&p.blocks, usize::from(p.len));
        }
        p
    }

    /// Its home layer, shares of the depth: where a founder is placed (`Summary::layer`).
    pub fn home_layer(&self) -> (f64, f64) {
        let (lo, hi) = self.summary.layer;
        (f64::from(lo) / 100.0, f64::from(hi) / 100.0)
    }

    /// Its home layer, % of the depth.
    pub fn home_layer_pct(&self) -> (u16, u16) {
        self.summary.layer
    }

    /// Whether it ever shoots (a live shooting setting).
    pub fn shoots(&self) -> bool {
        self.summary.shoots
    }

    /// Whether it defends its children (a live «защищать детёныша» block).
    pub fn defends(&self) -> bool {
        self.summary.defends
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks[..usize::from(self.len)]
    }

    /// The same program with every block of `action` changed by `f` (tests, tools).
    pub fn tuned(mut self, action: Action, f: impl Fn(&mut Block)) -> Program {
        let len = usize::from(self.len);
        self.blocks[..len].iter_mut().filter(|b| b.action == action).for_each(f);
        self.summary = Summary::of(&self.blocks, len);
        self
    }

    /// How many blocks are ever reached: up to the first deciding one that always fires. The
    /// deciding blocks after it are kept — they drift without effect and may come back after a
    /// deletion or a swap; the settings after it still apply.
    pub fn reachable(&self) -> usize {
        usize::from(self.summary.reachable)
    }

    /// Whether block `i` may act: not switched off, and a deciding block that is reached or a
    /// setting that no earlier unconditional setting of its kind hides.
    pub fn live(&self, i: usize) -> bool {
        i < usize::from(self.len) && self.summary.live & (1 << i) != 0
    }

    /// Blocks that never act (`live`).
    #[cfg(test)]
    fn dead(&self) -> usize {
        usize::from(self.len) - self.summary.live.count_ones() as usize
    }

    /// The first deciding block that always fires, which ends what is reached; None when none does
    /// (nothing deciding is then a way the creature can come to stand).
    pub fn ending(&self) -> Option<usize> {
        let last = self.reachable().checked_sub(1)?;
        self.blocks[last].always_fires().then_some(last)
    }

    /// How many times smaller its most permissive hunt takes prey (`Cond::PreySeen`; what the
    /// others read of it before its first move, `Menace::of`). None — it never hunts.
    pub fn hunt_ratio(&self) -> Option<f64> {
        (self.summary.hunt > 0).then(|| f64::from(self.summary.hunt) / 100.0)
    }

    /// Its defence, from its first live fight-back block: the enemy's size ratio it still fights,
    /// and the health share down to which it does (0 when the block does not test health). None:
    /// it never fights back.
    pub fn defence(&self) -> Option<(f64, f64)> {
        let s = self.summary;
        (s.fight > 0).then(|| (f64::from(s.fight) / 100.0, f64::from(s.fight_health) / 100.0))
    }

    /// `hunt_ratio` and `defence` raw, as `Menace` keeps them (hundredths, %; 0: none).
    pub(super) fn menace_raw(&self) -> (u16, (u16, u16)) {
        let s = self.summary;
        (s.hunt, (s.fight, s.fight_health))
    }

    /// The farthest a threat test looks, a share of sight: the first threat query covers it.
    pub fn threat_range(&self) -> f64 {
        f64::from(self.summary.threat) / 100.0
    }

    /// How far its first live wander block picks a target, a share of sight (the base without
    /// one): a creature that has just eaten picks its next one as far.
    pub fn wander_reach(&self) -> f64 {
        f64::from(self.summary.wander) / 100.0
    }

    /// Whether two programs behave alike: the same blocks, whatever the mutations that led there.
    pub fn same_blocks(&self, other: &Program) -> bool {
        self.blocks() == other.blocks()
    }

    /// Its blocks without their numbers (`Block::shape`).
    pub fn shape(&self) -> Vec<u64> {
        self.blocks().iter().map(Block::shape).collect()
    }

    /// Of programs of one shape, the one with every number the median of theirs (the upper of two
    /// middle ones): what a group of drifting programs holds to. None for none or mixed shapes.
    pub fn median(programs: &[Program]) -> Option<Program> {
        let mut m = *programs.first()?;
        Program::columns(programs, |i, slot, _, xs| *m.blocks[i].number(slot) = xs[xs.len() / 2])?;
        m.summary = Summary::of(&m.blocks, usize::from(m.len));
        Some(m)
    }

    /// How far the numbers of programs of one shape have spread: the mean over its numbers of
    /// the interquartile range as a share of the number's range (0: all alike, about 0.5: as
    /// random values). None for none or mixed shapes.
    pub fn spread(programs: &[Program]) -> Option<f64> {
        let (mut sum, mut count) = (0.0, 0);
        Program::columns(programs, |_, _, spec, xs| {
            let (lo, hi) = (xs[xs.len() / 4], xs[xs.len() * 3 / 4]);
            sum += f64::from(hi - lo) / f64::from(spec.hi - spec.lo).max(1.0);
            count += 1;
        })?;
        (count > 0).then(|| sum / f64::from(count))
    }

    /// Each drifting number of programs of one shape, sorted across them, given to `f` with its
    /// block, slot and spec. None (and no call) for none or mixed shapes.
    fn columns(programs: &[Program], mut f: impl FnMut(usize, usize, ParamSpec, &[u16])) -> Option<()> {
        let first = programs.first()?;
        let shape = first.shape();
        if programs.iter().any(|p| p.shape() != shape) {
            return None;
        }
        let mut xs = Vec::with_capacity(programs.len());
        for i in 0..usize::from(first.len) {
            for (slot, spec) in first.blocks[i].numbers() {
                xs.clear();
                xs.extend(programs.iter().map(|p| p.blocks[i].get(slot)));
                xs.sort_unstable();
                f(i, slot, spec, &xs);
            }
        }
        Some(())
    }

    /// The blocks as text lines, «2. если сытость < 30% → установка: гнать соперников у еды
    /// (соперник мельче в 1,5 раза)». Game UI.
    pub fn describe(&self) -> Vec<String> {
        self.blocks()
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let args = b.args_label();
                let args = if args.is_empty() { String::new() } else { format!(" ({args})") };
                let kind = if b.action.is_setting() { "установка: " } else { "" };
                format!("{}. если {} → {kind}{}{args}", i + 1, b.condition_label(), b.action.label())
            })
            .collect()
    }

    /// A child's drift: a third of the numbers of the program (`PROGRAM_DRIFT_SHARE`; never the
    /// flags and the indices) move by gauss(0, its nudge × `share`), held in range — `share` is
    /// the rule `program_drift` × the parent's `program_mutability` — as its genes drift. One draw
    /// a number, and two more (a gauss) for each that moves; none at all when `share` is 0. Not
    /// counted in `changes`.
    pub fn drift(&mut self, share: f64, rng: &mut Rng) {
        if share <= 0.0 {
            return;
        }
        let len = usize::from(self.len);
        for b in &mut self.blocks[..len] {
            // the slots of `Block::numbers`, in its order, without collecting them
            for slot in 0..SLOTS {
                if let Some(spec) = b.spec(slot).filter(ParamSpec::drifts)
                    && rng.random() < PROGRAM_DRIFT_SHARE
                {
                    let raw = b.number(slot);
                    *raw = spec.moved(*raw, spec.nudge * share, rng);
                }
            }
        }
        self.summary = Summary::of(&self.blocks, len);
    }

    /// The child's program without the other track: no transfer (tests, tools).
    pub fn mutate(&mut self, chance: f64, rng: &mut Rng) {
        self.mutate_with(chance, None, rng);
    }

    /// The child's program: with chance `chance` (the rule × the parent's `program_mutability`)
    /// one mutation, else an exact copy. One draw always; a mutation draws its kind and what it
    /// needs. `other` is the parent's other track, for a transfer. A mutation that cannot apply (a
    /// full program, a single block, nothing to nudge, no test to negate, nothing to copy) or lands
    /// where it was changes nothing and is not counted. No junk is born: a copy is of a live block
    /// that can stay live beside the original (not an unconditional setting, not a block that
    /// always fires), a new or copied deciding block goes where it is reached — no lower than the
    /// block that always fires — and a block added (a copy, a new one, a pair, a transfer) that
    /// would leave a block dead, itself or another, is not added, its draws spent.
    pub fn mutate_with(&mut self, chance: f64, other: Option<&Program>, rng: &mut Rng) {
        if rng.random() >= chance {
            return;
        }
        let before = *self;
        let len = usize::from(self.len);
        let ending = self.ending();
        let u = rng.random();
        let pick = |rng: &mut Rng, n: usize| rng.randint(0, n as i64 - 1) as usize;
        // where a block goes: a setting anywhere, a deciding one above the block that always fires
        // (or anywhere when none does)
        let place = |rng: &mut Rng, block: &Block| match ending {
            Some(end) if !block.action.is_setting() => pick(rng, end + 1),
            _ => pick(rng, len + 1),
        };
        // a live block of a program worth copying: its copy can live beside it
        let copy_of = |rng: &mut Rng, p: &Program| {
            let copyable = |i: usize| {
                let b = &p.blocks[i];
                p.live(i) && !(b.action.is_setting() && b.unconditional()) && !b.always_fires()
            };
            pick_where(rng, usize::from(p.len), copyable).map(|i| p.blocks[i])
        };
        // a live block of the other track: no original stands beside it here, so an unconditional
        // setting or an always-firing block may come too — the no-junk check below turns away a
        // copy that would be dead or kill one
        let transferable = |rng: &mut Rng, p: &Program| {
            pick_where(rng, usize::from(p.len), |i| p.live(i)).map(|i| p.blocks[i])
        };
        let mut acc = 0.0;
        let mut op = MUTATIONS.len() - 1;
        for (k, &(_, share)) in MUTATIONS.iter().enumerate() {
            acc += share;
            if u < acc {
                op = k;
                break;
            }
        }
        // where an adding mutation (a copy, a new block, a pair, a transfer) put its block
        let mut added = None;
        match MUTATIONS[op].0 {
            Mutation::Nudge => {
                // any number of the program: the tests' thresholds (slots 0‒2) and the action's (3‒)
                let blocks = &self.blocks;
                if let Some(k) = pick_where(rng, len * SLOTS, |k| blocks[k / SLOTS].spec(k % SLOTS).is_some())
                {
                    let (i, slot) = (k / SLOTS, k % SLOTS);
                    let spec = self.blocks[i].spec(slot).expect("a number");
                    let raw = self.blocks[i].number(slot);
                    *raw = spec.nudged(*raw, rng);
                }
            }
            Mutation::Condition => {
                let (i, s) = (pick(rng, len), pick(rng, TESTS));
                self.blocks[i].when[s] = random_test(rng);
            }
            Mutation::Negate => {
                // only a test with a condition: «always» is `Toggle`'s, so that a negation does
                // not knock a block out two times in three
                let i = pick(rng, len);
                let when = self.blocks[i].when;
                if let Some(s) = pick_where(rng, TESTS, |s| when[s].cond != Cond::Always) {
                    self.blocks[i].when[s].negate ^= true;
                }
            }
            Mutation::Action => {
                let i = pick(rng, len);
                let action = Action::ALL[pick(rng, Action::ALL.len())];
                let b = &mut self.blocks[i];
                b.args = action.args_from(b.action, &b.args);
                b.action = action;
            }
            Mutation::Swap => {
                if len >= 2 {
                    let i = pick(rng, len - 1);
                    self.blocks.swap(i, i + 1);
                }
            }
            Mutation::Duplicate => {
                if len < MAX_BLOCKS
                    && let Some(block) = copy_of(rng, self)
                {
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Delete => {
                if len > 1 {
                    // a dead block first — switched off, never reached, or a setting hidden by one
                    // of its kind — so that the dead do not pile up; any block when none is dead
                    let i = pick_where(rng, len, |i| !self.live(i)).unwrap_or_else(|| pick(rng, len));
                    self.blocks.copy_within(i + 1..len, i);
                    self.blocks[len - 1] = FILLER;
                    self.len -= 1;
                }
            }
            Mutation::Insert => {
                if len < MAX_BLOCKS {
                    let test = random_test(rng);
                    let action = Action::ALL[pick(rng, Action::ALL.len())];
                    let block = Block::when(test, action);
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Pair => {
                // a memory in one step: a setting that switches a mode on by a random test, and
                // that mode as a test on another live block with a free «always» — apart they
                // would be two mutations with nothing to select between them. Not the block that
                // always fires: a condition on it would bring the dead tail back to life. No such
                // reader: no memory either.
                if len < MAX_BLOCKS {
                    let mode = MODE.random(rng);
                    let setting = Block::when(random_test(rng), Action::Mode).with(0, mode);
                    let at = pick(rng, len + 1);
                    self.insert(at, setting);
                    let with = Summary::of(&self.blocks, len + 1);
                    let blocks = &self.blocks;
                    let reader = |i: usize| {
                        let b = &blocks[i];
                        i != at
                            && with.live & (1 << i) != 0
                            && !b.always_fires()
                            && b.when.iter().any(|t| t.always())
                    };
                    match pick_where(rng, len + 1, reader) {
                        Some(i) => {
                            let b = &mut self.blocks[i];
                            let slot = b.when.iter().position(|t| t.always()).expect("a free slot");
                            b.when[slot] = Test::at(Cond::Mode, mode);
                            added = Some(at);
                        }
                        None => *self = before,
                    }
                }
            }
            Mutation::Transfer => {
                // a live block of its other track — the adult's into the juvenile's or back —
                // copied to where it is reached: what one stage found the other may try
                if let Some(other) = other
                    && len < MAX_BLOCKS
                    && let Some(block) = transferable(rng, other)
                {
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Toggle => {
                // off: every «never» back to «always»; on: the first «always» to «never» (a block
                // of three real tests has none and stays on)
                let i = pick(rng, len);
                let b = &mut self.blocks[i];
                if b.off() {
                    for t in &mut b.when {
                        if t.never() {
                            t.negate = false;
                        }
                    }
                } else if let Some(t) = b.when.iter_mut().find(|t| t.always()) {
                    t.negate = true;
                }
            }
        }
        if (self.blocks, self.len) == (before.blocks, before.len) {
            return;
        }
        self.summary = Summary::of(&self.blocks, usize::from(self.len));
        if added.is_some_and(|at| self.kills(&before, at)) {
            // the added block would be born dead, or would kill one: not added
            *self = before;
            return;
        }
        self.changes = self.changes.saturating_add(1);
    }

    /// With a block added at `at` to `before`: whether it was born dead or left dead a block that
    /// lived before. Block by block, not by their count: a pair that kills one block while its
    /// reader brings another back to life still kills one.
    fn kills(&self, before: &Program, at: usize) -> bool {
        let was = |j: usize| match j.cmp(&at) {
            std::cmp::Ordering::Less => Some(j),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(j - 1),
        };
        !self.live(at)
            || (0..usize::from(self.len)).any(|j| was(j).is_some_and(|i| before.live(i) && !self.live(j)))
    }

    fn insert(&mut self, at: usize, block: Block) {
        let len = usize::from(self.len);
        self.blocks.copy_within(at..len, at + 1);
        self.blocks[at] = block;
        self.len += 1;
    }
}

/// An index below `n` where `pred` holds, picked uniformly: counts them, draws one, walks again —
/// no allocation. None, and no draw, when none holds.
fn pick_where(rng: &mut Rng, n: usize, pred: impl Fn(usize) -> bool) -> Option<usize> {
    let count = (0..n).filter(|&i| pred(i)).count();
    if count == 0 {
        return None;
    }
    let k = rng.randint(0, count as i64 - 1) as usize;
    (0..n).filter(|&i| pred(i)).nth(k)
}

/// A random test: any condition, a threshold drawn in range for one that has it.
fn random_test(rng: &mut Rng) -> Test {
    let cond = Cond::ALL[rng.randint(0, Cond::ALL.len() as i64 - 1) as usize];
    let param = cond.param().map_or(0, |p| p.random(rng));
    Test { cond, negate: false, param }
}

/// The kinds of a program mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    /// A number — a test's threshold or an action's parameter — moves (a flag flips).
    Nudge,
    /// A test becomes another random one.
    Condition,
    /// A test with a condition turns to its opposite («always» is left to `Toggle`).
    Negate,
    /// A block's action becomes a random one; a parameter of the same label and unit keeps its
    /// number, the others take their bases (`Action::args_from`).
    Action,
    /// A block trades places with the next one.
    Swap,
    /// A copy of a live block goes where it is reached.
    Duplicate,
    /// A block goes, a dead one first.
    Delete,
    /// A new block of one random test goes where it is reached.
    Insert,
    /// A block is switched off (its first «always» becomes «never») or on again.
    Toggle,
    /// A memory: a setting switching a mode on by a random test, and that mode as a test on
    /// another block.
    Pair,
    /// A live block of the other track copied in.
    Transfer,
}

/// The mutation kinds and their shares of the mutations; the shares sum to 1. Small changes
/// (a number, the order) are the most common, so a working program usually stays working. A
/// deletion is as likely as the kinds that add a block together (a copy, an insertion, a pair and
/// a transfer, which a child always has the other track for), so programs do not grow by
/// themselves; switching a block off is rare and its own kind.
const MUTATIONS: [(Mutation, f64); 11] = [
    (Mutation::Nudge, 0.25),
    (Mutation::Condition, 0.12),
    (Mutation::Negate, 0.08),
    (Mutation::Action, 0.08),
    (Mutation::Swap, 0.12),
    (Mutation::Duplicate, 0.06),
    (Mutation::Delete, 0.16),
    (Mutation::Insert, 0.05),
    (Mutation::Toggle, 0.03),
    (Mutation::Pair, 0.03),
    (Mutation::Transfer, 0.02),
];

/// A creature's two programs (`JUVENILE`, `ADULT`), shared rather than copied: most children
/// inherit them unchanged (every exact copy does), so a creature holds a pointer, not a kilobyte,
/// and the phases that walk the creatures stay compact. Read as an array; replaced whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Programs(Arc<[Program; 2]>);

impl Programs {
    pub fn new(programs: [Program; 2]) -> Programs {
        Programs(Arc::new(programs))
    }

    /// Both tracks the same program: a founder's template.
    pub fn both(program: Program) -> Programs {
        Programs::new([program; 2])
    }
}

impl std::ops::Deref for Programs {
    type Target = [Program; 2];

    fn deref(&self) -> &[Program; 2] {
        &self.0
    }
}

impl From<[Program; 2]> for Programs {
    fn from(programs: [Program; 2]) -> Programs {
        Programs::new(programs)
    }
}

impl PartialEq<[Program; 2]> for Programs {
    fn eq(&self, other: &[Program; 2]) -> bool {
        *self.0 == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_list_every_variant_in_order() {
        for (i, c) in Cond::ALL.iter().enumerate() {
            assert_eq!(*c as usize, i);
        }
        for (i, a) in Action::ALL.iter().enumerate() {
            assert_eq!(*a as usize, i);
            let params = a.params();
            assert!(params.len() <= MAX_ARGS, "{a:?}");
            for p in params {
                assert!(p.lo <= p.base && p.base <= p.hi, "{a:?}: {p:?}");
                assert!(p.hi < 1 << 12, "{a:?}: {p:?} fits the block's code");
                assert!(!p.drifts() || p.nudge >= 1.0, "{a:?}: {p:?} moves");
                assert!(p.drifts() || p.hi < 8, "{a:?}: {p:?} fits the block's shape");
            }
        }
        for c in Cond::ALL {
            if let Some(p) = c.param() {
                assert!(p.lo <= p.base && p.base <= p.hi, "{c:?}");
                assert!(!p.drifts() || p.nudge >= 1.0, "{c:?} moves");
                assert!(p.hi < 1 << 12 && (p.drifts() || p.hi < 8), "{c:?} fits the code and the shape");
            }
        }
        assert!(Cond::ALL.len() <= 32 && Action::ALL.len() <= 32, "5 bits each in the code");
        const { assert!(MAX_ARGS <= 10 && MAX_BLOCKS <= 32) };
        let total: f64 = MUTATIONS.iter().map(|m| m.1).sum();
        assert!((total - 1.0).abs() < 1e-12, "the shares sum to 1: {total}");
    }

    /// The templates carry the old behaviour genes' bases; the lurker differs only in its paces.
    #[test]
    fn the_templates_carry_the_old_bases() {
        let (s, l) = (Program::STANDARD, Program::LURKER);
        assert_eq!(s.reachable(), s.blocks().len(), "every block of a template is reached");
        assert_eq!(Program::template(Strategy::Standard), s);
        assert_eq!(Program::template(Strategy::Lurker), l);
        let differ: Vec<usize> = (0..s.blocks().len()).filter(|&i| s.blocks()[i] != l.blocks()[i]).collect();
        assert_eq!(differ.len(), 1, "the lurker differs only in its wander's pace: {differ:?}");
        assert_eq!(s.hunt_ratio(), Some(1.5));
        assert_eq!(s.defence(), Some((1.5, 0.5)));
        assert_eq!(s.wander_reach(), 2.0);
        assert!((s.threat_range() - 0.33).abs() < 1e-12);
        assert_eq!((s.home_layer(), s.shoots()), ((0.05, 1.0), false));
        let text = s.describe();
        assert_eq!(text.len(), 20);
        // what the genes and the world did: the settings on top
        assert_eq!(
            text[..6],
            [
                "1. если всегда → установка: слой (сверху 5%, снизу 100%)",
                "2. если всегда → установка: плавный ход (держит курс 30 тиков, поворот до 69°)",
                "3. если всегда → установка: делиться (с бака 70%, потомку 40%)",
                "4. если всегда → установка: лечиться (при баке больше 50%, без ударов 60 тиков)",
                "5. если всегда → установка: есть на ходу (растения, падаль, пока сытость не больше 100%)",
                "6. если всегда → установка: щадить детей (пока ребёнок не вырос до 100%)",
            ]
        );
        // the founders' memory: hunger, an alarm, a full tank
        assert_eq!(text[6], "7. если сытость < 30% → установка: включить режим (режим 3, на 60 тиков)");
        assert_eq!(text[7], "8. если включён режим 3 → установка: есть и чужую пищу");
        assert_eq!(
            text[9],
            "10. если охотник ближе 33% зрения → установка: включить режим (режим 1, на 61 тик)"
        );
        assert_eq!(
            text[10],
            "11. если угроза ближе 16% зрения → установка: включить режим (режим 1, на 61 тик)"
        );
        assert_eq!(text[11], "12. если сытость ≥ 95% → установка: включить режим (режим 2, на 200 тиков)");
        assert_eq!(text[12], "13. если здоровье ≥ 50% → дать сдачи (враг крупнее не более чем в 1,5 раза)");
        assert_eq!(
            text[13],
            "14. если включён режим 1 → убегать (бежать ещё 60 тиков, рывком, снова пугается угрозы ближе \
             33% зрения, темп 100%, прямо)"
        );
        assert_eq!(
            text[14],
            "15. если здоровье ≥ 60% → защищать детёныша (ребёнок ближе 50% зрения, при баке больше 50%, \
             не дольше 90 тиков, потом пауза 60 тиков, ребёнка ударили за 30 тиков)"
        );
        assert_eq!(
            text[15],
            "16. если всегда → охотиться (добыча мельче в 1,5 раза, осторожность 100%, терпение 30 тиков, \
             только если выгоднее, рывком, брошенную не трогать 180 тиков, добыча не дальше 100% зрения, \
             темп погони 100%)"
        );
        assert_eq!(
            text[16],
            "17. если включён режим 2 и сытость ≥ 85% → отдыхать (отдых 90 тиков, пауза 180 тиков)"
        );
        assert_eq!(
            text[18],
            "19. если всегда → к растению (темп 100%, держится выбранного, не самое выгодное)"
        );
        assert_eq!(
            *text.last().unwrap(),
            format!("{}. если всегда → бродить (темп 100%, цели до 200% зрения)", text.len())
        );
        assert!(l.describe().last().unwrap().contains("(темп 33%"));
    }

    #[test]
    fn blocks_after_one_that_always_fires_are_unreachable() {
        let p = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant),
            Block::does(Action::Ambush),
            Block::does(Action::Wander),
            Block::does(Action::EatForeign),
        ]);
        assert_eq!(p.reachable(), 2);
        assert_eq!(
            p.describe()[0],
            "1. если сытость < 40% → к растению (темп 100%, держится выбранного, не самое выгодное)"
        );
        assert!(!p.live(2), "a deciding block after one that always fires is never reached");
        assert!(p.live(3), "a setting applies wherever it stands");
        // a negated «always» is «never»: the block is off and does not end the program
        let off = Program::of(&[Block::when(Test::ALWAYS.not(), Action::Wander), Block::does(Action::Hunt)]);
        assert_eq!(off.reachable(), 2);
        assert_eq!(off.blocks()[0].condition_label(), "никогда");
        assert!(!off.live(0) && off.live(1));
        assert_eq!(off.hunt_ratio(), Some(1.5));
        // a setting never ends the program
        let setting = Program::of(&[Block::does(Action::EatForeign), Block::does(Action::Wander)]);
        assert_eq!(setting.reachable(), 2);
        // no hunt, no defence, no wander: nothing to fear, nothing to fight, the base wander
        let idle = Program::of(&[Block::does(Action::Ambush)]);
        assert_eq!((idle.hunt_ratio(), idle.defence(), idle.wander_reach()), (None, None, 2.0));
    }

    fn check(p: &Program) {
        assert!((1..=MAX_BLOCKS).contains(&p.blocks().len()), "{} blocks", p.blocks().len());
        for b in p.blocks() {
            for t in b.when {
                match t.cond.param() {
                    Some(s) => assert!((s.lo..=s.hi).contains(&t.param), "{t:?}"),
                    None => assert_eq!(t.param, 0, "{t:?}"),
                }
            }
            let params = b.action.params();
            for (i, a) in b.args.iter().enumerate() {
                match params.get(i) {
                    Some(s) => assert!((s.lo..=s.hi).contains(a), "{b:?}"),
                    None => assert_eq!(*a, 0, "{b:?}"),
                }
            }
        }
        assert!(p.blocks[usize::from(p.len)..].iter().all(|b| *b == FILLER), "the tail stays the filler");
        assert_eq!(p.summary, Summary::of(&p.blocks, usize::from(p.len)), "the summary is up to date");
    }

    /// Mutations keep a program valid (1 to `MAX_BLOCKS` blocks, every number in its range, the
    /// summary current), count only what changed it, and a long line of mutants grows and shrinks.
    #[test]
    fn mutations_keep_a_program_valid_and_change_it() {
        let mut rng = Rng::new(5);
        let mut p = Program::STANDARD;
        let start = p.blocks().len();
        let (mut longest, mut shortest, mut changed) = (0, MAX_BLOCKS, 0);
        for _ in 0..20_000 {
            let before = p;
            p.mutate(1.0, &mut rng);
            check(&p);
            let differs = !p.same_blocks(&before);
            assert_eq!(p.changes, before.changes.saturating_add(differs as u16), "only a change counts");
            changed += differs as usize;
            longest = longest.max(p.blocks().len());
            shortest = shortest.min(p.blocks().len());
        }
        assert!(longest > start && shortest < start, "it grows and shrinks: {shortest}‒{longest}");
        // a nudge by less than half a unit, a swap of equal blocks, the same random pick or a
        // copy into a full program change nothing; most mutations change something
        assert!(changed > 12_000, "changed {changed} of 20 000");
    }

    /// With chance `c` about that share of children mutate; a copy draws exactly one number.
    #[test]
    fn a_program_mutates_with_its_chance() {
        let mut rng = Rng::new(9);
        let n = 100_000;
        let mutated = (0..n)
            .filter(|_| {
                let mut p = Program::LURKER;
                p.mutate(0.05, &mut rng);
                p.changes == 1
            })
            .count();
        // a few mutations land where they were and are not counted
        assert!((4300..=5300).contains(&mutated), "5% expected: {mutated}");
        let mut a = Rng::new(3);
        let mut b = a.clone();
        let mut p = Program::STANDARD;
        p.mutate(0.0, &mut a);
        b.random();
        assert_eq!((p, a), (Program::STANDARD, b), "no mutation: one draw, the same program");
    }

    #[test]
    fn each_kind_of_mutation_does_its_change() {
        let mut rng = Rng::new(17);
        let (mut longer, mut shorter, mut negated, mut switched_off, mut reordered, mut nudged, mut flipped) =
            (0, 0, 0, 0, 0, 0, 0);
        let two = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 50), Action::Flee),
            Block::does(Action::Wander).with(0, 60),
        ]);
        for _ in 0..8000 {
            let mut p = two;
            p.mutate(1.0, &mut rng);
            let (a, b) = (two.blocks(), p.blocks());
            longer += (b.len() > a.len()) as usize;
            shorter += (b.len() < a.len()) as usize;
            if b.len() != a.len() {
                continue;
            }
            // a negation turns a real test; only a toggle touches an «always»
            switched_off += b.iter().any(Block::off) as usize;
            negated += (b[0].when[0].cond == Cond::Fullness && b[0].when[0].negate) as usize;
            reordered += (b[0] == a[1] && b[1] == a[0]) as usize;
            let same_shape = b[0].action == a[0].action && b[1].action == a[1].action;
            nudged += (same_shape
                && b[0].when[0].cond == Cond::Fullness
                && (b[0].when[0].param != 50 || b[0].args != a[0].args || b[1].args != a[1].args))
                as usize;
            flipped += (same_shape && b[0].args[1] != a[0].args[1]) as usize;
        }
        assert!(longer > 600 && shorter > 600, "insertions, copies and pairs {longer}, deletions {shorter}");
        assert!(negated > 220 && switched_off > 150, "negated {negated}, switched off {switched_off}");
        assert!(reordered > 700, "{reordered}");
        assert!(nudged > 1600 && flipped > 180, "nudged {nudged}, a flag flipped {flipped}");
    }

    /// A mutation moves an index to another of its values, never where it was; the drift leaves it.
    #[test]
    fn an_index_is_picked_anew_and_does_not_drift() {
        let mut rng = Rng::new(8);
        let mut seen = [0; MODES + 1];
        for _ in 0..4000 {
            let k = MODE.nudged(2, &mut rng);
            assert_ne!(k, 2);
            seen[usize::from(k)] += 1;
        }
        assert!(seen[1] > 1000 && seen[3] > 1000 && seen[4] > 1000, "{seen:?}");
        let mut p = Program::of(&[
            Block::when(Test::at(Cond::Mode, 3), Action::Ambush),
            Block::does(Action::Mode).with(0, 4),
            Block::does(Action::Wander),
        ]);
        for _ in 0..100 {
            p.drift(3.0, &mut rng);
        }
        assert_eq!((p.blocks()[0].when[0].param, p.blocks()[1].args[0]), (3, 4), "the drift leaves indices");
    }

    /// The drift moves a third of the numbers, never the shape or a flag, keeps them in range, is
    /// not counted as a mutation, and at 0 draws nothing.
    #[test]
    fn the_drift_moves_numbers_not_the_shape() {
        let mut rng = Rng::new(21);
        let mut p = Program::STANDARD;
        let (mut moved, mut numbers, mut total) = (0, 0, 0);
        for _ in 0..200 {
            let before = p;
            p.drift(1.0, &mut rng);
            check(&p);
            assert_eq!(p.shape(), Program::STANDARD.shape());
            assert_eq!(p.changes, 0);
            moved += !p.same_blocks(&before) as usize;
            for i in 0..p.blocks().len() {
                for (slot, _) in p.blocks[i].numbers() {
                    total += 1;
                    numbers += (p.blocks[i].get(slot) != before.blocks[i].get(slot)) as usize;
                }
            }
        }
        assert!(moved > 190, "almost every drift moves some number: {moved}");
        let share = numbers as f64 / total as f64;
        assert!((0.2..0.4).contains(&share), "a third of the numbers a drift, less the rounding: {share}");
        let hunt = p.blocks().iter().find(|b| b.action == Action::Hunt).unwrap();
        assert!(hunt.flag(3) && hunt.flag(4), "flags stay");
        assert_ne!(hunt.args[0], 150, "the ratio walked away in 200 generations");
        let (mut a, b) = (Rng::new(4), Rng::new(4));
        let mut still = Program::STANDARD;
        still.drift(0.0, &mut a);
        assert_eq!((still, a), (Program::STANDARD, b), "no drift, no draws");
    }

    /// The median of a drifting group keeps its shape and takes each number's middle.
    #[test]
    fn the_median_of_a_group_takes_each_numbers_middle() {
        let ratios = [120, 150, 190];
        let group: Vec<Program> =
            ratios.iter().map(|&r| Program::STANDARD.tuned(Action::Hunt, |b| b.args[0] = r)).collect();
        let m = Program::median(&group).unwrap();
        assert_eq!(m.shape(), Program::STANDARD.shape());
        assert_eq!(m.hunt_ratio(), Some(1.5));
        assert_eq!(Program::median(&[group[0], Program::of(&[Block::does(Action::Hunt)])]), None);
        assert_eq!(Program::median(&[]), None);
    }

    /// A pair that kills one block while its reader revives another leaves as many dead blocks as
    /// before, yet it killed one: it is no junk-free addition. The count alone let it through.
    #[test]
    fn an_addition_that_kills_one_block_and_revives_another_still_kills() {
        let conditional_mode = Block::when(Test::at(Cond::Fullness, 50), Action::Mode).with(0, 1);
        let before = Program::of(&[
            conditional_mode,
            Block::does(Action::Divide),
            Block::does(Action::Divide).with(0, 90),
            Block::does(Action::Wander),
        ]);
        assert!(before.live(0) && before.live(1) && !before.live(2), "the second division hidden");
        // the pair: an unconditional mode 1 on top, the first division its reader
        let after = Program::of(&[
            Block::does(Action::Mode).with(0, 1),
            conditional_mode,
            Block::when(Test::at(Cond::Mode, 1), Action::Divide),
            Block::does(Action::Divide).with(0, 90),
            Block::does(Action::Wander),
        ]);
        assert_eq!(after.dead(), before.dead(), "as many dead as before");
        assert!(!after.live(1) && after.live(3), "one killed, one revived");
        assert!(after.kills(&before, 0));
        // an addition that kills nothing passes
        let harmless = Program::of(&[
            conditional_mode,
            Block::does(Action::Divide),
            Block::does(Action::Divide).with(0, 90),
            Block::when(Test::at(Cond::Fullness, 20), Action::Rest),
            Block::does(Action::Wander),
        ]);
        assert!(!harmless.kills(&before, 3));
    }

    /// A transfer copies any live block of the other track, an unconditional setting or an
    /// always-firing block too: a track that lost them can get them back from the other one.
    #[test]
    fn a_transfer_brings_a_setting_and_an_ending_the_track_lacks() {
        let mut rng = Rng::new(5);
        let juvenile = Program::of(&[Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant)]);
        // numbers no other mutation would give, so only a transfer brings these blocks
        let adult =
            Program::of(&[Block::does(Action::Divide).with(0, 77), Block::does(Action::Wander).with(0, 37)]);
        let (mut setting, mut ending) = (false, false);
        for _ in 0..20_000 {
            let mut p = juvenile;
            p.mutate_with(1.0, Some(&adult), &mut rng);
            let b = p.blocks();
            setting |= b.contains(&adult.blocks()[0]);
            ending |= b.len() == 2 && b[1] == adult.blocks()[1];
        }
        assert!(setting && ending, "transferred: the setting {setting}, the wander {ending}");
    }

    /// The block that always fires ends a program whether it is the last block or settings follow
    /// it (the window then draws «стоит» as never reached); a program without one may stand.
    #[test]
    fn the_ending_is_the_always_firing_block_wherever_it_stands() {
        let eat = Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant);
        let last = Program::of(&[eat, Block::does(Action::Wander)]);
        let then_setting = Program::of(&[eat, Block::does(Action::Wander), Block::does(Action::Divide)]);
        let may_stand = Program::of(&[eat, Block::does(Action::Divide)]);
        assert_eq!((last.ending(), then_setting.ending(), may_stand.ending()), (Some(1), Some(1), None));
        assert_eq!(
            Program::STANDARD.ending().map(|i| Program::STANDARD.blocks()[i].action),
            Some(Action::Wander)
        );
    }

    /// Structure: no block added (a copy, a new one, a pair, a transfer) is born dead or kills
    /// one, a copy is of a live block, a pair brings a mode and a live reader of it together, a
    /// transfer copies a live block of the other track, and a replaced action keeps the pace.
    #[test]
    fn structural_mutations_build_reachable_working_programs() {
        let mut rng = Rng::new(11);
        // an off block, a decider, an always-firing wander, a dead decider after it
        let base = Program::of(&[
            Block::when(Test::ALWAYS.not(), Action::Ambush),
            Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant).with(0, 55),
            Block::does(Action::Wander),
            Block::does(Action::Hunt).with(1, 200),
        ]);
        assert_eq!((base.dead(), base.ending()), (2, Some(2)));
        let other = Program::of(&[
            Block::when(Test::ALWAYS.not(), Action::Torpor),
            Block::does(Action::EatCorpse).with(1, 70),
        ]);
        let (mut pairs, mut transfers, mut kept_pace) = (0, 0, 0);
        for _ in 0..6000 {
            let mut p = base;
            p.mutate_with(1.0, Some(&other), &mut rng);
            check(&p);
            let (a, b) = (base.blocks(), p.blocks());
            if b.len() == a.len() + 1 {
                assert!(p.dead() <= base.dead(), "an added block is no junk: {b:?}");
                assert_eq!(p.ending().map(|i| b[i]), Some(a[2]), "the wander still ends it: {b:?}");
                let at = (0..b.len()).find(|&i| a.get(i) != Some(&b[i])).unwrap();
                let new = b[at];
                if let Some(j) = a.iter().position(|x| *x == new) {
                    assert!(base.live(j), "a copy is of a live block: {j}");
                }
                assert_ne!(new, other.blocks()[0], "a transfer copies no dead block");
                transfers += (new == other.blocks()[1]) as usize;
                if let Some(setting) = b.iter().position(|x| x.action == Action::Mode) {
                    let mode = b[setting].args[0];
                    let tested = |x: &Block| x.when.iter().any(|t| t.cond == Cond::Mode && t.param == mode);
                    if let Some(r) = (0..b.len()).find(|&i| i != setting && tested(&b[i])) {
                        assert!(p.live(r) && !b[r].always_fires(), "a pair's reader lives: {b:?}");
                        pairs += 1;
                    }
                }
            } else if b.len() == a.len() && b[1].action != a[1].action && b[0] == a[0] && b[2..] == a[2..] {
                let pace = b[1].action.params().iter().position(|p| p.label == PACE.label);
                if let Some(k) = pace {
                    assert_eq!(b[1].args[k], 55, "the pace goes over to {:?}", b[1].action);
                    kept_pace += 1;
                }
            }
        }
        assert!(pairs > 100 && transfers > 60 && kept_pace > 5, "{pairs} {transfers} {kept_pace}");
        // an unconditional setting of a kind already set is dead, and a copy of it is not made
        let set = Program::of(&[
            Block::does(Action::Divide),
            Block::does(Action::Divide),
            Block::does(Action::Wander),
        ]);
        assert!(set.live(0) && !set.live(1) && set.dead() == 1);
        let conditional = Program::of(&[
            Block::when(Test::at(Cond::Fullness, 50), Action::Divide),
            Block::does(Action::Divide),
            Block::does(Action::Wander),
        ]);
        assert!(conditional.live(0) && conditional.live(1), "a conditional one hides nothing");
        let mut rng = Rng::new(12);
        let single = Program::of(&[Block::does(Action::Divide), Block::does(Action::Wander)]);
        for _ in 0..500 {
            let mut p = single;
            p.mutate_with(1.0, Some(&single), &mut rng);
            if p.blocks().len() > 2 {
                assert_eq!(p.dead(), 0, "{:?}", p.blocks());
            }
        }
    }

    /// The spread of a group's numbers: none for copies, about a half for random values.
    #[test]
    fn the_spread_of_a_group_tells_copies_from_random_numbers() {
        assert_eq!(Program::spread(&[Program::STANDARD; 5]), Some(0.0));
        let mut rng = Rng::new(2);
        let random: Vec<Program> = (0..400)
            .map(|_| {
                let mut p = Program::of(&[Block::does(Action::Hunt), Block::does(Action::Wander)]);
                for b in &mut p.blocks[..2] {
                    for (slot, spec) in b.numbers().collect::<Vec<_>>() {
                        *b.number(slot) = spec.random(&mut rng);
                    }
                }
                p
            })
            .collect();
        let spread = Program::spread(&random).unwrap();
        assert!((0.4..0.6).contains(&spread), "{spread}");
        assert_eq!(Program::spread(&[Program::STANDARD, Program::of(&[Block::does(Action::Hunt)])]), None);
        assert_eq!(Program::spread(&[]), None);
    }

    #[test]
    fn codes_tell_blocks_apart() {
        let mut codes: Vec<[u64; 3]> = Vec::new();
        for c in Cond::ALL {
            for a in Action::ALL {
                for negate in [false, true] {
                    let t = Test { cond: c, negate, param: c.param().map_or(0, |p| p.hi) };
                    let a_ = Test::ALWAYS;
                    codes.push(Block::when3(t, a_, a_, a).code());
                    codes.push(Block::when3(a_, t, a_, a).code());
                    codes.push(Block::when3(a_, a_, t, a).code());
                }
            }
        }
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        // a test in any slot with «always» in the others is the same block only for «always»
        assert_eq!(codes.len(), n - 2 * Action::ALL.len());
        let b = Block::does(Action::Hunt);
        for i in 0..MAX_ARGS {
            assert_ne!(b.code(), b.with(i, b.args[i] + 1).code(), "parameter {i} counts");
        }
        assert_eq!(
            b.shape(),
            b.with(5, 0).with(0, 300).with(7, 50).shape(),
            "numbers are no part of the shape"
        );
        assert_ne!(b.shape(), b.with(4, 0).shape(), "flags are");
        let mode = Block::does(Action::Mode);
        assert_ne!(mode.shape(), mode.with(0, 2).shape(), "a mode's number is");
        assert_eq!(mode.shape(), mode.with(1, 300).shape());
        let tested = |k| Block::when(Test::at(Cond::Mode, k), Action::Ambush).shape();
        assert_ne!(tested(1), tested(2), "so is a tested one's");
    }

    /// Of the settings of one kind the first that applies wins; each mode is a kind of its own.
    #[test]
    fn settings_of_a_kind_and_modes_have_their_kinds() {
        let rival = Block::does(Action::Rival);
        assert_eq!(rival.setting_kind(), rival.with(0, 300).setting_kind());
        assert_ne!(rival.setting_kind(), Block::does(Action::Reach).setting_kind());
        let mode = Block::does(Action::Mode);
        let kinds: Vec<u64> = (1..=MODES as u16).map(|k| mode.with(0, k).setting_kind()).collect();
        for (i, a) in kinds.iter().enumerate() {
            assert_eq!(a.count_ones(), 1);
            assert!(kinds[i + 1..].iter().all(|b| a & b == 0), "modes differ in kind");
            assert!(Action::ALL.iter().all(|&x| Block::does(x).setting_kind() & a == 0 || x == Action::Mode));
        }
        assert_eq!(mode.setting_kind(), mode.with(1, 5).setting_kind(), "the time is no part of it");
    }

    #[test]
    fn parameters_read_and_show_in_their_units() {
        let hunt = Block::does(Action::Hunt);
        assert_eq!(hunt.arg(0), 1.5);
        assert_eq!(hunt.arg(1), 1.0);
        assert_eq!(hunt.arg(2), 30.0);
        assert_eq!(hunt.arg(5), 180.0);
        assert!(hunt.flag(3) && hunt.flag(4));
        assert_eq!(Action::Hunt.params()[4].show(0), "не рывком");
        assert_eq!(Test::at(Cond::ThreatNear, 17).label(), "угроза ближе 17% зрения");
        assert_eq!(Test::at(Cond::ThreatNear, 17).value(), 0.17);
        assert_eq!(Test::is(Cond::Struck).label(), "его ударили за 1 тик");
        assert_eq!(Test::at(Cond::Struck, 22).not().label(), "его не били за 22 тика");
        let words: Vec<&str> = [1, 2, 5, 11, 12, 21, 24, 111].into_iter().map(ticks_word).collect();
        assert_eq!(words, ["тик", "тика", "тиков", "тиков", "тиков", "тик", "тика", "тиков"]);
        // the new units: a tilt, an angle, an index
        let tilt = Action::Flee.params()[4];
        assert_eq!([tilt.show(130), tilt.show(70), tilt.show(100)], ["вниз 30%", "вверх 30%", "прямо"]);
        assert_eq!((tilt.value(130), tilt.value(100)), (0.3, 0.0));
        let angle =
            ParamSpec { label: "поворот", unit: Unit::Angle, lo: 0, hi: 314, base: 120, nudge: 10.0 };
        assert_eq!((angle.show(120), angle.value(120)), ("поворот 69°".to_string(), 1.2));
        assert_eq!(Test::at(Cond::Mode, 2).label(), "включён режим 2");
        assert_eq!(Test::at(Cond::Mode, 3).not().label(), "выключен режим 3");
        assert_eq!(Test::at(Cond::Age, 70).not().label(), "возраст < 70%");
        assert_eq!(Block::does(Action::Mode).args_label(), "режим 1, на 60 тиков");
        let three = Block::when3(
            Test::at(Cond::Fullness, 40).not(),
            Test::is(Cond::PlantSeen),
            Test::is(Cond::Winded).not(),
            Action::EatPlant,
        );
        assert_eq!(three.condition_label(), "сытость < 40% и видит растение и не запыхался");
    }

    /// Programs are shared, compared by content, read as an array.
    #[test]
    fn programs_are_shared_and_compared_by_content() {
        let a = Programs::both(Program::LURKER);
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.0, &b.0), "a copy shares the programs");
        assert_eq!(a, Programs::from([Program::LURKER; 2]));
        assert_eq!(a, [Program::LURKER; 2]);
        assert_eq!(a[ADULT].blocks().len(), Program::LURKER.blocks().len());
        assert!(std::mem::size_of::<Programs>() <= 8);
    }
}
