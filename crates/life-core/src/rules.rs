//! Правила мира, которые игрок меняет в «Лаборатории», не трогая config.rs.
//!
//! У каждого мира свой `Rules`: миров в процессе бывает два сразу (фон меню и
//! игра). Значения по умолчанию — ровно константы из config.rs.

use crate::config::*;
use crate::flora::{self, Along, FoodAxis, Profile};
use crate::genome::creature::{GENES, Gene};

const BASE_SIZE: f64 = GENES[Gene::Size as usize].base;
const BASE_SPEED: f64 = GENES[Gene::Speed as usize].base;
const BASE_VISION: f64 = GENES[Gene::Vision as usize].base;

/// Имена настраиваемых правил — для отчёта (`--rule имя=число`) и настроек.
/// Профили еды — по шесть на ось, по порядку `flora::AXIS_PARAMS`.
pub const RULE_KEYS: [&str; 27] = [
    "plant_rate",
    "plant_energy",
    "mutation_sigma",
    "cost_scale",
    "size_power",
    "speed_power",
    "sight_power",
    "plant_depth_profile",
    "plant_depth_steepness",
    "plant_depth_end",
    "plant_depth_bend",
    "plant_depth_waves",
    "plant_depth_amplitude",
    "plant_width_profile",
    "plant_width_steepness",
    "plant_width_end",
    "plant_width_bend",
    "plant_width_waves",
    "plant_width_amplitude",
    "cannibalism",
    "cannibal_ratio",
    "repro_cost",
    "melee_damage_share",
    "shot_damage_share",
    "shot_energy_share",
    "shot_period",
    "plant_bite_yield",
];

/// Параметры профилей еды, пока их не выбрали (config.rs).
const FOOD_AXIS: FoodAxis = FoodAxis {
    profile: 0.0,
    steepness: PLANT_WIDTH_DECAY,
    end: PLANT_LINEAR_END,
    bend: PLANT_LOG_BEND,
    waves: PLANT_WAVES,
    amplitude: PLANT_WAVE_AMPLITUDE,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// Растений за тик на базовый мир 6000x4000 (не вероятность).
    pub plant_rate: f64,
    /// Энергии за одно растение.
    pub plant_energy: f64,
    /// Разброс мутаций существ.
    pub mutation_sigma: f64,
    /// Множитель ко всей цене статов.
    pub cost_scale: f64,
    /// Крутизна цены размера.
    pub size_power: f64,
    /// Крутизна цены скорости.
    pub speed_power: f64,
    /// Крутизна цены зрения.
    pub sight_power: f64,
    /// Где растёт еда: профиль по глубине и по ширине (`flora.rs`).
    pub plant_depth: FoodAxis,
    pub plant_width: FoodAxis,
    /// Едят ли существа мелких сородичей: 0 — нет, 1 — да.
    pub cannibalism: f64,
    /// Во сколько раз жертва-сородич мельче едока (по размеру).
    pub cannibal_ratio: f64,
    /// Цена рождения и сила/стоимость боя; умолчания берутся из config.rs.
    pub repro_cost: f64,
    pub melee_damage_share: f64,
    pub shot_damage_share: f64,
    pub shot_energy_share: f64,
    pub shot_period: f64,
    /// Доля питательной ценности растения, усваиваемая за пять порций.
    pub plant_bite_yield: f64,
    // производные коэффициенты — считает `renormalize`
    size_coef: f64,
    speed_coef: f64,
    sight_coef: f64,
}

impl Default for Rules {
    fn default() -> Self {
        let mut r = Rules {
            plant_rate: PLANT_SPAWN_CHANCE,
            plant_energy: ENERGY_FROM_PLANT,
            mutation_sigma: MUTATION_SIGMA,
            cost_scale: 1.0,
            size_power: SIZE_ENERGY_POWER,
            speed_power: SPEED_ENERGY_POWER,
            sight_power: SIGHT_ENERGY_POWER,
            plant_depth: FoodAxis {
                profile: Profile::Exp.index(),
                steepness: PLANT_DEPTH_DECAY,
                ..FOOD_AXIS
            },
            plant_width: FoodAxis { profile: Profile::Uniform.index(), ..FOOD_AXIS },
            cannibalism: CANNIBALISM,
            cannibal_ratio: CANNIBAL_RATIO,
            repro_cost: REPRO_COST,
            melee_damage_share: MELEE_DAMAGE_SHARE,
            shot_damage_share: SHOT_DAMAGE_SHARE,
            shot_energy_share: SHOT_ENERGY_SHARE,
            shot_period: SHOT_PERIOD as f64,
            plant_bite_yield: PLANT_BITE_YIELD,
            size_coef: 0.0,
            speed_coef: 0.0,
            sight_coef: 0.0,
        };
        r.renormalize();
        r
    }
}

