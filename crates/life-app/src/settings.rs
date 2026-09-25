//! Что игрок может настроить, в каких пределах и где это хранится. Порт
//! `app/settings.py` (тег python-final).
//!
//! `FIELDS` — единственное место, где описан каждый ползунок: подпись,
//! пояснение, пределы, шаг и формат. По нему строятся экран «Новый мир» и
//! лаборатория на ходу, и по нему же зажимаются значения из файла настроек —
//! поэтому они не могут разойтись.

use std::path::{Path, PathBuf};

use life_core::config::CREATURES_AT_START;
use life_core::flora::{Along, Profile};
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space, WorldConfig};
use serde_json::{Map, Value};

pub const SEED_MAX: u64 = 99_999;
/// Масштаб интерфейса; 0 — как в системе.
pub const UI_SCALES: [f64; 6] = [0.0, 1.0, 1.25, 1.5, 1.75, 2.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// «Мир»: с чего начинается партия.
    World,
    /// «Еда»: где растут растения. Это правила мира, их можно менять и на ходу.
    Food,
    /// «Лаборатория»: правила мира. Их можно менять и на ходу.
    Lab,
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
    Cannibalism,
    ReproCost,
    MeleeDamage,
    ShotDamage,
    ShotCost,
    ShotPeriod,
    PlantBiteYield,
}

pub struct Field {
    pub key: Key,
    pub label: &'static str,
    pub hint: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
    pub format: fn(f64) -> String,
    pub tab: Tab,
    /// Имя правила в `Rules` (None — стартовое условие, а не правило).
    pub rule: Option<&'static str>,
    /// Непустой — выбор из вариантов (значение — номер варианта), а не ползунок.
    pub choices: &'static [&'static str],
    /// Показывать ли поле сейчас: параметр профиля еды виден, только когда
    /// выбран его профиль.
    pub shown: fn(&Settings) -> bool,
    /// Галочка (значение 0 или 1), а не ползунок.
    pub toggle: bool,
}

/// Общее у полей-ползунков: всегда видны, вариантов нет.
const SLIDER: Field = Field {
    key: Key::Creatures,
    label: "",
    hint: "",
    lo: 0.0,
    hi: 1.0,
    step: 1.0,
    format: int,
    tab: Tab::World,
    rule: None,
    choices: &[],
    shown: |_| true,
    toggle: false,
};

/// Общее у галочек: 0 — нет, 1 — да.
const TOGGLE: Field = Field { lo: 0.0, hi: 1.0, step: 1.0, format: yes_no, toggle: true, ..SLIDER };

fn yes_no(v: f64) -> String {
    if v != 0.0 { "да" } else { "нет" }.into()
}

/// Подписи профилей еды — по порядку `Profile::ALL` (сверено тестом).
const PROFILES: [&str; 5] = ["равномерно", "линейно", "экспонента", "логарифм", "волны"];

fn profile_label(v: f64) -> String {
    Profile::of(v).label().into()
}

impl Field {
    /// Значение в пределах и на сетке шага (так его ставит ползунок).
    pub fn snap(&self, value: f64) -> f64 {
        let v = value.clamp(self.lo, self.hi);
        let v = self.lo + ((v - self.lo) / self.step).round() * self.step;
        // без хвостов вроде 0.30000000000000004
        (v.min(self.hi) * 1e6).round() / 1e6
    }

