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

/// The diets as rule keys name them, in the order of every `DIET_*` table (H/O/S/C).
pub const DIETS: [&str; 4] = ["herbivore", "omnivore", "scavenger", "carnivore"];

/// A diet's edges as rules: `{diet}_{edge}`, e.g. `carnivore_strike` (`DietEdges`).
pub const DIET_EDGES: [&str; 10] = [
    "strike",
    "health",
    "size_upkeep",
    "speed_upkeep",
    "smell",
    "plants",
    "meat",
    "rot",
    "bones",
    "young_plants",
];

/// Every diet edge's rule key, `[diet][edge]`.
pub const DIET_RULE_KEYS: [[&str; 10]; 4] = [
    [
        "herbivore_strike",
        "herbivore_health",
        "herbivore_size_upkeep",
        "herbivore_speed_upkeep",
        "herbivore_smell",
        "herbivore_plants",
        "herbivore_meat",
        "herbivore_rot",
        "herbivore_bones",
        "herbivore_young_plants",
    ],
    [
        "omnivore_strike",
        "omnivore_health",
        "omnivore_size_upkeep",
        "omnivore_speed_upkeep",
        "omnivore_smell",
        "omnivore_plants",
        "omnivore_meat",
        "omnivore_rot",
        "omnivore_bones",
        "omnivore_young_plants",
    ],
    [
        "scavenger_strike",
        "scavenger_health",
        "scavenger_size_upkeep",
        "scavenger_speed_upkeep",
        "scavenger_smell",
        "scavenger_plants",
        "scavenger_meat",
        "scavenger_rot",
        "scavenger_bones",
        "scavenger_young_plants",
    ],
    [
        "carnivore_strike",
        "carnivore_health",
        "carnivore_size_upkeep",
        "carnivore_speed_upkeep",
        "carnivore_smell",
        "carnivore_plants",
        "carnivore_meat",
        "carnivore_rot",
        "carnivore_bones",
        "carnivore_young_plants",
    ],
];

/// Имена настраиваемых правил — для отчёта (`--rule имя=число`) и настроек.
/// Профили еды — по шесть на ось, по порядку `flora::AXIS_PARAMS`; the diet edges come last.
pub const RULE_KEYS: [&str; 88] = {
    let mut all = [""; 88];
    let mut i = 0;
    while i < WORLD_RULE_KEYS.len() {
        all[i] = WORLD_RULE_KEYS[i];
        i += 1;
    }
    let mut d = 0;
    while d < DIETS.len() {
        let mut e = 0;
        while e < DIET_EDGES.len() {
            all[i] = DIET_RULE_KEYS[d][e];
            i += 1;
            e += 1;
        }
        d += 1;
    }
    all
};

/// The rules that are not a diet's edges.
const WORLD_RULE_KEYS: [&str; 48] = [
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
    "repro_cost",
    "melee_damage_share",
    "shot_damage_share",
    "shot_energy_share",
    "shot_period",
    "plant_bite_yield",
    "plant_patches",
    "plant_patch_size",
    "melee_size_power",
    "plant_patch_share",
    "size_cost",
    "speed_cost",
    "sight_cost",
    "speed_mass_power",
    "clone_share",
    "min_mutability",
    "diet_step",
    "diet_jump",
    "corpse_fresh",
    "corpse_bones",
    "corpse_sink",
    "corpse_decay",
    "corpse_rest",
    "corpse_bones_sink",
    "thermo_top",
    "thermo_bottom",
    "diet_meat_step",
    "diet_leap_carnivore",
    "diet_leap_scavenger",
];

/// Patches per base world — no more than this: at 1500 slots that is five places a patch, and
/// with fewer a patch is a lone plant, not an island.
pub const MAX_PATCHES: f64 = 300.0;

/// Параметры профилей еды, пока их не выбрали (config.rs).
const FOOD_AXIS: FoodAxis = FoodAxis {
    profile: 0.0,
    steepness: PLANT_WIDTH_DECAY,
    end: PLANT_LINEAR_END,
    bend: PLANT_LOG_BEND,
    waves: PLANT_WAVES,
    amplitude: PLANT_WAVE_AMPLITUDE,
};

