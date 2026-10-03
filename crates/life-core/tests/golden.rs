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
            name: "I: seed 3, the game's world x3, every diet",
            cfg: WorldConfig {
                seed: 3,
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
    &[(1, 0x67ca5675822e596d), (2, 0x381ea7179718eb05), (10, 0xc7a5c31a0f499480), (31, 0xee5864599b7dce10), (100, 0xf2adb1e5e35ae5f0), (250, 0x0e7f3ffcc6895b6a), (500, 0xe2ba9f50ca0627b7), (1000, 0x31378b6250b5e9f6), (2000, 0x68608279da5e8371), (3000, 0x0700756ada50d87b), ],
    // B: seed 3, giants
    &[(1, 0x9d8bca28d62cbb1d), (2, 0x4c1ec5562e4d1fbb), (10, 0xb8ec1bfa3e74de38), (31, 0xa9115bd4be6abda7), (100, 0x04e54955238de2d2), (250, 0xaf4ed3a01ad54eb2), (500, 0x6bfe5f2eb74ef518), (1000, 0x05236171fc6c0f26), (2000, 0x3fe5294132dab5d6), ],
    // C: seed 7, lab
    &[(1, 0x3a27fbf0f4e97685), (2, 0xac2ad28e839b6420), (10, 0x51b35d06183a600f), (31, 0x7fd08f933ed34c89), (100, 0x75e8544106cc3eb6), (250, 0xbdd6067ecf62f1a4), (500, 0x999cacc9ed511f67), (1000, 0x396cb90f6eb46930), (2000, 0x1ded8f48f3f34dca), (3000, 0x77058f78f9c844db), ],
    // D: seed 2, scale 10
    &[(1, 0x75f8a6b3de4ea47c), (2, 0x36a210e366710f96), (10, 0xedc0f54f2cb5478e), (31, 0x5338d03956172384), (100, 0x0f7ceb06a427c985), (250, 0x6ce3a173a3bb88ad), (500, 0xbf7346cad88b52bd), ],
    // E: seed 3, rules on the fly and a probe
    &[(1, 0x9d8bca28d62cbb1d), (2, 0x4c1ec5562e4d1fbb), (10, 0xc4ed9802a59d35c9), (31, 0x285912a7a920be05), (100, 0x50ee3d36731af2ff), (250, 0x7b697a22c95d0cf8), (500, 0xf2226ded89e48c24), (1000, 0xcb79a10721b9f159), ],
    // F: seed 5, a mix of strategies
    &[(1, 0xe6105bc6f4d6a514), (2, 0x4ed6051532f91934), (10, 0x2c808f43bd34e6cb), (31, 0xda597cf78ff5ba22), (100, 0x6e8ce6dd88123ddf), (250, 0x0e412ad0f1c9de93), (500, 0xb4fc1d58e6e25782), (1000, 0x94dd76c3cff4aa14), (2000, 0x6ec9be027e749e8e), ],
    // G: seed 6, a x10 square, food linear and in waves
    &[(1, 0x7986963b47a36677), (2, 0x031953ce90cd4ff7), (10, 0x0f3cb9fda63920c1), (31, 0xece2922a06c4e8e2), (100, 0x3868ce2c1eec5546), (250, 0x4e33e2361c43f0d9), (500, 0x4d2c93c61bada95f), (1000, 0x45ddf2a87960fdc6), ],
    // H: seed 8, fights
    &[(1, 0x9fe01a0a7de0716f), (2, 0x644568b0fd152929), (10, 0xfcf92d4234203200), (31, 0xdd8c72de45750236), (100, 0x23d4cea355027d7a), (250, 0xfa85ac8e98eab754), (500, 0xab985fd72a399f32), (1000, 0x92d0589d127b789d), (2000, 0x7f4c6d0b0a71aecd), ],
    // I: seed 3, the game's world x3, every diet
    &[(1, 0xdcc384472ce2b7ce), (2, 0xbfc8c14dd75ca809), (10, 0x553a99fb6029caea), (31, 0x92485f5281d1f6c1), (100, 0x7f09899423926793), (250, 0x00e0a665bce43f75), (500, 0x2a22a13b55954993), (1000, 0x174943423bab2b5a), (2000, 0xea7e86a24e19e846), (3000, 0xd2f9b65ed4f80937), ],
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
            8 => assert!(
                !w.creatures.is_empty()
                    && c.combat > 0
                    && c.ranged_shots > 0
                    && c.meat_bites > c.rot_bites
                    && c.rot_bites > 0
                    && c.bone_bites > 0,
                "{}: the whole food chain lives — hunts, shots, fresh meat, rot and bones: {c:?}",
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
