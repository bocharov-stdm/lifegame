//! Правила мира, которые игрок меняет в «Лаборатории», не трогая config.rs.
//!
//! У каждого мира свой `Rules`: миров в процессе бывает два сразу (фон меню и
//! игра). Значения по умолчанию — ровно константы из config.rs.

use crate::config::*;

const BASE_SIZE: f64 = VEGETARIAN_BASE_GENOM[0];
const BASE_SPEED: f64 = VEGETARIAN_BASE_GENOM[1];
const BASE_VISION: f64 = VEGETARIAN_BASE_GENOM[2];

/// Имена настраиваемых правил — для отчёта (`--rule имя=число`) и настроек.
pub const RULE_KEYS: [&str; 10] = [
    "plant_rate",
    "plant_energy",
    "mutation_sigma",
    "cost_scale",
    "size_power",
    "speed_power",
    "sight_power",
    "predator_divide_chance",
    "predator_max_energy",
    "predator_migration",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// Растений за тик на базовый мир 6000x4000 (не вероятность).
    pub plant_rate: f64,
    /// Энергии за одно растение.
    pub plant_energy: f64,
    /// Разброс мутаций травоядных.
    pub mutation_sigma: f64,
    /// Множитель ко всей цене статов.
    pub cost_scale: f64,
    /// Крутизна цены размера.
    pub size_power: f64,
    /// Крутизна цены скорости.
    pub speed_power: f64,
    /// Крутизна цены зрения.
    pub sight_power: f64,
    /// Шанс деления хищника.
    pub predator_divide_chance: f64,
    /// Запас энергии хищника.
    pub predator_max_energy: f64,
    /// Тиков между мигрантами; 0 — миграции нет.
    pub predator_migration: f64,
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
            mutation_sigma: VEGETARIAN_SIGMA,
            cost_scale: 1.0,
            size_power: SIZE_ENERGY_POWER,
            speed_power: SPEED_ENERGY_POWER,
            sight_power: SIGHT_ENERGY_POWER,
            predator_divide_chance: PREDATOR_DIVIDE_CHANCE,
            predator_max_energy: PREDATOR_MAX_ENERGY,
            predator_migration: PREDATOR_MIGRATION_PERIOD,
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
        match key {
            "plant_rate" => r.plant_rate = value,
            "plant_energy" => r.plant_energy = value,
            "mutation_sigma" => r.mutation_sigma = value,
            "cost_scale" => r.cost_scale = value,
            "size_power" => r.size_power = value,
            "speed_power" => r.speed_power = value,
            "sight_power" => r.sight_power = value,
            "predator_divide_chance" => r.predator_divide_chance = value,
            "predator_max_energy" => r.predator_max_energy = value,
            "predator_migration" => r.predator_migration = value,
            _ => return Err(format!("нет такого правила: {key}; есть {}", RULE_KEYS.join(", "))),
        }
        // Пределы — только те, за которыми правило теряет смысл, а не «разумные»:
        // лаборатория для того и нужна, чтобы ломать баланс. Отрицательная цена
        // статов кормила бы существ за то, что они живут; дробный период миграции
        // молча становился бы нулём, то есть выключал её.
        let allowed = match key {
            "predator_divide_chance" => (0.0..=1.0).contains(&value),
            "predator_max_energy" => value > 0.0,
            "predator_migration" => value >= 0.0 && value.fract() == 0.0,
            _ => value >= 0.0,
        };
        if !allowed {
            let need = match key {
                "predator_divide_chance" => "число от 0 до 1",
                "predator_max_energy" => "число больше 0",
                "predator_migration" => "целое число тиков, 0 — без миграции",
                _ => "число не меньше 0",
            };
            return Err(format!("правило {key}: нужно {need}, а не {value}"));
        }
        r.renormalize();
        Ok(r)
    }

    /// Значение правила по имени (для отчёта и настроек).
    pub fn get(&self, key: &str) -> Option<f64> {
        Some(match key {
            "plant_rate" => self.plant_rate,
            "plant_energy" => self.plant_energy,
            "mutation_sigma" => self.mutation_sigma,
            "cost_scale" => self.cost_scale,
            "size_power" => self.size_power,
            "speed_power" => self.speed_power,
            "sight_power" => self.sight_power,
            "predator_divide_chance" => self.predator_divide_chance,
            "predator_max_energy" => self.predator_max_energy,
            "predator_migration" => self.predator_migration,
            _ => return None,
        })
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