/// What a diet is good at (`config::DIET_*`, one row each): all read in `Phenotype::of`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DietEdges {
    /// Strike damage, times the world's shares (`DIET_STRIKE`); the strike's energy cost stays.
    pub strike: f64,
    /// Health per unit of size (`DIET_HEALTH`).
    pub health: f64,
    /// Factors of the size and speed terms of upkeep (`DIET_SIZE_COST`, `DIET_SPEED_COST`).
    pub size_upkeep: f64,
    pub speed_upkeep: f64,
    /// How far corpses are sensed, in shares of vision (`DIET_SMELL`).
    pub smell: f64,
    /// Digestibility of plants, fresh meat, rot and bones (`DIET_DIGESTION`), 0‒1.
    pub digestion: [f64; 4],
    /// Digestibility of plants while not grown to its own size, at least (`DIET_YOUNG_PLANTS`), 0‒1:
    /// the young digest `max(digestion[0], young_plants)`.
    pub young_plants: f64,
}

impl DietEdges {
    const fn of(d: usize) -> Self {
        DietEdges {
            strike: DIET_STRIKE[d],
            health: DIET_HEALTH[d],
            size_upkeep: DIET_SIZE_COST[d],
            speed_upkeep: DIET_SPEED_COST[d],
            smell: DIET_SMELL[d],
            digestion: DIET_DIGESTION[d],
            young_plants: DIET_YOUNG_PLANTS[d],
        }
    }

    fn slot(&mut self, edge: &str) -> Option<&mut f64> {
        Some(match edge {
            "strike" => &mut self.strike,
            "health" => &mut self.health,
            "size_upkeep" => &mut self.size_upkeep,
            "speed_upkeep" => &mut self.speed_upkeep,
            "smell" => &mut self.smell,
            "plants" => &mut self.digestion[0],
            "meat" => &mut self.digestion[1],
            "rot" => &mut self.digestion[2],
            "bones" => &mut self.digestion[3],
            "young_plants" => &mut self.young_plants,
            _ => return None,
        })
    }
}

