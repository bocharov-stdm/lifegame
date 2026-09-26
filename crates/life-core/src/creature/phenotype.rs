//! Фенотип существа: всё, что выводится из генома и правил мира один раз
//! при рождении и росте тела. Геном не меняется всю жизнь, поэтому ход (самый горячий код)
//! читает готовые числа. Правила меняются на ходу (лаборатория) — тогда фенотип
//! пересчитывается целиком (`Creature::apply_rules`).
//!
//! Единственное место, где ген действует и платит: новый ген получает здесь
//! своё действие, а его цена дописывается в конец суммы расхода.

use super::Strategy;
use crate::config::{
    DEEP_SAVING_FROM, DIET_DEEP_SAVING, DIET_DIGESTION, DIET_HEALTH, DIET_OWN, DIET_SIZE_COST, DIET_SMELL,
    DIET_SPEED_COST, DIET_STRIKE, ENERGY_PER_SIZE, FLEE_SIGHT_SHARE, SLOW_PACE,
};
use crate::flock::{FlockKind, Territoriality};
use crate::genome::CreatureGenome;
use crate::genome::creature::Gene;
use crate::rules::Rules;
use crate::space::Space;

/// What a creature eats: the `diet` gene. The order is the gene's variants and the rows of
/// `DIET_DIGESTION`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diet {
    Herbivore,
    Omnivore,
    Scavenger,
    Carnivore,
}

impl Diet {
    pub const ALL: [Diet; 4] = [Diet::Herbivore, Diet::Omnivore, Diet::Scavenger, Diet::Carnivore];

    pub fn from_gene(value: f64) -> Diet {
        Diet::ALL[(value.max(0.0) as usize).min(Diet::ALL.len() - 1)]
    }

    /// Digestibility of plants, fresh meat and rot (`DIET_DIGESTION`).
    pub fn digestion(self) -> [f64; 3] {
        DIET_DIGESTION[self as usize]
    }

    /// How much harder than a herbivore it strikes (`DIET_STRIKE`).
    pub fn strike_bonus(self) -> f64 {
        DIET_STRIKE[self as usize]
    }

    /// Health per unit of size (`DIET_HEALTH`).
    pub fn health(self) -> f64 {
        DIET_HEALTH[self as usize]
    }

    /// Factors of the size and speed terms of upkeep (`DIET_SIZE_COST`, `DIET_SPEED_COST`).
    pub fn upkeep_costs(self) -> [f64; 2] {
        [DIET_SIZE_COST[self as usize], DIET_SPEED_COST[self as usize]]
    }

    /// Corpses are sensed this far, in shares of vision (`DIET_SMELL`).
    pub fn smell(self) -> f64 {
        DIET_SMELL[self as usize]
    }

    /// Upkeep saved on the bottom (`DIET_DEEP_SAVING`).
    pub fn deep_saving(self) -> f64 {
        DIET_DEEP_SAVING[self as usize]
    }

