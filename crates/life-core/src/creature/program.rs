//! Behaviour programs: a creature's behaviour is an ordered list of blocks «if TESTS → ACTION».
//! Each tick the **settings** (`Action::is_setting`) whose tests hold apply first, wherever they
//! stand — eat the other niche's food too, strike rivals at food, go no farther from the layer for
//! food — and then the first deciding block whose tests hold and whose action can be done decides
//! the step (`strategy::plan`); an action that cannot be done (no prey, no corpse in sight, a
//! hopeless chase) falls through to the next block. When nothing decides, the creature stands.
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
    CHASE_GIVE_UP_TICKS, CHASE_PATIENCE, FLEE_SIGHT_SHARE, FLEE_TICKS, FLOCKS, PROGRAM_NUDGE_POINTS,
};
use crate::rng::Rng;
use std::sync::Arc;

/// At most this many blocks: a duplicate or an insertion into a full program does nothing.
pub const MAX_BLOCKS: usize = 24;
/// At most this many parameters an action reads.
pub const MAX_ARGS: usize = 6;

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

    /// The value as the engine reads it: a share for percents (0.5), times for ratios (1.5),
    /// ticks, 1 or 0 for a flag.
    pub fn value(&self, raw: u16) -> f64 {
        match self.unit {
            Unit::Percent | Unit::Sight | Unit::Ratio => f64::from(raw) / 100.0,
            Unit::Ticks | Unit::Flag => f64::from(raw),
        }
    }

    /// Game UI: the amount alone, «30%», «33% зрения», «1,5 раза», «31 тик».
    pub fn amount(&self, raw: u16) -> String {
        match self.unit {
            Unit::Percent => format!("{raw}%"),
            Unit::Sight => format!("{raw}% зрения"),
            Unit::Ratio => format!("{} раза", format!("{:.1}", f64::from(raw) / 100.0).replace('.', ",")),
            Unit::Ticks => format!("{raw} {}", ticks_word(raw)),
            Unit::Flag => (if raw != 0 { "да" } else { "нет" }).to_string(),
        }
    }

    /// Game UI: «добыча мельче в 1,5 раза», «рывком», «не рывком».
    pub fn show(&self, raw: u16) -> String {
        match self.unit {
            Unit::Flag if raw != 0 => self.label.to_string(),
            Unit::Flag => format!("не {}", self.label),
            _ => format!("{} {}", self.label, self.amount(raw)),
        }
    }

    /// The number moved by gauss(0, `sigma`), held in range.
    fn moved(&self, raw: u16, sigma: f64, rng: &mut Rng) -> u16 {
        let moved = f64::from(raw) + rng.gauss(0.0, sigma);
        moved.round().clamp(f64::from(self.lo), f64::from(self.hi)) as u16
    }

    /// A mutation's new value: a flag flips; a number moves by gauss(0, `nudge`), held in range.
    fn nudged(&self, raw: u16, rng: &mut Rng) -> u16 {
        if self.unit == Unit::Flag {
            return u16::from(raw == 0);
        }
        self.moved(raw, self.nudge, rng)
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

impl Cond {
    pub const ALL: [Cond; 10] = [
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
    /// Go where neighbours reported food (flocks are off: never done by a loner).
    FollowReport,
    /// Back into its flock's circle (flocks are off: never done by a loner).
    ReturnToCircle,
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
}

const PACE: ParamSpec = ParamSpec::percent("ход", 10, 100, 100);
const BURST: ParamSpec = ParamSpec::flag("рывком", true);
const BEST: ParamSpec = ParamSpec::flag("только если выгоднее", true);
const FIGHT_PARAMS: [ParamSpec; 1] = [ParamSpec::ratio("враг крупнее не более чем в", 150)];
const FLEE_PARAMS: [ParamSpec; 3] = [
    ParamSpec::ticks("бежать ещё", 0, 600, FLEE_TICKS as u16),
    BURST,
    // under way, only a threat this near makes it run its whole memory again (the old flight
    // distance); a farther one only steers it
    ParamSpec::sight("снова пугается угрозы ближе", 0, 100, FLEE_PCT, PROGRAM_NUDGE_POINTS),
];
const HUNT_PARAMS: [ParamSpec; 6] = [
    ParamSpec::ratio("добыча мельче в", 150),
    // the weight of the strikes it expects: 100% is the old base caution, 0 ignores them
    ParamSpec {
        label: "осторожность", unit: Unit::Percent, lo: 0, hi: 400, base: 100, nudge: 20.0
    },
    ParamSpec::ticks("терпение", 5, 300, CHASE_PATIENCE as u16),
    BEST,
    BURST,
    ParamSpec::ticks("брошенную не трогать", 0, 3000, CHASE_GIVE_UP_TICKS as u16),
];
const CORPSE_PARAMS: [ParamSpec; 2] = [BEST, PACE];
const PACE_PARAMS: [ParamSpec; 1] = [PACE];
const WANDER_PARAMS: [ParamSpec; 2] = [PACE, ParamSpec::sight("цели до", 20, 500, WANDER_REACH_PCT, 25.0)];
const REST_PARAMS: [ParamSpec; 2] =
    [ParamSpec::ticks("отдых", 1, 600, 90), ParamSpec::ticks("пауза", 0, 1000, 180)];
const RIVAL_PARAMS: [ParamSpec; 1] = [ParamSpec::ratio("соперник мельче в", 150)];
const REACH_PARAMS: [ParamSpec; 1] = [ParamSpec::percent("за слой не дальше", 0, 100, 100)];
const NO_PARAMS: [ParamSpec; 0] = [];

impl Action {
    pub const ALL: [Action; 16] = [
        Action::FightBack,
        Action::Flee,
        Action::Hunt,
        Action::EatCorpse,
        Action::EatPlant,
        Action::FollowReport,
        Action::ReturnToCircle,
        Action::Wander,
        Action::Ambush,
        Action::Surface,
        Action::Dive,
        Action::Rest,
        Action::Torpor,
        Action::EatForeign,
        Action::Rival,
        Action::Reach,
    ];

    /// The parameters the action reads, in the order of `Block::args`.
    pub const fn params(self) -> &'static [ParamSpec] {
        match self {
            Action::FightBack => &FIGHT_PARAMS,
            Action::Flee => &FLEE_PARAMS,
            Action::Hunt => &HUNT_PARAMS,
            Action::EatCorpse => &CORPSE_PARAMS,
            Action::EatPlant | Action::FollowReport | Action::Surface | Action::Dive => &PACE_PARAMS,
            Action::Wander => &WANDER_PARAMS,
            Action::Rest => &REST_PARAMS,
            Action::Rival => &RIVAL_PARAMS,
            Action::Reach => &REACH_PARAMS,
            Action::ReturnToCircle | Action::Ambush | Action::Torpor | Action::EatForeign => &NO_PARAMS,
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
        matches!(self, Action::EatForeign | Action::Rival | Action::Reach)
    }

    /// Whether the action is always done when reached: the deciding blocks after an unconditional
    /// one of these are never reached.
    pub const fn never_fails(self) -> bool {
        matches!(self, Action::Wander | Action::Ambush | Action::Torpor)
    }

    /// Whether it can do anything only in a flock: with flocks off (`config::FLOCKS`) the
    /// templates leave it out and no mutation brings it in.
    pub const fn needs_flock(self) -> bool {
        matches!(self, Action::FollowReport | Action::ReturnToCircle)
    }

    /// Game UI.
    pub const fn label(self) -> &'static str {
        match self {
            Action::FightBack => "дать сдачи",
            Action::Flee => "убегать",
            Action::Hunt => "охотиться",
            Action::EatCorpse => "к падали",
            Action::EatPlant => "к растению",
            Action::FollowReport => "к еде из вестей",
            Action::ReturnToCircle => "в круг стаи",
            Action::Wander => "бродить",
            Action::Ambush => "замереть",
            Action::Surface => "к верху слоя",
            Action::Dive => "ко дну слоя",
            Action::Rest => "отдыхать",
            Action::Torpor => "оцепенеть",
            Action::EatForeign => "есть и чужую пищу",
            Action::Rival => "гнать соперников у еды",
            Action::Reach => "за едой из слоя",
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
                 лишь задаёт направление. Рывком — быстрее, насколько позволяют мышцы (ген «рывок»)."
            }
            Action::Hunt => {
                "Гонится за самой выгодной добычей, которая мельче его в столько раз, и бьёт её. \
                 Осторожность — насколько боится ответных ударов (100% — обычная, 0 — не боится); \
                 погоню, которая за «терпение» тиков не сократила разрыв, бросает и столько тиков эту \
                 добычу не трогает. «Только если выгоднее» — не охотится, когда растение или падаль \
                 дают не меньше."
            }
            Action::EatCorpse => {
                "Идёт к лучшей видимой падали, которую ест, и ест. «Только если выгоднее» — когда \
                 растение даёт не больше."
            }
            Action::EatPlant => "Идёт к своему растению и ест.",
            Action::FollowReport => {
                "Идёт к еде, о которой сообщили соседи по стае. Стаи выключены: не срабатывает."
            }
            Action::ReturnToCircle => "Возвращается в круг своей стаи. Стаи выключены: не срабатывает.",
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
        }
    }
}