    /// Можно ли менять посреди партии: правила — да, стартовые условия — нет.
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

pub const FIELDS: [Field; 27] = [
    // ── Мир ──────────────────────────────────────────────────────────────────
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
        ..SLIDER
    },
    // Множитель, а не само число: в конфиге темп — 2.50008 в тик, и на сетку
    // ползунка он не ложится. Множитель 1.0 даёт ровно конфиг, бит в бит.
    Field {
        key: Key::PlantGrowth,
        label: "Рост растений",
        hint: "Сколько растений появляется за тик на участке 6000×4000. Больше еды — больше существ.",
        lo: 0.2,
        hi: 3.0,
        step: 0.1,
        format: |v| format!("{:.1} в тик", v * Rules::default().plant_rate),
        tab: Tab::World,
        rule: Some("plant_rate"),
        ..SLIDER
    },
    // Доля второго варианта стратегии; остальные — стандартные. Дальше стратегии
    // наследуются и мутируют сами, и при нуле затаившиеся всё равно появятся.
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
        ..SLIDER
    },
    // ── Лаборатория ─────────────────────────────────────────────────────────
    Field {
        key: Key::MutationSigma,
        label: "Сила мутаций",
        hint: "Насколько гены потомка отличаются от родительских. Мало — эволюция стоит, много — хаос.",
        lo: 0.05,
        hi: 1.0,
        step: 0.05,
        format: |v| format!("{v:.2}"),
        tab: Tab::Lab,
        rule: Some("mutation_sigma"),
        ..SLIDER
    },
    Field {
        key: Key::PlantEnergy,
        label: "Энергия растения",
        hint: "Сколько энергии даёт одно растение. Полный бак базового существа — 100.",
        lo: 10.0,
        hi: 150.0,
        step: 5.0,
        format: int,
        tab: Tab::Lab,
        rule: Some("plant_energy"),
        ..SLIDER
    },
    Field {
        key: Key::CostScale,
        label: "Цена статов",
        hint: "Множитель ко всей цене содержания: размера, скорости и зрения.",
        lo: 0.25,
        hi: 4.0,
        step: 0.25,
        format: |v| format!("×{v:.2}"),
        tab: Tab::Lab,
        rule: Some("cost_scale"),
        ..SLIDER
    },
    Field {
        key: Key::SizePower,
        label: "Крутизна цены размера",
        hint: "Как быстро дорожает размер. Ниже 2 крупное тело окупается, и размер раздувается без предела.",
        lo: 1.0,
        hi: 3.5,
        step: 0.1,
        format: |v| format!("{v:.1}"),
        tab: Tab::Lab,
        rule: Some("size_power"),
        ..SLIDER
    },
    Field {
        key: Key::SightPower,
        label: "Крутизна цены зрения",
        hint: "Как быстро дорожает зрение. Чем ниже, тем дешевле дальнозоркость и тем дальше видят потомки.",
        lo: 1.0,
        hi: 3.0,
        step: 0.1,
        format: |v| format!("{v:.1}"),
        tab: Tab::Lab,
        rule: Some("sight_power"),
        ..SLIDER
    },
    // ── Еда ─────────────────────────────────────────────────────────────────
    // Профиль по глубине и по ширине независимо; параметр профиля виден, только
    // когда выбран его профиль. Гены слоя под еду не подстраиваются.
    Field {
        key: Key::PlantDepthProfile,
        label: "Еда по глубине",
        hint: "Как густо растут растения от поверхности ко дну. Гены слоя от этого не меняются: \
               существа сами найдут, на какой глубине выгоднее жить.",
        lo: 0.0,
        hi: 4.0,
        step: 1.0,
        format: profile_label,
        tab: Tab::Food,
        rule: Some("plant_depth_profile"),
        choices: &PROFILES,
        ..SLIDER
    },
    Field {
        key: Key::PlantDepthSteepness,
        label: "Крутизна по глубине",
        hint: "Экспонента: чем больше, тем сильнее еда прижата к поверхности. \
               При 8 у дна еды в 3000 раз меньше, чем наверху; при 0 — поровну.",
        lo: 0.0,
        hi: 30.0,
        step: 0.5,
        format: |v| format!("{v:.1}"),
        tab: Tab::Food,
        rule: Some("plant_depth_steepness"),
        shown: |s| s.food(Along::Depth) == Profile::Exp,
        ..SLIDER
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
        shown: |s| s.food(Along::Depth) == Profile::Linear,
        ..SLIDER
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
        shown: |s| s.food(Along::Depth) == Profile::Log,
        ..SLIDER
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
        shown: |s| s.food(Along::Depth) == Profile::Waves,
        ..SLIDER
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
        shown: |s| s.food(Along::Depth) == Profile::Waves,
        ..SLIDER
    },
    Field {
        key: Key::PlantWidthProfile,
        label: "Еда по ширине",
        hint: "Как густо растут растения слева направо. Складывается с профилем по глубине: \
               например, волны по ширине дают богатые столбы.",
        lo: 0.0,
        hi: 4.0,
        step: 1.0,
        format: profile_label,
        tab: Tab::Food,
        rule: Some("plant_width_profile"),
        choices: &PROFILES,
        ..SLIDER
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
        shown: |s| s.food(Along::Width) == Profile::Exp,
        ..SLIDER
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
        shown: |s| s.food(Along::Width) == Profile::Linear,
        ..SLIDER
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
        shown: |s| s.food(Along::Width) == Profile::Log,
        ..SLIDER
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
        shown: |s| s.food(Along::Width) == Profile::Waves,
        ..SLIDER
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
        shown: |s| s.food(Along::Width) == Profile::Waves,
        ..SLIDER
    },
    // ── Каннибализм (лаборатория) ───────────────────────────────────────────
    Field {
        key: Key::Cannibalism,
        label: "Каннибализм",
        hint: "Включает активную охоту и бои при соприкосновении тел. \
               Родню и участников своей стаи атаковать нельзя.",
        tab: Tab::Lab,
        rule: Some("cannibalism"),
        ..TOGGLE
    },
    Field {
        key: Key::ReproCost,
        label: "Цена рождения",
        hint: "Энергия, которую родитель тратит сверх доли, отданной ребёнку.",
        lo: 0.0,
        hi: 50.0,
        step: 1.0,
        format: int,
        tab: Tab::Lab,
        rule: Some("repro_cost"),
        ..SLIDER
    },
    Field {
        key: Key::MeleeDamage,
        label: "Сила ближнего удара",
        hint: "Урон и цена удара в процентах от собственного диаметра. Урон ограничен четвертью здоровья цели.",
        lo: 0.01,
        hi: 0.25,
        step: 0.01,
        format: |v| format!("{:.0}%", v * 100.0),
        tab: Tab::Lab,
        rule: Some("melee_damage_share"),
        ..SLIDER
    },
    Field {
        key: Key::ShotDamage,
        label: "Сила выстрела",
        hint: "Урон в процентах от собственного диаметра. Урон ограничен четвертью здоровья цели.",
        lo: 0.005,
        hi: 0.10,
        step: 0.005,
        format: |v| format!("{:.1}%", v * 100.0),
        tab: Tab::Lab,
        rule: Some("shot_damage_share"),
        ..SLIDER
    },
    Field {
        key: Key::ShotCost,
        label: "Цена выстрела",
        hint: "Расход энергии на один выстрел в процентах от собственного диаметра.",
        lo: 0.005,
        hi: 0.20,
        step: 0.005,
        format: |v| format!("{:.1}%", v * 100.0),
        tab: Tab::Lab,
        rule: Some("shot_energy_share"),
        ..SLIDER
    },
    Field {
        key: Key::ShotPeriod,
        label: "Пауза между выстрелами",
        hint: "Минимальное число тиков между двумя выстрелами одного существа.",
        lo: 1.0,
        hi: 30.0,
        step: 1.0,
        format: |v| format!("{v:.0} тиков"),
        tab: Tab::Lab,
        rule: Some("shot_period"),
        ..SLIDER
    },
    Field {
        key: Key::PlantBiteYield,
        label: "Усвоение растений",
        hint: "Какая доля энергии растения достанется существу за все пять порций.",
        lo: 0.05,
        hi: 1.0,
        step: 0.01,
        format: |v| format!("{:.0}%", v * 100.0),
        tab: Tab::Lab,
        rule: Some("plant_bite_yield"),
        ..SLIDER
    },
];

