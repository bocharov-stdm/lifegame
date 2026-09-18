//! Травоядное: ищет растения в своём слое глубины, бежит от хищников, делится.
//!
//! Поиск соседей — забота мира. Существо получает его в виде замыканий
//! «найди ближайшее растение / ближайшего хищника»: так оно не знает о сетке,
//! а тесты подсовывают вместо неё обычный список. Замыкания ленивые: пока
//! существо бежит, растения оно не ищет вовсе.

use crate::config::*;
use crate::genome::Genom;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::space::Space;

#[derive(Clone, Debug)]
pub struct Vegetarian {
    /// Постоянный номер: по нему игра выбирает и следит за существом.
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub alive: bool,

    pub genom: Genom,
    pub size: f64,
    pub speed: f64,
    pub vision: f64,

    // ── слой обитания и границы (из генома, считаются при рождении) ─────────
    /// Слой без запаса на тело — по нему решается, дотянемся ли до еды.
    pub layer_lo: f64,
    pub layer_hi: f64,
    /// Полоса, в которой держится само тело. Всегда внутри мира.
    pub body_lo: f64,
    pub body_hi: f64,
    pub x_lo: f64,
    pub x_hi: f64,

    // ── предвычисленное: геном не меняется всю жизнь ────────────────────────
    pub max_energy: f64,
    pub upkeep: f64,
    pub vision2: f64,
    pub size2: f64,
    /// Радиус тела: по нему ловят и видят хищники.
    pub half: f64,
    flee2: f64,