    /// Its own food among plants, fresh meat and rot (`DIET_OWN`).
    pub fn own(self) -> [bool; 3] {
        DIET_OWN[self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    /// Диаметр тела.
    pub size: f64,
    pub speed: f64,
    pub sociability: f64,
    pub pack_instinct: bool,
    pub territoriality: Territoriality,
    /// How the family flock's circle moves; part of the flock mode.
    pub flock_kind: FlockKind,
    /// False: the layer genes do not hold the creature, its band is the whole depth.
    pub layer_bound: bool,
    /// Circle radius per square root of the member count, clamped to 50-500.
    pub flock_spacing: f64,
    pub care: f64,
    pub life_pace: f64,
    pub retreat: f64,
    /// Bravery 0..1: a stranger that could eat it but hunts nobody is feared only within
    /// `1 − bravery` of the usual flight distance; a hunting one within all of it.
    pub bravery: f64,
    /// What it eats; the three efficiencies are its row of `DIET_DIGESTION`. Zero: it neither
    /// eats that food nor goes for it.
    pub diet: Diet,
    pub plant_efficiency: f64,
    /// Fresh meat: a corpse right after death, and so what a hunt is worth.
    pub meat_efficiency: f64,
    /// Fully rotten meat; a rotting corpse is a mix by its rot share (`corpse_efficiency`).
    pub rot_efficiency: f64,
    /// Fresh meat and rot are its own food: sated, it eats and goes only for its own
    /// (`DIET_OWN`); below `picky` of its store, for any it digests.
    pub own_meat: bool,
    pub own_rot: bool,
    pub picky: f64,
    /// Health per unit of size (`DIET_HEALTH`).
    pub health_bonus: f64,
    /// How far it senses corpses (`DIET_SMELL` times vision).
    pub smell: f64,
    /// Upkeep saved at the bottom; the saving grows from `DEEP_SAVING_FROM` of the depth.
    pub deep_saving: f64,
    pub height: f64,
    pub prey_ratio: f64,
    /// How much a hunter weighs the strikes it expects from its prey and the prey's visible
    /// allies: 0 ignores them, 1 is the base, 2 is twice as careful.
    pub caution: f64,
    /// Below this share of its store a flock member forages outside its circle, and it keeps
    /// foraging until it has `FORAGE_FED` times as much (at most a full store).
    pub forage: f64,
    /// Возможность и личный стиль дальнего боя.
    pub shooter: bool,
    pub fire_preference: f64,
    pub fire_reserve: f64,
    pub plant_energy: f64,
    pub plant_bite_yield: f64,
    pub melee_damage_share: f64,
    pub shot_energy_share: f64,
    pub vision: f64,

    // ── слой обитания и границы ─────────────────────────────────────────────
    /// Слой из генов, без запаса на тело (его рисует окно).
    pub layer_lo: f64,
    pub layer_hi: f64,
    /// Домашняя полоса — слой с запасом на тело, всегда внутри мира. Слой
    /// мягкий: за видимой едой существо выходит из полосы, а без еды бродит в
    /// ней и возвращается в неё.
    pub body_lo: f64,
    pub body_hi: f64,
    /// Границы тела в мире: дальше них центр не заходит никогда.
    pub x_lo: f64,
    pub x_hi: f64,
    pub y_lo: f64,
    pub y_hi: f64,

    // ── энергия и предвычисленное ───────────────────────────────────────────
    pub max_energy: f64,
    /// Расход за тик.
    pub upkeep: f64,
    /// Медленный ход (`SLOW_PACE`): шаг и расход за тик на нём.
    pub slow_speed: f64,
    pub slow_upkeep: f64,
    pub vision2: f64,
    pub size2: f64,
    /// Радиус тела: по нему съедают сородичи (каннибализм).
    pub half: f64,
    /// С какого расстояния до края тела опасного чужака бежать
    /// (`FLEE_SIGHT_SHARE` зрения).
    pub flee: f64,

    /// Стратегия поведения (`strategy.rs`).
    pub strategy: Strategy,
}

impl Phenotype {
    pub fn of(genome: &CreatureGenome, rules: &Rules, space: &Space) -> Self {
        Self::at_size(genome, rules, space, genome[Gene::Size])
    }

    /// Фенотип по фактическому телу: наследственный предел хранится в геноме.
    pub fn at_size(genome: &CreatureGenome, rules: &Rules, space: &Space, size: f64) -> Self {
        let life_pace = genome[Gene::LifePace].clamp(0.5, 2.0);
        let (mut min_pct, mut max_pct) =
            (genome[Gene::MinY].clamp(0.0, 100.0), genome[Gene::MaxY].clamp(0.0, 100.0));
        if min_pct > max_pct {
            std::mem::swap(&mut min_pct, &mut max_pct);
        }
        let layer_bound = genome[Gene::LayerBound] < 0.5;
        // A free creature has no layer: its home band is the whole depth.
        let (layer_lo, layer_hi) = if layer_bound {
            (min_pct / 100.0 * space.height, max_pct / 100.0 * space.height)
        } else {
            (0.0, space.height)
        };

        // Запас на тело — не больше половины мира. Размер — ген, и при дешёвом
        // размере (лаборатория) тело бывает больше мира: с полным запасом
        // границы переворачивались, и зажимы перекидывали существо от края к краю.
        let margin_x = size.min(space.width / 2.0);
        let margin_y = size.min(space.height / 2.0);
        let (mut body_lo, mut body_hi) = (layer_lo + margin_y, layer_hi - margin_y);
        // Слой уже собственного тела — схлопываем полосу в линию посередине,
        // держа её в мире: у слоя на самом краю середина легла бы за границу.
        if body_lo > body_hi {
            let mid = ((body_lo + body_hi) / 2.0).clamp(margin_y, space.height - margin_y);
            body_lo = mid;
            body_hi = mid;
        }
        let (x_lo, x_hi) = (margin_x, space.width - margin_x);
        // Домашняя полоса лежит внутри этих границ: слой — в пределах мира.
        let (y_lo, y_hi) = (margin_y, space.height - margin_y);

        let speed = genome[Gene::Speed];
        let vision = genome[Gene::Vision];
        let diet = Diet::from_gene(genome[Gene::Diet]);
        let [plants, fresh, rot] = diet.digestion();
        let [_, own_meat, own_rot] = diet.own();
        let slow_speed = speed * SLOW_PACE;
        Phenotype {
            size,
            speed,
            sociability: genome[Gene::Sociability].clamp(0.0, 100.0) / 100.0,
            pack_instinct: genome[Gene::PackInstinct] >= 0.5,
            territoriality: Territoriality::from_gene(genome[Gene::Territoriality]),
            flock_kind: FlockKind::from_gene(genome[Gene::FlockKind]),
            layer_bound,
            flock_spacing: genome[Gene::FlockSpacing].clamp(50.0, 500.0),
            care: genome[Gene::Care].clamp(0.0, 100.0) / 100.0,
            life_pace,
            diet,
            plant_efficiency: plants,
            meat_efficiency: fresh,
            rot_efficiency: rot,
            own_meat,
            own_rot,
            picky: genome[Gene::Picky].clamp(0.0, 100.0) / 100.0,
            health_bonus: diet.health(),
            smell: vision * diet.smell(),
            deep_saving: diet.deep_saving(),
            height: space.height,
            prey_ratio: genome[Gene::PreyRatio].clamp(1.0, 5.0),
            caution: genome[Gene::Caution].clamp(0.0, 100.0) / 50.0,
            forage: genome[Gene::Forage].clamp(0.0, 100.0) / 100.0,
            shooter: genome[Gene::Shooter] >= 0.5,
            fire_preference: genome[Gene::FirePreference].clamp(0.0, 100.0) / 100.0,
            fire_reserve: genome[Gene::FireReserve].clamp(0.0, 100.0) / 100.0,
            plant_energy: rules.plant_energy,
            plant_bite_yield: rules.plant_bite_yield,
            melee_damage_share: rules.melee_damage_share,
            shot_energy_share: rules.shot_energy_share,
            retreat: 0.8 - 0.6 * genome[Gene::Bravery].clamp(0.0, 100.0) / 100.0,
            bravery: genome[Gene::Bravery].clamp(0.0, 100.0) / 100.0,
            vision,
            layer_lo,
            layer_hi,
            body_lo,
            body_hi,
            x_lo,
            x_hi,
            y_lo,
            y_hi,
            max_energy: size * ENERGY_PER_SIZE,
            upkeep: rules.upkeep_diet(size, speed, vision, diet.upkeep_costs()) * life_pace,
            slow_speed,
            slow_upkeep: rules.upkeep_diet(size, slow_speed, vision, diet.upkeep_costs()) * life_pace,
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            flee: vision * FLEE_SIGHT_SHARE,
            strategy: Strategy::from_gene(genome[Gene::Strategy]),
        }
    }
}

impl Phenotype {
    /// Efficiency on a corpse with rot share `rot`: fresh and rot mixed. A sated creature
    /// (`hungry` false) does not touch a corpse that is mostly another niche's food (0): a sated
    /// scavenger leaves the fresher half of the time to the hunters, a sated carnivore the rotten
    /// half to the scavengers.
    #[inline]
    pub fn corpse_efficiency(&self, rot: f64, hungry: bool) -> f64 {
        let own = if rot < 0.5 { self.own_meat } else { self.own_rot };
        if !hungry && !own {
            return 0.0;
        }
        self.meat_efficiency * (1.0 - rot) + self.rot_efficiency * rot
    }

    /// Below `picky` of its store it eats another niche's food too.
    #[inline]
    pub fn hungry(&self, energy: f64) -> bool {
        energy < self.max_energy * self.picky
    }

    /// The share of upkeep it pays at depth `y`: 1, or less for a deep dweller below
    /// `DEEP_SAVING_FROM` of the depth, down to `1 − deep_saving` on the bottom.
    #[inline]
    pub fn depth_upkeep(&self, y: f64) -> f64 {
        if self.deep_saving == 0.0 {
            return 1.0;
        }
        let t = ((y / self.height - DEEP_SAVING_FROM) / (1.0 - DEEP_SAVING_FROM)).clamp(0.0, 1.0);
        1.0 - self.deep_saving * t
    }

    /// Eats plants at all.
    #[inline]
    pub fn eats_plants(&self) -> bool {
        self.plant_efficiency > 0.0
    }

    /// Its melee damage: a share of its size, times its diet's bonus. The energy a strike costs
    /// is the share without the bonus (`strike_cost`).
    pub fn strike(&self) -> f64 {
        self.strike_cost() * self.diet.strike_bonus()
    }

    pub fn strike_cost(&self) -> f64 {
        self.size * self.melee_damage_share
    }

    /// Eats fresh meat, at least when hungry: others fear it.
    #[inline]
    pub fn hunts(&self) -> bool {
        self.meat_efficiency > 0.0
    }

    /// Hunts now: fresh meat is its own food, or it is hungry.
    #[inline]
    pub fn hunts_now(&self, energy: f64) -> bool {
        self.hunts() && (self.own_meat || self.hungry(energy))
    }

    /// Eats some corpse, fresh or rotten.
    #[inline]
    pub fn eats_corpses(&self) -> bool {
        self.meat_efficiency > 0.0 || self.rot_efficiency > 0.0
    }
}