pub fn field(key: Key) -> &'static Field {
    FIELDS.iter().find(|f| f.key == key).expect("поле есть в FIELDS")
}

/// Масштабы-пресеты экрана «Новый мир».
pub const PRESETS: [(&str, f64); 4] =
    [("Как раньше", 1.0), ("Остров", 10.0), ("Материк", 100.0), ("Планета", 1000.0)];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // ── старт ──────────────────────────────────────────────────────────────
    pub seed: u64,
    /// Новый сид на каждый «Начать».
    pub random_seed: bool,
    pub scale: f64,
    pub shape: Shape,
    /// Значения ползунков, в порядке `FIELDS`.
    pub values: [f64; FIELDS.len()],
    // ── экран ────────────────────────────────────────────────────────────────
    pub fullscreen: bool,
    pub ui_scale: f64,
    pub show_fps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let rules = Rules::default();
        Settings {
            seed: 1,
            random_seed: true,
            scale: 1.0,
            shape: WorldConfig::default().shape,
            values: FIELDS.map(|f| match f.key {
                Key::Creatures => CREATURES_AT_START as f64,
                // Упор игры — на каннибализм; у движка он по умолчанию выключен.
                Key::Cannibalism => 1.0,
                // Меньше плотность популяции при прежней модели жизненного цикла.
                Key::CostScale => 3.0,
                Key::PlantGrowth => 1.0,
                Key::Lurkers => 0.0,
                _ => rules.get(f.rule.expect("правило")).expect("правило есть в Rules"),
            }),
            fullscreen: false,
            ui_scale: 0.0,
            show_fps: false,
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

    /// Выбранный профиль еды по оси.
    pub fn food(&self, along: Along) -> Profile {
        Profile::of(self.get(match along {
            Along::Depth => Key::PlantDepthProfile,
            Along::Width => Key::PlantWidthProfile,
        }))
    }

    /// Правила мира из ползунков.
    pub fn rules(&self) -> Rules {
        let mut rules = Rules::default();
        for f in FIELDS.iter().filter(|f| f.live()) {
            let v = self.get(f.key);
            let v = if f.key == Key::PlantGrowth { Rules::default().plant_rate * v } else { v };
            // пределы FIELDS лежат внутри допустимого для Rules — проверено тестом
            rules = rules.with(f.rule.expect("правило"), v).expect("значение ползунка допустимо");
        }
        rules
    }

    /// Ползунки правил — из действующих правил мира (для лаборатории на ходу).
    pub fn take_rules(&mut self, rules: &Rules) {
        for f in FIELDS.iter().filter(|f| f.live()) {
            let v = rules.get(f.rule.expect("правило")).expect("правило есть в Rules");
            let v = if f.key == Key::PlantGrowth { v / Rules::default().plant_rate } else { v };
            self.values[index(f.key)] = f.snap(v);
        }
    }

    /// Мир из настроек. Численности на старте заданы на базовый участок и
    /// растут с площадью — плотность, а с ней и баланс, от масштаба не зависят.
    pub fn world_config(&self, seed: u64) -> WorldConfig {
        let space = Space::new(self.scale, self.shape);
        let per_area = |key| (self.get(key) * space.area_ratio()).round() as usize;
        // доли вариантов (стандартный, второй); ноль — пустая смесь, как у мира
        // по умолчанию
        let mix = |key| {
            let p = self.get(key);
            if p > 0.0 { vec![100.0 - p, p] } else { Vec::new() }
        };
        WorldConfig {
            seed,
            scale: self.scale,
            shape: self.shape,
            rules: self.rules(),
            n_creatures: Some(per_area(Key::Creatures)),
            strategies: mix(Key::Lurkers),
        }
    }

    /// Вернуть значения по умолчанию на одной вкладке.
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

    // ── файл ────────────────────────────────────────────────────────────────

    fn to_json(&self) -> Value {
        let mut m = Map::new();
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
        Value::Object(m)
    }

    /// Настройки из JSON. Мусор, чужие ключи и значения вне пределов не
    /// роняют игру: негодное остаётся по умолчанию, остальное зажимается.
    fn from_json(data: &Value) -> Settings {
        let mut s = Settings::default();
        let Some(m) = data.as_object() else { return s };
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
            // до переименования в «существ» ключ был другим
            let old = (f.key == Key::Creatures).then_some("n_vegetarians");
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
        s
    }

    /// Настройки из файла; нет файла или он битый — значения по умолчанию.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|data| Settings::from_json(&data))
            .unwrap_or_default()
    }

    /// Пишет атомарно: сначала во временный файл рядом, потом подменяет.
    /// Оборванная запись не оставит полфайла. Ошибка записи игру не роняет —
    /// настройки просто не запомнятся.
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
        // у профилей еды и каннибализма ключ файла — имя правила
        _ => field(key).rule.expect("у поля есть правило"),
    }
}

