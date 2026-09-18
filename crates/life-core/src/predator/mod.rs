//! Хищник: бродит, голодный — охотится с рывком вблизи, ловит при касании тел.

mod phenotype;
mod standard;
pub mod strategy;

pub use phenotype::Phenotype;
pub use strategy::{Intent, Me, Mind, Strategy};

use crate::config::*;
use crate::genome::PredatorGenome;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::senses::PredatorSenses;
pub use crate::senses::Prey;
use crate::space::Space;

#[derive(Clone, Debug)]
pub struct Predator {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub genome: PredatorGenome,
    /// Всё, что выведено из генома и правил при рождении (`phenotype.rs`).
    pub pheno: Phenotype,
    pub alive: bool,
    /// Память между ходами: цель блуждания (`strategy.rs`).
    pub mind: Mind,
    pub rng: Rng,
}

impl Predator {
    pub const DIAM: f64 = PREDATOR_DIAM;

    pub fn new(
        space: &Space,
        rules: &Rules,
        genome: PredatorGenome,
        x: Option<f64>,
        y: Option<f64>,
        energy: Option<f64>,
        mut rng: Rng,
    ) -> Self {
        let d = Self::DIAM as i64;
        let x = x.unwrap_or_else(|| rng.randint(d, space.width as i64 - d) as f64);
        let y = y.unwrap_or_else(|| rng.randint(d, space.height as i64 - d) as f64);
        let pheno = Phenotype::of(&genome, rules);
        let mut p = Predator {
            id: 0,
            x,
            y,
            energy: energy.unwrap_or(pheno.max_energy * 0.5),
            genome,
            pheno,
            alive: true,
            mind: Mind { tx: x, ty: y },
            rng,
        };
        p.choose_new_target(space);
        p
    }

    /// Новая цель блуждания (`standard::choose_target`).
    pub fn choose_new_target(&mut self, space: &Space) {
        standard::choose_target(self.x, self.y, &mut self.mind, &mut self.rng, space);
    }

    /// Правила поменялись посреди жизни: фенотип — как у только что
    /// рождённого, энергия не больше нового бака.
    pub fn apply_rules(&mut self, rules: &Rules) {
        self.pheno = Phenotype::of(&self.genome, rules);
        self.energy = self.energy.min(self.pheno.max_energy);
    }

    pub fn hungry(&self) -> bool {
        self.pheno.hungry(self.energy)
    }

    /// Один ход: стратегия решает, хищник делает шаг, после шага стратегия
    /// может выбрать новую цель. Что вокруг — стратегия спрашивает у `senses`.
    pub fn step(&mut self, space: &Space, senses: &impl PredatorSenses) {
        let me = Me { x: self.x, y: self.y, energy: self.energy, pheno: &self.pheno };
        let intent = strategy::decide(&me, &mut self.mind, &mut self.rng, senses);
        self.act(space, &intent);
        let me = Me { x: self.x, y: self.y, energy: self.energy, pheno: &self.pheno };
        strategy::settle(&me, &mut self.mind, &mut self.rng, space, &intent);
    }

    /// Сдвиг из намерения, зажим в мир, расход (одной суммой), смерть от голода.
    #[inline(always)]
    fn act(&mut self, space: &Space, intent: &Intent) {
        let d = Self::DIAM;
        self.x = (self.x + intent.dx).clamp(d, space.width - d);
        self.y = (self.y + intent.dy).clamp(d, space.height - d);

        self.energy -= intent.cost;
        if self.energy <= 0.0 {
            self.alive = false;
        }
    }

    /// Радиус поимки до края тела добычи: ловит, если тела соприкоснулись.
    /// None — сытый хищник не охотится.
    pub fn catch_reach(&self) -> Option<f64> {
        self.hungry().then_some(Self::DIAM / 2.0)
    }

    /// Съел добычу с энергией `energy`.
    pub fn eat(&mut self, energy: f64, space: &Space) {
        self.energy = self.pheno.max_energy.min(self.energy + energy);
        self.choose_new_target(space);
    }

    /// Ребёнок, если сытый хищник решился делиться. Номер выдаёт мир.
    pub fn maybe_divide(&mut self, space: &Space, rules: &Rules) -> Option<Predator> {
        if self.energy < self.pheno.max_energy * PREDATOR_REPRO_THRESHOLD
            || self.rng.random() <= 1.0 - rules.predator_divide_chance
        {
            return None;
        }
        self.energy -= self.pheno.max_energy * PREDATOR_REPRO_COST;
        let d = Self::DIAM;
        let cx = (self.x + self.rng.randint(-300, 300) as f64).clamp(d, space.width - d);
        let cy = (self.y + self.rng.randint(-300, 300) as f64).clamp(d, space.height - d);
        let genome = self.genome.mutate(&mut self.rng);
        let rng = self.rng.fork();
        let child = Predator::new(
            space,
            rules,
            genome,
            Some(cx),
            Some(cy),
            Some(self.pheno.max_energy * PREDATOR_CHILD_ENERGY),
            rng,
        );
        self.choose_new_target(space);
        Some(child)
    }
}