/// The actions a mutation may bring into a program: all of them, but those that need a flock
/// while flocks are off (`Action::needs_flock`), and how many there are.
const POOL: ([Action; Action::ALL.len()], usize) = {
    let mut pool = [Action::Wander; Action::ALL.len()];
    let (mut n, mut i) = (0, 0);
    while i < Action::ALL.len() {
        if FLOCKS || !Action::ALL[i].needs_flock() {
            pool[n] = Action::ALL[i];
            n += 1;
        }
        i += 1;
    }
    (pool, n)
};

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

/// «If both tests hold → the action with its parameters».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub when: [Test; 2],
    pub action: Action,
    /// The action's parameters, raw, in the order of `Action::params`; the rest are zero.
    pub args: [u16; MAX_ARGS],
}

/// What fills a program past its end: kept the same, so programs of the same blocks are equal.
const FILLER: Block = Block::does(Action::Wander);

impl Block {
    /// Unconditional, the parameters at their bases.
    pub const fn does(action: Action) -> Block {
        Block { when: [Test::ALWAYS, Test::ALWAYS], action, args: action.base_args() }
    }

    pub const fn when(test: Test, action: Action) -> Block {
        Block { when: [test, Test::ALWAYS], action, args: action.base_args() }
    }

    pub const fn when2(a: Test, b: Test, action: Action) -> Block {
        Block { when: [a, b], action, args: action.base_args() }
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
        self.when[0].never() || self.when[1].never()
    }