impl Rules {
    /// Копия с другим значением одного правила. Неизвестное имя и не конечное
    /// число — ошибка, а не молчание: NaN ломает не арифметику, а циклы
    /// (мутация ждёт gauss >= -0.9, а с сигмой NaN не дождётся никогда).
    pub fn with(&self, key: &str, value: f64) -> Result<Rules, String> {
        if !value.is_finite() {
            return Err(format!("правило {key}: нужно конечное число, а не {value}"));
        }
        let mut r = self.clone();
        if let Some((along, param)) = flora::split_key(key) {
            FoodAxis::check(param, value)
                .map_err(|need| format!("правило {key}: нужно {need}, а не {value}"))?;
            *r.food_axis_mut(along).slot(param).expect("параметр разобран split_key") = value;
            return Ok(r);
        }
        match key {
            "plant_rate" => r.plant_rate = value,
            "plant_energy" => r.plant_energy = value,
            "mutation_sigma" => r.mutation_sigma = value,
            "cost_scale" => r.cost_scale = value,
            "size_power" => r.size_power = value,
            "speed_power" => r.speed_power = value,
            "sight_power" => r.sight_power = value,
            "cannibalism" => r.cannibalism = value,
            "cannibal_ratio" => r.cannibal_ratio = value,
            "repro_cost" => r.repro_cost = value,
            "melee_damage_share" => r.melee_damage_share = value,
            "shot_damage_share" => r.shot_damage_share = value,
            "shot_energy_share" => r.shot_energy_share = value,
            "shot_period" => r.shot_period = value,
            "plant_bite_yield" => r.plant_bite_yield = value,
            _ => return Err(format!("нет такого правила: {key}; есть {}", RULE_KEYS.join(", "))),
        }
        // Пределы — только те, за которыми правило теряет смысл, а не «разумные»:
        // лаборатория для того и нужна, чтобы ломать баланс. Отрицательная цена
        // статов кормила бы существ за то, что они живут.
        let allowed = match key {
            "cannibalism" => value == 0.0 || value == 1.0,
            // при отношении 1 и меньше едят равных и даже крупных
            "cannibal_ratio" => value > 1.0,
            "shot_period" => value >= 1.0 && value.fract() == 0.0,
            "plant_bite_yield" => (0.0..=1.0).contains(&value),
            _ => value >= 0.0,
        };
        if !allowed {
            let need = match key {
                "cannibalism" => "0 (нет) или 1 (да)",
                "cannibal_ratio" => "число больше 1",
                "shot_period" => "целое число не меньше 1",
                "plant_bite_yield" => "число от 0 до 1",
                _ => "число не меньше 0",
            };
            return Err(format!("правило {key}: нужно {need}, а не {value}"));
        }
        r.renormalize();
        Ok(r)
    }

    /// Как `with`, но значение — текстом: числом, а у профиля еды — и именем
    /// (`plant_width_profile=waves`). Для флагов `--rule` отчёта и игры.
    pub fn with_text(&self, key: &str, text: &str) -> Result<Rules, String> {
        let text = text.trim();
        if !RULE_KEYS.contains(&key) {
            return Err(format!("нет такого правила: {key}; есть {}", RULE_KEYS.join(", ")));
        }
        if let Ok(value) = text.parse::<f64>() {
            return self.with(key, value);
        }
        match (flora::split_key(key), Profile::parse(text)) {
            (Some((_, "profile")), Some(p)) => self.with(key, p.index()),
            (Some((_, "profile")), None) => Err(format!(
                "правило {key}: нет профиля «{text}»; есть {}",
                Profile::ALL.map(|p| p.key()).join(", ")
            )),
            _ => Err(format!("правило {key}: «{text}» — не число")),
        }
    }

    /// Значение правила по имени (для отчёта и настроек).
    pub fn get(&self, key: &str) -> Option<f64> {
        if let Some((along, param)) = flora::split_key(key) {
            return self.food_axis(along).get(param);
        }
        Some(match key {
            "plant_rate" => self.plant_rate,
            "plant_energy" => self.plant_energy,
            "mutation_sigma" => self.mutation_sigma,
            "cost_scale" => self.cost_scale,
            "size_power" => self.size_power,
            "speed_power" => self.speed_power,
            "sight_power" => self.sight_power,
            "cannibalism" => self.cannibalism,
            "cannibal_ratio" => self.cannibal_ratio,
            "repro_cost" => self.repro_cost,
            "melee_damage_share" => self.melee_damage_share,
            "shot_damage_share" => self.shot_damage_share,
            "shot_energy_share" => self.shot_energy_share,
            "shot_period" => self.shot_period,
            "plant_bite_yield" => self.plant_bite_yield,
            _ => return None,
        })
    }

    /// Едят ли существа мелких сородичей.
    pub fn cannibals(&self) -> bool {
        self.cannibalism != 0.0
    }

    pub fn food_axis(&self, along: Along) -> &FoodAxis {
        match along {
            Along::Depth => &self.plant_depth,
            Along::Width => &self.plant_width,
        }
    }

    fn food_axis_mut(&mut self, along: Along) -> &mut FoodAxis {
        match along {
            Along::Depth => &mut self.plant_depth,
            Along::Width => &mut self.plant_width,
        }
    }

