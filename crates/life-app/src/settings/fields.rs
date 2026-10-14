//! `FIELDS`, the one description of every slider: its label and hint, its limits, step and
//! format, its tab and its rule. «Новый мир» and the lab are built by it, and a settings file is
//! clamped by it.

use life_core::Rules;
use life_core::flora::{Along, Profile};

use super::{COST_SHOWN, Key, PLANT_RATE, SPEED_COST_SHOWN, Settings, Tab};

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

/// The food profiles' labels — in the order of `Profile::ALL` (checked by a test).
const PROFILES: [&str; 7] =
    ["равномерно", "линейно", "экспонента", "логарифм", "волны", "игровое", "океаническое"];

/// The fields of how the dead sink: on land they lie where they fell (`Terrain`).
fn in_water(s: &Settings) -> bool {
    s.terrain == life_core::Terrain::Ocean
}

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
pub const FIELDS: [Field; 98] = {
    let mut all = [NUMBER; 98];
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

const FOUNDER_DIETS: &str = "Доли основателей по питанию — от суммы четырёх ползунков; все 0 — одни \
                             травоядные. Дальше питание наследуется и изредка мутирует: шансы — на \
                             вкладке «Эволюция».";
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

const BASE_FIELDS: [Field; 58] = [
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
    // The share of founders on the lurker template; the rest are standard. The strategy gene never
    // switches (`STRATEGY_SWITCH_CHANCE` is 0): at zero no creature carries the lurker template.
    Field {
        key: Key::Lurkers,
        label: "Затаившихся на старте",
        hint: "Доля основателей с шаблоном «затаившийся»: не видя еды, бродят втрое медленнее и \
               тратят меньше. Остальные — стандартные. Шаблон потомкам не меняется: наследуется и \
               мутирует сама программа.",
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
        hint: FOUNDER_DIETS,
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
        hint: FOUNDER_DIETS,
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
        hint: FOUNDER_DIETS,
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
        hint: FOUNDER_DIETS,
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
        label: "Размер мясоедов и падальщиков",
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
               При 1,25 мясоед вдвое крупнее убивает травоядного за два удара, втрое крупнее — за \
               один; при 0 урон просто по размеру.",
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
        visible: in_water,
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
        visible: in_water,
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
        visible: in_water,
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
        tab: Tab::Water,
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
        tab: Tab::Water,
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
        hint: "Шанс, что каждая из двух программ мутирующего ребёнка получит одну перемену — от \
               сдвига числа до нового блока или блока из другой дорожки. Умножается на мутагенность \
               поведения родителя. Сломанную программу отсеивает судьба её носителя.",
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
        key: Key::MateDistance,
        label: "Близость для спаривания",
        hint: "Насколько могут различаться геномы, чтобы двое спарились: среднее по числовым генам \
               число шагов мутации между ними. Ребёнок-не-копия отстоит от родителя примерно на 0,8. \
               Меньше — виды расходятся быстрее; больше — скрещиваются и дальние линии. Разное \
               питание не спаривается никогда.",
        lo: 0.1,
        hi: 10.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        decimals: 2,
        tab: Tab::Evolution,
        rule: Some("mate_distance"),
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
        tab: Tab::Diets,
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
        tab: Tab::Diets,
        rule: Some("diet_jump"),
        ..NUMBER
    },
    Field {
        key: Key::DietMeatStep,
        label: "Шаг к мясу",
        hint: "Шанс, что у мутирующего ребёнка питание сдвинется на шаг к мясу: травоядный → всеядный, \
               всеядный → падальщик или мясоед. Главная дорога к мясу, когда мясоедов нет среди \
               основателей. От мутагенности не зависит.",
        lo: 0.0,
        hi: 0.2,
        step: 0.0001,
        format: |v| format!("{:.2}%", v * 100.0),
        shown: 100.0,
        unit: " %",
        decimals: 2,
        tab: Tab::Diets,
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
        tab: Tab::Diets,
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
        tab: Tab::Diets,
        rule: Some("diet_leap_scavenger"),
        ..NUMBER
    },
];

pub fn field(key: Key) -> &'static Field {
    FIELDS.iter().find(|f| f.key == key).expect("поле есть в FIELDS")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