/// `carnivore_strike` → (3, "strike").
fn split_diet_key(key: &str) -> Option<(usize, &str)> {
    let (diet, edge) = key.split_once('_')?;
    let d = DIETS.iter().position(|&n| n == diet)?;
    DIET_EDGES.contains(&edge).then_some((d, edge))
}

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
    /// Цена рождения и сила/стоимость боя; умолчания берутся из config.rs.
    pub repro_cost: f64,
    pub melee_damage_share: f64,
    pub shot_damage_share: f64,
    pub shot_energy_share: f64,
    pub shot_period: f64,
    /// Доля питательной ценности растения, усваиваемая за пять порций.
    pub plant_bite_yield: f64,
    /// Patches of food per base world (0 — scattered) and their mean radius (`flora.rs`).
    pub plant_patches: f64,
    pub plant_patch_size: f64,
    /// How much harder a bigger body strikes (`config::MELEE_SIZE_POWER`); 0 — damage in
    /// proportion to size alone.
    pub melee_size_power: f64,
    /// Share of the plant slots in patches, % (`config::PLANT_PATCH_SHARE`).
    pub plant_patch_share: f64,
    /// What each term of the upkeep costs the base genome, times its config price (1 — as in
    /// config): the powers set how steeply a stat away from the base costs more, these set the price
    /// of the base itself.
    pub size_cost: f64,
    pub speed_cost: f64,
    pub sight_cost: f64,
    /// How the speed term grows with the body: `(size / 40) ** speed_mass_power`.
    pub speed_mass_power: f64,
    /// Share of children born exact copies, the floor of the mutability gene, and the chances of a
    /// mutating child's diet to step to a neighbour or jump to any other (`config.rs`).
    pub clone_share: f64,
    pub min_mutability: f64,
    pub diet_step: f64,
    pub diet_jump: f64,
    /// The chance of a step towards meat (`config::DIET_MEAT_STEP_CHANCE`).
    pub diet_meat_step: f64,
    /// The herbivore's own leaps past the omnivore, to the carnivore and to the scavenger; they
    /// replace its `diet_jump` (`config::HERBIVORE_LEAP_*`).
    pub diet_leap_carnivore: f64,
    pub diet_leap_scavenger: f64,
    /// A corpse's clock, ticks from death: fresh until; how far it sinks a tick; rotted down to the
    /// bones at; and the lowest share of the depth, %, where it comes to rest (`corpse.rs`).
    pub corpse_fresh: f64,
    pub corpse_sink: f64,
    pub corpse_decay: f64,
    pub corpse_rest: f64,
    /// How long bones lie, ticks, and how fast they sink (`SKELETON_*`).
    pub corpse_bones: f64,
    pub corpse_bones_sink: f64,
    /// The thermocline, % of depth: warm water above `thermo_top`, cold below `thermo_bottom`.
    pub thermo_top: f64,
    pub thermo_bottom: f64,
    /// What each diet is good at, by diet (H/O/S/C).
    pub diets: [DietEdges; 4],
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
                // the ocean reform (2026-09-27): richest a little below the surface
                profile: Profile::Ocean.index(),
                steepness: PLANT_DEPTH_DECAY,
                ..FOOD_AXIS
            },
            plant_width: FoodAxis { profile: Profile::Uniform.index(), ..FOOD_AXIS },
            repro_cost: REPRO_COST,
            melee_damage_share: MELEE_DAMAGE_SHARE,
            shot_damage_share: SHOT_DAMAGE_SHARE,
            shot_energy_share: SHOT_ENERGY_SHARE,
            shot_period: SHOT_PERIOD as f64,
            plant_bite_yield: PLANT_BITE_YIELD,
            plant_patches: PLANT_PATCHES,
            plant_patch_size: PLANT_PATCH_SIZE,
            melee_size_power: MELEE_SIZE_POWER,
            plant_patch_share: PLANT_PATCH_SHARE,
            size_cost: 1.0,
            speed_cost: 1.0,
            sight_cost: 1.0,
            speed_mass_power: SPEED_MASS_POWER,
            clone_share: CLONE_CHANCE,
            min_mutability: MIN_MUTABILITY,
            diet_step: DIET_STEP_CHANCE,
            diet_jump: DIET_JUMP_CHANCE,
            diet_meat_step: DIET_MEAT_STEP_CHANCE,
            diet_leap_carnivore: HERBIVORE_LEAP_CARNIVORE,
            diet_leap_scavenger: HERBIVORE_LEAP_SCAVENGER,
            corpse_fresh: CORPSE_FRESH_TICKS as f64,
            corpse_sink: CORPSE_SINK_SPEED,
            corpse_decay: CORPSE_DECAY_TICKS as f64,
            corpse_rest: CORPSE_REST_PCT,
            corpse_bones: SKELETON_TICKS as f64,
            corpse_bones_sink: SKELETON_SINK_SPEED,
            thermo_top: THERMO_TOP,
            thermo_bottom: THERMO_BOTTOM,
            diets: [DietEdges::of(0), DietEdges::of(1), DietEdges::of(2), DietEdges::of(3)],
            size_coef: 0.0,
            speed_coef: 0.0,
            sight_coef: 0.0,
        };
        r.renormalize();
        r
    }
}

