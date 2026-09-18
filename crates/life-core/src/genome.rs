//! Геном травоядного: семь генов по именам.

use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Genom {
    /// Размер: диаметр тела, радиус поедания и запас энергии.
    pub size: f64,
    pub speed: f64,
    /// Зрение: радиус поиска еды и хищников.
    pub vision: f64,
    /// Порог размножения, % от запаса энергии.
    pub repro_threshold: f64,
    /// Доля энергии, которая достаётся потомку, %.
    pub repro_share: f64,
    /// Верхняя граница слоя обитания, % глубины мира.
    pub min_y: f64,
    /// Нижняя граница слоя обитания, % глубины мира.
    pub max_y: f64,
}

/// Машинные имена генов (JSON отчёта и эталона Python), порядок — как в `Genom`.
pub const GENE_KEYS: [&str; 7] =
    ["size", "speed", "vision", "repro_threshold", "repro_share", "min_y", "max_y"];

/// Русские подписи генов в порядке `Genom::to_array`.
pub const GENE_LABELS: [&str; 7] =
    ["размер", "скорость", "зрение", "порог_разм", "доля_потомку", "min_y%", "max_y%"];

/// Какие гены — проценты: при мутации они держатся в 0‒100. Для слоя это
/// граница мира; для порога и доли выше 100 размножение всё равно невозможно,
/// но число вроде 180% в среднем геноме только путало бы.
pub const PERCENT: [bool; 7] = [false, false, false, true, true, true, true];

impl Genom {
    pub const fn from_array(g: [f64; 7]) -> Self {
        Genom {
            size: g[0],
            speed: g[1],
            vision: g[2],
            repro_threshold: g[3],
            repro_share: g[4],
            min_y: g[5],
            max_y: g[6],
        }
    }

    pub const fn to_array(&self) -> [f64; 7] {
        [self.size, self.speed, self.vision, self.repro_threshold, self.repro_share, self.min_y, self.max_y]
    }

    /// Геном потомка: каждый ген умножается на (1 + gauss(0, sigma)).
    ///
    /// Множитель не ниже 0.1: при большой сигме ген иначе уходил бы в ноль и
    /// в минус. Выпавшее ниже порога перетягивается заново, как в Python.
    /// Шанс перетягивания не выше половины при любой сигме, так что цикл конечен
    /// (сигма обязана быть конечной — это проверяет Rules).
    pub fn mutate(&self, sigma: f64, rng: &mut Rng) -> Genom {
        let mut out = self.to_array();
        for (i, gene) in out.iter_mut().enumerate() {
            let gauss = loop {
                let g = rng.gauss(0.0, sigma);
                if g >= -0.9 {
                    break g;
                }
            };
            let mut mutated = *gene * (1.0 + gauss);
            if PERCENT[i] {
                mutated = mutated.clamp(0.0, 100.0);
            }
            *gene = mutated.max(0.01);
        }
        Genom::from_array(out)
    }
}
