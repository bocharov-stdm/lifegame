//! The golden test: the world behaves exactly as it did when the constants were recorded.
//!
//! Needed for reorganisations without a change of behaviour (the genome as a table, the senses,
//! the strategies): they must not shift a single random number or a single formula. The world's
//! fingerprint at the checkpoint ticks is FNV-1a over the bits of what survives any refactoring:
//! coordinates, energy, numbers, genes, the programs, a probe of each creature's generator (an
//! extra or a missing draw changes the probe at once, not a hundred ticks later), and the
//! behaviour's memory, the corpses and the shots as their debug prints without the names of their
//! types and fields (`debug`): renaming a type or a field changes no behaviour, so it keeps the
//! fingerprint; reordering fields or renaming a unit variant of an enum does not.
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
use life_core::{Rules, Shape, Terrain, World, WorldConfig};

mod chain;

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

    /// A value's debug print less the names of its types and fields (a word followed by `:` or `{`):
    /// every number, flag and enum variant, in order.
    fn debug(&mut self, v: &impl std::fmt::Debug) {
        let text = format!("{v:?}");
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                self.u64(u64::from(bytes[i]));
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let next = text[i..].trim_start();
            let name = next.starts_with('{') || (next.starts_with(':') && !next.starts_with("::"));
            if !name {
                bytes[start..i].iter().for_each(|&b| self.u64(u64::from(b)));
            }
        }
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
        h.debug(c);
    }
    h.u64(w.shots.len() as u64);
    for shot in &w.shots {
        h.debug(shot);
    }
    h.u64(w.creatures.len() as u64);
    for v in &w.creatures {
        h.u64(v.id);
        h.debug(&v.mind);
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
    h.u64(probe.creature(id).expect("the probe is in the world").rng.clone().next_u64());
    h.0
}

