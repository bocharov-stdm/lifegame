//! A block's tests (`Cond`): what a creature checks before it acts, with their thresholds.
//! Append-only, like the gene table.

use crate::config::PROGRAM_NUDGE_POINTS;

use super::{FLEE_PCT, MODES, ParamSpec, Unit};

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
    /// A partner it could mate with (`Action::SeekMate`) closer to its body's edge than the
    /// threshold of its sight.
    MateSeen,
}

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
pub(super) const MODE: ParamSpec = ParamSpec::index("режим", MODES as u16);
const MATE: ParamSpec = ParamSpec::sight("пара ближе", 0, 100, 100, PROGRAM_NUDGE_POINTS);

impl Cond {
    pub const ALL: [Cond; 21] = [
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
        Cond::MateSeen,
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
            Cond::MateSeen => Some(MATE),
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
            Cond::MateSeen => ("пара ближе", "пары нет ближе"),
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
            Cond::MateSeen => {
                "Ближе этой доли зрения (до края тела) есть партнёр, с которым оно могло бы \
                 спариться: выросший, готовый (на его последнем ходу действовала «спариваться»), с \
                 близким геномом, не родня."
            }
        }
    }
}
