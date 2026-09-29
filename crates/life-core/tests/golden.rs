//! The golden test: the world behaves exactly as it did when the constants were recorded.
//!
//! Needed for reorganisations without a change of behaviour (the genome as a table, the senses,
//! the strategies): they must not shift a single random number or a single formula. The world's
//! fingerprint at the checkpoint ticks is FNV-1a over the bits of what survives any refactoring:
//! coordinates, energy, numbers, genes and a probe of each creature's generator (an extra or a
//! missing draw changes the probe at once, not a hundred ticks later). The fingerprint also
//! includes the behaviour's memory and the temporary protection of «parent — child of another
//! way of life» pairs.
//!
//! A deliberate change of behaviour (a new gene, a new strategy) breaks the test by definition:
//! then the constants are rewritten in a commit of their own, together with `--save-reference`.
//! The test prints a ready table to paste. A new case without recorded fingerprints also fails
//! the test — so that it does not pass silently.
//!
//! The maths (`ln`, `cos`, `powf`) comes from the system library, and on Linux the last bit may
//! differ: the constants were recorded on Windows and are checked only there; on other systems
//! the test prints the fingerprints.

use life_core::flora::Profile;
use life_core::genome::CreatureGenome;
use life_core::genome::creature::Gene;
use life_core::{Rules, Shape, World, WorldConfig};

const CHECKPOINTS: [u64; 10] = [1, 2, 10, 31, 100, 250, 500, 1000, 2000, 3000];

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    fn u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
}

/// The world's fingerprint. Only what is public and only what survives a refactoring; the only
/// things that change with it are the access paths to the genes — such edits are reviewed.
fn digest(w: &World) -> u64 {
    let mut h = Fnv::new();
    h.u64(w.tick);
    let c = w.counters;
    for v in [
        c.plants_grown,
        c.plants_eaten,
        c.plant_bites,
        c.meat_bites,
        c.ranged_shots,
        c.born,
        c.starved,
        c.old_age,
        c.combat,
    ] {
        h.u64(v);
    }
    h.u64(w.plants.len() as u64);
    for p in &w.plants {
        h.f64(p.x);
        h.f64(p.y);
        h.u64(p.alive() as u64);
        h.u64(p.slot().map_or(u64::MAX, |s| s as u64));
        h.u64(p.born as u64);
        h.u64(p.portions as u64);
    }
    h.u64(w.corpses.len() as u64);
    for c in &w.corpses {
        for byte in format!("{c:?}").bytes() {
            h.u64(byte as u64);
        }
    }
    h.u64(w.shots.len() as u64);
    for shot in &w.shots {
        for byte in format!("{shot:?}").bytes() {
            h.u64(byte as u64);
        }
    }
    h.u64(w.creatures.len() as u64);
    for v in &w.creatures {
        h.u64(v.id);
        for byte in format!("{:?}", v.mind).bytes() {
            h.u64(byte as u64);
        }
        h.u64(v.parent);
        h.f64(v.birth_size);
        h.f64(v.pheno.size);
        h.f64(v.age);
        h.f64(v.health);
        h.u64(v.reproduction_wait);
        h.u64(v.peaceful_ticks as u64);
        h.u64(v.death.map_or(0, |d| d as u64 + 1));
        h.u64(v.mind.flee_ticks as u64);
        h.f64(v.mind.flee_dx);
        h.f64(v.mind.flee_dy);
        h.u64(v.mind.attack.unwrap_or(0));
        h.u64(v.mind.target.is_some() as u64);
        if let Some((x, y)) = v.mind.target {
            h.f64(x);
            h.f64(y);
        }
        h.f64(v.x);
        h.f64(v.y);
        h.f64(v.energy);
        h.u64(v.alive as u64);
        // all the genes: a new gene shifts the mutation's draws anyway (except an inert choice gene
        // with one variant), and the strategy shows in the fingerprint at once
        for g in v.genome.to_values() {
            h.f64(g);
        }
        // the behaviour programs, block by block, and how far each is from its template
        for p in v.programs.iter() {
            for b in p.blocks() {
                for code in b.code() {
                    h.u64(code);
                }
            }
            h.u64(u64::from(p.changes));
        }
        h.u64(v.rng.clone().next_u64());
    }
    // the world's stream and the counter of numbers: probed on a copy of the world
    let mut probe = w.clone();
    let id = probe.spawn(CreatureGenome::BASE, 100.0, 100.0, None);
    h.u64(id);
    h.u64(probe.creature(id).expect("подсаженное существо").rng.clone().next_u64());
    h.0
}

fn rules(pairs: &[(&str, f64)]) -> Rules {
    pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
}

/// What is checked in the configuration: without it the fingerprint might miss a branch.
#[derive(Default)]
struct Seen {
    giant: f64,
    /// Ticks on which both variants of the creatures' strategy lived.
    both_strategies: u64,
}

