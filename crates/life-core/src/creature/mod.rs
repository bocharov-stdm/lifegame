//! Существо: ищет растения в своём слое глубины, бежит от чужих, которые могут
//! его съесть, делится.
//!
//! Поиск соседей — забота мира. Существо получает его в виде чувств
//! (`senses.rs`): «где ближайшее растение», «кто рядом опасен». Так оно не
//! знает о сетке, а тесты подсовывают вместо неё обычные замыкания.

mod lurker;
mod phenotype;
mod standard;
pub mod strategy;

pub use phenotype::{Diet, Phenotype, melee_damage, vigour};
pub use strategy::{Chase, Intent, Me, Mind, Strategy};

use crate::config::*;
use crate::genome::CreatureGenome;
use crate::genome::creature::Gene;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::senses::Senses;
use crate::space::Space;

/// Who is whose family: the creature's number and its parent's (0: no parent, a founder or a
/// spawned one; the world numbers from 1, so 0 is nobody's), how far it has grown and until
/// what growth of its own child it still knows the child.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Kinship {
    pub id: u64,
    pub parent: u64,
    /// Body diameter over the inherited adult size: 1 is adult.
    pub growth: f64,
    /// A parent knows its child while the child's `growth` is below this: `2 × care`, at most 1.
    /// The base parent knows it until it is adult, a careless one only while it is small; below
    /// 25% care not even at birth, since children are born at half of their adult size.
    pub knows_until: f64,
}

impl Kinship {
    /// Family: oneself, and a parent with its child while the parent still knows the child.
    /// Family neither strikes nor flees from each other. Once the child has grown past what its
    /// parent remembers they are strangers, as are siblings and grandchildren: whom to spare
    /// is inherited (`care`), not a rule of the world.
    #[inline(always)]
    pub fn kin(self, other: Kinship) -> bool {
        self.id == other.id
            || (other.parent == self.id && other.growth < self.knows_until)
            || (self.parent == other.id && self.growth < other.knows_until)
    }
}

/// What a creature bit last, and where — for the window's proboscis. The world writes it in the
/// eating phase; nothing in the engine reads it (like `Plant::born`), so it is no part of the
/// behaviour the golden test pins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meal {
    pub tick: u64,
    /// Centre of the food.
    pub x: f64,
    pub y: f64,
    pub food: Morsel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Morsel {
    Plant,
    /// A piece of a corpse at this stage.
    Corpse {
        stage: crate::corpse::Stage,
    },
}

/// Причина смерти задаётся ровно один раз.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Death {
    Starved,
    OldAge,
    Combat,
}

#[derive(Clone, Debug)]
pub struct Creature {
    /// Постоянный номер: по нему игра выбирает и следит за существом.
    pub id: u64,
    /// Номер родителя; 0 — стартовое или подсаженное. Родителя может уже не
    /// быть в живых: братья и сёстры узнают друг друга и без него.
    pub parent: u64,
    pub flock: u64,
    /// The circle of the family flock: where it feeds. None for loners.
    pub circle: Option<crate::flock::Circle>,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    /// Диаметр при рождении: вложенная в рост энергия доступна добытчику.
    pub birth_size: f64,
    /// The world it lives in: its bounds, and where its corpse comes to rest.
    space: Space,
    pub alive: bool,
    pub age: f64,
    pub health: f64,
    pub peaceful_ticks: u32,
    pub reproduction_wait: u64,
    pub death: Option<Death>,

    pub genome: CreatureGenome,
    /// Всё, что выведено из генома и правил при рождении (`phenotype.rs`).
    pub pheno: Phenotype,

    /// Память между ходами: цель блуждания, бегство (`strategy.rs`).
    pub mind: Mind,
    pub rng: Rng,
    /// The last bite (`Meal`); only the window reads it.
    pub meal: Option<Meal>,
    /// Ticks of burst used in a row, and ticks left winded after `BURST_TICKS` of it.
    pub dash: u32,
    pub winded: u32,
    /// Torpid this tick: hungry with no food in sight, it stands paying `TORPOR_UPKEEP`.
    pub torpid: bool,
}

