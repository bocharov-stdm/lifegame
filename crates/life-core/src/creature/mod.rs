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

pub use phenotype::Phenotype;
pub use strategy::{Intent, Me, Mind, Strategy};

use crate::config::*;
use crate::genome::CreatureGenome;
use crate::genome::creature::Gene;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::senses::Senses;
use crate::space::Space;

/// Кто кому родня: номер существа и номер его родителя (0 — родителя нет:
/// стартовое или подсаженное). Мир выдаёт номера с 1, так что 0 ничей.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Kinship {
    pub id: u64,
    pub parent: u64,
}

impl Kinship {
    /// Родня: сам, родитель и ребёнок, дети одного родителя. Родня друг друга
    /// не ест и друг от друга не бежит. Внуки и двоюродные — уже чужие: иначе
    /// за сотню поколений родным стал бы весь мир.
    #[inline(always)]
    pub fn kin(self, other: Kinship) -> bool {
        self.id == other.id
            || self.parent == other.id
            || other.parent == self.id
            || (self.parent != 0 && self.parent == other.parent)
    }
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
    /// Размеры мира нужны для роста без перемещения центра.
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
            health: pheno.size,
            peaceful_ticks: 60,
            reproduction_wait: (DIVIDE_PERIOD as f64 / pheno.life_pace).ceil() as u64,
            death: None,
            genome,
            pheno,
            mind: Mind::default(),
            rng,
        }
    }

    /// Номер и родитель: по ним узнают родню.
    #[inline(always)]
    pub fn kinship(&self) -> Kinship {
        Kinship { id: self.id, parent: self.parent }
    }

    /// Базовое существо из конфига в случайном месте.
    pub fn base(space: &Space, rules: &Rules, rng: Rng) -> Self {
        Creature::new(space, rules, CreatureGenome::BASE, None, None, None, rng)
    }

    /// Один ход: стратегия решает, куда идти, существо делает шаг.
    ///
    /// Что вокруг — стратегия спрашивает у `senses`.
    pub fn step(&mut self, senses: &impl Senses) {
        if !self.alive {
            return;
        }
        self.mind.social.tick += 1;
        self.age += self.pheno.life_pace;
        if self.age >= LIFESPAN {
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

    /// Шаг ровно на speed к точке намерения (и дальше неё), зажим в свою полосу,
    /// расход энергии, смерть от голода. Медленный ход — меньше шаг и расход.
    #[inline(always)]
    fn act(&mut self, intent: Intent) {
        let (speed, upkeep) = if intent.slow {
            (self.pheno.slow_speed, self.pheno.slow_upkeep)
        } else {
            (self.pheno.speed, self.pheno.upkeep)
        };
        let (x, y) = (self.x, self.y);
        let (dx, dy) = (intent.tx - x, intent.ty - y);
        let d = dx.hypot(dy);
        // Точка ближе шага — встаём ровно на неё. Проскочить её нельзя: при мягком
        // слое существо, возвращаясь на слой тоньше шага, качалось бы через него
        // туда-сюда вечно.
        let (nx, ny) = if d <= speed {
            (intent.tx, intent.ty)
        } else {
            let k = speed / d;
            (x + dx * k, y + dy * k)
        };
        // Существо уже стоит внутри своих границ, так что зажим только укорачивает
        // шаг: за ход оно сдвигается не дальше speed.
        self.x = nx.clamp(self.pheno.x_lo, self.pheno.x_hi);
        self.y = ny.clamp(self.pheno.y_lo, self.pheno.y_hi);
        let moved = (self.x - x, self.y - y);
        if moved.0.hypot(moved.1) > 1e-9 {
            self.mind.social.heading = Some(moved);
        }

        self.energy -= upkeep;
        if self.energy <= 0.0 {
            self.alive = false;
            self.death = Some(Death::Starved);
        }
    }

    /// Новые правила пересчитывают фенотип по прежнему фактическому телу.
    pub fn apply_rules(&mut self, rules: &Rules, space: &Space) {
        self.pheno = Phenotype::at_size(&self.genome, rules, space, self.pheno.size);
        self.space = *space;
        if !rules.cannibals() {
            self.mind.social.territory_avoid = None;
            self.mind.social.territory_guard = None;
            self.mind.social.territory_side = None;
            self.mind.social.territory_escape = None;
            self.mind.social.alarm = None;
            self.mind.social.observed_alarm = None;
            self.mind.social.hit = None;
            self.mind.social.aid = None;
            self.mind.social.context.alarm = None;
            self.mind.social.gathering = false;
            self.mind.social.shared_flee = false;
            if self.mind.social.activity == crate::social::Activity::Alarm {
                self.mind.social.activity = crate::social::Activity::Travelling;
            }
            self.mind.attack = None;
            self.mind.flee_ticks = 0;
            self.mind.flee_dx = 0.0;
            self.mind.flee_dy = 0.0;
        }
    }

    /// Съедено `eaten` растений: энергия, и стратегия узнаёт, что поело.
    pub fn feed(&mut self, eaten: usize, rules: &Rules) {
        if eaten == 0 {
            return;
        }
        self.nourish(
            rules.plant_energy * rules.plant_bite_yield / f64::from(crate::plant::PORTIONS)
                * eaten as f64
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
            pheno: &self.pheno,
        };
        strategy::after_eating(&me, &mut self.mind, &mut self.rng);
    }

    /// Растёт только на усвоенной пище; остаток наполняет запас.
    pub fn nourish(&mut self, gain: f64, rules: &Rules) {
        let gain = gain.max(0.0);
        let room = self.x.min(self.space.width - self.x).min(self.y.min(self.space.height - self.y));
        let limit = self.genome[Gene::Size].min(room).max(self.pheno.size);
        let growth = (gain * self.pheno.life_pace / (1.0 + self.pheno.life_pace) / GROWTH_ENERGY_PER_SIZE)
            .min(limit - self.pheno.size);
        let health_share = self.health / self.max_health();
        if growth > 0.0 {
            self.pheno = Phenotype::at_size(&self.genome, rules, &self.space, self.pheno.size + growth);
        }
        self.health = self.max_health() * health_share;
        self.energy = self.pheno.max_energy.min(self.energy + gain - growth * GROWTH_ENERGY_PER_SIZE);
    }

    /// Старение в последней пятой жизни уменьшает здоровье до половины.
    pub fn max_health(&self) -> f64 {
        self.pheno.size * (1.0 - ((self.age / LIFESPAN - 0.8) / 0.2).clamp(0.0, 1.0) * 0.5)
    }

    /// The inherited flock mode is the same: flocking, territoriality, strategy, shooting,
    /// flock kind and layer switch. A child with another mode leaves the family flock.
    pub fn same_mode(&self, other: &Creature) -> bool {
        let (a, b) = (&self.pheno, &other.pheno);
        a.pack_instinct == b.pack_instinct
            && a.territoriality == b.territoriality
            && a.strategy == b.strategy
            && a.shooter == b.shooter
            && a.flock_kind == b.flock_kind
            && a.layer_bound == b.layer_bound
    }

    /// Достигнут наследственный размер.
    pub fn adult(&self) -> bool {
        self.pheno.size >= self.genome[Gene::Size]
    }

    /// Бежит ли сейчас от кого-то (для окна игры и наблюдателя).
    pub fn fleeing(&self) -> bool {
        self.mind.flee_ticks > 0 || self.mind.social.shared_flee
    }

    /// Съеден сородич (каннибализм): его энергия — едоку, не выше полного бака.
    pub fn devour(&mut self, energy: f64, rules: &Rules) {
        self.nourish(energy * self.pheno.meat_efficiency, rules);
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
        let genome = self.genome.mutate(rules.mutation_sigma, &mut next_rng);
        let child_energy = (self.energy * (self.genome[Gene::ReproShare] / 100.0))
            .min(genome[Gene::Size] * 0.5 * ENERGY_PER_SIZE);
        let left = self.energy - child_energy - rules.repro_cost;
        if left < REPRO_RESERVE {
            return None;
        }
        self.rng = next_rng;
        self.reproduction_wait = (DIVIDE_PERIOD as f64 / self.pheno.life_pace).ceil() as u64;

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
        child.reproduction_wait = (DIVIDE_PERIOD as f64 / child.pheno.life_pace).ceil() as u64;
        child.birth_size = child.genome[Gene::Size] * 0.5;
        child.pheno = Phenotype::at_size(&child.genome, rules, space, child.birth_size);
        child.health = child.max_health();
        child.energy = child_energy.min(child.pheno.max_energy);
        self.energy -= child.energy + rules.repro_cost;
        Some(child)
    }
}
