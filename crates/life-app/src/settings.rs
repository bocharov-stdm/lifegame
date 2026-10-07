//! What the player can configure, within what limits and where it is stored. A port of
//! `app/settings.py` (tag python-final).
//!
//! `FIELDS` is the only place where every slider is described: the label, the explanation, the
//! limits, the step and the format. The «Новый мир» screen and the lab on the fly are built by
//! it, and the values from the settings file are clamped by it too — so they cannot drift apart.

use std::path::{Path, PathBuf};

use life_core::config::{CREATURES_AT_START, DIET_START_MIX, MEAT_FOUNDER_SIZE};
use life_core::flora::{Along, Profile};
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space, WorldConfig};
use serde_json::{Map, Value};

pub const SEED_MAX: u64 = 99_999;
/// Plants a tick per base area at the energy density 1 (`config::PLANT_SPAWN_CHANCE`): the input
/// shows the density in plants a tick.
const PLANT_RATE: f64 = life_core::config::PLANT_SPAWN_CHANCE;
/// The game's own price of life (`cost_scale`): the sweep of 2026-09-29 (the player's world, 8 seeds
/// × 40 000 ticks, 2 to 5) kept carnivores in every world and scavengers in half at 2, the best mix;
/// 3, the old default, lost the scavengers, 4 and 5 the carnivores.
pub const GAME_COST_SCALE: f64 = 2.0;
/// The price of life is shown per 100 of the game's own: 100 is `GAME_COST_SCALE`.
const COST_SHOWN: f64 = 100.0 / GAME_COST_SCALE;
/// The game's own price of speed (`speed_cost`, on top of the price of life 2): the sweep of
/// 2026-09-29 (60 000 ticks, 8 seeds; speed at half price against the price of life 2 alone) kept
/// carnivores in 8 worlds and scavengers in 7 (control 7 and 6), the best mix of the single stats —
/// a difference of a world or two, within the noise, the user's pick. Shown per 100 of it.
pub const GAME_SPEED_COST: f64 = 0.5;
const SPEED_COST_SHOWN: f64 = 100.0 / GAME_SPEED_COST;
/// The interface scale; 0 — as in the system.
pub const UI_SCALES: [f64; 6] = [0.0, 1.0, 1.25, 1.5, 1.75, 2.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// «Мир»: what a game begins with.
    World,
    /// The world's rules by topic; they can be changed mid-game in the lab too.
    /// «Еда»: how much food comes and where plants grow.
    Food,
    /// «Тело»: what a body costs to keep and to bear.
    Body,
    /// «Питание»: what each diet is good at, as a table.
    Diets,
    /// «Бой».
    Combat,
    /// «Трупы»: how corpses rot and sink.
    Corpses,
    /// «Эволюция»: how children inherit.
    Evolution,
}

impl Tab {
    /// The rule tabs, in the order the lab and «Новый мир» show them.
    pub const RULES: [(Tab, &'static str); 6] = [
        (Tab::Food, "Еда"),
        (Tab::Body, "Тело"),
        (Tab::Diets, "Питание"),
        (Tab::Combat, "Бой"),
        (Tab::Corpses, "Трупы"),
        (Tab::Evolution, "Эволюция"),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Creatures,
    PlantGrowth,
    MutationSigma,
    PlantEnergy,
    CostScale,
    SizePower,
    SightPower,
    Lurkers,
    Herbivores,
    Omnivores,
    Scavengers,
    Carnivores,
    PlantDepthProfile,
    PlantDepthSteepness,
    PlantDepthEnd,
    PlantDepthBend,
    PlantDepthWaves,
    PlantDepthAmplitude,
    PlantWidthProfile,
    PlantWidthSteepness,
    PlantWidthEnd,
    PlantWidthBend,
    PlantWidthWaves,
    PlantWidthAmplitude,
    ReproCost,
    MeleeDamage,
    ShotDamage,
    ShotCost,
    ShotPeriod,
    PlantBiteYield,
    PlantPatches,
    PlantPatchSize,
    MeleeSizePower,
    PlantPatchShare,
    SpeedPower,
    SizeCost,
    SpeedCost,
    SightCost,
    SpeedMassPower,
    CloneShare,
    MinMutability,
    DietStep,
    DietJump,
    DietMeatStep,
    DietLeapCarnivore,
    DietLeapScavenger,
    CorpseFresh,
    CorpseBones,
    CorpseSink,
    CorpseDecay,
    CorpseRest,
    CorpseBonesSink,
    ThermoTop,
    ThermoBottom,
    MeatFounders,
    ProgramMutation,
    ProgramDrift,
    /// A diet's edge: (diet H/O/S/C, edge in `rules::DIET_EDGES` order).
    Diet(u8, u8),
}

#[derive(Clone, Copy)]
pub struct Field {
    pub key: Key,
    pub label: &'static str,
    pub hint: &'static str,
    /// The hard limits: typed values are held inside them. Past them the rule stops making sense
    /// or the simulation would choke (a billion plants a tick); `limit` says which, in words.
    pub lo: f64,
    pub hi: f64,
    /// Precision of the value, and the drag step of the input.
    pub step: f64,
    /// How the value is written in texts (the chronicle, the base value).
    pub format: fn(f64) -> String,
    /// The input shows the value times `shown` with `unit` after it and `decimals` digits: a share
    /// 0.44 is typed as 44 %.
    pub shown: f64,
    pub unit: &'static str,
    pub decimals: usize,
    /// Why the limits are where they are; empty — they are just where the rule stops making sense.
    pub limit: &'static str,
    pub tab: Tab,
    /// The rule's name in `Rules` (None — a starting condition, not a rule).
    pub rule: Option<&'static str>,
    /// Non-empty — a choice from variants (the value is the variant's number), not a slider.
    pub choices: &'static [&'static str],
    /// Whether to show the field now: a food profile's parameter is visible only when its profile
    /// is chosen.
    pub visible: fn(&Settings) -> bool,
}

/// What number fields share: always visible, no variants, shown as stored.
const NUMBER: Field = Field {
    key: Key::Creatures,
    label: "",
    hint: "",
    lo: 0.0,
    hi: 1.0,
    step: 1.0,
    format: int,
    shown: 1.0,
    unit: "",
    decimals: 0,
    limit: "",
    tab: Tab::World,
    rule: None,
    choices: &[],
    visible: |_| true,
};

/// The version of the defaults a settings file was saved with. A file saves every value, so a
/// changed default would never reach a player who saved before it: a file loads each key of
/// `CHANGED_DEFAULTS` changed after the file's version as the new default, the rest as saved. 1 — the
/// ocean reform (2026-09-27): the «океаническое» profile, the corpse stages' times; 2 — the price of
/// life 3 → 2 (2026-09-29); 3 — the price of speed 1 → 0.5 (2026-09-29); 4 — the diet edges
/// calibrated by the user (2026-09-29); 5 — the herbivore's and the omnivore's health, the
/// herbivore's size price and fresh meat (2026-09-29).
const DEFAULTS_VERSION: u64 = 5;
/// A key whose default changed and the version that changed it.
const CHANGED_DEFAULTS: [(Key, u64); 19] = [
    (Key::PlantDepthProfile, 1),
    (Key::CorpseFresh, 1),
    (Key::CorpseDecay, 1),
    (Key::CostScale, 2),
    (Key::SpeedCost, 3),
    // diet edges: (diet H/O/S/C, edge in `rules::DIET_EDGES` order)
    (Key::Diet(0, 9), 4),
    (Key::Diet(1, 0), 4),
    (Key::Diet(1, 4), 4),
    (Key::Diet(1, 5), 4),
    (Key::Diet(1, 6), 4),
    (Key::Diet(1, 7), 4),
    (Key::Diet(1, 9), 4),
    (Key::Diet(2, 6), 4),
    (Key::Diet(2, 9), 4),
    (Key::Diet(3, 7), 4),
    (Key::Diet(0, 1), 5),
    (Key::Diet(0, 2), 5),
    (Key::Diet(0, 6), 5),
    (Key::Diet(1, 1), 5),
];

/// The food profiles' labels — in the order of `Profile::ALL` (checked by a test).
const PROFILES: [&str; 7] =
    ["равномерно", "линейно", "экспонента", "логарифм", "волны", "игровое", "океаническое"];

fn profile_label(v: f64) -> String {
    Profile::of(v).label().into()
}

impl Field {
    /// The value within the limits and on the step grid (that is how a slider sets it).
    pub fn snap(&self, value: f64) -> f64 {
        let v = value.clamp(self.lo, self.hi);
        let v = self.lo + ((v - self.lo) / self.step).round() * self.step;
        // without tails like 0.30000000000000004
        (v.min(self.hi) * 1e6).round() / 1e6
    }