impl Creature {
    /// Новое существо. Координата None — случайная в своей домашней полосе;
    /// заданная зажимается только в мир: ребёнок рождается у родителя, а слой у
    /// него свой, мутировавший, — домой он дойдёт сам, телепорт был бы прыжком.
    /// energy None — полбака; заданная не больше бака.
    pub fn new(
        space: &Space,
        rules: &Rules,
        genome: CreatureGenome,
        x: Option<f64>,
        y: Option<f64>,
        energy: Option<f64>,
        mut rng: Rng,
    ) -> Self {
        let pheno = Phenotype::of(&genome, rules, space);
        let energy = match energy {
            Some(e) => e.min(pheno.max_energy),
            None => pheno.max_energy * 0.5,
        };
        let x = x.unwrap_or_else(|| rng.uniform(pheno.x_lo, pheno.x_hi)).clamp(pheno.x_lo, pheno.x_hi);
        let y = y.unwrap_or_else(|| rng.uniform(pheno.body_lo, pheno.body_hi)).clamp(pheno.y_lo, pheno.y_hi);

        Creature {
            id: 0,
            parent: 0,
            flock: 0,
            circle: None,
            x,
            y,
            energy,
            birth_size: pheno.size,
            space: *space,
            alive: true,
            age: 0.0,
            health: pheno.size * pheno.health_bonus,
            peaceful_ticks: 60,
            reproduction_wait: DIVIDE_PERIOD,
            death: None,
            genome,
            pheno,
            mind: Mind::default(),
            rng,
            meal: None,
            dash: 0,
            winded: 0,
            torpid: false,
        }
    }

    /// The world it lives in.
    pub fn space(&self) -> &Space {
        &self.space
    }

    /// Number, parent and growth: by them a parent knows its growing child.
    #[inline(always)]
    pub fn kinship(&self) -> Kinship {
        Kinship {
            id: self.id,
            parent: self.parent,
            growth: self.pheno.size / self.genome[Gene::Size].max(0.01),
            knows_until: (2.0 * self.pheno.care).min(1.0),
        }
    }

    /// Один ход: стратегия решает, куда идти, существо делает шаг.
    ///
    /// Что вокруг — стратегия спрашивает у `senses`.
    pub fn step(&mut self, senses: &impl Senses) {
        if !self.alive {
            return;
        }
        self.mind.social.tick += 1;
        self.age += 1.0;
        if self.age >= self.pheno.lifespan {
            self.alive = false;
            self.death = Some(Death::OldAge);
            return;
        }
        self.health = self.health.min(self.max_health());
        self.peaceful_ticks = self.peaceful_ticks.saturating_add(1);
        if self.peaceful_ticks >= 60 && self.energy > self.pheno.max_energy * 0.5 {
            let healed = (self.max_health() * 0.002)
                .min(self.max_health() - self.health)
                .min(self.energy - self.pheno.max_energy * 0.5)
                .max(0.0);
            self.health += healed;
            self.energy -= healed;
        }
        if self.adult() {
            self.reproduction_wait = self.reproduction_wait.saturating_sub(1);
        }
        let me = Me {
            x: self.x,
            y: self.y,
            energy: self.energy,
            kinship: self.kinship(),
            flock: self.flock,
            circle: self.circle,
            health_share: self.health / self.max_health(),
            health: self.health,
            pheno: &self.pheno,
        };
        let intent = strategy::decide(&me, &mut self.mind, &mut self.rng, senses);
        let intent = crate::territory::steer(self, intent);
        if self.mind.social.territory_guard.is_some() && intent.attack.is_some() && !self.fleeing() {
            self.mind.social.activity = crate::social::Activity::Alarm;
            self.mind.social.rest_until = 0;
            self.mind.social.course = None;
        }
        self.mind.attack = intent.attack;
        self.act(intent);
    }

