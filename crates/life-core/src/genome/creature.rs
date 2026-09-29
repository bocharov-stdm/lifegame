//! A creature's genome.

use super::{GeneKind, GeneSpec, Genome, Mutation, Variant, bases};
use crate::config::{
    COLD_BLOOD_STEP, DIET_JUMP_CHANCE, DIET_MEAT_STEP_CHANCE, DIET_STEP_CHANCE, HERBIVORE_LEAP_CARNIVORE,
    HERBIVORE_LEAP_SCAVENGER, LIFESPAN_BASE, LIFESPAN_MAX, LIFESPAN_MIN, STRATEGY_SWITCH_CHANCE,
};
use crate::creature::Programs;
use crate::creature::strategy::VARIANTS as STRATEGIES;
use crate::rng::Rng;

/// A creature's genes: the row numbers of `GENES`.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gene {
    Size,
    Speed,
    Vision,
    Strategy,
    Mutability,
    Maturation,
    Diet,
    Lifespan,
    ColdBlood,
    Burst,
    ProgramMutability,
}

impl Gene {
    pub const ALL: [Gene; N] = [
        Gene::Size,
        Gene::Speed,
        Gene::Vision,
        Gene::Strategy,
        Gene::Mutability,
        Gene::Maturation,
        Gene::Diet,
        Gene::Lifespan,
        Gene::ColdBlood,
        Gene::Burst,
        Gene::ProgramMutability,
    ];
}

pub const N: usize = 11;

/// Which diets a child's diet may step to: the herbivore only to the omnivore; the omnivore is a
/// fork to the herbivore, the scavenger and the carnivore, a third each; the scavenger and the
/// carnivore to each other and back to the omnivore. In a chain one meat diet could arise only
/// from the other, and whichever stood last never appeared in the validation worlds.
pub const DIET_NEIGHBOURS: [&[usize]; 4] = [&[1], &[0, 2, 3], &[1, 3], &[1, 2]];

/// The neighbours towards meat: the herbivore to the omnivore, the omnivore to the scavenger and
/// the carnivore. A step there has its own, larger chance (`DIET_MEAT_STEP_CHANCE`): meat-eating
/// founders starve before there is meat, so the meat diets arise from mutants.
pub const DIET_TOWARDS_MEAT: [&[usize]; 4] = [&[1], &[2, 3], &[], &[]];

/// A diet's own leaps past its neighbours, with their chances: the herbivore may leap straight to
/// the carnivore or the scavenger (`HERBIVORE_LEAP_*`), so the meat diets do not hang on the
/// omnivores alone, who dwindle to 1–2% of a world. The others keep the general jump.
pub const DIET_LEAPS: [&[(usize, f64)]; 4] =
    [&[(3, HERBIVORE_LEAP_CARNIVORE), (2, HERBIVORE_LEAP_SCAVENGER)], &[], &[], &[]];

/// What a creature can digest (`config::DIET_DIGESTION`). A mutation steps to a neighbour in
/// `DIET_NEIGHBOURS`. Labels are game UI.
pub const DIET_VARIANTS: [Variant; 4] = [
    Variant {
        key: "herbivore",
        label: "травоядный",
        about: "Ест только растения и усваивает их лучше всех. Крепкий: здоровья в полтора раза больше, крупное тело дешевле.",
    },
    Variant {
        key: "omnivore",
        label: "всеядный",
        about: "Ест растения, свежее мясо и гниль, но всё усваивает хуже специалистов. Чужой пищи для него нет. Бьёт в полтора раза сильнее травоядного, чует падаль чуть дальше, чем видит; пока не вырос, усваивает растения полностью.",
    },
    Variant {
        key: "scavenger",
        label: "падальщик",
        about: "Ест только мясо: гниль усваивает лучше всех, свежее — как мясоед, но берёт его только голодным. Пока не вырос, растёт и на растениях. Чует трупы втрое дальше, чем видит; в глубине живёт экономнее.",
    },
    Variant {
        key: "carnivore",
        label: "мясоед",
        about: "Ест мясо: свежее усваивает полностью, гниль — плохо и только голодным. Пока не вырос, растёт на растениях. Бьёт сильнее всех, бегает дешевле, чует трупы в полтора раза дальше, чем видит.",
    },
];