/// Error for a key `Rules` does not know.
fn unknown(key: &str) -> String {
    format!("нет такого правила: {key}; есть {}", RULE_KEYS.join(", "))
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
        if let Some((d, edge)) = split_diet_key(key) {
            // Digesting more than all of a food, or saving more than the whole upkeep, would make
            // energy from nothing: energy only grows in plants and passes along the chain.
            let (allowed, need) = match edge {
                "plants" | "meat" | "rot" | "bones" | "young_plants" => {
                    ((0.0..=1.0).contains(&value), "доля от 0 до 1: больше — энергия из ничего")
                }
                "health" => (value > 0.0, "число больше 0"),
                _ => (value >= 0.0, "число не меньше 0"),
            };
            if !allowed {
                return Err(format!("правило {key}: нужно {need}, а не {value}"));
            }
            *r.diets[d].slot(edge).expect("ребро разобрано split_diet_key") = value;
            return Ok(r);
        }
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
            "repro_cost" => r.repro_cost = value,
            "melee_damage_share" => r.melee_damage_share = value,
            "shot_damage_share" => r.shot_damage_share = value,
            "shot_energy_share" => r.shot_energy_share = value,
            "shot_period" => r.shot_period = value,
            "plant_bite_yield" => r.plant_bite_yield = value,
            "plant_patches" => r.plant_patches = value,
            "plant_patch_size" => r.plant_patch_size = value,
            "melee_size_power" => r.melee_size_power = value,
            "plant_patch_share" => r.plant_patch_share = value,
            "size_cost" => r.size_cost = value,
            "speed_cost" => r.speed_cost = value,
            "sight_cost" => r.sight_cost = value,
            "speed_mass_power" => r.speed_mass_power = value,
            "clone_share" => r.clone_share = value,
            "min_mutability" => r.min_mutability = value,
            "diet_step" => r.diet_step = value,
            "diet_jump" => r.diet_jump = value,
            "diet_meat_step" => r.diet_meat_step = value,
            "diet_leap_carnivore" => r.diet_leap_carnivore = value,
            "diet_leap_scavenger" => r.diet_leap_scavenger = value,
            "corpse_fresh" => r.corpse_fresh = value,
            "corpse_bones" => r.corpse_bones = value,
            "corpse_bones_sink" => r.corpse_bones_sink = value,
            "thermo_top" => r.thermo_top = value,
            "thermo_bottom" => r.thermo_bottom = value,
            "corpse_sink" => r.corpse_sink = value,
            "corpse_decay" => r.corpse_decay = value,
            "corpse_rest" => r.corpse_rest = value,
            _ => return Err(unknown(key)),
        }
        // Пределы — только те, за которыми правило теряет смысл, а не «разумные»:
        // лаборатория для того и нужна, чтобы ломать баланс. Отрицательная цена
        // статов кормила бы существ за то, что они живут.
        let allowed = match key {
            "shot_period" => value >= 1.0 && value.fract() == 0.0,
            "plant_bite_yield" => (0.0..=1.0).contains(&value),
            "plant_patches" => value.fract() == 0.0 && (0.0..=MAX_PATCHES).contains(&value),
            "plant_patch_size" => value >= PLANT_RADIUS,
            "plant_patch_share" | "corpse_rest" | "thermo_top" | "thermo_bottom" => {
                (0.0..=100.0).contains(&value)
            }
            "clone_share"
            | "diet_step"
            | "diet_jump"
            | "diet_meat_step"
            | "diet_leap_carnivore"
            | "diet_leap_scavenger" => (0.0..=1.0).contains(&value),
            "min_mutability" => (0.0..=MAX_MUTABILITY).contains(&value),
            "corpse_fresh" | "corpse_bones" | "corpse_decay" => value >= 1.0 && value.fract() == 0.0,
            "corpse_sink" | "corpse_bones_sink" => value > 0.0,
            _ => value >= 0.0,
        };
        if !allowed {
            let need = match key {
                "shot_period" => "целое число не меньше 1",
                "plant_bite_yield" => "число от 0 до 1",
                "plant_patches" => "целое число от 0 до 300",
                "plant_patch_size" => "число не меньше радиуса растения (10)",
                "plant_patch_share" | "corpse_rest" | "thermo_top" | "thermo_bottom" => "число от 0 до 100",
                "clone_share"
                | "diet_step"
                | "diet_jump"
                | "diet_meat_step"
                | "diet_leap_carnivore"
                | "diet_leap_scavenger" => "доля от 0 до 1",
                "min_mutability" => "число от 0 до 10",
                "corpse_fresh" | "corpse_bones" | "corpse_decay" => "целое число тиков не меньше 1",
                "corpse_sink" | "corpse_bones_sink" => "число больше 0",
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
            return Err(unknown(key));
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
        if let Some((d, edge)) = split_diet_key(key) {
            return self.diets[d].clone().slot(edge).map(|v| *v);
        }
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
            "repro_cost" => self.repro_cost,
            "melee_damage_share" => self.melee_damage_share,
            "shot_damage_share" => self.shot_damage_share,
            "shot_energy_share" => self.shot_energy_share,
            "shot_period" => self.shot_period,
            "plant_bite_yield" => self.plant_bite_yield,
            "plant_patches" => self.plant_patches,
            "plant_patch_size" => self.plant_patch_size,
            "melee_size_power" => self.melee_size_power,
            "plant_patch_share" => self.plant_patch_share,
            "size_cost" => self.size_cost,
            "speed_cost" => self.speed_cost,
            "sight_cost" => self.sight_cost,
            "speed_mass_power" => self.speed_mass_power,
            "clone_share" => self.clone_share,
            "min_mutability" => self.min_mutability,
            "diet_step" => self.diet_step,
            "diet_jump" => self.diet_jump,
            "diet_meat_step" => self.diet_meat_step,
            "diet_leap_carnivore" => self.diet_leap_carnivore,
            "diet_leap_scavenger" => self.diet_leap_scavenger,
            "corpse_fresh" => self.corpse_fresh,
            "corpse_bones" => self.corpse_bones,
            "corpse_bones_sink" => self.corpse_bones_sink,
            "thermo_top" => self.thermo_top,
            "thermo_bottom" => self.thermo_bottom,
            "corpse_sink" => self.corpse_sink,
            "corpse_decay" => self.corpse_decay,
            "corpse_rest" => self.corpse_rest,
            _ => return None,
        })
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
        // the prices are exactly 1.0 by default: the products keep config's bits
        self.size_coef = SIZE_ENERGY_COEF
            * self.cost_scale
            * BASE_SIZE.powf(SIZE_ENERGY_POWER - self.size_power)
            * self.size_cost;
        self.speed_coef = SPEED_ENERGY_COEF
            * self.cost_scale
            * BASE_SPEED.powf(SPEED_ENERGY_POWER - self.speed_power)
            * self.speed_cost;
        self.sight_coef = SIGHT_ENERGY_COEF
            * self.cost_scale
            * BASE_VISION.powf(SIGHT_ENERGY_POWER - self.sight_power)
            * self.sight_cost;
    }

    /// Расход энергии за тик: COEF * стат ** POWER, суммарно по трём статам.
    /// Цена скорости ещё и растёт с размером: см. SPEED_MASS_POWER.
    pub fn upkeep(&self, size: f64, speed: f64, vision: f64) -> f64 {
        self.upkeep_diet(size, speed, vision, [1.0, 1.0])
    }

    /// Upkeep with the size and speed terms times the diet's factors (`DietEdges::size_upkeep`,
    /// `speed_upkeep`).
    pub fn upkeep_diet(&self, size: f64, speed: f64, vision: f64, [size_cost, speed_cost]: [f64; 2]) -> f64 {
        let mass = (size / BASE_SIZE).powf(self.speed_mass_power);
        self.size_coef * size.powf(self.size_power) * size_cost
            + self.speed_coef * speed.powf(self.speed_power) * mass * speed_cost
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
            ("plant_patches", -1.0),
            ("plant_patches", 2.5),
            ("plant_patches", MAX_PATCHES + 1.0),
            ("plant_patch_size", PLANT_RADIUS - 1.0),
            ("melee_size_power", -0.5),
            ("plant_patch_share", -1.0),
            ("plant_patch_share", 100.5),
            ("diet_leap_carnivore", 1.5),
            ("diet_leap_scavenger", -0.1),
        ] {
            assert!(rules.with(key, value).is_err(), "{key}={value}");
        }
    }

    #[test]
    fn diet_edges_default_to_config_and_never_make_energy() {
        let r = Rules::default();
        for (d, row) in DIET_RULE_KEYS.iter().enumerate() {
            let expected = [
                DIET_STRIKE[d],
                DIET_HEALTH[d],
                DIET_SIZE_COST[d],
                DIET_SPEED_COST[d],
                DIET_SMELL[d],
                DIET_DIGESTION[d][0],
                DIET_DIGESTION[d][1],
                DIET_DIGESTION[d][2],
                DIET_DIGESTION[d][3],
                DIET_YOUNG_PLANTS[d],
            ];
            for (key, v) in row.iter().zip(expected) {
                assert_eq!(r.get(key), Some(v), "{key}");
            }
        }
        assert_eq!(r.with("carnivore_strike", 2.0).unwrap().diets[3].strike, 2.0);
        assert_eq!(r.with("scavenger_rot", 0.5).unwrap().diets[2].digestion[2], 0.5);
        for (key, v) in [
            ("carnivore_meat", 1.01),
            ("herbivore_plants", 1.5),
            ("scavenger_rot", -0.1),
            ("scavenger_bones", 1.1),
            ("carnivore_young_plants", 1.1),
            ("herbivore_health", 0.0),
            ("omnivore_strike", -1.0),
        ] {
            assert!(r.with(key, v).is_err(), "{key}={v}");
        }
        assert!(r.with("carnivore_wings", 1.0).is_err());
        assert!(r.with("dragon_strike", 1.0).is_err());
    }

    #[test]
    fn профиль_еды_по_умолчанию_как_в_конфиге() {
        let r = Rules::default();
        assert_eq!(r.plant_depth.kind(), Profile::Ocean);
        assert_eq!(r.plant_depth.steepness, PLANT_DEPTH_DECAY);
        assert_eq!(r.plant_width.kind(), Profile::Uniform);
        assert_eq!((r.plant_patches, r.plant_patch_size), (PLANT_PATCHES, PLANT_PATCH_SIZE));
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