fn rules(pairs: &[(&str, f64)]) -> Rules {
    pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("a rule"))
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
            name: "A: seed 1, default",
            cfg: WorldConfig { seed: 1, ..Default::default() },
            ticks: 3000,
            before: |_| {},
        },
        Case {
            name: "B: seed 3, giants",
            cfg: WorldConfig {
                seed: 3,
                rules: rules(&[("size_power", 1.0), ("plant_energy", 120.0)]),
                ..Default::default()
            },
            ticks: 2000,
            before: |_| {},
        },
        Case {
            name: "C: seed 7, lab",
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
            name: "D: seed 2, scale 10",
            cfg: WorldConfig { seed: 2, scale: 10.0, shape: Shape::Strip, ..Default::default() },
            ticks: 500,
            before: |_| {},
        },
        Case {
            name: "E: seed 3, rules on the fly and a probe",
            cfg: WorldConfig { seed: 3, ..Default::default() },
            ticks: 1000,
            before: |w| match w.tick {
                400 => w.set_rules(rules(&[("cost_scale", 2.0), ("size_power", 2.0)])),
                600 => {
                    let g = w.creatures.first().map(|v| v.genome).expect("creatures alive");
                    w.spawn(g, 3000.0, 500.0, None);
                }
                _ => {}
            },
        },
        Case {
            name: "F: seed 5, a mix of strategies",
            cfg: WorldConfig { seed: 5, strategies: vec![1.0, 1.0], ..Default::default() },
            ticks: 2000,
            before: |_| {},
        },
        // The shape and the tabular food profiles: another path of plant sampling.
        Case {
            name: "G: seed 6, a x10 square, food linear and in waves",
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
            name: "H: seed 8, fights",
            cfg: WorldConfig { seed: 8, ..Default::default() },
            ticks: 2000,
            before: |_| {},
        },
        // The player's world in small (`CLAUDE.md`, baseline conditions): the game's prices and
        // food, 2:1, half lurkers and every diet among the founders — the hunters, the scavengers,
        // rot and bones, which the default world meets only through rare mutants.
        Case {
            name: "I: seed 8, the game's world x3, every diet",
            cfg: WorldConfig {
                seed: 8,
                scale: 3.0,
                shape: Shape::R2x1,
                rules: rules(&[
                    ("cost_scale", 2.0),
                    ("speed_cost", 0.5),
                    ("plant_rate", 0.5),
                    ("plant_depth_steepness", 5.0),
                ]),
                strategies: vec![1.0, 1.0],
                diets: vec![55.0, 25.0, 10.0, 10.0],
                ..Default::default()
            },
            ticks: 3000,
            before: |_| {},
        },
        // The same world on land: the dead lie where they fell.
        Case {
            name: "J: seed 4, the game's world x3 on land",
            cfg: WorldConfig {
                seed: 4,
                scale: 3.0,
                shape: Shape::R2x1,
                terrain: Terrain::Land,
                rules: rules(&[
                    ("cost_scale", 2.0),
                    ("speed_cost", 0.5),
                    ("plant_rate", 0.5),
                    ("plant_depth_steepness", 5.0),
                ]),
                strategies: vec![1.0, 1.0],
                diets: vec![55.0, 25.0, 10.0, 10.0],
                ..Default::default()
            },
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
    &[(1, 0x37bf6ccfdb46f7b8), (2, 0x9fa77ba26e6628da), (10, 0x28798b85b5c4a617), (31, 0xd9ba88010d81751c), (100, 0xdd518517606bb2b0), (250, 0x3da4a9c234827423), (500, 0xb24e3c3f44171702), (1000, 0xe030f84fe0e5e86c), (2000, 0x5c724b9aa13e64ad), (3000, 0x0e8071407d4a56a5), ],
    // B: seed 3, giants
    &[(1, 0x91bc30e4b3aba68c), (2, 0xdf1ffd0beb740fda), (10, 0x7f1e22b1d572104d), (31, 0x154a2ddb4df8fd71), (100, 0x6879d5b694d0a35a), (250, 0x773929c214812725), (500, 0x99f61b05ef1cd64c), (1000, 0x0d90194ff10cb934), (2000, 0x5c58e747d6ae716d), ],
    // C: seed 7, lab
    &[(1, 0x85d8d55d6ba9976b), (2, 0x65d649b8f5c9b0e6), (10, 0x68e010cc7d438c6f), (31, 0xc4e842e48c26dbb7), (100, 0x47a706f85bc18dc3), (250, 0x9436e5ff859a5a34), (500, 0xf65a7ff09a0b43a7), (1000, 0xc9b099a9147660b9), (2000, 0x21a98a1a679da9f2), (3000, 0x04626505535b6b6b), ],
    // D: seed 2, scale 10
    &[(1, 0xe6c605854b6df25c), (2, 0x149542d6e69786c2), (10, 0x94e0acfc65da6240), (31, 0x2ab5b2009652dfa7), (100, 0xdcf1c89e2423a0ad), (250, 0x96f602b883c6cba3), (500, 0x914bd5977b26cccc), ],
    // E: seed 3, rules on the fly and a probe
    &[(1, 0x91bc30e4b3aba68c), (2, 0xdf1ffd0beb740fda), (10, 0xe645277be6f5eee0), (31, 0xa6ff11b5cf2985c1), (100, 0x06d3a09dd25a6e31), (250, 0x499a60a4805c02ab), (500, 0x8082836e98ceca41), (1000, 0xaafd8a474d44540c), ],
    // F: seed 5, a mix of strategies
    &[(1, 0x97e2db0c1dda6536), (2, 0x6a7ea7e0db431a18), (10, 0xff267befdcec2127), (31, 0x56ce033d34ed119e), (100, 0xab11906b806ef65c), (250, 0xbd68fcf93ca3e39a), (500, 0x7e1da75d172633a5), (1000, 0x86fbafe209f76779), (2000, 0x280726bfc9ad7be4), ],
    // G: seed 6, a x10 square, food linear and in waves
    &[(1, 0xc93fc6d9afd396f4), (2, 0x4c42429220a31865), (10, 0xe397e64a1edd007a), (31, 0x8da9ecb9bac5a0f0), (100, 0x2be51e8270cf3c69), (250, 0xe73bdb85f0c134a8), (500, 0xb5fe5592d87cbc09), (1000, 0x239cc99370dd6a05), ],
    // H: seed 8, fights
    &[(1, 0xa32665a81bf8ba49), (2, 0xd197e17b1fc8eb6f), (10, 0x0dc913569b14a7d6), (31, 0x152cb75f1df293a1), (100, 0x6d99710a67132ffe), (250, 0x24a503a2e6619762), (500, 0x450c704413b8f29f), (1000, 0x17fe40e09b67290d), (2000, 0x425e926210f92ae7), ],
    // I: seed 8, the game's world x3, every diet
    &[(1, 0x0c7e2f055381e24a), (2, 0xca068219502d9a20), (10, 0xe0ad4a488d35d4ad), (31, 0x99dc44d9ecc7e4db), (100, 0xeb842f6727b94252), (250, 0xf6c18c4b448b517f), (500, 0x345cf989e29a11a2), (1000, 0x62a4e58b0f6febe1), (2000, 0xb034910aba6b3858), (3000, 0x542a7618411a5d59), ],
    // J: seed 4, the game's world x3 on land
    &[(1, 0xe03e441a1153cbb8), (2, 0x0bcff4c6636b7372), (10, 0x2b8c23882342a2f7), (31, 0x4d47010fc565a55b), (100, 0x471efea4df75e658), (250, 0xfcff5ce46647163a), (500, 0xad84d708c29facb3), (1000, 0x1b1a3d30b9e1d4f8), (2000, 0x3a6b658949514089), ],
];

#[cfg(not(windows))]
const GOLDEN: &[&[(u64, u64)]] = &[];