/// A creature's mutation: the multiplier is no lower than 0.1, and one that falls lower is drawn
/// again, as in the Python version. The sigma comes from the world's rules.
const SCALE: Mutation = Mutation::Scale { keep_above: None, reject_below: Some(-0.9) };

/// The gene table. Only append to the end (see `genome/mod.rs`).
///
/// Percent genes are held in 0‒100 when they mutate. The layer, the division, the shooting and the
/// care for children were genes until `life-behavior/14`: they are settings of the behaviour
/// programs now (`program.rs`).
pub const GENES: [GeneSpec; N] = [
    GeneSpec {
        key: "size",
        label: "размер",
        about: "Диаметр тела: радиус поедания и запас энергии.",
        kind: GeneKind::Absolute,
        base: 40.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "speed",
        label: "скорость",
        about: "Шаг за тик.",
        kind: GeneKind::Absolute,
        base: 10.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "vision",
        label: "зрение",
        about: "Радиус поиска еды.",
        kind: GeneKind::Absolute,
        base: 400.0,
        mutation: SCALE,
    },
    GeneSpec {
        key: "strategy",
        label: "происхождение",
        about: "С какой программы поведения начинал род основателей. Потомкам не меняется: наследуется и мутирует сама программа.",
        kind: GeneKind::Choice(&STRATEGIES),
        base: 0.0,
        mutation: Mutation::Switch { chance: STRATEGY_SWITCH_CHANCE },
    },
    GeneSpec {
        key: "mutability",
        label: "мутагенность",
        about: "Множитель на разброс мутаций генов тела у потомка — всех, и этого тоже. Программы \
                поведения мутируют по своей мутагенности.",
        kind: GeneKind::Absolute,
        base: 1.0,
        mutation: SCALE,
    },
    // Replaced the numeric `life_pace` in place (see `genome/mod.rs`): the same law, so the same
    // draws.
    GeneSpec {
        key: "maturation",
        label: "взросление",
        about: "Какая доля съеденного идёт в рост, пока не вырос; остальное — в запас. Быстро растущий ходит с пустым баком, %.",
        kind: GeneKind::Percent,
        base: 50.0,
        mutation: SCALE,
    },
    // Replaced the numeric `carnivory` in place (see `genome/mod.rs`).
    GeneSpec {
        key: "diet",
        label: "питание",
        about: "Что ест и как усваивает; потомок изредка сдвигается на шаг: травоядный ↔ всеядный, всеядный → падальщик или мясоед, падальщик ↔ мясоед; травоядный изредка сразу становится мясоедом, ещё реже падальщиком; остальные совсем редко перескакивают в любое питание. Мясоед бьёт сильнее всех, за ним всеядный и падальщик.",
        kind: GeneKind::Choice(&DIET_VARIANTS),
        base: 0.0,
        mutation: Mutation::Neighbours {
            chance: DIET_STEP_CHANCE,
            rise: DIET_MEAT_STEP_CHANCE,
            jump: DIET_JUMP_CHANCE,
            of: &DIET_NEIGHBOURS,
            up: &DIET_TOWARDS_MEAT,
            leaps: &DIET_LEAPS,
        },
    },
    GeneSpec {
        key: "lifespan",
        label: "срок_жизни",
        about: "Сколько тиков живёт. С 70% срока слабеет: к 90% скорость, зрение, удар и здоровье — 70% прежних (500–10 000).",
        kind: GeneKind::Absolute,
        base: LIFESPAN_BASE,
        mutation: SCALE,
    },
    GeneSpec {
        key: "cold_blood",
        label: "хладнокровие",
        about: "Насколько тело остывает вместе с водой. В холодной глубине такое существо дешевле живёт \
                и медленнее плавает: при 100% в самой холодной воде содержание вдвое дешевле, скорость \
                на 40% ниже. В тёплой воде разницы нет, %.",
        kind: GeneKind::Percent,
        // warm-blooded, as before the gene; it moves off zero by points, not by a factor
        base: 0.0,
        mutation: Mutation::Shift { points: COLD_BLOOD_STEP },
    },
    GeneSpec {
        key: "burst",
        label: "рывок",
        about: "Во сколько раз быстрее своей скорости бросается в погоне и в бегстве (1–2): не дольше \
                20 тиков подряд, потом 60 тиков отдышки. Мышцы стоят и в покое — четверть цены \
                прибавки к скорости.",
        kind: GeneKind::Absolute,
        base: 1.0,
        mutation: SCALE,
    },
    // The programs' own rate, apart from the body's: the tempo of behaviour and the tempo of the
    // body need not be one. Its floor is the body's (`min_mutability`), so it cannot fall to zero.
    GeneSpec {
        key: "program_mutability",
        label: "мутагенность поведения",
        about: "Множитель на дрейф чисел и мутации программ поведения у потомка. Не ниже пола \
                мутагенности: до нуля не падает.",
        kind: GeneKind::Absolute,
        base: 1.0,
        mutation: SCALE,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreatureGenome([f64; N]);

impl CreatureGenome {
    /// The starting genome: the table's bases.
    pub const BASE: Self = Self(bases(&GENES));

    pub const fn from_values(values: [f64; N]) -> Self {
        Self(values)
    }

    pub const fn to_values(&self) -> [f64; N] {
        self.0
    }

    /// The same genome with another value of one gene.
    pub const fn with(mut self, gene: Gene, value: f64) -> Self {
        self.0[gene as usize] = value;
        self
    }

    /// The child's genome with config's heredity and this sigma (`mutate_by`).
    pub fn mutate(&self, sigma: f64, rng: &mut Rng) -> Self {
        self.mutate_by(&super::Heredity::with_sigma(sigma), rng)
    }

    /// The child's genome: an exact copy with `clone_share`, otherwise mutated (`mutate_values`).
    pub fn mutate_by(&self, h: &super::Heredity, rng: &mut Rng) -> Self {
        if rng.random() < h.clone_share {
            return *self; // an exact copy: no gene mutates
        }
        self.mutated(h, rng)
    }

    /// The child's genome and behaviour programs: with `clone_share` exact copies of all (the
    /// programs shared, not copied); otherwise the genes mutate (`mutate_values`), then each
    /// program — the juvenile one, then the adult one — drifts, a third of its numbers
    /// (`Program::drift`, the rule `program_drift`), and mutates with the rule `program_mutation`
    /// (`Program::mutate_with`, the parent's other track at hand for a transfer), both times the
    /// parent's `program_mutability` (the body's `mutability` does not touch them).
    pub fn inherit(&self, programs: &Programs, h: &super::Heredity, rng: &mut Rng) -> (Self, Programs) {
        if rng.random() < h.clone_share {
            return (*self, programs.clone());
        }
        let child = self.mutated(h, rng);
        // the programs' own rate; a transfer copies from the parent's other track
        let behaviour = super::mutability_of(self[Gene::ProgramMutability], h.min_mutability);
        let mut next = **programs;
        for stage in 0..2 {
            next[stage].drift(h.program_drift * behaviour, rng);
            next[stage].mutate_with(h.program_mutation * behaviour, Some(&programs[1 - stage]), rng);
        }
        let programs = if next == **programs { programs.clone() } else { Programs::new(next) };
        (child, programs)
    }

    /// Every gene by its law, no clone draw.
    fn mutated(&self, h: &super::Heredity, rng: &mut Rng) -> Self {
        let mut child = *self;
        let mutability = super::mutability_of(self[Gene::Mutability], h.min_mutability);
        // `DIET_LEAPS` with the world's chances, in the same order
        let herbivore = [(3, h.diet_leap_carnivore), (2, h.diet_leap_scavenger)];
        let diet = super::DietChances {
            step: h.diet_step,
            rise: h.diet_meat_step,
            jump: h.diet_jump,
            leaps: [&herbivore, &[], &[], &[]],
        };
        super::mutate_values(&mut child.0, &GENES, h.sigma, mutability, rng, Some(diet));
        child.0[Gene::Mutability as usize] = super::mutability_of(child[Gene::Mutability], h.min_mutability);
        child.0[Gene::ProgramMutability as usize] =
            super::mutability_of(child[Gene::ProgramMutability], h.min_mutability);
        child.0[Gene::Lifespan as usize] = child[Gene::Lifespan].clamp(LIFESPAN_MIN, LIFESPAN_MAX);
        child
    }
}

impl core::ops::Index<Gene> for CreatureGenome {
    type Output = f64;

    #[inline]
    fn index(&self, gene: Gene) -> &f64 {
        &self.0[gene as usize]
    }
}

impl Genome for CreatureGenome {
    const GENES: &'static [GeneSpec] = &GENES;

    fn values(&self) -> &[f64] {
        &self.0
    }

    fn values_mut(&mut self) -> &mut [f64] {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn номера_генов_совпадают_с_таблицей() {
        for (i, g) in Gene::ALL.iter().enumerate() {
            assert_eq!(*g as usize, i);
        }
        let mut keys: Vec<&str> = GENES.iter().map(|g| g.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), N, "имена генов не повторяются");
        assert_eq!(CreatureGenome::BASE.get("vision"), Some(400.0));
        assert_eq!(CreatureGenome::BASE[Gene::Diet], 0.0, "a spawned creature is a herbivore");
    }

    #[test]
    fn диета_меняется_редко_и_почти_всегда_на_шаг() {
        // the neighbours go both ways
        for (k, near) in DIET_NEIGHBOURS.iter().enumerate() {
            assert!(near.iter().all(|&j| j != k && DIET_NEIGHBOURS[j].contains(&k)), "{k}: {near:?}");
        }
        assert_eq!(DIET_NEIGHBOURS[0], &[1]);
        for (k, up) in DIET_TOWARDS_MEAT.iter().enumerate() {
            assert!(up.iter().all(|j| DIET_NEIGHBOURS[k].contains(j)), "{k}: up is among the neighbours");
        }
        let mut rng = Rng::new(21);
        let n = 200_000;
        for (start, near) in DIET_NEIGHBOURS.iter().enumerate() {
            let parent = CreatureGenome::BASE.with(Gene::Diet, start as f64);
            let (mut steps, mut jumps) = (0, 0);
            for _ in 0..n {
                let child = parent.mutate(0.3, &mut rng)[Gene::Diet];
                assert_eq!(child.fract(), 0.0);
                if child != start as f64 {
                    if near.contains(&(child as usize)) { steps += 1 } else { jumps += 1 }
                }
            }
            // half the children are copies; of the rest a step towards meat 2%, another step 0.5%,
            // a jump 0.01% of which some land on neighbours
            let up = DIET_TOWARDS_MEAT[start];
            let chance = if up.is_empty() { 0.0 } else { DIET_MEAT_STEP_CHANCE }
                + if near.len() > up.len() { DIET_STEP_CHANCE } else { 0.0 };
            let expected = n as f64 * 0.5 * chance;
            assert!(
                (expected * 0.85..=expected * 1.15 + 20.0).contains(&(steps as f64)),
                "steps from {start}: {steps} of {n}, expected {expected}"
            );
            if start == 0 {
                // the herbivore's own leaps: to the carnivore 0.1%, to the scavenger 0.01%
                let expected = n as f64 * 0.5 * (HERBIVORE_LEAP_CARNIVORE + HERBIVORE_LEAP_SCAVENGER);
                assert!(
                    (expected * 0.7..=expected * 1.3).contains(&(jumps as f64)),
                    "leaps from the herbivore: {jumps}, expected {expected}"
                );
            } else if near.len() == 3 {
                assert_eq!(jumps, 0, "the omnivore has every diet for a neighbour");
            }
        }
    }

    /// The general jump is rare (0.01%), so it is counted apart, pooled over the scavenger and the
    /// carnivore (their one non-neighbour is the herbivore) and over enough children to expect ~30
    /// jumps: with ~3 per diet, as it was, any change to the draw order could turn it into 0.
    #[test]
    fn a_diet_jump_past_the_neighbours_is_rare_but_happens() {
        let mut rng = Rng::new(22);
        let n = 1_000_000;
        let mut jumps = 0;
        for start in [2usize, 3] {
            let parent = CreatureGenome::BASE.with(Gene::Diet, start as f64);
            for _ in 0..n {
                let child = parent.mutate(0.3, &mut rng)[Gene::Diet] as usize;
                if child != start && !DIET_NEIGHBOURS[start].contains(&child) {
                    jumps += 1;
                }
            }
        }
        // half are copies; a jump picks one of the three other diets, one of which is far
        let expected = 2.0 * n as f64 * 0.5 * DIET_JUMP_CHANCE / 3.0;
        assert!(
            (expected * 0.4..=expected * 1.8).contains(&(jumps as f64)),
            "jumps past the neighbours: {jumps}, expected {expected:.0}"
        );
    }

    /// `cold_blood` starts warm-blooded at 0 and a mutation moves it by points, so it leaves zero,
    /// and it stays within 0‒100.
    #[test]
    fn cold_blood_moves_off_zero_by_points() {
        let mut rng = Rng::new(3);
        let (mut moved, mut sum) = (0, 0.0);
        for _ in 0..10_000 {
            let child = CreatureGenome::BASE.mutate(0.3, &mut rng)[Gene::ColdBlood];
            assert!((0.0..=100.0).contains(&child), "{child}");
            moved += (child > 0.0) as usize;
            sum += child;
        }
        // half are copies; of the rest half go up (the other half clamp to 0)
        assert!((2000..=3000).contains(&moved), "moved off zero: {moved}");
        // gauss(0, 10) above zero averages 10 × sqrt(2/π) ≈ 8 points
        let mean = sum / moved as f64;
        assert!((6.0..=10.0).contains(&mean), "{mean}");
        let full = CreatureGenome::BASE.with(Gene::ColdBlood, 100.0);
        assert!((0..1000).all(|_| full.mutate(0.3, &mut rng)[Gene::ColdBlood] <= 100.0));
    }

    /// A parent's mutability stretches the spread of every gene, its own too; the ceiling
    /// `MAX_MUTABILITY` keeps it finite.
    #[test]
    fn мутагенность_растягивает_разброс_потомков() {
        let spread = |m: f64| {
            let parent = CreatureGenome::BASE.with(Gene::Mutability, m);
            let mut rng = Rng::new(3);
            let (mut size, mut own) = (0.0, 0.0);
            for _ in 0..2000 {
                let child = parent.mutate(0.3, &mut rng);
                size += (child[Gene::Size] / 40.0 - 1.0).abs();
                own += (child[Gene::Mutability] / m - 1.0).abs();
            }
            (size / 2000.0, own / 2000.0)
        };
        let (low, high) = (spread(0.2), spread(2.0));
        assert!(high.0 > low.0 * 5.0, "size: {:.3} against {:.3}", high.0, low.0);
        assert!(high.1 > low.1 * 5.0, "mutability itself: {:.3} against {:.3}", high.1, low.1);
        let base = spread(1.0);
        let expected = 0.3 * 0.8 * (1.0 - crate::config::CLONE_CHANCE);
        assert!(
            (base.0 - expected).abs() < 0.03,
            "at 1 the spread is the rules' sigma (half are copies): {:.3}",
            base.0
        );
        let capped = CreatureGenome::BASE.with(Gene::Mutability, 1e300).mutate(0.3, &mut Rng::new(1));
        assert!(capped.to_values().iter().all(|v| v.is_finite()), "the ceiling: the genome stays finite");
    }

    /// The gene `program_mutability` sets how often the programs mutate and how far their numbers
    /// drift; the body's `mutability` does not touch them.
    #[test]
    fn program_mutability_scales_the_programs_not_the_body() {
        // (children whose programs mutated, the summed drift of the adult hunt's ratio)
        let programs = |gene: Gene, m: f64| {
            let parent = CreatureGenome::BASE.with(gene, m);
            let mut rng = Rng::new(317);
            let h = super::super::Heredity::with_sigma(0.0);
            let both = Programs::both(crate::creature::Program::STANDARD);
            let (mut mutated, mut drift) = (0, 0.0);
            for _ in 0..50_000 {
                let (_, child) = parent.inherit(&both, &h, &mut rng);
                mutated += child.iter().any(|p| p.changes > 0) as usize;
                drift += (child[crate::creature::ADULT].hunt_ratio().unwrap_or(1.5) - 1.5).abs();
            }
            (mutated as f64, drift)
        };
        let (rare, frequent) =
            (programs(Gene::ProgramMutability, 0.2), programs(Gene::ProgramMutability, 2.0));
        assert!(frequent.0 > rare.0 * 5.0, "programs mutated: {} against {}", frequent.0, rare.0);
        assert!(frequent.1 > rare.1 * 5.0, "numbers drifted: {:.0} against {:.0}", frequent.1, rare.1);
        // the body's mutability: the programs change alike (only its rare switches of a choice
        // gene shift the draws)
        let (calm, wild) = (programs(Gene::Mutability, 0.2), programs(Gene::Mutability, 2.0));
        let alike = |a: f64, b: f64| (0.8..1.25).contains(&(a / b));
        assert!(alike(calm.0, wild.0) && alike(calm.1, wild.1), "{calm:?} against {wild:?}");
    }
}