/// Где лежит файл настроек: `%APPDATA%\TinyLife\settings.json` и аналоги.
/// Новое имя, а не `user_settings.json` Python-версии: форматы разные.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "TinyLife").map(|d| d.config_dir().join("settings.json"))
}

/// Что поменялось в правилах — строка для хроники: «энергия растения 50 → 80».
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

    /// Подсказки называют числа из таблицы генов словами: поменяли базу —
    /// подсказка не должна врать.
    #[test]
    fn подсказки_цитируют_базы_генов() {
        use life_core::config::ENERGY_PER_SIZE;
        use life_core::genome::creature::{GENES, Gene};
        let hint = |key| FIELDS.iter().find(|f| f.key == key).expect("поле есть").hint;
        let tank = GENES[Gene::Size as usize].base * ENERGY_PER_SIZE;
        assert!(hint(Key::PlantEnergy).contains(&format!("существа — {tank:.0}")));
    }

    /// Игровой профиль включает бои и более дорогую жизнь для меньшей плотности.
    #[test]
    fn по_умолчанию_спокойный_игровой_профиль() {
        let want = Rules::default().with("cannibalism", 1.0).unwrap().with("cost_scale", 3.0).unwrap();
        assert_eq!(Settings::default().rules(), want);
        let mut s = Settings::default();
        s.set(Key::Cannibalism, 0.0);
        s.set(Key::CostScale, 1.0);
        assert_eq!(s.rules(), Rules::default());
    }

    #[test]
    fn пределы_ползунков_допустимы_для_правил() {
        // оба края каждого ползунка собирают правила без ошибки
        for f in &FIELDS {
            for v in [f.lo, f.hi] {
                let mut s = Settings::default();
                s.set(f.key, v);
                let _ = s.rules();
                let _ = s.world_config(1);
            }
        }
    }

    #[test]
    fn значение_ложится_на_сетку_шага_без_хвостов() {
        let f = field(Key::MutationSigma);
        assert_eq!(f.snap(0.3100001), 0.3);
        assert_eq!(f.snap(99.0), 1.0);
        assert_eq!(f.snap(-5.0), 0.05);
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
        assert_eq!(s.get(Key::PlantEnergy), 150.0);
        assert_eq!(s.get(Key::SizePower), Settings::default().get(Key::SizePower));
        assert_eq!(s.ui_scale, 1.25);
        assert!(!s.fullscreen, "не bool — по умолчанию");
        assert_eq!(Settings::from_json(&serde_json::json!([1, 2])), Settings::default());
    }

    #[test]
    fn запись_атомарна_и_читается_обратно() {
        let dir = std::env::temp_dir().join(format!("tinylife-test-{}", std::process::id()));
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
        assert_eq!(s.world_config(1).creatures_at_start(), CREATURES_AT_START * 100);
        assert_eq!(Settings::default().world_config(1).creatures_at_start(), 20);
    }

    /// Доля второй стратегии — смесь мира; ноль — пустая смесь, как по умолчанию.
    #[test]
    fn доли_стратегий_становятся_смесью_мира() {
        assert!(Settings::default().world_config(1).strategies.is_empty());
        let mut s = Settings::default();
        s.set(Key::Lurkers, 30.0);
        assert_eq!(s.world_config(1).strategies, vec![70.0, 30.0]);
    }

    /// Подписи вариантов — по порядку профилей движка, а у каждого правила
    /// профиля еды есть поле.
    #[test]
    fn поля_еды_покрывают_профили() {
        assert_eq!(PROFILES, Profile::ALL.map(Profile::label));
        for key in life_core::rules::RULE_KEYS.iter().filter(|k| life_core::flora::split_key(k).is_some()) {
            assert!(FIELDS.iter().any(|f| f.rule == Some(*key)), "{key}: нет поля");
        }
        for f in FIELDS.iter().filter(|f| !f.choices.is_empty()) {
            assert_eq!((f.lo, f.hi, f.step), (0.0, (f.choices.len() - 1) as f64, 1.0), "{}", f.label);
        }
    }

    #[test]
    fn параметр_профиля_виден_при_своём_профиле() {
        let mut s = Settings::default();
        let shown = |s: &Settings, key| (field(key).shown)(s);
        assert!(shown(&s, Key::PlantDepthSteepness), "по умолчанию — экспонента");
        assert!(!shown(&s, Key::PlantDepthWaves));
        assert!(!shown(&s, Key::PlantWidthSteepness), "по ширине — равномерно, параметров нет");
        s.set(Key::PlantWidthProfile, Profile::Waves.index());
        assert!(shown(&s, Key::PlantWidthWaves) && shown(&s, Key::PlantWidthAmplitude));
        assert!(!shown(&s, Key::PlantWidthEnd));
        assert_eq!(s.rules().plant_width.kind(), Profile::Waves);
        let old = Settings::default();
        assert_eq!(describe_change(&old, &s).as_deref(), Some("правила: еда по ширине равномерно → волны"));
    }

    #[test]
    fn изменение_правил_описывается_для_хроники() {
        let old = Settings::default();
        let mut new = old.clone();
        new.set(Key::PlantEnergy, 80.0);
        new.set(Key::Creatures, 50.0); // стартовое условие — не правило
        assert_eq!(describe_change(&old, &new).as_deref(), Some("правила: энергия растения 50 → 80"));
        assert_eq!(describe_change(&old, &old), None);
    }

    /// Combat is a toggle. An old file (with predator keys, «n_vegetarians» and the removed
    /// `cannibal_ratio` rule) is read, with combat on.
    #[test]
    fn галочка_каннибализма() {
        assert!(field(Key::Cannibalism).toggle);

        let old = Settings::from_json(&serde_json::json!({
            "n_predators": 10, "plant_energy": 80, "n_vegetarians": 50, "cannibal_ratio": 1.5
        }));
        assert!(old.get(Key::Cannibalism) == 1.0 && old.get(Key::PlantEnergy) == 80.0);
        assert_eq!(old.get(Key::Creatures), 50.0, "старый ключ численности читается");
        let mut new = Settings::default();
        new.set(Key::Cannibalism, 0.0);
        assert_eq!(
            describe_change(&Settings::default(), &new).as_deref(),
            Some("правила: каннибализм да → нет")
        );
    }

    #[test]
    fn ползунки_правил_берутся_из_мира() {
        let mut s = Settings::default();
        let mut rules_src = Settings::default();
        rules_src.set(Key::PlantGrowth, 2.0);
        rules_src.set(Key::CostScale, 3.0);
        s.take_rules(&rules_src.rules());
        assert_eq!(s.get(Key::PlantGrowth), 2.0);
        assert_eq!(s.get(Key::CostScale), 3.0);
    }
}
