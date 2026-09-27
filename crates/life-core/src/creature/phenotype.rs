//! Фенотип существа: всё, что выводится из генома и правил мира один раз
//! при рождении и росте тела. Геном не меняется всю жизнь, поэтому ход (самый горячий код)
//! читает готовые числа. Правила меняются на ходу (лаборатория) — тогда фенотип
//! пересчитывается целиком (`Creature::apply_rules`).
//!
//! Единственное место, где ген действует и платит: новый ген получает здесь
//! своё действие, а его цена дописывается в конец суммы расхода.

use super::Strategy;
use crate::config::{
    DEEP_SAVING_FROM, DIET_OWN, ENERGY_PER_SIZE, FLEE_SIGHT_SHARE, LIFESPAN_MAX, LIFESPAN_MIN, OLD_AGE_FROM,
    OLD_AGE_FULL, OLD_AGE_VIGOUR, SLOW_PACE,
};
use crate::flock::{FlockKind, Territoriality};
use crate::genome::CreatureGenome;
use crate::genome::creature::Gene;
use crate::rules::{DietEdges, Rules};
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

    /// What it is good at, by the world's rules (`Rules::diets`, defaults `config::DIET_*`).
    pub fn edges(self, rules: &Rules) -> &DietEdges {
        &rules.diets[self as usize]
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
    /// Share of digested food that goes into growth until grown (the `maturation` gene, 0..1).
    pub maturation: f64,
    /// Ticks of life (the `lifespan` gene, clamped).
    pub lifespan: f64,
    /// 1 until old age, down to `OLD_AGE_VIGOUR` (`vigour`): speed, vision, strike and health are
    /// times this.
    pub vigour: f64,
    pub retreat: f64,
    /// Bravery 0..1: a stranger that could eat it but hunts nobody is feared only within
    /// `1 − bravery` of the usual flight distance; a hunting one within all of it.
    pub bravery: f64,
    /// What it eats; the three efficiencies are its diet's digestion (`Rules::diets`). Zero: it
    /// neither eats that food nor goes for it.
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
    /// Health per unit of size (`DietEdges::health`).
    pub health_bonus: f64,
    /// Strike damage times this (`DietEdges::strike`); the energy a strike costs does not change.
    pub strike_bonus: f64,
    /// How far it senses corpses (`DietEdges::smell` times vision).
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
    /// A bigger body's strike is times (size ratio) ** this (`Rules::melee_size_power`).
    pub melee_size_power: f64,
    /// The shot's energy cost per unit of size (`Rules::shot_energy_share`), read where the rules
    /// are not at hand (`territory::steer`).
    pub shot_energy_share: f64,
    /// Below this share of its store it strikes a smaller stranger eating beside it (`rivals`).
    pub rivalry: f64,
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
    /// Body radius: contact is the sum of two radii.
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
        Self::aged(genome, rules, space, size, 1.0)
    }

    /// The phenotype of an actual body at `vigour` (`vigour`): an old one is slower, sees less,
    /// strikes weaker and has less health, and pays the upkeep of the speed and sight it has.
    pub fn aged(genome: &CreatureGenome, rules: &Rules, space: &Space, size: f64, vigour: f64) -> Self {
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

        let speed = genome[Gene::Speed] * vigour;
        let vision = genome[Gene::Vision] * vigour;
        let diet = Diet::from_gene(genome[Gene::Diet]);
        let edges = *diet.edges(rules);
        let [plants, fresh, rot] = edges.digestion;
        let diet_upkeep = [edges.size_upkeep, edges.speed_upkeep];
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
            maturation: genome[Gene::Maturation].clamp(0.0, 100.0) / 100.0,
            lifespan: genome[Gene::Lifespan].clamp(LIFESPAN_MIN, LIFESPAN_MAX),
            vigour,
            diet,
            // a juvenile gut until grown to its own size: never below the grown one
            // (`DietEdges::young_plants`)
            plant_efficiency: if size < genome[Gene::Size] { plants.max(edges.young_plants) } else { plants },
            meat_efficiency: fresh,
            rot_efficiency: rot,
            own_meat,
            own_rot,
            picky: genome[Gene::Picky].clamp(0.0, 100.0) / 100.0,
            health_bonus: edges.health,
            strike_bonus: edges.strike,
            smell: vision * edges.smell,
            deep_saving: edges.deep_saving,
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
            melee_size_power: rules.melee_size_power,
            shot_energy_share: rules.shot_energy_share,
            rivalry: genome[Gene::Rivalry].clamp(0.0, 100.0) / 100.0,
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
            upkeep: rules.upkeep_diet(size, speed, vision, diet_upkeep),
            slow_speed,
            slow_upkeep: rules.upkeep_diet(size, slow_speed, vision, diet_upkeep),
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            flee: vision * FLEE_SIGHT_SHARE,
            strategy: Strategy::from_gene(genome[Gene::Strategy]),
        }
    }
}

/// Strength at `age` of a creature that lives `lifespan` ticks: 1, then from `OLD_AGE_FROM` of its
/// life falling linearly to `OLD_AGE_VIGOUR` at `OLD_AGE_FULL`, and so to its death.
#[inline]
pub fn vigour(age: f64, lifespan: f64) -> f64 {
    let t = ((age / lifespan - OLD_AGE_FROM) / (OLD_AGE_FULL - OLD_AGE_FROM)).clamp(0.0, 1.0);
    1.0 - (1.0 - OLD_AGE_VIGOUR) * t
}

/// Melee damage of a strike `strike` from a body of size `size` to one of size `target`: times
/// (size / target) ** `power` when the striker is the bigger (`config::MELEE_SIZE_POWER`), else as is.
#[inline]
pub fn melee_damage(strike: f64, size: f64, target: f64, power: f64) -> f64 {
    if size <= target { strike } else { strike * (size / target).powf(power) }
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

    /// Its melee damage: a share of its size, times its diet's bonus and its vigour. The energy a
    /// strike costs is the share without them (`strike_cost`).
    pub fn strike(&self) -> f64 {
        self.strike_cost() * self.strike_bonus * self.vigour
    }

    pub fn strike_cost(&self) -> f64 {
        self.size * self.melee_damage_share
    }

    /// Its melee damage to a body of size `target`: bigger bodies strike disproportionately harder.
    #[inline]
    pub fn strike_on(&self, target: f64) -> f64 {
        melee_damage(self.strike(), self.size, target, self.melee_size_power)
    }

    /// Below `rivalry` of its store it strikes a smaller stranger eating beside it.
    #[inline]
    pub fn rivals(&self, energy: f64) -> bool {
        energy < self.max_energy * self.rivalry
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
}
