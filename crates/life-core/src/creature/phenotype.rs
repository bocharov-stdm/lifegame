//! A creature's phenotype: everything derived from the genome and the world's rules once, at
//! birth and as the body grows. The genome does not change all life long, so the move (the
//! hottest code) reads ready numbers. The rules change on the fly (the lab) — then the phenotype
//! is recomputed whole (`Creature::apply_rules`).
//!
//! The only place where a gene acts and pays: a new gene gets its effect here, and its price is
//! appended to the end of the upkeep sum.

use super::Strategy;
use crate::config::{
    BURST_MAX, BURST_UPKEEP_SHARE, COLD_SAVING, COLD_SLOWING, DIET_OWN, ENERGY_PER_SIZE, LIFESPAN_MAX,
    LIFESPAN_MIN, OLD_AGE_FROM, OLD_AGE_FULL, OLD_AGE_VIGOUR,
};
use crate::corpse::Stage;
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

    /// Its own food among plants, fresh meat, rot and bones (`DIET_OWN`).
    pub fn own(self) -> [bool; 4] {
        DIET_OWN[self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phenotype {
    /// The diameter of the body.
    pub size: f64,
    pub speed: f64,
    /// Share of digested food that goes into growth until grown (the `maturation` gene, 0..1).
    pub maturation: f64,
    /// Ticks of life (the `lifespan` gene, clamped).
    pub lifespan: f64,
    /// 1 until old age, down to `OLD_AGE_VIGOUR` (`vigour`): speed, vision, strike and health are
    /// times this.
    pub vigour: f64,
    /// What it eats; the four efficiencies are its diet's digestion (`Rules::diets`). Zero: it
    /// neither eats that food nor goes for it.
    pub diet: Diet,
    pub plant_efficiency: f64,
    /// Fresh meat: a corpse right after death, and so what a hunt is worth.
    pub meat_efficiency: f64,
    /// A rotting corpse, and its bones (`corpse::Stage`).
    pub rot_efficiency: f64,
    pub bones_efficiency: f64,
    /// Fresh meat, rot and bones are its own food (`DIET_OWN`): it eats and goes for another
    /// niche's only on a tick its program says so (`Action::EatForeign`).
    pub own_meat: bool,
    pub own_rot: bool,
    pub own_bones: bool,
    /// Health per unit of size (`DietEdges::health`).
    pub health_bonus: f64,
    /// Strike damage times this (`DietEdges::strike`); the energy a strike costs does not change.
    pub strike_bonus: f64,
    /// How far it senses corpses (`DietEdges::smell` times vision).
    pub smell: f64,
    /// How much the body takes the water's temperature (the `cold_blood` gene, 0‒1), and the
    /// thermocline, y from and to (`Rules::thermo_*`).
    pub cold_blood: f64,
    pub thermo: (f64, f64),
    /// The world's depth: depths in programs are shares of it.
    pub height: f64,
    pub plant_energy: f64,
    pub plant_bite_yield: f64,
    pub melee_damage_share: f64,
    /// A bigger body's strike is times (size ratio) ** this (`Rules::melee_size_power`).
    pub melee_size_power: f64,
    pub vision: f64,

    // ── bounds ─────────────────────────────────────────────────────────────
    /// The body's bounds in the world: its centre never goes past them. Its layer is its program's
    /// setting (`Stance::layer`, `band`).
    pub x_lo: f64,
    pub x_hi: f64,
    pub y_lo: f64,
    pub y_hi: f64,

    // ── energy and what is precomputed ─────────────────────────────────────
    pub max_energy: f64,
    /// Upkeep a tick at full speed (for showing and for weighing), and its parts: the body and eyes
    /// paid standing (`still_upkeep`), and the price of a step of length `s`, `speed_price ×
    /// s ** speed_power` — paid for the step actually taken (`step_cost`).
    pub upkeep: f64,
    pub still_upkeep: f64,
    pub speed_price: f64,
    pub speed_power: f64,
    /// A burst's speed factor, when its program chases or flees with one (the `burst` gene, 1 to
    /// `BURST_MAX`).
    pub burst: f64,
    pub vision2: f64,
    pub size2: f64,
    /// Body radius: contact is the sum of two radii.
    pub half: f64,

    /// The template its program started from (`strategy.rs`).
    pub strategy: Strategy,
}

impl Phenotype {
    pub fn of(genome: &CreatureGenome, rules: &Rules, space: &Space) -> Self {
        Self::at_size(genome, rules, space, genome[Gene::Size])
    }

    /// The phenotype for the actual body: the hereditary limit is kept in the genome.
    pub fn at_size(genome: &CreatureGenome, rules: &Rules, space: &Space, size: f64) -> Self {
        Self::aged(genome, rules, space, size, 1.0)
    }

    /// The phenotype of an actual body at `vigour` (`vigour`): an old one is slower, sees less,
    /// strikes weaker and has less health, and pays the upkeep of the speed and sight it has.
    pub fn aged(genome: &CreatureGenome, rules: &Rules, space: &Space, size: f64, vigour: f64) -> Self {
        // The margin for the body is no more than half the world. Size is a gene, and at a cheap size
        // (the lab) a body can be bigger than the world: with the full margin the bounds turned over,
        // and the clamps threw a creature from edge to edge.
        let margin_x = size.min(space.width / 2.0);
        let margin_y = size.min(space.height / 2.0);
        let (x_lo, x_hi) = (margin_x, space.width - margin_x);
        // The home band (`band`) lies inside these bounds: the layer is within the world.
        let (y_lo, y_hi) = (margin_y, space.height - margin_y);

        let speed = genome[Gene::Speed] * vigour;
        let vision = genome[Gene::Vision] * vigour;
        let diet = Diet::from_gene(genome[Gene::Diet]);
        let edges = *diet.edges(rules);
        let [plants, fresh, rot, bones] = edges.digestion;
        let diet_upkeep = [edges.size_upkeep, edges.speed_upkeep];
        let [_, own_meat, own_rot, own_bones] = diet.own();
        let strategy = Strategy::from_gene(genome[Gene::Strategy]);
        let burst = genome[Gene::Burst].clamp(1.0, BURST_MAX);
        // the size and sight terms once, the speed term at each speed needed below
        let parts = rules.upkeep_parts(size, vision, diet_upkeep);
        let at_speed = parts.at(speed);
        // the muscles for a burst cost standing: a share of the speed term the extra speed adds
        let muscles = BURST_UPKEEP_SHARE * (parts.at(speed * burst) - at_speed);
        let still_upkeep = parts.at(0.0) + muscles;
        Phenotype {
            size,
            speed,
            maturation: genome[Gene::Maturation].clamp(0.0, 100.0) / 100.0,
            lifespan: genome[Gene::Lifespan].clamp(LIFESPAN_MIN, LIFESPAN_MAX),
            vigour,
            diet,
            // a juvenile gut until grown to its own size: never below the grown one
            // (`DietEdges::young_plants`)
            plant_efficiency: if size < genome[Gene::Size] { plants.max(edges.young_plants) } else { plants },
            meat_efficiency: fresh,
            rot_efficiency: rot,
            bones_efficiency: bones,
            own_meat,
            own_rot,
            own_bones,
            health_bonus: edges.health,
            strike_bonus: edges.strike,
            smell: vision * edges.smell,
            cold_blood: genome[Gene::ColdBlood].clamp(0.0, 100.0) / 100.0,
            thermo: (rules.thermo_top / 100.0 * space.height, rules.thermo_bottom / 100.0 * space.height),
            height: space.height,
            plant_energy: rules.plant_energy,
            plant_bite_yield: rules.plant_bite_yield,
            melee_damage_share: rules.melee_damage_share,
            melee_size_power: rules.melee_size_power,
            vision,
            x_lo,
            x_hi,
            y_lo,
            y_hi,
            max_energy: size * ENERGY_PER_SIZE,
            upkeep: at_speed + muscles,
            still_upkeep,
            // the speed term at a step of 1: `speed ** power` is 1 there
            speed_price: parts.at(1.0) - (still_upkeep - muscles),
            speed_power: rules.speed_power,
            burst,
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            strategy,
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
    /// Efficiency on a corpse at `stage`. Unless its program lets it eat foreign food this tick
    /// (`foreign`, `Action::EatForeign`), another niche's food is worth nothing (0): a scavenger
    /// leaves fresh corpses to the hunters, a carnivore rot and bones to the scavengers.
    #[inline]
    pub fn corpse_efficiency(&self, stage: Stage, foreign: bool) -> f64 {
        let (efficiency, own) = match stage {
            Stage::Fresh => (self.meat_efficiency, self.own_meat),
            Stage::Rot => (self.rot_efficiency, self.own_rot),
            Stage::Bones => (self.bones_efficiency, self.own_bones),
        };
        if foreign || own { efficiency } else { 0.0 }
    }

    /// How cold the water is at depth `y`: 0 above the thermocline, a smooth step to 1 below it.
    #[inline]
    pub fn coldness(&self, y: f64) -> f64 {
        let (top, bottom) = self.thermo;
        if bottom <= top {
            return if y >= top { 1.0 } else { 0.0 };
        }
        let t = ((y - top) / (bottom - top)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// (speed, upkeep) factors at depth `y`: a cold-blooded body is slower and cheaper in the cold.
    #[inline]
    pub fn temper(&self, y: f64) -> (f64, f64) {
        if self.cold_blood == 0.0 {
            return (1.0, 1.0);
        }
        let c = self.cold_blood * self.coldness(y);
        (1.0 - COLD_SLOWING * c, 1.0 - COLD_SAVING * c)
    }

    /// Upkeep a tick for a step of length `step`: the body and eyes, and the speed term for the
    /// step actually taken — standing costs no speed, even at `speed_power` 0 (`0 ** 0` is 1).
    #[inline]
    pub fn step_cost(&self, step: f64) -> f64 {
        let term = if self.speed_power == 2.0 {
            step * step
        } else if step == 0.0 {
            0.0
        } else {
            step.powf(self.speed_power)
        };
        self.still_upkeep + self.speed_price * term
    }

    /// Its layer's depths, y, from shares of the world's depth (`Stance::layer`).
    #[inline]
    pub fn layer(&self, shares: (f64, f64)) -> (f64, f64) {
        (shares.0 * self.height, shares.1 * self.height)
    }

    /// Its home band in the layer `shares` (`Stance::layer`): the layer less a margin for the body,
    /// always inside the world. The layer is soft: it leaves the band for food it sees, and without
    /// food it wanders in it and walks back to it.
    #[inline]
    pub fn band(&self, shares: (f64, f64)) -> (f64, f64) {
        let (layer_lo, layer_hi) = self.layer(shares);
        let margin_y = self.size.min(self.height / 2.0);
        let (lo, hi) = (layer_lo + margin_y, layer_hi - margin_y);
        if lo <= hi {
            return (lo, hi);
        }
        // A layer narrower than the body itself — collapse the band into a line in the middle, keeping
        // it in the world: for a layer right at the edge the middle would fall past the border.
        let mid = ((lo + hi) / 2.0).clamp(margin_y, self.height - margin_y);
        (mid, mid)
    }

    /// Eats plants at all.
    #[inline]
    pub fn eats_plants(&self) -> bool {
        self.plant_efficiency > 0.0
    }

    /// Eats a corpse at any stage at all: one that does not never looks for one.
    #[inline]
    pub fn eats_corpses(&self) -> bool {
        self.meat_efficiency > 0.0 || self.rot_efficiency > 0.0 || self.bones_efficiency > 0.0
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

    /// Eats fresh meat, at least when hungry: others fear it.
    #[inline]
    pub fn hunts(&self) -> bool {
        self.meat_efficiency > 0.0
    }

    /// Hunts now: fresh meat is its own food, or its program lets it eat foreign food this tick.
    #[inline]
    pub fn hunts_now(&self, foreign: bool) -> bool {
        self.hunts() && (self.own_meat || foreign)
    }
}
