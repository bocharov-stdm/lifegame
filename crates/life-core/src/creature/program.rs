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
//! an exact copy, every number drifts a little, like its genes (`Program::drift`, the rule
//! `program_drift` × mutability), and with the rule `program_mutation` × mutability one mutation
//! changes each — a number, a test, an action, the order, a copy, a deletion or a new block
//! (`Program::mutate`). Founders start from the template of their `strategy` gene
//! (`Program::template`), which carries the values of the behaviour genes the programs replaced.
//!
//! A program is behaviour: it costs no upkeep and creates no energy — an action only chooses where
//! to step or how to stand; `Creature::act` pays. Its catch is its consequences: a program that
//! never flees is eaten, one that never looks for food starves.
//!
//! **`Cond`, `Action` and their parameter tables are append-only**, like the gene tables: their
//! numbers are drawn by the mutation and hashed by the golden test. Labels are game UI (Russian).

use super::strategy::Strategy;
use crate::config::{
    CHASE_GIVE_UP_TICKS, CHASE_PATIENCE, FLEE_SIGHT_SHARE, FLEE_TICKS, PROGRAM_NUDGE_POINTS,
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

const PACE: ParamSpec = ParamSpec::percent("ход", 10, 100, 100);
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
    ParamSpec::percent("ход погони", 10, 100, 100),
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
                 хватает энергии на удар. Охотник знает это правило жертвы и боится её ударов, только \
                 если сам в него попадает."
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
                 когда растение или падаль дают не меньше. Ход погони — доля скорости."
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
            Action::Ambush => "Стоит на месте: за ход не платит, только за тело и глаза.",
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
                "Установка на этот тик: ест и чужую пищу — падальщик свежее мясо, мясоед гниль и кости. \
                 Без неё — только свою."
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
                 Без неё не делится вовсе."
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
                 Охотники считают такого родителя защитником его детей."
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
        self.when[0].always() && self.when[1].always() && self.when[2].always() && self.action.never_fails()
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

    /// The numbers of the block the drift moves: each test's threshold and each parameter but the
    /// flags and indices, as (slot, its spec); slots 0‒2 are the tests, 3‒ the parameters.
    fn numbers(&self) -> impl Iterator<Item = (usize, ParamSpec)> + '_ {
        let tests =
            (0..TESTS).filter_map(|s| self.when[s].cond.param().filter(ParamSpec::drifts).map(|p| (s, p)));
        let args = self.action.params().iter().enumerate().filter(|(_, p)| p.drifts());
        tests.chain(args.map(|(i, p)| (TESTS + i, *p)))
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
    /// The farthest a threat test looks, % of sight.
    threat: u16,
    /// Its most permissive hunt's ratio; 0: it never hunts.
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
    /// It has a live block defending its children: hunters count it as its child's ally.
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
        let mut s = Summary {
            reachable: reachable as u8,
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
            if !b.off() && (b.action.is_setting() || i < reachable) {
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
                        let mut t = 0;
                        while t < TESTS {
                            let test = b.when[t];
                            if !test.negate
                                && matches!(test.cond, Cond::Health)
                                && test.param > s.fight_health
                            {
                                s.fight_health = test.param;
                            }
                            t += 1;
                        }
                    }
                    Action::Wander if !wanders => {
                        wanders = true;
                        s.wander = b.args[1];
                    }
                    Action::Layer
                        if !layered && b.when[0].always() && b.when[1].always() && b.when[2].always() =>
                    {
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
        // below the old picky and rivalry (30%) it eats the other niche's food and fights for its own
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::EatForeign),
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::Rival),
        // the old bravery 50%: it defends itself down to half its health
        Block::when(Test::at(Cond::Health, 50), Action::FightBack),
        Block::when(Test::at(Cond::HunterNear, FLEE_PCT), Action::Flee),
        Block::when(Test::at(Cond::ThreatNear, calm), Action::Flee),
        Block::when(Test::is(Cond::Fleeing), Action::Flee),
        // the old parent's cover, while it stood firm: 10 points above its fight-back threshold
        Block::when(Test::at(Cond::Health, 60), Action::DefendChild),
        Block::does(Action::Hunt),
        // the old rest gene: from 95% fullness, given up 10 points lower
        Block::when(Test::at(Cond::Fullness, 95), Action::Rest),
        Block::when2(Test::is(Cond::Resting), Test::at(Cond::Fullness, 85), Action::Rest),
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

    /// Whether block `i` may act: not switched off, and a setting or reached.
    pub fn live(&self, i: usize) -> bool {
        let b = &self.blocks()[i];
        !b.off() && (b.action.is_setting() || i < self.reachable())
    }

    /// How many times smaller its most permissive hunt takes prey: others fear it by this. None —
    /// it never hunts.
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
        let first = *programs.first()?;
        let shape = first.shape();
        if programs.iter().any(|p| p.shape() != shape) {
            return None;
        }
        let mut m = first;
        for i in 0..usize::from(first.len) {
            for (slot, _) in first.blocks[i].numbers() {
                let mut xs: Vec<u16> = programs.iter().map(|p| p.blocks[i].get(slot)).collect();
                xs.sort_unstable();
                *m.blocks[i].number(slot) = xs[xs.len() / 2];
            }
        }
        m.summary = Summary::of(&m.blocks, usize::from(m.len));
        Some(m)
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

    /// A child's drift: every number of the program but the flags moves by gauss(0, its nudge ×
    /// `share`), held in range — `share` is the rule `program_drift` × mutability — as its genes
    /// drift. One draw a number; none at all when `share` is 0. Not counted in `changes`.
    pub fn drift(&mut self, share: f64, rng: &mut Rng) {
        if share <= 0.0 {
            return;
        }
        let len = usize::from(self.len);
        for b in &mut self.blocks[..len] {
            let numbers: Vec<(usize, ParamSpec)> = b.numbers().collect();
            for (slot, spec) in numbers {
                let raw = b.number(slot);
                *raw = spec.moved(*raw, spec.nudge * share, rng);
            }
        }
        self.summary = Summary::of(&self.blocks, len);
    }

    /// The child's program: with chance `chance` (the rule × the parent's mutability) one
    /// mutation, else an exact copy. One draw always; a mutation draws its kind and what it needs.
    /// A mutation that cannot apply (a full program, a single block, nothing to nudge) or lands
    /// where it was changes nothing and is not counted.
    pub fn mutate(&mut self, chance: f64, rng: &mut Rng) {
        if rng.random() >= chance {
            return;
        }
        let before = (self.blocks, self.len);
        let len = usize::from(self.len);
        let u = rng.random();
        let pick = |rng: &mut Rng, n: usize| rng.randint(0, n as i64 - 1) as usize;
        let mut acc = 0.0;
        let mut op = MUTATIONS.len() - 1;
        for (k, &(_, share)) in MUTATIONS.iter().enumerate() {
            acc += share;
            if u < acc {
                op = k;
                break;
            }
        }
        match MUTATIONS[op].0 {
            Mutation::Nudge => {
                // every number of the program: the tests' thresholds (slots 0‒2) and the action's (3‒)
                let slots: Vec<(usize, usize)> = (0..len)
                    .flat_map(|i| (0..TESTS + MAX_ARGS).map(move |s| (i, s)))
                    .filter(|&(i, s)| {
                        if s < TESTS {
                            self.blocks[i].when[s].cond.param().is_some()
                        } else {
                            s - TESTS < self.blocks[i].action.params().len()
                        }
                    })
                    .collect();
                if !slots.is_empty() {
                    let (i, s) = slots[pick(rng, slots.len())];
                    let b = &mut self.blocks[i];
                    let spec = if s < TESTS {
                        b.when[s].cond.param().expect("a test with a threshold")
                    } else {
                        b.action.params()[s - TESTS]
                    };
                    let raw = b.number(s);
                    *raw = spec.nudged(*raw, rng);
                }
            }
            Mutation::Condition => {
                let (i, s) = (pick(rng, len), pick(rng, TESTS));
                self.blocks[i].when[s] = random_test(rng);
            }
            Mutation::Negate => {
                let (i, s) = (pick(rng, len), pick(rng, TESTS));
                self.blocks[i].when[s].negate ^= true;
            }
            Mutation::Action => {
                let i = pick(rng, len);
                let action = Action::ALL[pick(rng, Action::ALL.len())];
                self.blocks[i].action = action;
                self.blocks[i].args = action.base_args();
            }
            Mutation::Swap => {
                if len >= 2 {
                    let i = pick(rng, len - 1);
                    self.blocks.swap(i, i + 1);
                }
            }
            Mutation::Duplicate => {
                if len < MAX_BLOCKS {
                    let (i, at) = (pick(rng, len), pick(rng, len + 1));
                    self.insert(at, self.blocks[i]);
                }
            }
            Mutation::Delete => {
                if len > 1 {
                    let i = pick(rng, len);
                    self.blocks.copy_within(i + 1..len, i);
                    self.blocks[len - 1] = FILLER;
                    self.len -= 1;
                }
            }
            Mutation::Insert => {
                if len < MAX_BLOCKS {
                    let at = pick(rng, len + 1);
                    let test = random_test(rng);
                    let action = Action::ALL[pick(rng, Action::ALL.len())];
                    self.insert(at, Block::when(test, action));
                }
            }
        }
        if (self.blocks, self.len) != before {
            self.changes = self.changes.saturating_add(1);
            self.summary = Summary::of(&self.blocks, usize::from(self.len));
        }
    }

    fn insert(&mut self, at: usize, block: Block) {
        let len = usize::from(self.len);
        self.blocks.copy_within(at..len, at + 1);
        self.blocks[at] = block;
        self.len += 1;
    }
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
    /// A test turns to its opposite: «always» becomes «never», switching the block off.
    Negate,
    /// A block's action becomes a random one, its parameters at their bases.
    Action,
    /// A block trades places with the next one.
    Swap,
    /// A copy of a block goes to a random place.
    Duplicate,
    Delete,
    /// A new random block goes to a random place.
    Insert,
}