    /// Whether it decides every time it is reached.
    pub const fn always_fires(&self) -> bool {
        self.when[0].always() && self.when[1].always() && self.action.never_fails()
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

    /// Everything about the block packed into numbers: for digests and counting programs.
    pub fn code(&self) -> [u64; 2] {
        let test = |t: Test| u64::from(t.cond as u8) | u64::from(t.negate) << 5 | u64::from(t.param) << 6;
        let tests = test(self.when[0])
            | test(self.when[1]) << 22
            | u64::from(self.action as u8) << 44
            | u64::from(self.args[5]) << 52;
        let mut args = 0;
        for (i, a) in self.args[..5].iter().enumerate() {
            args |= u64::from(*a) << (12 * i);
        }
        [tests, args]
    }

    /// The block without its numbers — its tests' conditions and negations, its action and its
    /// flags: what stays while the numbers drift. The report groups programs by it.
    pub fn shape(&self) -> u64 {
        let test = |t: Test| u64::from(t.cond as u8) | u64::from(t.negate) << 5;
        let mut flags = 0_u64;
        for (i, p) in self.action.params().iter().enumerate() {
            if p.unit == Unit::Flag && self.args[i] != 0 {
                flags |= 1 << i;
            }
        }
        test(self.when[0]) | test(self.when[1]) << 6 | u64::from(self.action as u8) << 12 | flags << 20
    }

    /// The numbers of the block the drift moves: each test's threshold and each parameter but the
    /// flags, as (slot, its spec); slots 0‒1 are the tests, 2‒ the parameters.
    fn numbers(&self) -> impl Iterator<Item = (usize, ParamSpec)> + '_ {
        let tests = (0..2).filter_map(|s| self.when[s].cond.param().map(|p| (s, p)));
        let args = self.action.params().iter().enumerate().filter(|(_, p)| p.unit != Unit::Flag);
        tests.chain(args.map(|(i, p)| (2 + i, *p)))
    }

