//! Травоядное: ищет растения в своём слое глубины, бежит от хищников, делится.
//!
//! Поиск соседей — забота мира. Существо получает его в виде чувств
//! (`senses.rs`): «где ближайшее растение / ближайший хищник». Так оно не знает
//! о сетке, а тесты подсовывают вместо неё обычные замыкания. Чувства ленивые:
//! пока существо бежит, растения оно не ищет вовсе.

mod phenotype;
mod standard;
pub mod strategy;

pub use phenotype::Phenotype;
pub use strategy::{Intent, Me, Mind, Strategy};

use crate::config::*;
use crate::genome::VegetarianGenome;
use crate::genome::vegetarian::Gene;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::senses::VegetarianSenses;
use crate::space::Space;

#[derive(Clone, Debug)]
pub struct Vegetarian {
    /// Постоянный номер: по нему игра выбирает и следит за существом.
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub alive: bool,

    pub genome: VegetarianGenome,
    /// Всё, что выведено из генома и правил при рождении (`phenotype.rs`).
    pub pheno: Phenotype,

    /// Память между ходами: бегство, цель блуждания (`strategy.rs`).
    pub mind: Mind,
    pub rng: Rng,
}

impl Vegetarian {
    /// Новое травоядное. Координата None — случайная в своей полосе; заданная
    /// тоже зажимается в полосу: ребёнок рождается у родителя, а слой у него
    /// свой, мутировавший, и без зажима первый ход телепортировал бы его.
    /// energy None — полбака; заданная не больше бака.
    pub fn new(
        space: &Space,
        rules: &Rules,
        genome: VegetarianGenome,
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
        let y = y
            .unwrap_or_else(|| rng.uniform(pheno.body_lo, pheno.body_hi))
            .clamp(pheno.body_lo, pheno.body_hi);

        Vegetarian { id: 0, x, y, energy, alive: true, genome, pheno, mind: Mind::default(), rng }
    }

    /// Базовое травоядное из конфига в случайном месте.
    pub fn base(space: &Space, rules: &Rules, rng: Rng) -> Self {
        Vegetarian::new(space, rules, VegetarianGenome::BASE, None, None, None, rng)
    }

    /// Один ход: стратегия решает, куда идти, существо делает шаг.
    ///
    /// Что вокруг — стратегия спрашивает у `senses`.
    pub fn step(&mut self, senses: &impl VegetarianSenses) {
        let me = Me { x: self.x, y: self.y, energy: self.energy, pheno: &self.pheno };
        let intent = strategy::decide(&me, &mut self.mind, &mut self.rng, senses);
        self.act(intent);
    }

    /// Шаг ровно на speed к точке намерения (и дальше неё), зажим в свою полосу,
    /// расход энергии, смерть от голода.
    #[inline(always)]
    fn act(&mut self, intent: Intent) {
        let (x, y, speed) = (self.x, self.y, self.pheno.speed);
        let (mut dx, mut dy) = (intent.tx - x, intent.ty - y);
        let d = dx.hypot(dy);
        if d != 0.0 {
            let k = speed / d;
            dx *= k;
            dy *= k;
        }
        // Существо уже стоит внутри своих границ, так что зажим только укорачивает
        // шаг: за ход оно сдвигается не дальше speed.
        self.x = (x + dx).clamp(self.pheno.x_lo, self.pheno.x_hi);
        self.y = (y + dy).clamp(self.pheno.body_lo, self.pheno.body_hi);

        self.energy -= self.pheno.upkeep;
        if self.energy <= 0.0 {
            self.alive = false;
        }
    }

    /// Правила поменялись посреди жизни (лаборатория на ходу): фенотип — как у
    /// только что рождённого с тем же геномом.
    pub fn apply_rules(&mut self, rules: &Rules, space: &Space) {
        self.pheno = Phenotype::of(&self.genome, rules, space);
    }

    /// Съедено `eaten` растений: энергия, и стратегия узнаёт, что поело.
    pub fn feed(&mut self, eaten: usize, rules: &Rules) {
        if eaten == 0 {
            return;
        }
        self.energy = self.pheno.max_energy.min(self.energy + rules.plant_energy * eaten as f64);
        let me = Me { x: self.x, y: self.y, energy: self.energy, pheno: &self.pheno };
        strategy::after_eating(&me, &mut self.mind, &mut self.rng);
    }

    /// Ребёнок, если после деления у родителя остаётся резерв. Номер ребёнку
    /// выдаёт мир.
    pub fn maybe_divide(&mut self, space: &Space, rules: &Rules) -> Option<Vegetarian> {
        let threshold = self.pheno.max_energy * (self.genome[Gene::ReproThreshold] / 100.0);
        if self.energy < threshold + VEGETARIAN_REPRO_RESERVE {
            return None;
        }
        // Резерв проверяется и ПОСЛЕ дележа: доля ребёнка считается от всей
        // энергии, и без этого родитель отдавал всё до нуля и умирал.
        let child_energy = self.energy * (self.genome[Gene::ReproShare] / 100.0);
        let left = self.energy - child_energy - VEGETARIAN_REPRO_COST;
        if left < VEGETARIAN_REPRO_RESERVE {
            return None;
        }
        self.energy = left;
        let genome = self.genome.mutate(rules.mutation_sigma, &mut self.rng);

        // Смещения по осям независимые: с одним общим дети ложились на диагональ.
        let span = self.pheno.size * 2.0;
        let cx = self.x + self.rng.uniform(-span, span);
        let cy = self.y + self.rng.uniform(-span, span);
        let rng = self.rng.fork();
        Some(Vegetarian::new(space, rules, genome, Some(cx), Some(cy), Some(child_energy), rng))
    }
}