    /// Whether it can be changed in the middle of a game: rules — yes, starting conditions — no.
    pub fn live(&self) -> bool {
        self.rule.is_some()
    }
}

fn int(v: f64) -> String {
    format!("{v:.0}")
}

fn percent(v: f64) -> String {
    format!("{v:.0}%")
}

/// A factor per 100 of its common value: ×1 is written 100.
fn per100(v: f64) -> String {
    format!("{:.0}", v * 100.0)
}

/// Every field: the world's and the rules' (`BASE_FIELDS`), then the diet edges, diet by diet
/// (`diet_field`).
pub const FIELDS: [Field; 97] = {
    let mut all = [NUMBER; 97];
    let mut i = 0;
    while i < BASE_FIELDS.len() {
        all[i] = BASE_FIELDS[i];
        i += 1;
    }
    let mut d = 0;
    while d < 4 {
        let mut e = 0;
        while e < DIET_ROWS.len() {
            all[i] = diet_field(d, e);
            i += 1;
            e += 1;
        }
        d += 1;
    }
    all
};

/// The «Питание» table's rows, in `rules::DIET_EDGES` order: what the row is called and what it
/// means; a cell is one diet's value.
pub const DIET_ROWS: [(&str, &str); 10] = [
    ("Удар", "Сила удара; 100 — обычная, 300 — втрое сильнее. Энергии удар стоит столько же."),
    (
        "Здоровье",
        "Здоровья на единицу размера; 100 — обычно. Крепкого дольше убивать, и охотник это взвешивает.",
    ),
    ("Цена размера", "Цена размера в содержании; 100 — обычная, меньше — большое тело обходится дешевле."),
    ("Цена скорости", "Цена скорости в содержании; 100 — обычная, меньше — бегать дешевле."),
    (
        "Нюх",
        "Как далеко чует трупы; 100 — на своё зрение, 300 — втрое дальше. Нюх бесплатный, платят только за зрение.",
    ),
    ("Растения", "Какую долю энергии растения усваивает. 0 — растения не ест и к ним не идёт."),
    (
        "Свежее мясо",
        "Какую долю свежего мяса усваивает: столько и стоит для него охота. 0 — не охотится и \
         свежих трупов не ест, а его и не боятся.",
    ),
    ("Гниль", "Какую долю гнили усваивает: труп после свежего срока и до костей."),
    (
        "Кости",
        "Какую долю костей усваивает. Кости остаются от каждого трупа — объеденного или сгнившего — \
         и долго лежат на дне.",
    ),
    (
        "Растения в детстве",
        "Не меньше какой доли энергии растения усваивает, пока не дорос до своего размера; 0 — как \
         взрослый. Детёныш мясоеда \
         растёт на растениях и охотится взрослым; делятся только взрослые.",
    ),
];

/// A cell's own name, for the chronicle's «правила: удар мясоеда ×1.50 → ×2.00».
const DIET_FIELD_LABELS: [[&str; 10]; 4] = [
    [
        "Удар травоядного",
        "Здоровье травоядного",
        "Цена размера травоядного",
        "Цена скорости травоядного",
        "Нюх травоядного",
        "Растения травоядного",
        "Свежее мясо травоядного",
        "Гниль травоядного",
        "Кости травоядного",
        "Растения в детстве травоядного",
    ],
    [
        "Удар всеядного",
        "Здоровье всеядного",
        "Цена размера всеядного",
        "Цена скорости всеядного",
        "Нюх всеядного",
        "Растения всеядного",
        "Свежее мясо всеядного",
        "Гниль всеядного",
        "Кости всеядного",
        "Растения в детстве всеядного",
    ],
    [
        "Удар падальщика",
        "Здоровье падальщика",
        "Цена размера падальщика",
        "Цена скорости падальщика",
        "Нюх падальщика",
        "Растения падальщика",
        "Свежее мясо падальщика",
        "Гниль падальщика",
        "Кости падальщика",
        "Растения в детстве падальщика",
    ],
    [
        "Удар мясоеда",
        "Здоровье мясоеда",
        "Цена размера мясоеда",
        "Цена скорости мясоеда",
        "Нюх мясоеда",
        "Растения мясоеда",
        "Свежее мясо мясоеда",
        "Гниль мясоеда",
        "Кости мясоеда",
        "Растения в детстве мясоеда",
    ],
];

const EATEN: &str = "Больше 100% — энергия из ничего: она только растёт в растениях и переходит по цепочке.";

/// The field of diet `d`'s edge `e`. A factor is shown per 100 of a common body (×1 is 100).
const fn diet_field(d: usize, e: usize) -> Field {
    let times = Field {
        key: Key::Diet(d as u8, e as u8),
        label: DIET_FIELD_LABELS[d][e],
        hint: DIET_ROWS[e].1,
        lo: 0.1,
        hi: 5.0,
        step: 0.01,
        format: per100,
        shown: 100.0,
        tab: Tab::Diets,
        rule: Some(life_core::rules::DIET_RULE_KEYS[d][e]),
        ..NUMBER
    };
    let share = Field {
        lo: 0.0,
        hi: 1.0,
        format: |v| format!("{:.0}%", v * 100.0),
        unit: " %",
        limit: EATEN,
        ..times
    };
    match e {
        0 => Field { lo: 0.0, ..times },
        1 => Field {
            limit: "Без здоровья существо погибало бы, едва родившись.", ..times
        },
        2 => Field {
            limit: "Почти бесплатное тело раздулось бы без предела, а с ним и поиск соседей.",
            ..times
        },
        4 => Field {
            lo: 0.0,
            hi: 4.0,
            limit: "Дальше каждый обнюхивал бы каждый тик полмира, и мир тормозил бы.",
            ..times
        },
        5..=9 => share,
        _ => times,
    }
}

const BASE_FIELDS: [Field; 57] = [
    // ── World: what a game begins with ──────────────────────────────────────────
    Field {
        key: Key::Creatures,
        label: "Существ на старте",
        hint: "Сколько существ на каждом участке 6000×4000 в первый момент. \
               В большом мире их во столько раз больше, во сколько он больше.",
        lo: 1.0,
        hi: 200.0,
        step: 1.0,
        format: int,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    // The share of the strategy's second variant; the rest are standard. Later the strategies
    // are inherited and mutate on their own, and at zero lurkers will appear anyway.
    Field {
        key: Key::Lurkers,
        label: "Затаившихся на старте",
        hint: "Доля существ, которые, не видя еды, бродят втрое медленнее и тратят меньше. \
               Остальные — стандартные. Дальше стратегия наследуется и изредка мутирует.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: percent,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    // Founders' diets: shares relative to the sum of the four sliders (all zero — herbivores).
    Field {
        key: Key::Herbivores,
        label: "Травоядных на старте",
        hint: "Доли основателей по питанию считаются от суммы четырёх ползунков. Дальше питание \
               наследуется и изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: int,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    Field {
        key: Key::Omnivores,
        label: "Всеядных на старте",
        hint: "Доли основателей по питанию считаются от суммы четырёх ползунков. Дальше питание \
               наследуется и изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: int,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    Field {
        key: Key::Scavengers,
        label: "Падальщиков на старте",
        hint: "Доли основателей по питанию считаются от суммы четырёх ползунков. Дальше питание \
               наследуется и изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: int,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    Field {
        key: Key::Carnivores,
        label: "Мясоедов на старте",
        hint: "Доли основателей по питанию считаются от суммы четырёх ползунков. Дальше питание \
               наследуется и изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: int,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    Field {
        key: Key::MeatFounders,
        label: "Размер мясоедов на старте",
        hint: "Размер основателей-падальщиков и мясоедов; 100 — обычный, 200 — вдвое крупнее. Равные \
               остальным, они не находили добычи и умирали с голоду. Дальше размер наследуется как обычно.",
        lo: 0.25,
        hi: 5.0,
        step: 0.05,
        format: per100,
        shown: 100.0,
        tab: Tab::World,
        rule: None,
        ..NUMBER
    },
    // ── Food: how much of it and where it grows ─────────────────────────────────
    // A multiplier, not the number itself: in the config the pace is 2.50008 a tick, and it does
    // not fall on a slider's grid. A multiplier of 1.0 gives exactly the config, bit for bit.
    Field {
        key: Key::PlantGrowth,
        label: "Условная удельная плотность энергии",
        hint: "Сколько пищи приходит в мир: столько растений вырастает за тик на каждом участке \
               6000×4000. Меньше — мир голоднее, существ меньше, и за еду они борются жёстче. \
               Больше — сытнее и теснее.",
        // the user plays at 0.2 without overcrowding and wanted to go lower still
        lo: 0.04,
        hi: 20.0,
        step: 0.004,
        format: |v| format!("{:.2} в тик", v * Rules::default().plant_rate),
        shown: PLANT_RATE,
        unit: " в тик",
        decimals: 2,
        limit: "Больше 50 в тик на участок — и тик тонет в посевах, а растений всё равно не больше, чем мест под них.",
        tab: Tab::Food,
        rule: Some("plant_rate"),
        ..NUMBER
    },
    Field {
        key: Key::PlantEnergy,
        label: "Энергия растения",
        hint: "Сколько энергии в одном растении. Для сравнения: полный бак базового существа — 100.",
        lo: 10.0,
        hi: 500.0,
        step: 1.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_energy"),
        ..NUMBER
    },
    Field {
        key: Key::PlantBiteYield,
        label: "Усвоение растений",
        hint: "Какую часть энергии растения травоядный получает, съев его целиком (за пять укусов). \
               Остальное теряется: траву переваривать трудно. Другие питания усваивают от этого свою долю.",
        lo: 0.05,
        hi: 1.0,
        step: 0.01,
        shown: 100.0,
        unit: " %",
        format: |v| format!("{:.0}%", v * 100.0),
        tab: Tab::Food,
        rule: Some("plant_bite_yield"),
        ..NUMBER
    },
    // The profile by depth and by width independently; a profile's parameter is visible only when
    // its profile is chosen. The layer genes do not adjust to the food.
    Field {
        key: Key::PlantDepthProfile,
        label: "Еда по глубине",
        hint: "Как густо растут растения от поверхности ко дну. «Игровое» — как в море: сытый верх, \
               ниже еды всё меньше, дно мёртвое. «Океаническое» — как в настоящем океане: больше всего еды \
               чуть ниже поверхности, глубже всё меньше. Гены слоя от этого не меняются: существа сами найдут, \
               на какой глубине выгоднее жить.",
        lo: 0.0,
        hi: 6.0,
        step: 1.0,
        format: profile_label,
        tab: Tab::Food,
        rule: Some("plant_depth_profile"),
        choices: &PROFILES,
        ..NUMBER
    },
    Field {
        key: Key::PlantDepthSteepness,
        label: "Крутизна по глубине",
        hint: "Как быстро еды становится меньше с глубиной. Чем больше, тем сильнее всё прижато \
               к поверхности: при 8 у дна еды в 3000 раз меньше, чем наверху, при 0 — поровну. \
               В «игровом» спад начинается под ровным верхним слоем, в «океаническом» — под \
               самым богатым слоем.",
        lo: 0.0,
        hi: 30.0,
        step: 0.5,
        format: |v| format!("{v:.1}"),
        tab: Tab::Food,
        rule: Some("plant_depth_steepness"),
        visible: |s| matches!(s.food(Along::Depth), Profile::Exp | Profile::Game | Profile::Ocean),
        ..NUMBER
    },
    Field {
        key: Key::PlantDepthEnd,
        label: "Еды у дна",
        hint: "Линейно: сколько еды у дна, в процентах от поверхности. 100% — поровну.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: percent,
        tab: Tab::Food,
        rule: Some("plant_depth_end"),
        visible: |s| s.food(Along::Depth) == Profile::Linear,
        ..NUMBER
    },
    Field {
        key: Key::PlantDepthBend,
        label: "Изгиб по глубине",
        hint: "Логарифм: чем больше, тем глубже еды почти столько же, сколько наверху, \
               и тем резче она кончается у дна.",
        lo: 0.0,
        hi: 200.0,
        step: 1.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_depth_bend"),
        visible: |s| s.food(Along::Depth) == Profile::Log,
        ..NUMBER
    },
    Field {
        key: Key::PlantDepthWaves,
        label: "Полос по глубине",
        hint: "Волны: сколько богатых едой полос от поверхности до дна.",
        lo: 1.0,
        hi: 10.0,
        step: 1.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_depth_waves"),
        visible: |s| s.food(Along::Depth) == Profile::Waves,
        ..NUMBER
    },
    Field {
        key: Key::PlantDepthAmplitude,
        label: "Размах полос по глубине",
        hint: "Волны: насколько между полосами беднее, чем в них. 100% — между полосами пусто.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: percent,
        tab: Tab::Food,
        rule: Some("plant_depth_amplitude"),
        visible: |s| s.food(Along::Depth) == Profile::Waves,
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthProfile,
        label: "Еда по ширине",
        hint: "Как густо растут растения слева направо. Складывается с профилем по глубине: \
               например, волны по ширине дают богатые столбы.",
        lo: 0.0,
        hi: 6.0,
        step: 1.0,
        format: profile_label,
        tab: Tab::Food,
        rule: Some("plant_width_profile"),
        choices: &PROFILES,
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthSteepness,
        label: "Крутизна по ширине",
        hint: "Экспонента: чем больше, тем сильнее еда прижата к левому краю. При 0 — поровну.",
        lo: 0.0,
        hi: 30.0,
        step: 0.5,
        format: |v| format!("{v:.1}"),
        tab: Tab::Food,
        rule: Some("plant_width_steepness"),
        visible: |s| matches!(s.food(Along::Width), Profile::Exp | Profile::Game | Profile::Ocean),
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthEnd,
        label: "Еды у правого края",
        hint: "Линейно: сколько еды у правого края, в процентах от левого. 100% — поровну.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: percent,
        tab: Tab::Food,
        rule: Some("plant_width_end"),
        visible: |s| s.food(Along::Width) == Profile::Linear,
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthBend,
        label: "Изгиб по ширине",
        hint: "Логарифм: чем больше, тем дальше вправо еды почти столько же, сколько слева, \
               и тем резче она кончается у правого края.",
        lo: 0.0,
        hi: 200.0,
        step: 1.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_width_bend"),
        visible: |s| s.food(Along::Width) == Profile::Log,
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthWaves,
        label: "Полос по ширине",
        hint: "Волны: сколько богатых едой полос слева направо. Одна — остров посередине.",
        lo: 1.0,
        hi: 10.0,
        step: 1.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_width_waves"),
        visible: |s| s.food(Along::Width) == Profile::Waves,
        ..NUMBER
    },
    Field {
        key: Key::PlantWidthAmplitude,
        label: "Размах полос по ширине",
        hint: "Волны: насколько между полосами беднее, чем в них. 100% — между полосами пусто.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: percent,
        tab: Tab::Food,
        rule: Some("plant_width_amplitude"),
        visible: |s| s.food(Along::Width) == Profile::Waves,
        ..NUMBER
    },
    // Patches keep the profile: each region of equal fertility holds the same room for plants.
    Field {
        key: Key::PlantPatches,
        label: "Зарослей",
        hint: "Сколько островков зарослей на каждом участке 6000×4000. Заросли разные: крупные и мелкие, \
               густые и редкие, но профиль еды они не меняют. 0 — растения рассыпаны по профилю.",
        lo: 0.0,
        hi: 100.0,
        step: 1.0,
        format: |v| if v == 0.0 { "россыпью".into() } else { format!("{v:.0}") },
        tab: Tab::Food,
        rule: Some("plant_patches"),
        ..NUMBER
    },
    Field {
        key: Key::PlantPatchSize,
        label: "Размер зарослей",
        hint: "Средний радиус островка; каждый берёт свой, от половины до полутора. \
               Для сравнения: базовое существо — 40 в поперечнике.",
        lo: 50.0,
        hi: 1000.0,
        step: 10.0,
        format: int,
        tab: Tab::Food,
        rule: Some("plant_patch_size"),
        visible: |s| s.get(Key::PlantPatches) > 0.0,
        ..NUMBER
    },
    Field {
        key: Key::PlantPatchShare,
        label: "Доля в зарослях",
        hint: "Сколько растений растёт в зарослях, остальные — россыпью между ними. \
               Глубже света меньше: заросли там реже, мельче и беднее, а россыпи больше.",
        lo: 0.0,
        hi: 100.0,
        step: 5.0,
        format: |v| format!("{v:.0}%"),
        tab: Tab::Food,
        rule: Some("plant_patch_share"),
        visible: |s| s.get(Key::PlantPatches) > 0.0,
        ..NUMBER
    },
    // ── Body: the price of upkeep and of birth ──────────────────────────────────
    Field {
        key: Key::CostScale,
        label: "Общая цена жизни",
        hint: "Цена всего содержания тела — размера, скорости и зрения сразу; 100 — обычная. \
               Дороже — падальщики почти не держатся, а при 200 мясоеды держатся лишь в половине \
               миров; дешевле — мясоедов и падальщиков больше.",
        lo: 0.1,
        hi: 10.0,
        step: 0.02,
        format: |v| format!("{:.0}", v * COST_SHOWN),
        shown: COST_SHOWN,
        tab: Tab::Body,
        rule: Some("cost_scale"),
        ..NUMBER
    },
    Field {
        key: Key::SizeCost,
        label: "Цена размера",
        hint: "Сколько обходится тело базового размера (40); 100 — как задумано. \
               Степень решает, насколько дороже тело крупнее.",
        lo: 0.1,
        hi: 10.0,
        step: 0.01,
        format: per100,
        shown: 100.0,
        limit: "Почти бесплатное тело раздулось бы без предела, а с ним и поиск соседей.",
        tab: Tab::Body,
        rule: Some("size_cost"),
        ..NUMBER
    },
    Field {
        key: Key::SizePower,
        label: "Степень цены размера",
        hint: "Как быстро дорожает тело крупнее базового: цена растёт как размер в этой степени. \
               При 2,5 вдвое крупнее — в 5,7 раза дороже. Ниже 2 крупное тело окупается, \
               и размер раздувается без предела.",
        lo: 1.0,
        hi: 4.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Body,
        rule: Some("size_power"),
        ..NUMBER
    },
    Field {
        key: Key::SpeedCost,
        label: "Цена скорости",
        hint: "Сколько обходится базовая скорость (10); 100 — цена игры, вдвое дешевле задуманной: \
               бегать дешевле, и мясоедам с падальщиками проще жить. 200 — как задумано.",
        lo: 0.1,
        hi: 10.0,
        step: 0.005,
        format: |v| format!("{:.0}", v * SPEED_COST_SHOWN),
        shown: SPEED_COST_SHOWN,
        tab: Tab::Body,
        rule: Some("speed_cost"),
        ..NUMBER
    },
    Field {
        key: Key::SpeedPower,
        label: "Степень цены скорости",
        hint: "Как быстро дорожает скорость выше базовой: при 2 вдвое быстрее — вчетверо дороже.",
        lo: 1.0,
        hi: 4.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Body,
        rule: Some("speed_power"),
        ..NUMBER
    },
    Field {
        key: Key::SpeedMassPower,
        label: "Масса в цене бега",
        hint: "Двигать большое тело дороже: цена скорости ещё умножается на (размер / 40) в этой степени. \
               При 1 вдвое крупнее бегает вдвое дороже, при 0 масса не важна.",
        lo: 0.0,
        hi: 3.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Body,
        rule: Some("speed_mass_power"),
        ..NUMBER
    },
    Field {
        key: Key::SightCost,
        label: "Цена зрения",
        hint: "Сколько обходится базовое зрение (400); 100 — как задумано.",
        lo: 0.1,
        hi: 10.0,
        step: 0.01,
        format: per100,
        shown: 100.0,
        limit: "Почти бесплатное зрение выросло бы на весь мир, и каждый тик каждый смотрел бы на всех.",
        tab: Tab::Body,
        rule: Some("sight_cost"),
        ..NUMBER
    },
    Field {
        key: Key::SightPower,
        label: "Степень цены зрения",
        hint: "Как быстро дорожает зрение дальше базового: при 2 вдвое дальше — вчетверо дороже. \
               Чем ниже, тем дешевле дальнозоркость и тем дальше видят потомки.",
        lo: 1.0,
        hi: 4.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Body,
        rule: Some("sight_power"),
        ..NUMBER
    },
    Field {
        key: Key::ReproCost,
        label: "Цена рождения",
        hint: "Сколько энергии родитель теряет на само рождение — сверх той, что отдаёт ребёнку в бак.",
        lo: 0.0,
        hi: 100.0,
        step: 1.0,
        format: int,
        tab: Tab::Body,
        rule: Some("repro_cost"),
        ..NUMBER
    },
    // ── Fight ───────────────────────────────────────────────────────────────────
    Field {
        key: Key::MeleeDamage,
        label: "Сила ближнего удара",
        hint: "Сколько здоровья снимает удар вблизи и сколько энергии он стоит ударившему — \
               в процентах от собственного размера. Крупнее цели — бьёт сильнее (см. перевес размера).",
        lo: 0.01,
        hi: 0.25,
        step: 0.001,
        shown: 100.0,
        unit: " %",
        decimals: 1,
        format: |v| format!("{:.0}%", v * 100.0),
        tab: Tab::Combat,
        rule: Some("melee_damage_share"),
        ..NUMBER
    },
    Field {
        key: Key::MeleeSizePower,
        label: "Перевес размера",
        hint: "Насколько крупный бьёт сильнее: урон умножается на «во сколько раз я крупнее» в этой степени. \
               При 1,25 втрое крупный мясоед убивает травоядного за два удара, при 1,75 — за один; \
               при 0 урон просто по размеру.",
        lo: 0.0,
        hi: 3.0,
        step: 0.01,
        decimals: 2,
        format: |v| format!("{v:.2}"),
        tab: Tab::Combat,
        rule: Some("melee_size_power"),
        ..NUMBER
    },
    Field {
        key: Key::ShotDamage,
        label: "Сила выстрела",
        hint: "Урон выстрела в процентах от размера стрелка; больше четверти здоровья цели он не снимает. \
               Стреляет только тот, в чьей программе есть установка «стрелять».",
        lo: 0.005,
        hi: 0.10,
        step: 0.001,
        shown: 100.0,
        unit: " %",
        decimals: 1,
        format: |v| format!("{:.1}%", v * 100.0),
        tab: Tab::Combat,
        rule: Some("shot_damage_share"),
        ..NUMBER
    },
    Field {
        key: Key::ShotCost,
        label: "Цена выстрела",
        hint: "Сколько энергии стоит один выстрел, в процентах от размера стрелка.",
        lo: 0.005,
        hi: 0.20,
        step: 0.001,
        shown: 100.0,
        unit: " %",
        decimals: 1,
        format: |v| format!("{:.1}%", v * 100.0),
        tab: Tab::Combat,
        rule: Some("shot_energy_share"),
        ..NUMBER
    },
    Field {
        key: Key::ShotPeriod,
        label: "Пауза между выстрелами",
        hint: "Сколько тиков стрелок перезаряжается между выстрелами.",
        lo: 1.0,
        hi: 1000.0,
        step: 1.0,
        format: |v| format!("{v:.0} тиков"),
        unit: " тиков",
        tab: Tab::Combat,
        rule: Some("shot_period"),
        ..NUMBER
    },
    // ── Corpses ─────────────────────────────────────────────────────────────────
    Field {
        key: Key::CorpseFresh,
        label: "Свежий",
        hint: "Сколько тиков после смерти труп лежит свежим на месте гибели. Свежее мясо любят мясоеды. \
               Потом он сразу становится гнилью и тонет.",
        lo: 1.0,
        hi: 10_000.0,
        step: 1.0,
        format: |v| format!("{v:.0} тиков"),
        unit: " тиков",
        tab: Tab::Corpses,
        rule: Some("corpse_fresh"),
        ..NUMBER
    },
    Field {
        key: Key::CorpseBones,
        label: "Кости лежат",
        hint: "Сколько тиков лежат кости — десятая часть мяса, что остаётся от объеденного или сгнившего \
               трупа. Кости тают равномерно всё это время. Их ест только падальщик.",
        lo: 1.0,
        hi: 100_000.0,
        step: 1.0,
        format: |v| format!("{v:.0} тиков"),
        unit: " тиков",
        limit: "Дольше — кости копятся десятками тысяч, и мир тормозит на них.",
        tab: Tab::Corpses,
        rule: Some("corpse_bones"),
        ..NUMBER
    },
    Field {
        key: Key::CorpseSink,
        label: "Скорость погружения",
        hint: "Сколько проходит тонущий труп за тик, пока не ляжет на своё место внизу. Пока тонет, его \
               можно есть. Обычное существо плывёт 10 за тик: труп быстрее падальщики не догонят. \
               В высоком мире труп может истлеть, не долетев до дна.",
        lo: 0.1,
        hi: 50.0,
        step: 0.1,
        format: |v| format!("{v:.1} за тик"),
        unit: " за тик",
        decimals: 1,
        tab: Tab::Corpses,
        rule: Some("corpse_sink"),
        ..NUMBER
    },
    Field {
        key: Key::CorpseDecay,
        label: "Сгнил до костей",
        hint: "К какому тику после смерти гниль истлевает до костей: мясо тает равномерно всё это время. \
               Объеденный труп оставляет кости раньше.",
        lo: 1.0,
        hi: 20_000.0,
        step: 1.0,
        format: |v| format!("{v:.0} тиков"),
        unit: " тиков",
        limit: "Дольше — трупы копятся тысячами, и мир тормозит на них.",
        tab: Tab::Corpses,
        rule: Some("corpse_decay"),
        ..NUMBER
    },
    Field {
        key: Key::CorpseRest,
        label: "Зона на дне",
        hint: "В какой нижней части глубины трупы и скелеты ложатся, каждый на своё место. \
               25% — по всей нижней четверти.",
        lo: 0.0,
        hi: 100.0,
        step: 1.0,
        format: percent,
        unit: " %",
        tab: Tab::Corpses,
        rule: Some("corpse_rest"),
        ..NUMBER
    },
    Field {
        key: Key::CorpseBonesSink,
        label: "Кости тонут",
        hint: "Сколько проходят кости за тик, пока не лягут на место трупа внизу: тяжёлые, они падают \
               быстро.",
        lo: 0.1,
        hi: 500.0,
        step: 0.1,
        format: |v| format!("{v:.1} за тик"),
        unit: " за тик",
        decimals: 1,
        tab: Tab::Corpses,
        rule: Some("corpse_bones_sink"),
        ..NUMBER
    },
    // ── Cold ────────────────────────────────────────────────────────────────────
    Field {
        key: Key::ThermoTop,
        label: "Тёплая вода до",
        hint: "До какой глубины вода тёплая, % глубины мира. Ниже она остывает до «Холодной воды с».",
        lo: 0.0,
        hi: 100.0,
        step: 1.0,
        format: percent,
        unit: " %",
        tab: Tab::Food,
        rule: Some("thermo_top"),
        ..NUMBER
    },
    Field {
        key: Key::ThermoBottom,
        label: "Холодная вода с",
        hint: "С какой глубины вода холодная, % глубины мира. Хладнокровные там живут дешевле и плавают \
               медленнее — насколько, решает их ген «хладнокровие».",
        lo: 0.0,
        hi: 100.0,
        step: 1.0,
        format: percent,
        unit: " %",
        tab: Tab::Food,
        rule: Some("thermo_bottom"),
        ..NUMBER
    },
    // ── Evolution ───────────────────────────────────────────────────────────────
    Field {
        key: Key::MutationSigma,
        label: "Сила мутаций",
        hint: "Насколько сильно гены ребёнка отличаются от родительских, когда он мутирует: при 0,3 \
               размер ребёнка в среднем на четверть другой. Мало — эволюция стоит, много — хаос.",
        lo: 0.05,
        hi: 2.0,
        step: 0.01,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Evolution,
        rule: Some("mutation_sigma"),
        ..NUMBER
    },
    Field {
        key: Key::CloneShare,
        label: "Копий без мутаций",
        hint: "Какая доля детей рождается точной копией родителя. Копии держат проверенный геном \
               линии, остальные дети мутируют.",
        lo: 0.0,
        hi: 1.0,
        step: 0.01,
        format: |v| format!("{:.0}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        tab: Tab::Evolution,
        rule: Some("clone_share"),
        ..NUMBER
    },
    Field {
        key: Key::MinMutability,
        label: "Нижний предел мутагенности",
        hint: "Мутагенность — наследуемый множитель силы мутаций. Отбор тянет её вниз (ребёнок без \
               мутаций в среднем удачнее), и без предела она уходила в ноль — эволюция замирала.",
        lo: 0.0,
        hi: 2.0,
        step: 0.01,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Evolution,
        rule: Some("min_mutability"),
        ..NUMBER
    },
    Field {
        key: Key::ProgramMutation,
        label: "Мутации поведения",
        hint: "Какая доля мутирующих детей получает изменённую программу поведения: сдвиг числа, \
               другое условие, отрицание или действие, перестановку, копию, удаление, новый блок, \
               выключение или включение блока, режим вместе с его проверкой, блок из другой \
               дорожки. Умножается на мутагенность поведения родителя. Сломанную программу \
               отсеивает судьба её носителя.",
        lo: 0.0,
        hi: 1.0,
        step: 0.001,
        format: |v| format!("{:.1}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 1,
        tab: Tab::Evolution,
        rule: Some("program_mutation"),
        ..NUMBER
    },
    Field {
        key: Key::ProgramDrift,
        label: "Дрейф чисел поведения",
        hint: "У каждого мутирующего ребёнка треть чисел программы поведения сдвигается, как гены: \
               при 100 порог — примерно на 10 пунктов, отношение — на 0,2, время — на четверть базы. \
               Умножается на мутагенность поведения. 0 — числа меняют только редкие мутации.",
        lo: 0.0,
        hi: 5.0,
        step: 0.05,
        format: per100,
        shown: 100.0,
        tab: Tab::Evolution,
        rule: Some("program_drift"),
        ..NUMBER
    },
    Field {
        key: Key::DietStep,
        label: "Шаг питания назад",
        hint: "Шанс, что у мутирующего ребёнка питание сдвинется на шаг назад, к растениям, или вбок: \
               всеядный → травоядный, падальщик и мясоед → всеядный или друг в друга. От мутагенности \
               не зависит.",
        lo: 0.0,
        hi: 0.2,
        step: 0.0001,
        format: |v| format!("{:.2}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 2,
        tab: Tab::Evolution,
        rule: Some("diet_step"),
        ..NUMBER
    },
    Field {
        key: Key::DietJump,
        label: "Скачок питания",
        hint: "Шанс, что у мутирующего ребёнка питание сменится на любое другое, не только соседнее. \
               У травоядного вместо него свои скачки — в мясоеда и в падальщика.",
        lo: 0.0,
        hi: 0.1,
        step: 0.00001,
        format: |v| format!("{:.3}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 3,
        tab: Tab::Evolution,
        rule: Some("diet_jump"),
        ..NUMBER
    },
    Field {
        key: Key::DietMeatStep,
        label: "Шаг к мясу",
        hint: "Шанс, что у мутирующего ребёнка питание сдвинется на шаг к мясу: травоядный → всеядный, \
               всеядный → падальщик или мясоед. Мясоедов и падальщиков нет среди основателей, они \
               появляются только так. От мутагенности не зависит.",
        lo: 0.0,
        hi: 0.2,
        step: 0.0001,
        format: |v| format!("{:.2}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 2,
        tab: Tab::Evolution,
        rule: Some("diet_meat_step"),
        ..NUMBER
    },
    Field {
        key: Key::DietLeapCarnivore,
        label: "Скачок травоядного в мясоеда",
        hint: "Шанс, что у мутирующего травоядного ребёнок сразу станет мясоедом, минуя всеядного. \
               Всеядных в мире мало, и мясоеды не должны зависеть только от них. От мутагенности не \
               зависит.",
        lo: 0.0,
        hi: 0.1,
        step: 0.00001,
        format: |v| format!("{:.3}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 3,
        tab: Tab::Evolution,
        rule: Some("diet_leap_carnivore"),
        ..NUMBER
    },
    Field {
        key: Key::DietLeapScavenger,
        label: "Скачок травоядного в падальщика",
        hint: "Шанс, что у мутирующего травоядного ребёнок сразу станет падальщиком, минуя всеядного. \
               От мутагенности не зависит.",
        lo: 0.0,
        hi: 0.1,
        step: 0.00001,
        format: |v| format!("{:.3}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 3,
        tab: Tab::Evolution,
        rule: Some("diet_leap_scavenger"),
        ..NUMBER
    },
];

pub fn field(key: Key) -> &'static Field {
    FIELDS.iter().find(|f| f.key == key).expect("поле есть в FIELDS")
}

/// The scale presets of the «Новый мир» screen.
pub const PRESETS: [(&str, f64); 4] =
    [("Как раньше", 1.0), ("Остров", 10.0), ("Материк", 100.0), ("Планета", 1000.0)];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // ── start ──────────────────────────────────────────────────────────────
    pub seed: u64,
    /// A new seed for every «Начать».
    pub random_seed: bool,
    pub scale: f64,
    pub shape: Shape,
    /// The sliders' values, in the order of `FIELDS`.
    pub values: [f64; FIELDS.len()],
    // ── screen ───────────────────────────────────────────────────────────────
    pub fullscreen: bool,
    pub ui_scale: f64,
    pub show_fps: bool,
    // ── the world's computation ──────────────────────────────────────────────
    /// Threads for the creatures' decisions: 0 — auto (`auto_threads`), 1 — no parallelism.
    pub threads: usize,
    /// Keep the thread that steps the world on a hybrid processor's fast cores.
    pub fast_cores: bool,
}

/// The processor's threads.
pub fn cpu_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Auto: every thread but two — one for the window and the drawing, one for the simulation thread,
/// which waits while the pool works.
pub fn auto_threads() -> usize {
    cpu_threads().saturating_sub(2).max(1)
}

impl Default for Settings {
    fn default() -> Self {
        let rules = Rules::default();
        Settings {
            seed: 1,
            random_seed: true,
            // the player's own world (2026-09-26): big, wide, hungry, half lurkers, food falls off
            // softer than the engine's default; the founders' diets are the engine's (no meat
            // eaters), though the player deals 55/25/10/10
            scale: 20.0,
            shape: Shape::R2x1,
            values: FIELDS.map(|f| match f.key {
                Key::Creatures => CREATURES_AT_START as f64,
                Key::CostScale => GAME_COST_SCALE,
                Key::SpeedCost => GAME_SPEED_COST,
                Key::PlantGrowth => 0.2,
                Key::Lurkers => 50.0,
                Key::PlantDepthSteepness => 5.0,
                Key::Herbivores => DIET_START_MIX[0],
                Key::Omnivores => DIET_START_MIX[1],
                Key::Scavengers => DIET_START_MIX[2],
                Key::Carnivores => DIET_START_MIX[3],
                Key::MeatFounders => MEAT_FOUNDER_SIZE,
                _ => rules.get(f.rule.expect("правило")).expect("правило есть в Rules"),
            }),
            fullscreen: false,
            ui_scale: 0.0,
            show_fps: false,
            threads: 0,
            fast_cores: true,
        }
    }
}

fn index(key: Key) -> usize {
    FIELDS.iter().position(|f| f.key == key).expect("поле есть в FIELDS")
}

impl Settings {
    pub fn get(&self, key: Key) -> f64 {
        self.values[index(key)]
    }

    pub fn set(&mut self, key: Key, value: f64) {
        self.values[index(key)] = field(key).snap(value);
    }

    /// The chosen food profile along an axis.
    pub fn food(&self, along: Along) -> Profile {
        Profile::of(self.get(match along {
            Along::Depth => Key::PlantDepthProfile,
            Along::Width => Key::PlantWidthProfile,
        }))
    }

    /// A rule's slider as the rule's value: food growth is shown as a factor of the default.
    fn rule_value(&self, key: Key) -> f64 {
        let v = self.get(key);
        if key == Key::PlantGrowth { Rules::default().plant_rate * v } else { v }
    }

    /// The world's rules from the sliders, checked as a whole (`Rules::with_all`), so the order of
    /// the fields does not matter. Each slider's limits lie inside what `Rules` allows (a test);
    /// a combination whose upkeep left the numbers would be refused, and the window shows that.
    pub fn rules(&self) -> Result<Rules, String> {
        Rules::default().with_all(
            FIELDS.iter().filter(|f| f.live()).map(|f| (f.rule.expect("правило"), self.rule_value(f.key))),
        )
    }

    /// The world's `current` rules with only the sliders moved since `taken` read them from it (the
    /// lab's «Применить»): a rule nobody touched keeps its exact value, even one off the sliders' grid
    /// or past their range (`life-app --rule cost_scale=20`), which the sliders would clamp. So a
    /// slider's value can still be refused: with such a rule its upkeep may leave the numbers.
    pub fn rules_over(&self, current: &Rules, taken: &Settings) -> Result<Rules, String> {
        current.with_all(
            FIELDS
                .iter()
                .filter(|f| f.live() && self.get(f.key) != taken.get(f.key))
                .map(|f| (f.rule.expect("правило"), self.rule_value(f.key))),
        )
    }

    /// The rules' sliders from the world's current rules (for the lab on the fly).
    pub fn take_rules(&mut self, rules: &Rules) {
        for f in FIELDS.iter().filter(|f| f.live()) {
            let v = rules.get(f.rule.expect("правило")).expect("правило есть в Rules");
            let v = if f.key == Key::PlantGrowth { v / Rules::default().plant_rate } else { v };
            self.values[index(f.key)] = f.snap(v);
        }
    }

    /// The world from the settings. The counts at the start are given for the base area and grow
    /// with the area — the density, and with it the balance, do not depend on the scale. Refused
    /// when the rules are (`rules`).
    pub fn world_config(&self, seed: u64) -> Result<WorldConfig, String> {
        let space = Space::new(self.scale, self.shape);
        let per_area = |key| (self.get(key) * space.area_ratio()).round() as usize;
        // the shares of the variants (standard, the second); zero — an empty mix, as in the default world
        let mix = |key| {
            let p = self.get(key);
            if p > 0.0 { vec![100.0 - p, p] } else { Vec::new() }
        };
        Ok(WorldConfig {
            seed,
            scale: self.scale,
            shape: self.shape,
            rules: self.rules()?,
            n_creatures: Some(per_area(Key::Creatures)),
            strategies: mix(Key::Lurkers),
            diets: [Key::Herbivores, Key::Omnivores, Key::Scavengers, Key::Carnivores]
                .map(|k| self.get(k))
                .to_vec(),
            meat_founder_size: self.get(Key::MeatFounders),
        })
    }

    /// Return the defaults on one tab.
    pub fn reset(&mut self, tab: Tab) {
        let default = Settings::default();
        for (i, f) in FIELDS.iter().enumerate() {
            if f.tab == tab {
                self.values[i] = default.values[i];
            }
        }
        if tab == Tab::World {
            self.random_seed = default.random_seed;
            self.scale = default.scale;
            self.shape = default.shape;
        }
    }

    pub fn is_default(&self, key: Key) -> bool {
        self.get(key) == Settings::default().get(key)
    }

    // ── file ────────────────────────────────────────────────────────────────

    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("defaults".into(), DEFAULTS_VERSION.into());
        m.insert("seed".into(), self.seed.into());
        m.insert("random_seed".into(), self.random_seed.into());
        m.insert("scale".into(), self.scale.into());
        m.insert("shape".into(), self.shape.key().into());
        for (f, v) in FIELDS.iter().zip(self.values) {
            m.insert(json_key(f.key).into(), v.into());
        }
        m.insert("fullscreen".into(), self.fullscreen.into());
        m.insert("ui_scale".into(), self.ui_scale.into());
        m.insert("show_fps".into(), self.show_fps.into());
        m.insert("threads".into(), self.threads.into());
        m.insert("fast_cores".into(), self.fast_cores.into());
        Value::Object(m)
    }

    /// Settings from JSON. Garbage, foreign keys and values out of limits do not crash the game:
    /// what is unfit stays at the default, the rest is clamped.
    fn from_json(data: &Value) -> Settings {
        let mut s = Settings::default();
        let Some(m) = data.as_object() else { return s };
        // a file saved before defaults changed keeps the player's other values, not those
        let version = m.get("defaults").and_then(Value::as_u64).unwrap_or(0);
        let num = |k: &str| m.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
        let flag = |k: &str| m.get(k).and_then(Value::as_bool);
        if let Some(v) = num("seed") {
            s.seed = (v.round() as u64).clamp(1, SEED_MAX);
        }
        if let Some(v) = flag("random_seed") {
            s.random_seed = v;
        }
        if let Some(v) = num("scale") {
            s.scale = v.clamp(MIN_SCALE, MAX_SCALE);
        }
        if let Some(shape) = m.get("shape").and_then(Value::as_str).and_then(|k| Shape::parse(k).ok()) {
            s.shape = shape;
        }
        for (i, f) in FIELDS.iter().enumerate() {
            // before the rename to «существ» the key was different
            let old = (f.key == Key::Creatures).then_some("n_vegetarians");
            if CHANGED_DEFAULTS.iter().any(|&(key, changed)| key == f.key && version < changed) {
                continue;
            }
            if let Some(v) = num(json_key(f.key)).or_else(|| old.and_then(num)) {
                s.values[i] = f.snap(v);
            }
        }
        if let Some(v) = flag("fullscreen") {
            s.fullscreen = v;
        }
        if let Some(v) = num("ui_scale") {
            s.ui_scale =
                UI_SCALES.into_iter().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs())).unwrap_or(0.0);
        }
        if let Some(v) = flag("show_fps") {
            s.show_fps = v;
        }
        // more threads than the processor has (a file from another computer) — as many as it has
        if let Some(v) = m.get("threads").and_then(Value::as_u64) {
            s.threads = (v as usize).min(cpu_threads());
        }
        if let Some(v) = flag("fast_cores") {
            s.fast_cores = v;
        }
        s
    }

    /// The threads the decisions get: the chosen number, or auto.
    pub fn threads_in_use(&self) -> usize {
        if self.threads == 0 { auto_threads() } else { self.threads }
    }

    /// Settings from a file; no file or a broken one — the defaults.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|data| Settings::from_json(&data))
            .unwrap_or_default()
    }

    /// Writes atomically: first into a temporary file next to it, then swaps it in. An interrupted
    /// write will not leave half a file. A write error does not crash the game — the settings just
    /// will not be remembered.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let dir = path.parent().ok_or("у файла настроек нет папки")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&self.to_json()).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!(".settings-{}.tmp", std::process::id()));
        let result = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result.map_err(|e| e.to_string())
    }
}

fn json_key(key: Key) -> &'static str {
    match key {
        Key::Creatures => "n_creatures",
        Key::PlantGrowth => "plant_growth",
        Key::MutationSigma => "mutation_sigma",
        Key::PlantEnergy => "plant_energy",
        Key::CostScale => "cost_scale",
        Key::SizePower => "size_power",
        Key::SightPower => "sight_power",
        Key::Lurkers => "lurkers_percent",
        Key::Herbivores => "diet_herbivores",
        Key::Omnivores => "diet_omnivores",
        Key::Carnivores => "diet_carnivores",
        Key::Scavengers => "diet_scavengers",
        Key::MeatFounders => "meat_founder_size",
        // for the food profiles the file's key is the rule's name
        _ => field(key).rule.expect("у поля есть правило"),
    }
}

/// Where the settings file lies: `%APPDATA%\lifegame\config\settings.json` and its analogues.
/// A new name, not the Python version's `user_settings.json`: the formats differ. The game used to
/// be called TinyLife: a file left in that folder is copied over once, so the settings survive.
pub fn default_path() -> Option<PathBuf> {
    let path =
        directories::ProjectDirs::from("", "", "lifegame").map(|d| d.config_dir().join("settings.json"))?;
    if !path.exists()
        && let Some(old) = directories::ProjectDirs::from("", "", "TinyLife")
            .map(|d| d.config_dir().join("settings.json"))
            .filter(|old| old.exists())
    {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::copy(old, &path);
    }
    Some(path)
}

/// What has changed in the rules — a line for the chronicle: «энергия растения 50 → 80».
pub fn describe_change(old: &Settings, new: &Settings) -> Option<String> {
    let parts: Vec<String> = FIELDS
        .iter()
        .filter(|f| f.live() && old.get(f.key) != new.get(f.key))
        .map(|f| {
            format!(
                "{} {} → {}",
                f.label.to_lowercase(),
                (f.format)(old.get(f.key)),
                (f.format)(new.get(f.key))
            )
        })
        .collect();
    (!parts.is_empty()).then(|| format!("правила: {}", parts.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hints name the numbers from the gene table in words: if the base changes, the hint must
    /// not lie.
    #[test]
    fn подсказки_цитируют_базы_генов() {
        use life_core::config::ENERGY_PER_SIZE;
        use life_core::genome::creature::{GENES, Gene};
        let hint = |key| FIELDS.iter().find(|f| f.key == key).expect("поле есть").hint;
        let tank = GENES[Gene::Size as usize].base * ENERGY_PER_SIZE;
        assert!(hint(Key::PlantEnergy).contains(&format!("существа — {tank:.0}")));
    }

    /// The game's default is the player's own world: a dearer life (×2, fewer creatures; the calm
    /// profile «Спокойнее» is ×3), a fifth of the food, a softer food slope; ×20, 2:1, half lurkers.
    #[test]
    fn the_default_is_the_players_own_world() {
        let want = Rules::default()
            .with("cost_scale", 2.0)
            .and_then(|r| r.with("speed_cost", 0.5))
            .and_then(|r| r.with("plant_rate", Rules::default().plant_rate * 0.2))
            .and_then(|r| r.with("plant_depth_steepness", 5.0))
            .unwrap();
        let s = Settings::default();
        assert_eq!(s.rules(), Ok(want));
        assert_eq!((s.scale, s.shape), (20.0, Shape::R2x1));
        assert_eq!(s.world_config(1).unwrap().strategies, vec![50.0, 50.0]);
        let mut s = Settings::default();
        for (key, v) in [
            (Key::CostScale, 1.0),
            (Key::SpeedCost, 1.0),
            (Key::PlantGrowth, 1.0),
            (Key::PlantDepthSteepness, 8.0),
        ] {
            s.set(key, v);
        }
        assert_eq!(s.rules(), Ok(Rules::default()));
    }

    #[test]
    fn every_sliders_edges_make_valid_rules() {
        // both edges of every slider assemble the rules without a refusal
        for f in &FIELDS {
            for v in [f.lo, f.hi] {
                let mut s = Settings::default();
                s.set(f.key, v);
                s.rules().unwrap_or_else(|e| panic!("{:?} = {v}: {e}", f.key));
                s.world_config(1).unwrap_or_else(|e| panic!("{:?} = {v}: {e}", f.key));
            }
        }
    }

    #[test]
    fn значение_ложится_на_сетку_шага_без_хвостов() {
        let f = field(Key::MutationSigma);
        assert_eq!(f.snap(0.3000001), 0.3);
        assert_eq!(f.snap(99.0), 2.0);
        assert_eq!(f.snap(-5.0), 0.05);
    }

    /// The threads survive a file: saved and read back as they were; a file from a bigger computer
    /// asks for no more threads than this one has, garbage leaves the defaults (auto, fast cores).
    #[test]
    fn потоки_сохраняются_и_не_выходят_за_процессор() {
        let s = Settings { threads: 1, fast_cores: false, ..Default::default() };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        let big = Settings::from_json(&serde_json::json!({ "threads": 999, "fast_cores": true }));
        assert_eq!(big.threads, cpu_threads());
        let junk = Settings::from_json(&serde_json::json!({ "threads": -3, "fast_cores": "да" }));
        assert_eq!((junk.threads, junk.fast_cores), (0, true));
        assert_eq!(Settings::default().threads_in_use(), auto_threads());
        assert!(auto_threads() >= 1 && auto_threads() <= cpu_threads());
    }

    #[test]
    fn файл_переживает_мусор_и_чужие_ключи() {
        let data = serde_json::json!({
            "seed": 1e12, "scale": 1e9, "plant_energy": 9999, "size_power": "много",
            "mutation_sigma": f64::NAN.to_string(), "чужой": 1, "ui_scale": 1.3, "fullscreen": 1,
            "shape": "круг"
        });
        let s = Settings::from_json(&data);
        assert_eq!(s.shape, Settings::default().shape);
        assert_eq!(s.seed, SEED_MAX);
        assert_eq!(s.scale, MAX_SCALE);
        assert_eq!(s.get(Key::PlantEnergy), 500.0, "held at the hard limit");
        assert_eq!(s.get(Key::SizePower), Settings::default().get(Key::SizePower));
        assert_eq!(s.ui_scale, 1.25);
        assert!(!s.fullscreen, "не bool — по умолчанию");
        assert_eq!(Settings::from_json(&serde_json::json!([1, 2])), Settings::default());
    }

    #[test]
    fn запись_атомарна_и_читается_обратно() {
        let dir = std::env::temp_dir().join(format!("lifegame-test-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = Settings { seed: 777, scale: 100.0, shape: Shape::Square, ..Default::default() };
        s.set(Key::PlantEnergy, 80.0);
        s.set(Key::ShotDamage, 0.05);
        s.set(Key::ShotCost, 0.10);
        s.save(&path).expect("запись");
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "{ битый").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let leftovers =
            std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name() != "settings.json");
        assert_eq!(leftovers.count(), 0, "временных файлов не осталось");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn старые_настройки_получают_исходные_цены_а_выборочный_сброс_не_трогает_остальное() {
        let mut s = Settings::from_json(&serde_json::json!({"plant_energy": 80}));
        assert_eq!(s.get(Key::ShotDamage), life_core::config::SHOT_DAMAGE_SHARE);
        assert_eq!(s.get(Key::ShotCost), life_core::config::SHOT_ENERGY_SHARE);
        s.set(Key::ShotDamage, 0.06);
        s.set(Key::ShotCost, 0.10);
        s.set(Key::ShotDamage, Settings::default().get(Key::ShotDamage));
        assert_eq!(s.get(Key::ShotDamage), life_core::config::SHOT_DAMAGE_SHARE);
        assert_eq!(s.get(Key::ShotCost), 0.10);
        assert_eq!(s.get(Key::PlantEnergy), 80.0);
    }

    #[test]
    fn численности_растут_с_площадью() {
        let s = Settings { scale: 100.0, ..Default::default() };
        assert_eq!(s.world_config(1).unwrap().creatures_at_start(), CREATURES_AT_START * 100);
        let s = Settings { scale: 1.0, ..Default::default() };
        assert_eq!(s.world_config(1).unwrap().creatures_at_start(), 20);
    }

    /// The second strategy's share is the world's mix; zero — an empty mix.
    #[test]
    fn доли_стратегий_становятся_смесью_мира() {
        let mut s = Settings::default();
        s.set(Key::Lurkers, 0.0);
        assert!(s.world_config(1).unwrap().strategies.is_empty());
        s.set(Key::Lurkers, 30.0);
        assert_eq!(s.world_config(1).unwrap().strategies, vec![70.0, 30.0]);
    }

    /// The variants' labels are in the order of the engine's profiles, and every rule of a food
    /// profile has a field.
    #[test]
    fn поля_еды_покрывают_профили() {
        assert_eq!(PROFILES, Profile::ALL.map(Profile::label));
        let food = |k: &&&str| life_core::flora::split_key(k).is_some() || k.starts_with("plant_patch");
        for key in life_core::rules::RULE_KEYS.iter().filter(food) {
            assert!(FIELDS.iter().any(|f| f.rule == Some(*key)), "{key}: нет поля");
        }
        for f in FIELDS.iter().filter(|f| !f.choices.is_empty()) {
            assert_eq!((f.lo, f.hi, f.step), (0.0, (f.choices.len() - 1) as f64, 1.0), "{}", f.label);
        }
    }

    #[test]
    fn параметр_профиля_виден_при_своём_профиле() {
        let mut s = Settings::default();
        let shown = |s: &Settings, key| (field(key).visible)(s);
        assert!(shown(&s, Key::PlantDepthSteepness), "by default «игровое»: its fall has a steepness");
        assert!(!shown(&s, Key::PlantDepthWaves));
        assert!(shown(&s, Key::PlantPatchSize), "по умолчанию — заросли");
        s.set(Key::PlantDepthProfile, Profile::Exp.index());
        assert!(shown(&s, Key::PlantDepthSteepness));
        s.set(Key::PlantPatches, 0.0);
        assert!(!shown(&s, Key::PlantPatchSize), "россыпью — размера нет");
        s = Settings::default();
        assert!(!shown(&s, Key::PlantWidthSteepness), "по ширине — равномерно, параметров нет");
        s.set(Key::PlantWidthProfile, Profile::Waves.index());
        assert!(shown(&s, Key::PlantWidthWaves) && shown(&s, Key::PlantWidthAmplitude));
        assert!(!shown(&s, Key::PlantWidthEnd));
        assert_eq!(s.rules().unwrap().plant_width.kind(), Profile::Waves);
        let old = Settings::default();
        assert_eq!(describe_change(&old, &s).as_deref(), Some("правила: еда по ширине равномерно → волны"));
    }

    #[test]
    fn изменение_правил_описывается_для_хроники() {
        let old = Settings::default();
        let mut new = old.clone();
        new.set(Key::PlantEnergy, 80.0);
        new.set(Key::Creatures, 50.0); // a starting condition is not a rule
        assert_eq!(describe_change(&old, &new).as_deref(), Some("правила: энергия растения 50 → 80"));
        assert_eq!(describe_change(&old, &old), None);
    }

    /// The lab's «Применить» sends only what the player moved: a rule the world got past the
    /// sliders' range or off their grid (`life-app --rule cost_scale=20`) keeps its exact value, as
    /// the chronicle's note, which names only the moved slider, says.
    #[test]
    fn applying_the_lab_keeps_the_rules_nobody_touched() {
        let current = Rules::default().with("cost_scale", 20.0).unwrap().with("size_power", 2.537).unwrap();
        let mut taken = Settings::default();
        taken.take_rules(&current);
        assert_ne!(taken.rules(), Ok(current.clone()), "the sliders alone clamp and round them");
        let mut lab = taken.clone();
        lab.set(Key::PlantEnergy, 80.0);
        assert_eq!(describe_change(&taken, &lab).as_deref(), Some("правила: энергия растения 50 → 80"));
        assert_eq!(lab.rules_over(&current, &taken), current.with("plant_energy", 80.0));
        assert_eq!(taken.rules_over(&current, &taken), Ok(current), "nothing moved, nothing changes");

        // past the sliders' range a slider's own value can overflow the upkeep: refused, not a panic
        let costly = Rules::default().with("cost_scale", 1e303).unwrap();
        let mut taken = Settings::default();
        taken.take_rules(&costly);
        let mut lab = taken.clone();
        lab.set(Key::SpeedCost, 10.0);
        assert!(lab.rules_over(&costly, &taken).is_err());
    }

    /// An old file (with predator keys, «n_vegetarians», the removed `cannibal_ratio` rule and
    /// the removed combat switch `cannibalism`) is read; combat is always on anyway.
    #[test]
    fn старый_файл_настроек_читается() {
        let old = Settings::from_json(&serde_json::json!({
            "n_predators": 10, "plant_energy": 80, "n_vegetarians": 50, "cannibal_ratio": 1.5,
            "cannibalism": 0
        }));
        assert_eq!(old.get(Key::PlantEnergy), 80.0);
        assert_eq!(old.get(Key::Creatures), 50.0, "старый ключ численности читается");
        assert_eq!(old.rules(), Settings::default().rules().unwrap().with("plant_energy", 80.0));
    }

    /// A file saved before the ocean reform keeps the player's values but takes the new defaults
    /// that changed then; one saved after keeps everything, and a saved file says its version.
    #[test]
    fn a_file_from_before_changed_defaults_takes_them() {
        use life_core::flora::Profile;
        let saved = serde_json::json!({
            "plant_energy": 80, "plant_depth_profile": Profile::Game.index(), "corpse_fresh": 150,
            "corpse_decay": 1800
        });
        let old = Settings::from_json(&saved);
        assert_eq!(old.get(Key::PlantEnergy), 80.0, "the player's value stays");
        let new = Settings::default();
        for (key, _) in CHANGED_DEFAULTS {
            assert_eq!(old.get(key), new.get(key), "{key:?} takes the new default");
        }
        assert_eq!(new.get(Key::PlantDepthProfile), Profile::Ocean.index());
        let mut current = saved.clone();
        current["defaults"] = DEFAULTS_VERSION.into();
        let kept = Settings::from_json(&current);
        assert_eq!(kept.get(Key::PlantDepthProfile), Profile::Game.index(), "chosen after the change");
        assert_eq!(kept.get(Key::CorpseFresh), 150.0);
        assert_eq!(new.to_json()["defaults"], DEFAULTS_VERSION);
    }

    /// A file of version 1 saved the old prices (life 3, speed 1) as its defaults: it takes the new
    /// ones and keeps what it chose after the ocean reform; a version 2 file takes only the new
    /// price of speed, and a current file keeps both.
    #[test]
    fn a_file_takes_only_the_defaults_changed_after_it() {
        use life_core::flora::Profile;
        let saved = serde_json::json!({
            "defaults": 1, "cost_scale": 3.0, "speed_cost": 1.0,
            "plant_depth_profile": Profile::Game.index()
        });
        let s = Settings::from_json(&saved);
        assert_eq!(s.get(Key::CostScale), GAME_COST_SCALE);
        assert_eq!(s.get(Key::SpeedCost), GAME_SPEED_COST);
        assert_eq!(s.get(Key::PlantDepthProfile), Profile::Game.index());
        let mut v2 = saved.clone();
        v2["defaults"] = 2.into();
        let s = Settings::from_json(&v2);
        assert_eq!(s.get(Key::CostScale), 3.0, "the price of life chosen after its change");
        assert_eq!(s.get(Key::SpeedCost), GAME_SPEED_COST);
        let mut current = saved.clone();
        current["defaults"] = DEFAULTS_VERSION.into();
        let s = Settings::from_json(&current);
        assert_eq!((s.get(Key::CostScale), s.get(Key::SpeedCost)), (3.0, 1.0));
    }

    /// The game's price of life reads 100, and every factor with a common value shows it as 100.
    #[test]
    fn factors_read_100_at_their_common_value() {
        let s = Settings::default();
        for key in [Key::CostScale, Key::SizeCost, Key::SpeedCost, Key::SightCost, Key::ProgramDrift] {
            let f = field(key);
            assert_eq!((f.format)(s.get(key)), "100", "{}", f.label);
            assert_eq!(s.get(key) * f.shown, 100.0, "{}", f.label);
        }
        let f = field(Key::CostScale);
        assert_eq!(f.snap(3.0), 3.0, "the old price is on the grid");
        assert_eq!((f.format)(3.0), "150");
        let f = field(Key::SpeedCost);
        assert_eq!(f.snap(1.0), 1.0, "the designed price of speed is on the grid");
        assert_eq!((f.format)(1.0), "200");
        assert_eq!((field(Key::Diet(3, 0)).format)(s.get(Key::Diet(3, 0))), "300", "carnivore strike ×3");
        assert_eq!((field(Key::Diet(1, 5)).format)(s.get(Key::Diet(1, 5))), "80%", "a share stays in %");
    }

    #[test]
    fn ползунки_правил_берутся_из_мира() {
        let mut s = Settings::default();
        let mut rules_src = Settings::default();
        rules_src.set(Key::PlantGrowth, 2.0);
        rules_src.set(Key::CostScale, 3.0);
        s.take_rules(&rules_src.rules().unwrap());
        assert_eq!(s.get(Key::PlantGrowth), 2.0);
        assert_eq!(s.get(Key::CostScale), 3.0);
    }
}