    fn number(&mut self, slot: usize) -> &mut u16 {
        if slot < 2 { &mut self.when[slot].param } else { &mut self.args[slot - 2] }
    }

    fn get(&self, slot: usize) -> u16 {
        if slot < 2 { self.when[slot].param } else { self.args[slot - 2] }
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
        let mut s =
            Summary { reachable: reachable as u8, threat: 0, hunt: 0, fight: 0, fight_health: 0, wander: 0 };
        let (mut fights, mut wanders) = (false, false);
        i = 0;
        while i < len {
            let b = &blocks[i];
            if !b.off() && (b.action.is_setting() || i < reachable) {
                let mut t = 0;
                while t < 2 {
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
                        while t < 2 {
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
/// genes the programs replaced — wandering and going to reported food at `pace` % of its speed.
/// Without `flocks` the blocks that need a flock are left out.
const fn founders(pace: u16, flocks: bool) -> Program {
    // the old bravery 50%: a calm stranger may come half as near as a hunting one
    let calm = FLEE_PCT / 2;
    let list = [
        // below the old picky and rivalry (30%) it eats the other niche's food and fights for its own
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::EatForeign),
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::Rival),
        // the old bravery 50%: it defends itself down to half its health
        Block::when(Test::at(Cond::Health, 50), Action::FightBack),
        Block::when(Test::at(Cond::HunterNear, FLEE_PCT), Action::Flee),
        Block::when(Test::at(Cond::ThreatNear, calm), Action::Flee),
        Block::when(Test::is(Cond::Fleeing), Action::Flee),
        Block::does(Action::Hunt),
        // the old rest gene: from 95% fullness, given up 10 points lower
        Block::when(Test::at(Cond::Fullness, 95), Action::Rest),
        Block::when2(Test::is(Cond::Resting), Test::at(Cond::Fullness, 85), Action::Rest),
        Block::does(Action::EatCorpse),
        Block::does(Action::EatPlant),
        Block::does(Action::FollowReport).with(0, pace),
        Block::does(Action::ReturnToCircle),
        Block::does(Action::Wander).with(0, pace),
    ];
    let mut kept = [FILLER; MAX_BLOCKS];
    let (mut n, mut i) = (0, 0);
    while i < list.len() {
        if flocks || !list[i].action.needs_flock() {
            kept[n] = list[i];
            n += 1;
        }
        i += 1;
    }
    Program::made(kept, n)
}

impl Program {
    /// The standard behaviour: wandering at full speed.
    pub const STANDARD: Program = founders(100, FLOCKS);

    /// The lurker: the same, but it wanders (and goes to reported food) at a third of its speed.
    pub const LURKER: Program = founders(33, FLOCKS);

    /// The standard behaviour with the flock blocks (back to the circle, to reported food), as the
    /// template is when flocks are on: the dormant flock layer's tests give it to their members.
    pub const IN_FLOCKS: Program = founders(100, true);

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

    /// The health share down to which it defends itself (`defence`).
    pub fn defends_to(&self) -> Option<f64> {
        self.defence().map(|(_, health)| health)
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
                // every number of the program: the tests' thresholds (slots 0‒1) and the action's (2‒)
                let slots: Vec<(usize, usize)> = (0..len)
                    .flat_map(|i| (0..2 + MAX_ARGS).map(move |s| (i, s)))
                    .filter(|&(i, s)| match s {
                        0 | 1 => self.blocks[i].when[s].cond.param().is_some(),
                        _ => s - 2 < self.blocks[i].action.params().len(),
                    })
                    .collect();
                if !slots.is_empty() {
                    let (i, s) = slots[pick(rng, slots.len())];
                    let b = &mut self.blocks[i];
                    let spec = if s < 2 {
                        b.when[s].cond.param().expect("a test with a threshold")
                    } else {
                        b.action.params()[s - 2]
                    };
                    let raw = b.number(s);
                    *raw = spec.nudged(*raw, rng);
                }
            }
            Mutation::Condition => {
                let (i, s) = (pick(rng, len), pick(rng, 2));
                self.blocks[i].when[s] = random_test(rng);
            }
            Mutation::Negate => {
                let (i, s) = (pick(rng, len), pick(rng, 2));
                self.blocks[i].when[s].negate ^= true;
            }
            Mutation::Action => {
                let i = pick(rng, len);
                let action = POOL.0[pick(rng, POOL.1)];
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
                    let action = POOL.0[pick(rng, POOL.1)];
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
                assert!(p.unit == Unit::Flag || p.nudge >= 1.0, "{a:?}: {p:?} moves");
            }
        }
        for c in Cond::ALL {
            if let Some(p) = c.param() {
                assert!(p.lo <= p.base && p.base <= p.hi, "{c:?}");
                assert!(p.nudge >= 1.0, "{c:?} moves");
            }
        }
        let total: f64 = MUTATIONS.iter().map(|m| m.1).sum();
        assert!((total - 1.0).abs() < 1e-12, "the shares sum to 1: {total}");
        let pool = &POOL.0[..POOL.1];
        assert_eq!(pool.iter().any(|a| a.needs_flock()), FLOCKS, "flock actions only with flocks");
        assert!(pool.contains(&Action::Hunt) && pool.contains(&Action::Reach));
    }

    /// The templates carry the old behaviour genes' bases; the lurker differs only in its paces.
    #[test]
    fn the_templates_carry_the_old_bases() {
        let (s, l) = (Program::STANDARD, Program::LURKER);
        assert_eq!(s.reachable(), s.blocks().len(), "every block of a template is reached");
        assert_eq!(Program::template(Strategy::Standard), s);
        assert_eq!(Program::template(Strategy::Lurker), l);
        let differ: Vec<usize> = (0..s.blocks().len()).filter(|&i| s.blocks()[i] != l.blocks()[i]).collect();
        assert_eq!(
            differ.len(),
            if FLOCKS { 2 } else { 1 },
            "the lurker differs only in its paces: {differ:?}"
        );
        assert_eq!(s.blocks().iter().any(|b| b.action.needs_flock()), FLOCKS);
        let flock = Program::IN_FLOCKS;
        assert!(flock.blocks().iter().any(|b| b.action == Action::ReturnToCircle));
        assert_eq!(flock.blocks().len(), s.blocks().len() + if FLOCKS { 0 } else { 2 });
        assert_eq!(s.hunt_ratio(), Some(1.5));
        assert_eq!(s.defence(), Some((1.5, 0.5)));
        assert_eq!(s.wander_reach(), 2.0);
        assert!((s.threat_range() - 0.33).abs() < 1e-12);
        let text = s.describe();
        assert_eq!(text[0], "1. если сытость < 30% → установка: есть и чужую пищу");
        assert_eq!(text[2], "3. если здоровье ≥ 50% → дать сдачи (враг крупнее не более чем в 1,5 раза)");
        assert_eq!(
            text[3],
            "4. если охотник ближе 33% зрения → убегать (бежать ещё 60 тиков, рывком, снова пугается угрозы \
             ближе 33% зрения)"
        );
        assert_eq!(
            text[6],
            "7. если всегда → охотиться (добыча мельче в 1,5 раза, осторожность 100%, терпение 30 тиков, \
             только если выгоднее, рывком, брошенную не трогать 180 тиков)"
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
        assert_eq!(p.describe()[0], "1. если сытость < 40% → к растению (ход 100%)");
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
            assert!(FLOCKS || !b.action.needs_flock(), "{b:?} needs a flock");
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
        assert!(nudged > 2000 && flipped > 300, "nudged {nudged}, a flag flipped {flipped}");
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
        let mut codes: Vec<[u64; 2]> = Vec::new();
        for c in Cond::ALL {
            for a in Action::ALL {
                for negate in [false, true] {
                    let t = Test { cond: c, negate, param: c.param().map_or(0, |p| p.hi) };
                    codes.push(Block::when2(t, Test::ALWAYS, a).code());
                    codes.push(Block::when2(Test::ALWAYS, t, a).code());
                }
            }
        }
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        // a test in either slot with «always» in the other is the same block only for «always»
        assert_eq!(codes.len(), n - Action::ALL.len());
        let b = Block::does(Action::Hunt);
        assert_ne!(b.code(), b.with(4, 0).code(), "the fifth parameter counts");
        assert_ne!(b.code(), b.with(5, 0).code(), "the sixth parameter counts");
        assert_eq!(b.shape(), b.with(5, 0).with(0, 300).shape(), "numbers are no part of the shape");
        assert_ne!(b.shape(), b.with(4, 0).shape(), "flags are");
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
