//! Геном таблицей: у каждого вида — список генов (`GENES`) с именем, подписью,
//! базой, видом значения и законом мутации. Всё, что обходит гены (мутация,
//! сводки, отчёт, графики, карточка существа), идёт по таблице, а не по
//! номерам: новый ген — одна строка таблицы и его действие в фенотипе.
//!
//! **Таблица только дописывается в конец.** От порядка генов зависят порядок
//! случайных чисел при мутации (значит, каждый сид), позиции генов в эталоне
//! баланса и JSON отчёта.
//! Deliberate exceptions: the numeric `carnivory` row was replaced in place by the choice gene
//! `diet` (model `life-behavior/10`), and `life_pace` by `maturation` (same law, same draws). A
//! gene that no longer acts should not keep a dead row that still draws numbers, and replacing it
//! in place keeps every later gene's position.
//!
//! Значение гена всегда f64: у гена-выбора это номер варианта (0, 1, 2…).

pub mod creature;

use crate::config::{MAX_MUTABILITY, MIN_MUTABILITY};
use crate::rng::Rng;

pub use creature::CreatureGenome;

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
    /// A step to a neighbouring variant: `of[k]` lists the neighbours of variant `k` (the diets:
    /// the omnivore is a fork to the herbivore, the scavenger and the carnivore), `up[k]` those of
    /// them up the chain (the diets: towards meat). A step up has chance `rise`, a step to any
    /// other neighbour `chance`. One of several neighbours is picked with equal odds (one more
    /// draw); with one there is no second draw. Otherwise it may leap past its neighbours: to the
    /// variants `leaps[k]` lists, each with its own chance (no more draws), or, when that list is
    /// empty, with chance `jump` to any other variant (one more draw). The same first draw decides
    /// all of it. With one variant nothing is drawn.
    Neighbours {
        chance: f64,
        rise: f64,
        jump: f64,
        of: &'static [&'static [usize]],
        up: &'static [&'static [usize]],
        leaps: &'static [&'static [(usize, f64)]],
    },
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
/// законов `Scale` — из правил мира.
/// `mutability` — ген мутагенности родителя (`mutability_of`): умножает и
/// разброс, и шанс смены варианта, у всех генов сразу, включая себя самого.
///
/// Порядок и число случайных чисел — часть поведения мира: у существ цикл
/// gauss до множителя не ниже 0.1 (`reject_below`), с `keep_above` — ещё жребий
/// «оставить» перед ним.
///
/// `diet` replaces the chances of the `Neighbours` law (step, step up, jump, own leaps) with the
/// world's rules.
pub(crate) fn mutate_values(
    values: &mut [f64],
    genes: &[GeneSpec],
    sigma: f64,
    mutability: f64,
    rng: &mut Rng,
    diet: Option<DietChances>,
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
            Mutation::Neighbours { chance, rise, jump, of, up, leaps } => {
                let (chance, rise, jump, leaps) = match &diet {
                    Some(d) => (d.step, d.rise, d.jump, &d.leaps[..]),
                    None => (chance, rise, jump, leaps),
                };
                let n = spec.variants().map_or(0, <[Variant]>::len);
                if n < 2 {
                    continue;
                }
                // the diet changes regardless of mutability: its chances are small anyway
                let u = rng.random();
                let current = (*value as usize).min(n - 1);
                let ups = up.get(current).copied().unwrap_or(&[]);
                let near = of.get(current).copied().unwrap_or(&[]);
                let rise = if ups.is_empty() { 0.0 } else { rise };
                // one of the listed neighbours, with a draw only when there is a choice
                let mut pick = |mut options: std::iter::Peekable<std::slice::Iter<usize>>, k: usize| match k {
                    0 => *value,
                    1 => *options.peek().copied().expect("one option") as f64,
                    k => *options.nth(rng.randint(0, k as i64 - 1) as usize).expect("in range") as f64,
                };
                if u < rise {
                    *value = pick(ups.iter().peekable(), ups.len());
                } else if u < rise + chance {
                    let others: Vec<usize> = near.iter().copied().filter(|j| !ups.contains(j)).collect();
                    *value = pick(others.iter().peekable(), others.len());
                } else {
                    let own = leaps.get(current).copied().unwrap_or(&[]);
                    if own.is_empty() {
                        if u < rise + chance + jump {
                            // any other variant, as `Switch` picks it
                            let k = rng.randint(0, n as i64 - 2) as usize;
                            *value = (k + (k >= current) as usize) as f64;
                        }
                    } else {
                        let mut edge = rise + chance;
                        for &(target, p) in own {
                            edge += p;
                            if u < edge {
                                *value = target as f64;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The diet's chances from the world's rules, in place of the `Neighbours` law's own: a step, a
/// step up, the general jump and each variant's own leaps (`Mutation::Neighbours`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DietChances<'a> {
    pub step: f64,
    pub rise: f64,
    pub jump: f64,
    pub leaps: [&'a [(usize, f64)]; 4],
}

/// Mutability from the gene's value, from `floor` (`MIN_MUTABILITY` by default) to `MAX_MUTABILITY`.
/// Without the ceiling the multiplier could grow generation by generation to infinity and the sigma
/// become NaN; without the floor selection drove it to zero and evolution stopped.
pub(crate) fn mutability_of(gene: f64, floor: f64) -> f64 {
    gene.clamp(floor.min(MAX_MUTABILITY), MAX_MUTABILITY)
}

/// How children inherit, from the world's rules: the mutation sigma, the share of exact copies,
/// the floor of mutability and the diet's chances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heredity {
    pub sigma: f64,
    pub clone_share: f64,
    pub min_mutability: f64,
    pub diet_step: f64,
    pub diet_meat_step: f64,
    pub diet_jump: f64,
    /// The herbivore's own leaps to the carnivore and to the scavenger (`DIET_LEAPS`).
    pub diet_leap_carnivore: f64,
    pub diet_leap_scavenger: f64,
}

impl Heredity {
    pub fn of(rules: &crate::Rules) -> Heredity {
        Heredity {
            sigma: rules.mutation_sigma,
            clone_share: rules.clone_share,
            min_mutability: rules.min_mutability,
            diet_step: rules.diet_step,
            diet_meat_step: rules.diet_meat_step,
            diet_jump: rules.diet_jump,
            diet_leap_carnivore: rules.diet_leap_carnivore,
            diet_leap_scavenger: rules.diet_leap_scavenger,
        }
    }

    /// config.rs's heredity with another sigma: for tests and tools.
    pub fn with_sigma(sigma: f64) -> Heredity {
        Heredity {
            sigma,
            clone_share: crate::config::CLONE_CHANCE,
            min_mutability: MIN_MUTABILITY,
            diet_step: crate::config::DIET_STEP_CHANCE,
            diet_meat_step: crate::config::DIET_MEAT_STEP_CHANCE,
            diet_jump: crate::config::DIET_JUMP_CHANCE,
            diet_leap_carnivore: crate::config::HERBIVORE_LEAP_CARNIVORE,
            diet_leap_scavenger: crate::config::HERBIVORE_LEAP_SCAVENGER,
        }
    }
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

/// Order in which `n` founders take a start mix that must not line up with another one: two
/// mixes dealt by `variant_for` over the same numbers match block by block (with lurkers dealt
/// last, the first herbivores would all be standard). `ranks[i]` is founder `i`'s place in the
/// order of the fractional parts of `(i + 0.5)·φ`, so `variant_for(ranks[i], n, …)` keeps the
/// shares exact and spreads each variant over the founders. No draws.
pub fn spread_ranks(n: usize) -> Vec<usize> {
    const GOLDEN: f64 = 0.618_033_988_749_894_9;
    let key = |i: usize| ((i as f64 + 0.5) * GOLDEN).fract();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| key(a).total_cmp(&key(b)).then(a.cmp(&b)));
    let mut ranks = vec![0; n];
    for (rank, i) in order.into_iter().enumerate() {
        ranks[i] = rank;
    }
    ranks
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
            mutate_values(&mut v, &[choice(&ONE, 1.0)], 0.3, 1.0, &mut rng, None);
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
                mutate_values(&mut v, &[choice(&THREE, 1.0)], 0.3, 1.0, &mut rng, None);
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

    /// A chain a — b — c.
    const CHAIN: [&[usize]; 3] = [&[1], &[0, 2], &[1]];

    fn chain(chance: f64) -> GeneSpec {
        GeneSpec {
            mutation: Mutation::Neighbours { chance, rise: 0.0, jump: 0.0, of: &CHAIN, up: &[], leaps: &[] },
            ..choice(&THREE, chance)
        }
    }

    #[test]
    fn шаг_только_к_соседу() {
        let mut rng = Rng::new(11);
        let mut seen = [[0usize; 3]; 3];
        for (start, row) in seen.iter_mut().enumerate() {
            for _ in 0..600 {
                let mut v = [start as f64];
                mutate_values(&mut v, &[chain(1.0)], 0.3, 1.0, &mut rng, None);
                row[v[0] as usize] += 1;
            }
        }
        assert_eq!(seen[0], [0, 600, 0], "from the first end only inwards");
        assert_eq!(seen[2], [0, 600, 0], "from the last end only inwards");
        assert_eq!(seen[1][1], 0, "an inner variant always steps");
        assert!(seen[1][0] > 240 && seen[1][2] > 240, "both ways about equally: {:?}", seen[1]);
    }

    /// A jump leaps anywhere but stays put, and it shares the first draw with the step.
    #[test]
    fn прыжок_в_любой_другой_вариант() {
        const FAR: [&[usize]; 3] = [&[1], &[0, 2], &[1]];
        let spec = GeneSpec {
            mutation: Mutation::Neighbours {
                chance: 0.0,
                rise: 0.0,
                jump: 1.0,
                of: &FAR,
                up: &[],
                leaps: &[],
            },
            ..choice(&THREE, 1.0)
        };
        let mut rng = Rng::new(8);
        let mut seen = [0usize; 3];
        for _ in 0..3000 {
            let mut v = [0.0];
            mutate_values(&mut v, &[spec], 0.3, 1.0, &mut rng, None);
            seen[v[0] as usize] += 1;
        }
        assert_eq!(seen[0], 0, "a jump always leaves");
        assert!(seen[2] > 1300, "a jump reaches a variant that is not a neighbour: {seen:?}");
    }

    /// A fork: the middle variant steps to either of three neighbours alike; one with a single
    /// neighbour draws only the chance.
    #[test]
    fn развилка_делит_шаг_поровну() {
        const FOUR: [Variant; 4] = [
            Variant { key: "a", label: "а", about: "" },
            Variant { key: "b", label: "б", about: "" },
            Variant { key: "c", label: "в", about: "" },
            Variant { key: "d", label: "г", about: "" },
        ];
        const FORK: [&[usize]; 4] = [&[1], &[0, 2, 3], &[1, 3], &[1, 2]];
        let spec = GeneSpec {
            mutation: Mutation::Neighbours {
                chance: 1.0,
                rise: 0.0,
                jump: 0.0,
                of: &FORK,
                up: &[],
                leaps: &[],
            },
            ..choice(&FOUR, 1.0)
        };
        let mut rng = Rng::new(3);
        let mut seen = [0usize; 4];
        for _ in 0..3000 {
            let mut v = [1.0];
            mutate_values(&mut v, &[spec], 0.3, 1.0, &mut rng, None);
            seen[v[0] as usize] += 1;
        }
        assert_eq!(seen[1], 0);
        assert!(seen.iter().enumerate().all(|(k, &n)| k == 1 || (850..=1150).contains(&n)), "{seen:?}");
        let (mut a, mut b) = (Rng::new(4), Rng::new(4));
        let mut v = [0.0];
        mutate_values(&mut v, &[spec], 0.3, 1.0, &mut a, None);
        b.random();
        assert_eq!((v, a), ([1.0], b), "a single neighbour: only the chance is drawn");
    }

    /// A step up the chain has its own chance; from a variant with nothing above only the others
    /// remain, and a step down never goes up.
    #[test]
    fn a_step_up_has_its_own_chance() {
        const FORK: [&[usize]; 4] = [&[1], &[0, 2, 3], &[1, 3], &[1, 2]];
        const UP: [&[usize]; 4] = [&[1], &[2, 3], &[], &[]];
        const FOUR: [Variant; 4] = [
            Variant { key: "a", label: "а", about: "" },
            Variant { key: "b", label: "б", about: "" },
            Variant { key: "c", label: "в", about: "" },
            Variant { key: "d", label: "г", about: "" },
        ];
        let spec = GeneSpec {
            mutation: Mutation::Neighbours {
                chance: 0.1,
                rise: 0.4,
                jump: 0.0,
                of: &FORK,
                up: &UP,
                leaps: &[],
            },
            ..choice(&FOUR, 1.0)
        };
        let mut rng = Rng::new(9);
        let mut seen = [[0usize; 4]; 4];
        for (start, row) in seen.iter_mut().enumerate() {
            for _ in 0..10_000 {
                let mut v = [start as f64];
                mutate_values(&mut v, &[spec], 0.3, 1.0, &mut rng, None);
                row[v[0] as usize] += 1;
            }
        }
        assert!((3700..=4300).contains(&seen[0][1]), "up 40%: {:?}", seen[0]);
        assert!((3700..=4300).contains(&(seen[1][2] + seen[1][3])), "up to either: {:?}", seen[1]);
        assert!(seen[1][2] > 1700 && seen[1][3] > 1700, "{:?}", seen[1]);
        assert!((850..=1150).contains(&seen[1][0]), "down 10%: {:?}", seen[1]);
        assert!((850..=1150).contains(&(seen[2][1] + seen[2][3])), "nothing above: 10%: {:?}", seen[2]);
    }

    /// A variant with leaps of its own leaps only there, each with its chance, and never by the
    /// general jump; the others keep the general jump.
    #[test]
    fn own_leaps_replace_the_general_jump() {
        const FORK: [&[usize]; 4] = [&[1], &[0, 2, 3], &[1, 3], &[1, 2]];
        const LEAPS: [&[(usize, f64)]; 4] = [&[(3, 0.2), (2, 0.05)], &[], &[], &[]];
        const FOUR: [Variant; 4] = [
            Variant { key: "a", label: "а", about: "" },
            Variant { key: "b", label: "б", about: "" },
            Variant { key: "c", label: "в", about: "" },
            Variant { key: "d", label: "г", about: "" },
        ];
        let spec = GeneSpec {
            mutation: Mutation::Neighbours {
                chance: 0.0,
                rise: 0.0,
                jump: 0.5,
                of: &FORK,
                up: &[],
                leaps: &LEAPS,
            },
            ..choice(&FOUR, 1.0)
        };
        let mut rng = Rng::new(17);
        let mut from_first = [0usize; 4];
        for _ in 0..10_000 {
            let mut v = [0.0];
            mutate_values(&mut v, &[spec], 0.3, 1.0, &mut rng, None);
            from_first[v[0] as usize] += 1;
        }
        assert_eq!(from_first[1], 0, "no general jump from a variant with leaps of its own");
        assert!((1800..=2200).contains(&from_first[3]), "20%: {from_first:?}");
        assert!((400..=600).contains(&from_first[2]), "5%: {from_first:?}");
        let mut moved = 0;
        for _ in 0..10_000 {
            let mut v = [2.0];
            mutate_values(&mut v, &[spec], 0.3, 1.0, &mut rng, None);
            moved += (v[0] != 2.0) as usize;
        }
        assert!((4700..=5300).contains(&moved), "the others keep the general jump: {moved}");
    }

    #[test]
    fn a_diet_step_is_rare_and_ignores_mutability() {
        let steps = |mutability: f64| {
            let mut rng = Rng::new(13);
            (0..100_000)
                .filter(|_| {
                    let mut v = [1.0];
                    mutate_values(&mut v, &[chain(0.001)], 0.3, mutability, &mut rng, None);
                    v[0] != 1.0
                })
                .count()
        };
        let (base, doubled) = (steps(1.0), steps(2.0));
        assert!((60..=140).contains(&base), "about 0.1%: {base}");
        assert_eq!(doubled, base, "mutability does not touch the diet's chances");
        let mut rng = Rng::new(5);
        let before = rng.clone();
        let mut v = [0.0];
        mutate_values(
            &mut v,
            &[GeneSpec {
                mutation: Mutation::Neighbours {
                    chance: 1.0,
                    rise: 1.0,
                    jump: 1.0,
                    of: &[&[]],
                    up: &[&[]],
                    leaps: &[&[(0, 1.0)]],
                },
                ..choice(&ONE, 1.0)
            }],
            0.3,
            1.0,
            &mut rng,
            None,
        );
        assert_eq!((v, rng), ([0.0], before), "one variant: nothing drawn");
    }

    #[test]
    fn вперемешку_доли_те_же_а_блоки_не_совпадают() {
        let n = 20;
        let ranks = spread_ranks(n);
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..n).collect::<Vec<_>>(), "a permutation");
        let diet: Vec<usize> =
            (0..n).map(|i| variant_for(ranks[i], n, &[55.0, 25.0, 10.0, 10.0], 4)).collect();
        let count = |k| diet.iter().filter(|&&d| d == k).count();
        assert_eq!([count(0), count(1), count(2), count(3)], [11, 5, 2, 2]);
        // a 50/50 mix dealt in order: each half gets herbivores and the rest
        for half in [0..10, 10..20] {
            assert!(diet[half.clone()].contains(&0) && diet[half].iter().any(|&d| d != 0));
        }
    }

    #[test]
    fn выбор_с_нулевым_шансом_не_меняется() {
        let mut rng = Rng::new(1);
        let mut v = [2.0];
        for _ in 0..100 {
            mutate_values(&mut v, &[choice(&THREE, 0.0)], 0.3, 1.0, &mut rng, None);
        }
        assert_eq!(v, [2.0]);
    }
}