/// The mutation kinds and their shares of the mutations; the shares sum to 1. Small changes
/// (a number, the order) are the most common, so a working program usually stays working.
const MUTATIONS: [(Mutation, f64); 8] = [
    (Mutation::Nudge, 0.35),
    (Mutation::Condition, 0.12),
    (Mutation::Negate, 0.08),
    (Mutation::Action, 0.08),
    (Mutation::Swap, 0.15),
    (Mutation::Duplicate, 0.08),
    (Mutation::Delete, 0.08),
    (Mutation::Insert, 0.06),
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
        assert_eq!(text[6], "7. если сытость < 30% → установка: есть и чужую пищу");
        assert_eq!(text[8], "9. если здоровье ≥ 50% → дать сдачи (враг крупнее не более чем в 1,5 раза)");
        assert_eq!(
            text[12],
            "13. если здоровье ≥ 60% → защищать детёныша (ребёнок ближе 50% зрения, при баке больше 50%, \
             не дольше 90 тиков, потом пауза 60 тиков, ребёнка ударили за 30 тиков)"
        );
        assert_eq!(
            text[9],
            "10. если охотник ближе 33% зрения → убегать (бежать ещё 60 тиков, рывком, снова пугается угрозы \
             ближе 33% зрения, ход 100%, прямо)"
        );
        assert_eq!(
            text[13],
            "14. если всегда → охотиться (добыча мельче в 1,5 раза, осторожность 100%, терпение 30 тиков, \
             только если выгоднее, рывком, брошенную не трогать 180 тиков, добыча не дальше 100% зрения, \
             ход погони 100%)"
        );
        assert_eq!(
            text[17],
            "18. если всегда → к растению (ход 100%, держится выбранного, не самое выгодное)"
        );
        assert_eq!(
            *text.last().unwrap(),
            format!("{}. если всегда → бродить (ход 100%, цели до 200% зрения)", text.len())
        );
        assert!(l.describe().last().unwrap().contains("(ход 33%"));
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
            "1. если сытость < 40% → к растению (ход 100%, держится выбранного, не самое выгодное)"
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
    /// summary current), count only what changed it, and a long line of mutants fills up.
    #[test]
    fn mutations_keep_a_program_valid_and_change_it() {
        let mut rng = Rng::new(5);
        let mut p = Program::STANDARD;
        let (mut longest, mut changed) = (0, 0);
        for _ in 0..20_000 {
            let before = p;
            p.mutate(1.0, &mut rng);
            check(&p);
            let differs = !p.same_blocks(&before);
            assert_eq!(p.changes, before.changes.saturating_add(differs as u16), "only a change counts");
            changed += differs as usize;
            longest = longest.max(p.blocks().len());
        }
        assert_eq!(longest, MAX_BLOCKS, "insertions fill a program up");
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
        let (mut longer, mut shorter, mut negated, mut reordered, mut nudged, mut flipped) =
            (0, 0, 0, 0, 0, 0);
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
            negated +=
                b.iter().zip(a).any(|(x, y)| x.when.iter().zip(y.when).any(|(s, t)| s.negate != t.negate))
                    as usize;
            reordered += (b[0] == a[1] && b[1] == a[0]) as usize;
            let same_shape = b[0].action == a[0].action && b[1].action == a[1].action;
            nudged += (same_shape
                && b[0].when[0].cond == Cond::Fullness
                && (b[0].when[0].param != 50 || b[0].args != a[0].args || b[1].args != a[1].args))
                as usize;
            flipped += (same_shape && b[0].args[1] != a[0].args[1]) as usize;
        }
        assert!(longer > 600 && shorter > 300, "insertions and copies {longer}, deletions {shorter}");
        assert!(negated > 400 && reordered > 900, "{negated} {reordered}");
        assert!(nudged > 2000 && flipped > 250, "nudged {nudged}, a flag flipped {flipped}");
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

    /// The drift moves the numbers, never the shape or a flag, keeps them in range, is not counted
    /// as a mutation, and at 0 draws nothing.
    #[test]
    fn the_drift_moves_numbers_not_the_shape() {
        let mut rng = Rng::new(21);
        let mut p = Program::STANDARD;
        let mut moved = 0;
        for _ in 0..200 {
            let before = p;
            p.drift(1.0, &mut rng);
            check(&p);
            assert_eq!(p.shape(), Program::STANDARD.shape());
            assert_eq!(p.changes, 0);
            moved += !p.same_blocks(&before) as usize;
        }
        assert!(moved > 190, "almost every drift moves some number: {moved}");
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
