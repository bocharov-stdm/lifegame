//! A block's actions (`Action`): what a creature does or sets, with their parameters.
//! Append-only, like the gene table.

use crate::config::{CHASE_GIVE_UP_TICKS, CHASE_PATIENCE, FLEE_TICKS, PROGRAM_NUDGE_POINTS};

use super::cond::MODE;
use super::params::WANDER_REACH_PCT;
use super::{FLEE_PCT, MAX_ARGS, ParamSpec, Unit};

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

pub(super) const PACE: ParamSpec = ParamSpec::percent("темп", 10, 100, 100);
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