#[test]
fn мир_ведёт_себя_как_при_записи() {
    let mut table = String::new();
    let cases = cases();
    // a new case without recorded fingerprints must not pass silently
    let mut first_mismatch = (!GOLDEN.is_empty() && GOLDEN.len() != cases.len())
        .then(|| format!("fingerprints recorded for {} cases of {}", GOLDEN.len(), cases.len()));
    for (i, case) in cases.iter().enumerate() {
        let (got, w, seen) = run(case);

        // The configuration must touch what it exists for.
        let c = w.counters;
        match i {
            1 => assert!(seen.giant > 100.0, "{}: giants grew ({:.0})", case.name, seen.giant),
            5 => assert!(
                seen.both_strategies >= case.ticks / 2,
                "{}: both strategies live together at least half the run ({} ticks)",
                case.name,
                seen.both_strategies
            ),
            0 | 2 | 3 | 4 | 6 => assert!(
                !w.creatures.is_empty() && c.plants_eaten > 0 && c.born > 0,
                "{}: life goes on — creatures eat and divide",
                case.name
            ),
            7 => assert!(c.combat > 0, "{}: creatures die in fights", case.name),
            // the same world and seed as the energy test, the same predicate (`the_chain_by_seed`)
            8 => assert!(
                chain::whole_chain(&w),
                "{}: the whole food chain lives — hunts, shots, fresh meat, rot, bones, division: {c:?}",
                case.name
            ),
            9 => assert!(
                c.corpses > 0 && w.corpses.iter().all(|corpse| corpse.y == corpse.y0),
                "{}: life goes on and the dead lie where they fell",
                case.name
            ),
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
                .map(|(_, g)| format!("{}: the first difference at tick {}", case.name, g.0))
                .or_else(|| {
                    (expected.len() != got.len())
                        .then(|| format!("{}: another number of checkpoints", case.name))
                });
        }
    }
    // the Re-record workflow (`.github/workflows/rerecord.yml`): the table goes into this file
    // instead of being compared — a deliberate change of behaviour, recorded in a commit of its own
    if std::env::var_os("GOLDEN_RECORD").is_some() {
        return record(&table);
    }
    if GOLDEN.is_empty() {
        eprintln!("No fingerprints for this system. The table to paste:\n{table}");
        return;
    }
    if let Some(m) = first_mismatch {
        panic!("The world's behaviour changed. {m}.\nIf that is meant, the new table:\n{table}");
    }
}

/// Writes `table` over the recorded one in this file (`GOLDEN_RECORD`). Only on Windows, where the
/// fingerprints are checked: another system's maths may differ in the last bit.
fn record(table: &str) {
    if !cfg!(windows) {
        panic!("golden is recorded on Windows only");
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.rs");
    let source = std::fs::read_to_string(path).expect("golden.rs is read");
    // a Windows checkout may hold the file with CRLF
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let head = ["#[cfg(windows)]", "#[rustfmt::skip]", "const GOLDEN: &[&[(u64, u64)]] = &[", ""].join(nl);
    let start = source.find(&head).expect("the recorded table") + head.len();
    let end = start + source[start..].find(&format!("];{nl}")).expect("the end of the recorded table");
    let recorded = format!("{}{}{}", &source[..start], table.replace('\n', nl), &source[end..]);
    std::fs::write(path, recorded).expect("golden.rs is written");
    eprintln!("Recorded the table:\n{table}");
}

/// The names in a debug print are no part of the fingerprint: a renamed field or type keeps it,
/// another number, flag or variant does not.
#[test]
fn a_renamed_field_or_type_keeps_the_fingerprint() {
    #[derive(Debug)]
    enum Kind {
        Rot,
        Bones,
    }
    #[allow(dead_code)]
    #[derive(Debug)]
    struct Old {
        flee_ticks: u32,
        target: Option<(f64, f64)>,
        kind: Kind,
    }
    #[allow(dead_code)]
    #[derive(Debug)]
    struct Renamed {
        flee_for: u32,
        goal: Option<(f64, f64)>,
        kind: Kind,
    }
    fn of(v: &impl std::fmt::Debug) -> u64 {
        let mut h = Fnv::new();
        h.debug(v);
        h.0
    }
    let old = of(&Old { flee_ticks: 3, target: Some((1.5, -2e-7)), kind: Kind::Rot });
    assert_eq!(old, of(&Renamed { flee_for: 3, goal: Some((1.5, -2e-7)), kind: Kind::Rot }));
    assert_ne!(old, of(&Old { flee_ticks: 3, target: Some((1.5, -2e-7)), kind: Kind::Bones }));
    assert_ne!(old, of(&Old { flee_ticks: 3, target: None, kind: Kind::Rot }));
    assert_ne!(old, of(&Old { flee_ticks: 30, target: Some((1.5, -2e-7)), kind: Kind::Rot }));
    assert_ne!(old, of(&Old { flee_ticks: 3, target: Some((1.5, 2e-7)), kind: Kind::Rot }));
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
