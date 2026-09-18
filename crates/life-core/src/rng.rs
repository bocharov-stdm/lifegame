//! Генератор случайных чисел: у каждого существа свой поток.
//!
//! В Python был один общий `random` на весь процесс, и сессия игры подменяла его
//! состояние вокруг своих тиков, чтобы «Заново» повторяло мир. Здесь общего
//! генератора нет: поток существа выводится из сида ребёнку при рождении, и
//! существо тянет числа только из своего потока. Поэтому результат не зависит
//! ни от порядка обхода, ни от числа потоков процессора, а два мира в одном
//! процессе друг другу не мешают.
//!
//! Внутри — SplitMix64: быстрый, 8 байт состояния, хорошее перемешивание.
//! Криптостойкость здесь не нужна.

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// Перемешивание SplitMix64: из любого u64 — «случайный» u64.
#[inline]
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: mix(seed) }
    }

    /// Поток, заданный несколькими ключами: сид мира и назначение потока.
    pub fn keyed(seed: u64, key: u64) -> Self {
        Rng { state: mix(mix(seed) ^ key.wrapping_mul(GOLDEN)) }
    }

    /// Независимый поток для потомка: берёт одно число из родительского.
    pub fn fork(&mut self) -> Self {
        Rng::new(self.next_u64())
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        mix(self.state)
    }

    /// Равномерно в [0, 1).
    #[inline]
    pub fn random(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Равномерно в [a, b].
    #[inline]
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// Целое равномерно в [a, b] включительно.
    #[inline]
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        let span = (b - a + 1) as u64;
        a + (self.next_u64() % span) as i64
    }

    /// Нормальное распределение (Бокс — Мюллер). Второе значение пары
    /// выбрасывается: так состояние остаётся одним u64.
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        let u1 = 1.0 - self.random(); // (0, 1] — логарифм нуля не берём
        let u2 = self.random();
        mu + sigma * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Равновероятный выбор из двух.
    #[inline]
    pub fn choose2(&mut self, a: f64, b: f64) -> f64 {
        if self.random() < 0.5 { a } else { b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn распределения_похожи_на_правду() {
        let mut r = Rng::new(1);
        let n = 200_000;
        let mean = (0..n).map(|_| r.random()).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.005, "среднее random() = {mean}");

        let g: Vec<f64> = (0..n).map(|_| r.gauss(0.0, 1.0)).collect();
        let m = g.iter().sum::<f64>() / n as f64;
        let var = g.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n as f64;
        assert!(m.abs() < 0.01 && (var - 1.0).abs() < 0.02, "gauss: среднее {m}, дисперсия {var}");

        for _ in 0..10_000 {
            let k = r.randint(-3, 3);
            assert!((-3..=3).contains(&k));
        }
    }

    #[test]
    fn один_сид_один_поток() {
        let a: Vec<u64> = {
            let mut r = Rng::keyed(7, 3);
            (0..5).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::keyed(7, 3);
            (0..5).map(|_| r.next_u64()).collect()
        };
        let c: Vec<u64> = {
            let mut r = Rng::keyed(7, 4);
            (0..5).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