    /// Смена показателя меняет только КРУТИЗНУ: базовый стат стоит столько же,
    /// сколько стоил. Иначе показатель 1.5 вместо 2.5 сделал бы размер почти
    /// бесплатным целиком, и опыт мерил бы не то. При показателях из конфига
    /// множитель — base ** 0.0, то есть ровно 1.0.
    fn renormalize(&mut self) {
        self.size_coef =
            SIZE_ENERGY_COEF * self.cost_scale * BASE_SIZE.powf(SIZE_ENERGY_POWER - self.size_power);
        self.speed_coef =
            SPEED_ENERGY_COEF * self.cost_scale * BASE_SPEED.powf(SPEED_ENERGY_POWER - self.speed_power);
        self.sight_coef =
            SIGHT_ENERGY_COEF * self.cost_scale * BASE_VISION.powf(SIGHT_ENERGY_POWER - self.sight_power);
    }

    /// Расход энергии за тик: COEF * стат ** POWER, суммарно по трём статам.
    /// Цена скорости ещё и растёт с размером: см. SPEED_MASS_POWER.
    pub fn upkeep(&self, size: f64, speed: f64, vision: f64) -> f64 {
        let mass = (size / BASE_SIZE).powf(SPEED_MASS_POWER);
        self.size_coef * size.powf(self.size_power)
            + self.speed_coef * speed.powf(self.speed_power) * mass
            + self.sight_coef * vision.powf(self.sight_power)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn каждое_правило_читается_и_пишется_по_имени() {
        let r = Rules::default();
        for key in RULE_KEYS {
            let v = r.get(key).unwrap_or_else(|| panic!("{key} читается"));
            assert_eq!(r.with(key, v).expect(key), r, "{key}: записать то же — ничего не поменять");
        }
        assert_eq!(r.get("нет_такого"), None);
    }

    #[test]
    fn новые_цены_и_интервалы_не_принимают_бессмысленные_значения() {
        let rules = Rules::default();
        for (key, value) in [
            ("repro_cost", -1.0),
            ("melee_damage_share", -0.1),
            ("shot_energy_share", -0.1),
            ("shot_period", 0.0),
            ("shot_period", 1.5),
            ("plant_bite_yield", 1.1),
        ] {
            assert!(rules.with(key, value).is_err(), "{key}={value}");
        }
    }

    #[test]
    fn профиль_еды_по_умолчанию_как_в_конфиге() {
        let r = Rules::default();
        assert_eq!(r.plant_depth.kind(), Profile::Exp);
        assert_eq!(r.plant_depth.steepness, PLANT_DEPTH_DECAY);
        assert_eq!(r.plant_width.kind(), Profile::Uniform);
    }

    /// Пределы — только за которыми правило теряет смысл.
    #[test]
    fn профиль_еды_отвергает_бессмыслицу() {
        let r = Rules::default();
        for (key, v) in [
            ("plant_depth_profile", 1.5),
            ("plant_depth_profile", Profile::ALL.len() as f64),
            ("plant_width_profile", -1.0),
            ("plant_width_waves", 0.0),
            ("plant_width_waves", 2.5),
            ("plant_depth_amplitude", 150.0),
            ("plant_depth_end", -1.0),
            ("plant_depth_steepness", flora::MAX_STEEPNESS + 1.0),
            ("plant_width_bend", -0.1),
            ("plant_width_bend", f64::NAN),
        ] {
            assert!(r.with(key, v).is_err(), "{key}={v} должно быть отвергнуто");
        }
        assert!(r.with("plant_width_amplitude", 100.0).is_ok());
        assert!(r.with("plant_depth_steepness", 0.0).is_ok(), "ноль — равномерно, это осмысленно");
    }

    #[test]
    fn каннибализм_выключен_по_умолчанию_и_отвергает_бессмыслицу() {
        let r = Rules::default();
        assert!(!r.cannibals());
        assert!(r.with("cannibalism", 1.0).unwrap().cannibals());
        for (key, v) in
            [("cannibalism", 0.5), ("cannibalism", 2.0), ("cannibal_ratio", 1.0), ("cannibal_ratio", 0.5)]
        {
            assert!(r.with(key, v).is_err(), "{key}={v} должно быть отвергнуто");
        }
        assert_eq!(r.with("cannibal_ratio", 1.5).unwrap().cannibal_ratio, 1.5);
    }

    #[test]
    fn профиль_можно_назвать_именем() {
        let r = Rules::default();
        let waves = r.with_text("plant_width_profile", "waves").expect("имя профиля");
        assert_eq!(waves.plant_width.kind(), Profile::Waves);
        assert_eq!(r.with_text("plant_width_profile", " волны ").unwrap(), waves);
        assert_eq!(r.with_text("plant_width_profile", "4").unwrap(), waves);
        assert_eq!(r.with_text("plant_energy", "80").unwrap().plant_energy, 80.0);
        assert!(r.with_text("plant_width_profile", "круги").unwrap_err().contains("waves"));
        assert!(r.with_text("plant_energy", "много").is_err());
        assert!(r.with_text("нет_такого", "waves").unwrap_err().contains("нет такого правила"));
    }
}