    pub flee_ticks: u32,
    /// Последний вектор бегства (единичный): бежим по нему, и когда хищник
    /// пропал из виду.
    flee_dx: f64,
    flee_dy: f64,
    /// Цель блуждания. None до первого выбора: существо, родившееся во время
    /// бегства, иначе пошло бы потом к месту своего рождения.
    target: Option<(f64, f64)>,
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
        genom: Genom,
        x: Option<f64>,
        y: Option<f64>,
        energy: Option<f64>,
        mut rng: Rng,
    ) -> Self {
        let size = genom.size;
        let (mut min_pct, mut max_pct) = (genom.min_y.clamp(0.0, 100.0), genom.max_y.clamp(0.0, 100.0));
        if min_pct > max_pct {
            std::mem::swap(&mut min_pct, &mut max_pct);
        }
        let layer_lo = min_pct / 100.0 * space.height;
        let layer_hi = max_pct / 100.0 * space.height;

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

        let max_energy = size * VEGETARIAN_ENERGY_PER_SIZE;
        let energy = match energy {
            Some(e) => e.min(max_energy),
            None => max_energy * 0.5,
        };

        let x = x.unwrap_or_else(|| rng.uniform(x_lo, x_hi)).clamp(x_lo, x_hi);
        let y = y.unwrap_or_else(|| rng.uniform(body_lo, body_hi)).clamp(body_lo, body_hi);
        let vision = genom.vision;

        Vegetarian {
            id: 0,
            x,
            y,
            energy,
            alive: true,
            genom,
            size,
            speed: genom.speed,
            vision,
            layer_lo,
            layer_hi,
            body_lo,
            body_hi,
            x_lo,
            x_hi,
            max_energy,
            upkeep: rules.upkeep(size, genom.speed, vision),
            vision2: vision * vision,
            size2: size * size,
            half: size / 2.0,
            flee2: (vision / 3.0) * (vision / 3.0),
            flee_ticks: 0,
            flee_dx: 0.0,
            flee_dy: 0.0,
            target: None,
            rng,
        }
    }

    /// Базовое травоядное из конфига в случайном месте.
    pub fn base(space: &Space, rules: &Rules, rng: Rng) -> Self {
        Vegetarian::new(space, rules, Genom::from_array(VEGETARIAN_BASE_GENOM), None, None, None, rng)
    }

    /// Случайная точка в пределах зрения и своей полосы; 10 попыток, иначе стоим.
    fn pick_random_target(&mut self) {
        let (lo, hi) = (self.body_lo, self.body_hi);
        // слой схлопнут в линию: случайная точка на неё не попадёт никогда,
        // и существо стояло бы столбом — гуляем только вдоль линии
        let flat = lo == hi;
        for _ in 0..10 {
            let angle = self.rng.uniform(0.0, std::f64::consts::TAU);
            let dist = self.rng.uniform(self.vision * 0.5, self.vision * 2.0);
            let tx = self.x + angle.cos() * dist;
            let ty = if flat { lo } else { self.y + angle.sin() * dist };
            if self.x_lo <= tx && tx <= self.x_hi && lo <= ty && ty <= hi {
                self.target = Some((tx, ty));
                return;
            }
        }
        self.target = Some((self.x, self.y));
    }

    /// Один ход.
    ///
    /// `nearest_predator(x, y, r2)` — ближайший хищник строго ближе √r2, или None.
    /// `nearest_plant(x, y, r2)` — ближайшее живое растение строго ближе √r2.
    pub fn step(
        &mut self,
        nearest_predator: impl FnOnce(f64, f64, f64) -> Option<(f64, f64, f64)>,
        nearest_plant: impl FnOnce(f64, f64, f64) -> Option<(f64, f64)>,
    ) {
        let (x, y, speed) = (self.x, self.y, self.speed);

        // (x, y, квадрат расстояния) ближайшего хищника в пределах зрения
        let pred = nearest_predator(x, y, self.vision2);

        let mut fleeing = false;
        if self.flee_ticks > 0 {
            fleeing = true;
            self.flee_ticks -= 1;
        }
        if let Some((_, _, d2)) = pred
            && d2 < self.flee2
        {
            self.flee_ticks = FLEE_TICKS;
            fleeing = true;
        }

        let (tx, ty) = if fleeing {
            if let Some((px, py, _)) = pred {
                let (dx, dy) = (x - px, y - py);
                let d = dx.hypot(dy);
                if d != 0.0 {
                    self.flee_dx = dx / d;
                    self.flee_dy = dy / d;
                }
            }
            (x + self.flee_dx * speed, y + self.flee_dy * speed)
        } else {
            // ВАЖНО: сперва ищем ближайшее вообще и только потом отбрасываем
            // чужой слой. Искать ближайшее В СЛОЕ — другое поведение: существо
            // перестанет отвлекаться на недосягаемую еду и не будет блуждать.
            let food = nearest_plant(x, y, self.vision2)
                .filter(|&(_, py)| self.layer_lo <= py && py <= self.layer_hi);
            match food {
                Some(p) => p,
                None => {
                    match self.target {
                        None => self.pick_random_target(),
                        Some((tx, ty)) => {
                            let (dx, dy) = (x - tx, y - ty);
                            if dx * dx + dy * dy < speed * speed {
                                self.pick_random_target();
                            }
                        }
                    }
                    self.target.unwrap()
                }
            }
        };

        let (mut dx, mut dy) = (tx - x, ty - y);
        let d = dx.hypot(dy);
        if d != 0.0 {
            let k = speed / d;
            dx *= k;
            dy *= k;
        }
        // Существо уже стоит внутри своих границ, так что зажим только укорачивает
        // шаг: за ход оно сдвигается не дальше speed.
        self.x = (x + dx).clamp(self.x_lo, self.x_hi);
        self.y = (y + dy).clamp(self.body_lo, self.body_hi);

        self.energy -= self.upkeep;
        if self.energy <= 0.0 {
            self.alive = false;
        }
    }

    /// Правила поменялись посреди жизни (лаборатория на ходу): пересчитать то,
    /// что при рождении было вычислено из правил. Как в `new`.
    pub fn apply_rules(&mut self, rules: &Rules) {
        self.upkeep = rules.upkeep(self.size, self.speed, self.vision);
    }

    /// Съедено `eaten` растений: энергия и сразу новая цель, чтобы не топтаться.
    pub fn feed(&mut self, eaten: usize, rules: &Rules) {
        if eaten == 0 {
            return;
        }
        self.energy = self.max_energy.min(self.energy + rules.plant_energy * eaten as f64);
        self.pick_random_target();
    }

    /// Ребёнок, если после деления у родителя остаётся резерв. Номер ребёнку
    /// выдаёт мир.
    pub fn maybe_divide(&mut self, space: &Space, rules: &Rules) -> Option<Vegetarian> {
        let threshold = self.max_energy * (self.genom.repro_threshold / 100.0);
        if self.energy < threshold + VEGETARIAN_REPRO_RESERVE {
            return None;
        }
        // Резерв проверяется и ПОСЛЕ дележа: доля ребёнка считается от всей
        // энергии, и без этого родитель отдавал всё до нуля и умирал.
        let child_energy = self.energy * (self.genom.repro_share / 100.0);
        let left = self.energy - child_energy - VEGETARIAN_REPRO_COST;
        if left < VEGETARIAN_REPRO_RESERVE {
            return None;
        }
        self.energy = left;
        let genom = self.genom.mutate(rules.mutation_sigma, &mut self.rng);

        // Смещения по осям независимые: с одним общим дети ложились на диагональ.
        let span = self.size * 2.0;
        let cx = self.x + self.rng.uniform(-span, span);
        let cy = self.y + self.rng.uniform(-span, span);
        let rng = self.rng.fork();
        Some(Vegetarian::new(space, rules, genom, Some(cx), Some(cy), Some(child_energy), rng))
    }
}