    /// A step of exactly its speed towards the intent's point (never past it), clamped to the
    /// world, the energy for the step actually taken, death from hunger. Wandering goes at the
    /// slow pace (`slow_speed`); torpor, the cold and a burst change the step here.
    #[inline(always)]
    fn act(&mut self, intent: Intent) {
        let speed = if intent.slow { self.pheno.slow_speed } else { self.pheno.speed };
        let (x, y) = (self.x, self.y);
        // a cold-blooded body in cold water is slower and cheaper (`Phenotype::temper`)
        let (slower, cheaper) = self.pheno.temper(y);
        // torpor: hungry, only wandering (no food, prey or threat in sight), it stands and sleeps.
        // A rest is slow too, but it is a sated creature's choice with food maybe in sight: resting
        // pays its standing upkeep, never the torpor's share.
        self.torpid = self.energy < self.pheno.max_energy * self.pheno.torpor
            && intent.slow
            && intent.attack.is_none()
            && !self.fleeing()
            && self.mind.social.activity != crate::social::Activity::Resting;
        if self.torpid {
            self.energy -= self.pheno.still_upkeep * TORPOR_UPKEEP * cheaper;
            if self.energy <= 0.0 {
                self.alive = false;
                self.death = Some(Death::Starved);
            }
            return;
        }
        let (mut tx, mut ty) = (intent.tx, intent.ty);
        let (mut dx, mut dy) = (tx - x, ty - y);
        let mut d = dx.hypot(dy);
        // a burst in a chase to a goal beyond a normal step, or in flight
        let fleeing = self.fleeing() && d > 0.0;
        let chasing = intent.attack.is_some() && d > speed * slower;
        let burst = self.burst(!intent.slow && (chasing || fleeing));
        let speed = speed * slower * burst;
        if fleeing && burst > 1.0 && d < speed {
            // a flight's goal is one normal step away: the burst goes the whole step that way
            (dx, dy, d) = (dx / d * speed, dy / d * speed, speed);
            (tx, ty) = (x + dx, y + dy);
        }
        // Точка ближе шага — встаём ровно на неё. Проскочить её нельзя: при мягком
        // слое существо, возвращаясь на слой тоньше шага, качалось бы через него
        // туда-сюда вечно.
        let (nx, ny) = if d <= speed {
            (tx, ty)
        } else {
            let k = speed / d;
            (x + dx * k, y + dy * k)
        };
        // Существо уже стоит внутри своих границ, так что зажим только укорачивает
        // шаг: за ход оно сдвигается не дальше speed.
        self.x = nx.clamp(self.pheno.x_lo, self.pheno.x_hi);
        self.y = ny.clamp(self.pheno.y_lo, self.pheno.y_hi);
        let moved = (self.x - x, self.y - y);
        let step = moved.0.hypot(moved.1);
        if step > 1e-9 {
            self.mind.social.heading = Some(moved);
        }

        // the speed term is paid for the step actually taken: standing, resting or eating costs
        // only the body and the eyes
        self.energy -= self.pheno.step_cost(step) * cheaper;
        if self.energy <= 0.0 {
            self.alive = false;
            self.death = Some(Death::Starved);
        }
    }

    /// The speed factor of a burst this tick (`BURST_*`): `wanted` in a chase or in flight with the
    /// goal beyond a normal step. Winded after `BURST_TICKS` in a row; a tick without gives one back.
    fn burst(&mut self, wanted: bool) -> f64 {
        if self.winded > 0 {
            self.winded -= 1;
            return 1.0;
        }
        if !wanted || self.pheno.burst <= 1.0 {
            self.dash = self.dash.saturating_sub(1);
            return 1.0;
        }
        self.dash += 1;
        if self.dash >= BURST_TICKS {
            (self.dash, self.winded) = (0, BURST_REST);
        }
        self.pheno.burst
    }

    /// Новые правила пересчитывают фенотип по прежнему фактическому телу.
    pub fn apply_rules(&mut self, rules: &Rules, space: &Space) {
        self.pheno = Phenotype::aged(&self.genome, rules, space, self.pheno.size, self.pheno.vigour);
        self.space = *space;
    }

    /// Old age weakens it (`phenotype::vigour`): the world calls this before every tick, since the
    /// phenotype needs the rules. Nothing changes until old age, nor once it has fully set in.
    pub fn grow_old(&mut self, rules: &Rules) {
        let vigour = vigour(self.age, self.pheno.lifespan);
        if vigour != self.pheno.vigour {
            self.pheno = Phenotype::aged(&self.genome, rules, &self.space, self.pheno.size, vigour);
            self.health = self.health.min(self.max_health());
        }
    }

    /// One bite of a plant: a portion's energy at its plant efficiency, and the strategy learns it
    /// has eaten.
    pub fn feed(&mut self, rules: &Rules) {
        self.nourish(
            rules.plant_energy * rules.plant_bite_yield / f64::from(crate::plant::PORTIONS)
                * self.pheno.plant_efficiency,
            rules,
        );
        let me = Me {
            x: self.x,
            y: self.y,
            energy: self.energy,
            kinship: self.kinship(),
            flock: self.flock,
            circle: self.circle,
            health_share: self.health / self.max_health(),
            health: self.health,
            pheno: &self.pheno,
        };
        strategy::after_eating(&me, &mut self.mind, &mut self.rng);
    }

