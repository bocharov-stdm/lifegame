//! Геном таблицей: у каждого вида — список генов (`GENES`) с именем, подписью,
//! базой, видом значения и законом мутации. Всё, что обходит гены (мутация,
//! сводки, отчёт, графики, карточка существа), идёт по таблице, а не по
//! номерам: новый ген — одна строка таблицы и его действие в фенотипе.
//!
//! **Таблица только дописывается в конец.** От порядка генов зависят порядок
//! случайных чисел при мутации (значит, каждый сид), позиции генов в эталоне
//! баланса и JSON отчёта.
//!
//! Значение гена всегда f64: у гена-выбора это номер варианта (0, 1, 2…).

pub mod predator;
pub mod vegetarian;

use crate::config::MAX_MUTABILITY;
use crate::rng::Rng;

pub use predator::PredatorGenome;
pub use vegetarian::VegetarianGenome;

/// Вариант гена-выбора: например, стратегия поведения.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub key: &'static str,
    pub label: &'static str,
    pub about: &'static str,
}

/// Какое значение у гена.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneKind {
    /// Число в своих единицах (размер, скорость, зрение).
    Absolute,
    /// Проценты: при мутации держатся в 0‒100.
    Percent,
    /// Один из вариантов; значение — его номер.
    Choice(&'static [Variant]),
}

/// Как ген меняется у потомка.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mutation {
    /// Умножение на (1 + gauss(0, sigma)). `keep_above: Some(p)` — сначала
    /// жребий: выпало больше p — ген не меняется. `reject_below: Some(m)` —
    /// множитель ниже 1 + m перетягивается заново (без этого при большой
    /// сигме ген уходил бы в ноль и в минус).
    Scale { keep_above: Option<f64>, reject_below: Option<f64> },
    /// Смена варианта с шансом `chance` на любой другой. С одним вариантом
    /// жребий не тянется вовсе: ген инертен и не сдвигает случайные числа.
    Switch { chance: f64 },
}

/// Строка таблицы генов.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneSpec {
    /// Машинное имя: JSON отчёта, эталон баланса.
    pub key: &'static str,
    /// Короткая подпись для таблиц и графиков.
    pub label: &'static str,
    /// Что ген делает — для справки и подсказок.
    pub about: &'static str,
    pub kind: GeneKind,
    /// Значение у стартовых существ.
    pub base: f64,
    pub mutation: Mutation,
}

impl GeneSpec {
    pub const fn is_percent(&self) -> bool {
        matches!(self.kind, GeneKind::Percent)
    }

    pub const fn variants(&self) -> Option<&'static [Variant]> {
        match self.kind {
            GeneKind::Choice(v) => Some(v),
            _ => None,
        }
    }
}

/// Геном вида: значения по таблице `GENES`.
pub trait Genome: Copy + PartialEq + core::fmt::Debug + 'static {
    const GENES: &'static [GeneSpec];

    fn values(&self) -> &[f64];

    fn values_mut(&mut self) -> &mut [f64];

    /// Значение гена по машинному имени.
    fn get(&self, key: &str) -> Option<f64> {
        index_of(Self::GENES, key).map(|i| self.values()[i])
    }
}

/// Номер гена в таблице по машинному имени.
pub fn index_of(genes: &[GeneSpec], key: &str) -> Option<usize> {
    genes.iter().position(|g| g.key == key)
}

/// Мутация значений по таблице, ген за геном в её порядке. `sigma` — разброс
/// законов `Scale`: у травоядных из правил мира, у хищника — из конфига.
/// `mutability` — ген мутагенности родителя (`mutability_of`): умножает и
/// разброс, и шанс смены варианта, у всех генов сразу, включая себя самого.
///
/// Порядок и число случайных чисел — часть поведения мира: у травоядных цикл
/// gauss до множителя не ниже 0.1, у хищника жребий «оставить» и один gauss.
pub(crate) fn mutate_values(
    values: &mut [f64],
    genes: &[GeneSpec],
    sigma: f64,
    mutability: f64,
    rng: &mut Rng,
) {
    let sigma = sigma * mutability;
    for (value, spec) in values.iter_mut().zip(genes) {
        match spec.mutation {
            Mutation::Scale { keep_above, reject_below } => {
                if let Some(p) = keep_above
                    && rng.random() > p
                {
                    continue;
                }
                // Шанс перетягивания не выше половины при любой сигме, так что
                // цикл конечен (сигма обязана быть конечной — это проверяет Rules).
                let gauss = match reject_below {
                    Some(floor) => loop {
                        let g = rng.gauss(0.0, sigma);
                        if g >= floor {
                            break g;
                        }
                    },
                    None => rng.gauss(0.0, sigma),
                };
                let mut mutated = *value * (1.0 + gauss);
                if spec.is_percent() {
                    mutated = mutated.clamp(0.0, 100.0);
                }
                *value = mutated.max(0.01);
            }
            Mutation::Switch { chance } => {
                let n = spec.variants().map_or(0, <[Variant]>::len);
                if n < 2 {
                    continue; // один вариант: менять не на что, жребий не тянем
                }
                if rng.random() < chance * mutability {
                    // любой другой вариант, равновероятно
                    let k = rng.randint(0, n as i64 - 2) as usize;
                    let current = *value as usize;
                    *value = (k + (k >= current) as usize) as f64;
                }
            }
        }
    }
}