struct Case {
    name: &'static str,
    cfg: WorldConfig,
    ticks: u64,
    /// An intervention before a tick: rules on the fly, a probe.
    before: fn(&mut World),
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "A: сид 1, по умолчанию",
            cfg: WorldConfig { seed: 1, ..Default::default() },
            ticks: 3000,
            before: |_| {},
        },
        Case {
            name: "B: сид 3, гиганты",
            cfg: WorldConfig {
                seed: 3,
                rules: rules(&[("size_power", 1.0), ("plant_energy", 120.0)]),
                ..Default::default()
            },
            ticks: 2000,
            before: |_| {},
        },
        Case {
            name: "C: сид 7, лаборатория",
            cfg: WorldConfig {
                seed: 7,
                rules: rules(&[("mutation_sigma", 1.0), ("plant_energy", 80.0)]),
                ..Default::default()
            },
            ticks: 3000,
            before: |_| {},
        },
        // The strip explicitly: it was recorded before the shapes, and the default is now 3:2.
        Case {
            name: "D: сид 2, масштаб 10",
            cfg: WorldConfig { seed: 2, scale: 10.0, shape: Shape::Strip, ..Default::default() },
            ticks: 500,
            before: |_| {},
        },
        Case {
            name: "E: сид 3, правила на ходу и подсадка",
            cfg: WorldConfig { seed: 3, ..Default::default() },
            ticks: 1000,
            before: |w| match w.tick {
                400 => w.set_rules(rules(&[("cost_scale", 2.0), ("size_power", 2.0)])),
                600 => {
                    let g = w.creatures.first().map(|v| v.genome).expect("существа живы");
                    w.spawn(g, 3000.0, 500.0, None);
                }
                _ => {}
            },
        },
        Case {
            name: "F: сид 5, смесь стратегий",
            cfg: WorldConfig { seed: 5, strategies: vec![1.0, 1.0], ..Default::default() },
            ticks: 2000,
            before: |_| {},
        },
        // The shape and the tabular food profiles: another path of plant sampling.
        Case {
            name: "G: сид 6, квадрат x10, еда линейно и волнами",
            cfg: WorldConfig {
                seed: 6,
                scale: 10.0,
                shape: Shape::Square,
                rules: rules(&[
                    ("plant_depth_profile", Profile::Linear.index()),
                    ("plant_width_profile", Profile::Waves.index()),
                ]),
                ..Default::default()
            },
            ticks: 1000,
            before: |_| {},
        },
        // Another seed of the default world: combat is always on.
        Case {
            name: "H: сид 8, бои",
            cfg: WorldConfig { seed: 8, ..Default::default() },
            ticks: 2000,
            before: |_| {},
        },
    ]
}

fn run(case: &Case) -> (Vec<(u64, u64)>, World, Seen) {
    let mut w = World::new(&case.cfg);
    let mut seen = Seen::default();
    let mut out = Vec::new();
    for _ in 0..case.ticks {
        (case.before)(&mut w);
        w.step();
        seen.giant = w.creatures.iter().map(|v| v.pheno.size).fold(seen.giant, f64::max);
        let both = |kinds: &mut dyn Iterator<Item = f64>| {
            let mut seen = [false; 2];
            kinds.for_each(|k| seen[(k != 0.0) as usize] = true);
            (seen[0] && seen[1]) as u64
        };
        seen.both_strategies += both(&mut w.creatures.iter().map(|v| v.genome[Gene::Strategy]));
        if CHECKPOINTS.contains(&w.tick) {
            out.push((w.tick, digest(&w)));
        }
    }
    (out, w, seen)
}