    /// Растёт только на усвоенной пище: the `maturation` share of it until grown; the rest fills
    /// the tank. Only the gene limits growth, not where it stands: next to the surface, where the
    /// food is, a body used to stop growing until it walked a diameter away. A body grown against an
    /// edge is pushed inside its new bounds by the growth, a fraction of a unit a bite.
    pub fn nourish(&mut self, gain: f64, rules: &Rules) {
        let gain = gain.max(0.0);
        let limit = self.genome[Gene::Size].max(self.pheno.size);
        let growth = (gain * self.pheno.maturation / GROWTH_ENERGY_PER_SIZE).min(limit - self.pheno.size);
        let health_share = self.health / self.max_health();
        if growth > 0.0 {
            let (size, vigour) = (self.pheno.size + growth, self.pheno.vigour);
            self.pheno = Phenotype::aged(&self.genome, rules, &self.space, size, vigour);
            self.x = self.x.clamp(self.pheno.x_lo, self.pheno.x_hi);
            self.y = self.y.clamp(self.pheno.y_lo, self.pheno.y_hi);
        }
        self.health = self.max_health() * health_share;
        self.energy = self.pheno.max_energy.min(self.energy + gain - growth * GROWTH_ENERGY_PER_SIZE);
    }

    /// Здоровье — размер тела, times the diet's bonus (`DIET_HEALTH`) and its vigour (old age,
    /// `phenotype::vigour`).
    pub fn max_health(&self) -> f64 {
        self.pheno.size * self.pheno.health_bonus * self.pheno.vigour
    }

    /// The inherited flock mode is the same: flocking, territoriality, strategy, shooting,
    /// flock kind, layer switch and diet. A child with another mode leaves the family flock: a
    /// circle is a feeding place, and a meat-eater finds nothing to eat among its own.
    pub fn same_mode(&self, other: &Creature) -> bool {
        let (a, b) = (&self.pheno, &other.pheno);
        a.pack_instinct == b.pack_instinct
            && a.territoriality == b.territoriality
            && a.strategy == b.strategy
            && a.shooter == b.shooter
            && a.flock_kind == b.flock_kind
            && a.layer_bound == b.layer_bound
            && a.diet == b.diet
    }

    /// Достигнут наследственный размер.
    pub fn adult(&self) -> bool {
        self.pheno.size >= self.genome[Gene::Size]
    }

    /// Бежит ли сейчас от кого-то (для окна игры и наблюдателя).
    pub fn fleeing(&self) -> bool {
        self.mind.flee_ticks > 0 || self.mind.social.shared_flee
    }

    /// Ребёнок, если после деления у родителя остаётся резерв. Номер ребёнку
    /// выдаёт мир.
    pub fn maybe_divide(&mut self, space: &Space, rules: &Rules) -> Option<Creature> {
        if !self.alive || !self.adult() || self.reproduction_wait > 0 {
            return None;
        }
        let threshold = self.pheno.max_energy * (self.genome[Gene::ReproThreshold] / 100.0);
        if self.energy < threshold + REPRO_RESERVE {
            return None;
        }
        // Резерв проверяется и ПОСЛЕ дележа: доля ребёнка считается от всей
        // энергии, и без этого родитель отдавал всё до нуля и умирал.
        // Пробуем наследование на копии генератора: неудачная попытка рождения
        // не тратит случайные числа, а вместимость ребёнка уже известна.
        let mut next_rng = self.rng.clone();
        let genome = self.genome.mutate_by(&crate::genome::Heredity::of(rules), &mut next_rng);
        let child_energy = (self.energy * (self.genome[Gene::ReproShare] / 100.0))
            .min(genome[Gene::Size] * 0.5 * ENERGY_PER_SIZE);
        let left = self.energy - child_energy - rules.repro_cost;
        if left < REPRO_RESERVE {
            return None;
        }
        self.rng = next_rng;
        self.reproduction_wait = DIVIDE_PERIOD;

        // Смещения по осям независимые: с одним общим дети ложились на диагональ.
        let span = self.pheno.size * 2.0;
        let cx = self.x + self.rng.uniform(-span, span);
        let cy = self.y + self.rng.uniform(-span, span);
        let rng = self.rng.fork();
        let baby_genome = genome.with(Gene::Size, genome[Gene::Size] * 0.5);
        let mut child = Creature::new(space, rules, baby_genome, Some(cx), Some(cy), Some(child_energy), rng);
        child.genome = genome;
        child.parent = self.id;
        let same_mode = self.pheno.pack_instinct && child.pheno.pack_instinct && self.same_mode(&child);
        child.flock = if same_mode && self.rng.random() < 0.99 { self.flock } else { 0 };
        child.reproduction_wait = DIVIDE_PERIOD;
        child.birth_size = child.genome[Gene::Size] * 0.5;
        child.pheno = Phenotype::at_size(&child.genome, rules, space, child.birth_size);
        child.health = child.max_health();
        child.energy = child_energy.min(child.pheno.max_energy);
        self.energy -= child.energy + rules.repro_cost;
        Some(child)
    }
}
