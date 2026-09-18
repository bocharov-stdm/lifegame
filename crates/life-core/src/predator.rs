//! Хищник: бродит, голодный — охотится с рывком вблизи, ловит при касании тел.

use crate::config::*;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::space::Space;

/// Что хищнику нужно знать о добыче: где она и какого размера.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prey {
    pub x: f64,
    pub y: f64,
    pub half: f64,
}

#[derive(Clone, Debug)]
pub struct Predator {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub max_energy: f64,
    pub speed: f64,
    pub vision: f64,
    pub alive: bool,
    /// Скорость и зрение не меняются всю жизнь — расход считается один раз.
    pub upkeep: f64,
    pub tx: f64,
    pub ty: f64,
    pub rng: Rng,
}

impl Predator {
    pub const DIAM: f64 = PREDATOR_DIAM;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        space: &Space,
        rules: &Rules,
        x: Option<f64>,
        y: Option<f64>,
        energy: Option<f64>,
        speed: f64,
        vision: f64,
        mut rng: Rng,
    ) -> Self {
        let d = Self::DIAM as i64;
        let x = x.unwrap_or_else(|| rng.randint(d, space.width as i64 - d) as f64);
        let y = y.unwrap_or_else(|| rng.randint(d, space.height as i64 - d) as f64);
        let max_energy = rules.predator_max_energy;
        let mut p = Predator {
            id: 0,
            x,
            y,
            energy: energy.unwrap_or(max_energy * 0.5),
            max_energy,
            speed,
            vision,
            alive: true,
            upkeep: rules.upkeep(Self::DIAM, speed, vision),
            tx: x,
            ty: y,
            rng,
        };
        p.choose_new_target(space);
        p
    }

    /// Цель — случайная точка квадрата со стороной 2*WANDER_RADIUS вокруг себя,
    /// сдвинутого внутрь тех же границ, в которых держится сам хищник. Цель у
    /// самой стены была недостижима: один хищник простоял так 426 тиков и умер.
    pub fn choose_new_target(&mut self, space: &Space) {
        let (a, b) = wander_span(self.x, space.width);
        self.tx = self.rng.uniform(a, b);
        let (a, b) = wander_span(self.y, space.height);
        self.ty = self.rng.uniform(a, b);
    }

    pub fn hungry(&self) -> bool {
        self.energy < self.max_energy * PREDATOR_HUNGRY
    }

    /// Один ход. `nearest_prey(x, y, vision)` — ближайшая по центрам живая добыча,
    /// которую видно по краю тела (d < vision + half). Сытый её не ищет.
    pub fn step(&mut self, space: &Space, nearest_prey: impl FnOnce(f64, f64, f64) -> Option<Prey>) {
        let prey = if self.hungry() { nearest_prey(self.x, self.y, self.vision) } else { None };
        let mut cost = self.upkeep;
        let (dx, dy) = match prey {
            Some(p) => {
                let (dx, dy) = (p.x - self.x, p.y - self.y);
                let dist = dx.hypot(dy);
                let mut step = self.speed;
                // Рывок: вблизи хищник догоняет и того, кто быстрее его на
                // дистанции. Он стоит энергии — гнаться рывком вечно нельзя.
                if dist - p.half - Self::DIAM / 2.0 < PREDATOR_SPRINT_RANGE {
                    step *= PREDATOR_SPRINT_MULT;
                    cost += PREDATOR_SPRINT_COST;
                }
                let step = step.min(dist); // не проскакивать добычу насквозь
                if dist != 0.0 { (dx * step / dist, dy * step / dist) } else { (0.0, 0.0) }
            }
            None => {
                let (dx, dy) = (self.tx - self.x, self.ty - self.y);
                let dist = dx.hypot(dy);
                if dist == 0.0 { (0.0, 0.0) } else { (dx * self.speed / dist, dy * self.speed / dist) }
            }
        };

        let d = Self::DIAM;
        self.x = (self.x + dx).clamp(d, space.width - d);
        self.y = (self.y + dy).clamp(d, space.height - d);

        self.energy -= cost;
        if self.energy <= 0.0 {
            self.alive = false;
        }
        if prey.is_none() && (self.x - self.tx).hypot(self.y - self.ty) < self.speed {
            self.choose_new_target(space);
        }
    }

    /// Радиус поимки до края тела добычи: ловит, если тела соприкоснулись.
    /// None — сытый хищник не охотится.
    pub fn catch_reach(&self) -> Option<f64> {
        self.hungry().then_some(Self::DIAM / 2.0)
    }

    /// Съел добычу с энергией `energy`.
    pub fn eat(&mut self, energy: f64, space: &Space) {
        self.energy = self.max_energy.min(self.energy + energy);
        self.choose_new_target(space);
    }

    fn mutate(&mut self, value: f64) -> f64 {
        (value * (1.0 + self.rng.gauss(0.0, PREDATOR_SIGMA))).max(0.01)
    }

    /// Ребёнок, если сытый хищник решился делиться. Номер выдаёт мир.
    pub fn maybe_divide(&mut self, space: &Space, rules: &Rules) -> Option<Predator> {
        if self.energy < self.max_energy * PREDATOR_REPRO_THRESHOLD
            || self.rng.random() <= 1.0 - rules.predator_divide_chance
        {
            return None;
        }
        self.energy -= self.max_energy * PREDATOR_REPRO_COST;
        let d = Self::DIAM;
        let cx = (self.x + self.rng.randint(-300, 300) as f64).clamp(d, space.width - d);
        let cy = (self.y + self.rng.randint(-300, 300) as f64).clamp(d, space.height - d);
        let speed = if self.rng.random() > 0.6 { self.speed } else { self.mutate(self.speed) };
        let vision = if self.rng.random() > 0.6 { self.vision } else { self.mutate(self.vision) };
        let rng = self.rng.fork();
        let child = Predator::new(
            space,
            rules,
            Some(cx),
            Some(cy),
            Some(self.max_energy * PREDATOR_CHILD_ENERGY),
            speed,
            vision,
            rng,
        );
        self.choose_new_target(space);
        Some(child)
    }
}

/// Отрезок длиной 2*WANDER_RADIUS вокруг pos, сдвинутый внутрь мира.
fn wander_span(pos: f64, world: f64) -> (f64, f64) {
    let (lo, hi) = (PREDATOR_DIAM, world - PREDATOR_DIAM);
    let (mut a, mut b) = (pos - WANDER_RADIUS, pos + WANDER_RADIUS);
    if a < lo {
        b += lo - a;
        a = lo;
    }
    if b > hi {
        a = (a - (b - hi)).max(lo);
        b = hi;
    }
    (a, b)
}