/// Мутагенность из значения гена: не выше `MAX_MUTABILITY`. Без потолка
/// множитель, уходя вверх поколение за поколением, мог бы дорасти до
/// бесконечности, а сигма — стать NaN.
pub(crate) fn mutability_of(gene: f64) -> f64 {
    gene.min(MAX_MUTABILITY)
}

/// Какой вариант гена-выбора получит существо `i` из `n` при стартовой смеси
/// `shares` (доли по порядку вариантов; пустая — у всех первый).
///
/// Без жребия: существо `i` берёт вариант, чья накопленная доля покрывает
/// точку (i + 0.5) / n. Смесь, разыгранная случайно, сдвинула бы все случайные
/// числа мира — и тот же сид дал бы другой мир при другой смеси.
pub fn variant_for(i: usize, n: usize, shares: &[f64], variants: usize) -> usize {
    let total: f64 = shares.iter().sum();
    if variants == 0 || n == 0 || total.is_nan() || total <= 0.0 {
        return 0;
    }
    let at = (i as f64 + 0.5) / n as f64 * total;
    let mut acc = 0.0;
    for (k, share) in shares.iter().enumerate() {
        acc += share;
        if at < acc {
            return k.min(variants - 1);
        }
    }
    (shares.len() - 1).min(variants - 1)
}

/// Базовые значения таблицы — стартовый геном.
pub(crate) const fn bases<const N: usize>(genes: &[GeneSpec; N]) -> [f64; N] {
    let mut out = [0.0; N];
    let mut i = 0;
    while i < N {
        out[i] = genes[i].base;
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE: [Variant; 3] = [
        Variant { key: "a", label: "а", about: "" },
        Variant { key: "b", label: "б", about: "" },
        Variant { key: "c", label: "в", about: "" },
    ];
    const ONE: [Variant; 1] = [Variant { key: "a", label: "а", about: "" }];

    fn choice(variants: &'static [Variant], chance: f64) -> GeneSpec {
        GeneSpec {
            key: "strategy",
            label: "стратегия",
            about: "",
            kind: GeneKind::Choice(variants),
            base: 0.0,
            mutation: Mutation::Switch { chance },
        }
    }

    #[test]
    fn выбор_из_одного_варианта_не_тянет_чисел() {
        let mut rng = Rng::new(5);
        let before = rng.clone();
        let mut v = [0.0];
        for _ in 0..100 {
            mutate_values(&mut v, &[choice(&ONE, 1.0)], 0.3, 1.0, &mut rng);
        }
        assert_eq!(v, [0.0]);
        assert_eq!(rng, before, "ген с одним вариантом не сдвигает случайные числа");
    }

    #[test]
    fn выбор_меняется_на_другой_и_в_пределах() {
        let mut rng = Rng::new(9);
        let mut seen = [0usize; 3];
        for start in 0..3 {
            for _ in 0..300 {
                let mut v = [start as f64];
                mutate_values(&mut v, &[choice(&THREE, 1.0)], 0.3, 1.0, &mut rng);
                let k = v[0] as usize;
                assert!(k < 3 && k != start, "с шансом 1 вариант меняется на другой: {start} → {k}");
                assert_eq!(v[0], k as f64, "номер варианта — целое");
                seen[k] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n > 150), "все варианты выпадают: {seen:?}");
    }

    #[test]
    fn стартовая_смесь_раздаётся_по_долям_без_жребия() {
        let got: Vec<usize> = (0..10).map(|i| variant_for(i, 10, &[0.3, 0.5, 0.2], 3)).collect();
        assert_eq!(got, [0, 0, 0, 1, 1, 1, 1, 1, 2, 2]);
        assert!((0..5).all(|i| variant_for(i, 5, &[], 3) == 0), "пустая смесь — у всех первый");
        assert!(
            (0..5).all(|i| variant_for(i, 5, &[0.0, 1.0], 1) == 0),
            "вариантов меньше долей — последний есть"
        );
    }

    #[test]
    fn выбор_с_нулевым_шансом_не_меняется() {
        let mut rng = Rng::new(1);
        let mut v = [2.0];
        for _ in 0..100 {
            mutate_values(&mut v, &[choice(&THREE, 0.0)], 0.3, 1.0, &mut rng);
        }
        assert_eq!(v, [2.0]);
    }
}