#[cfg(windows)]
#[rustfmt::skip]
const GOLDEN: &[&[(u64, u64)]] = &[
    // A: seed 1, default
    &[(1, 0x5e19ba4f3dfc3315), (2, 0x1a59056a3304b2d0), (10, 0x9314b5d49241748c), (31, 0x90a18df8c1e9ca51), (100, 0x8a607b31a03557ef), (250, 0x22153b0e2e609be4), (500, 0xd875e3e42e3e64a8), (1000, 0x8cf627f4abee340c), (2000, 0x4adc2e4ce65913db), (3000, 0x95f5f71144761c40), ],
    // B: seed 3, giants
    &[(1, 0x6411f5c331fa53e4), (2, 0xc2b333c94b0c4dad), (10, 0x0d18286e8bd4bf39), (31, 0x653c57bf6864d1c1), (100, 0x01a13d5ccc896a9b), (250, 0x1c3f5cefccf7f4c9), (500, 0x52deebf3198e17ae), (1000, 0x69f6ed334dd1f305), (2000, 0xcd44020c9e1413c3), ],
    // C: seed 7, lab
    &[(1, 0xd34b6099dcaec19a), (2, 0xb509ce0e769adf3b), (10, 0x7ad1f2e501739cd2), (31, 0x699fa3d6cc4b5777), (100, 0xe1fed2f794cba630), (250, 0x2a48b33649697f37), (500, 0x5b4b7e884df4af10), (1000, 0x53671ba9d7bc7e89), (2000, 0x3ab9865e82a0cae6), (3000, 0x8bf6a27c1d367a7e), ],
    // D: seed 2, scale 10
    &[(1, 0x060fcc7182def373), (2, 0x2f694834705d912a), (10, 0x39e9d92e15bfbb64), (31, 0x16c2e3d8977a91e0), (100, 0xfd8c705862ed6b39), (250, 0xb9eaf24c810ec12e), (500, 0x7684fe81fc6ed1cb), ],
    // E: seed 3, rules on the fly and a probe
    &[(1, 0x3296835985831824), (2, 0x4551e9badfdef199), (10, 0x404e48936cfbdb82), (31, 0xbb88766a73890091), (100, 0x0194f8582c1ed67d), (250, 0x64d8a18ad0af7a44), (500, 0xace1b0af644afb77), (1000, 0x1c6102cda5b03618), ],
    // F: seed 5, a mix of strategies
    &[(1, 0x5f930a716b3d0745), (2, 0xff3a3a5a5d2cc4d9), (10, 0xf912cfa932f7bc23), (31, 0x05caf8e72aeb9e2a), (100, 0x4845e8b0039084ab), (250, 0x2d4fda62dfa6942a), (500, 0xc11480137b1b1012), (1000, 0x688d582f4133e4ff), (2000, 0x80e405312385cda4), ],
    // G: seed 6, a x10 square, food linear and in waves
    &[(1, 0x316a0050afabb123), (2, 0x0f129ee645c9d467), (10, 0xf4a2e8105bafc532), (31, 0xd0446a37e08f9540), (100, 0x71bbe321a2a07776), (250, 0xe541871ec174e7b7), (500, 0xf92d4fb9b3d20329), (1000, 0xe703957a1704c94f), ],
    // H: seed 8, fights
    &[(1, 0x2b4fc30cc50b28c0), (2, 0x3222278f776584ea), (10, 0xf0d011b1cfd9b4d7), (31, 0x5042718eae962a97), (100, 0x4003f3d635ba9389), (250, 0x0cee5e8b28caa804), (500, 0x81dd0bbc5f6b7d9b), (1000, 0xceb536b27130ae94), (2000, 0xbb6ffb0026c8b299), ],
];

#[cfg(not(windows))]
const GOLDEN: &[&[(u64, u64)]] = &[];

#[test]
fn мир_ведёт_себя_как_при_записи() {
    let mut table = String::new();
    let cases = cases();
    // a new case without recorded fingerprints must not pass silently
    let mut first_mismatch = (!GOLDEN.is_empty() && GOLDEN.len() != cases.len())
        .then(|| format!("отпечатков записано для {} случаев из {}", GOLDEN.len(), cases.len()));
    for (i, case) in cases.iter().enumerate() {
        let (got, w, seen) = run(case);

        // The configuration must touch what it exists for.
        let c = w.counters;
        match i {
            1 => assert!(seen.giant > 100.0, "{}: гиганты выросли ({:.0})", case.name, seen.giant),
            5 => assert!(
                seen.both_strategies >= case.ticks / 2,
                "{}: обе стратегии живут вместе хотя бы полпрогона ({} тиков)",
                case.name,
                seen.both_strategies
            ),
            0 | 2 | 3 | 4 | 6 => assert!(
                !w.creatures.is_empty() && c.plants_eaten > 0 && c.born > 0,
                "{}: жизнь идёт — существа едят и делятся",
                case.name
            ),
            7 => assert!(c.combat > 0, "{}: сородичей едят", case.name),
            _ => {}
        }

        table.push_str(&format!("    // {}\n    &[", case.name));
        for (tick, d) in &got {
            table.push_str(&format!("({tick}, 0x{d:016x}), "));
        }
        table.push_str("],\n");

        if let Some(expected) = GOLDEN.get(i)
            && first_mismatch.is_none()
        {
            first_mismatch = expected
                .iter()
                .zip(&got)
                .find(|(e, g)| e != g)
                .map(|(_, g)| format!("{}: первое расхождение на тике {}", case.name, g.0))
                .or_else(|| {
                    (expected.len() != got.len())
                        .then(|| format!("{}: другое число контрольных тиков", case.name))
                });
        }
    }
    if GOLDEN.is_empty() {
        eprintln!("Отпечатков для этой системы нет. Таблица для вставки:\n{table}");
        return;
    }
    if let Some(m) = first_mismatch {
        panic!("Поведение мира изменилось. {m}.\nЕсли так и задумано — новая таблица:\n{table}");
    }
}

/// A wide check: the fingerprint at the end of a run over 50 seeds of two worlds.
/// Run by hand before and after a refactoring, the outputs are compared:
/// `cargo test -p life-core --release --test golden -- --ignored --nocapture > before.txt`
#[test]
#[ignore]
fn отпечатки_по_сидам() {
    for seed in 1..=50 {
        for (name, r) in [
            ("умолч", Rules::default()),
            ("гиганты", rules(&[("size_power", 1.0)])),
            ("calm", rules(&[("cost_scale", 3.0)])),
        ] {
            let mut w = World::new(&WorldConfig { seed, rules: r, ..Default::default() });
            for _ in 0..1500 {
                w.step();
            }
            println!("{seed:>2} {name:<8} {:016x}", digest(&w));
        }
    }
}
