//! Золотой тест: мир ведёт себя ровно так же, как в момент записи констант.
//!
//! Нужен для реорганизаций без смены поведения (геном таблицей, чувства,
//! стратегии): они не должны сдвинуть ни одного случайного числа и ни одной
//! формулы. Отпечаток мира на контрольных тиках — FNV-1a по битам того, что
//! переживает любой рефакторинг: координаты, энергия, номера, гены и проба
//! генератора каждого существа (лишний или пропущенный розыгрыш меняет пробу
//! сразу, а не через сотню тиков). В отпечаток также входит память поведения,
//! включая временную защиту отделившихся стай.
//!
//! Намеренная смена поведения (новый ген, новая стратегия) ломает тест по
//! определению: тогда константы переписываются отдельным коммитом, вместе с
//! `--save-reference`. Тест печатает готовую таблицу для вставки. Новый случай
//! без записанных отпечатков тоже роняет тест — чтобы не проходил молча.
//!
//! Математика (`ln`, `cos`, `powf`) — из системной библиотеки, и на Linux
//! последний бит может отличаться: константы записаны на Windows и
//! проверяются только там; на других системах тест печатает отпечатки.

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

/// Отпечаток мира. Только публичное и только то, что переживёт рефакторинг;
/// меняются при нём разве что пути доступа к генам — такие правки ревьюятся.
fn digest(w: &World) -> u64 {
    let mut h = Fnv::new();
    h.u64(w.tick);
    h.u64(w.next_flock);
    for ((a, b), until) in w.grace.entries() {
        h.u64(a);
        h.u64(b);
        h.u64(until);
    }
    for byte in format!(
        "{:?}{:?}{:?}{:?}",
        w.split_watches, w.social_counts, w.territory.encounters, w.territory.attacks
    )
    .bytes()
    {
        h.u64(byte as u64);
    }
    let c = w.counters;
    for v in [
        c.plants_grown,
        c.plants_eaten,
        c.plant_bites,
        c.meat_bites,
        c.ranged_shots,
        c.territorial_fights,
        c.born,
        c.starved,
        c.cannibalized,
        c.old_age,
        c.combat,
    ] {
        h.u64(v);
    }
    h.u64(w.plants.len() as u64);
    for p in &w.plants {
        h.f64(p.x);
        h.f64(p.y);
        h.u64(p.alive as u64);
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
        for byte in format!("{:?}", v.mind.social).bytes() {
            h.u64(byte as u64);
        }
        h.u64(v.parent);
        h.u64(v.flock);
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
        h.u64(v.flock_goal.is_some() as u64);
        if let Some(g) = v.flock_goal {
            for n in [g.x, g.y, g.tx, g.ty] {
                h.f64(n);
            }
        }
        h.f64(v.x);
        h.f64(v.y);
        h.f64(v.energy);
        h.u64(v.alive as u64);
        // все гены: новый ген и так сдвигает розыгрыши мутации (кроме инертного
        // гена-выбора с одним вариантом), а стратегия видна в отпечатке сразу
        for g in v.genome.to_values() {
            h.f64(g);
        }
        h.u64(v.rng.clone().next_u64());
    }
    h.u64(w.flocks.len() as u64);
    for (id, f) in &w.flocks {
        for byte in format!("{f:?}").bytes() {
            h.u64(byte as u64);
        }
        h.u64(*id);
        h.u64(f.members as u64);
        h.u64(f.remaining as u64);
        for n in [f.goal.x, f.goal.y, f.goal.tx, f.goal.ty] {
            h.f64(n);
        }
        h.u64(f.rng.clone().next_u64());
    }
    // поток мира и счётчик номеров: подсадка в копии мира
    let mut probe = w.clone();
    let id = probe.spawn(CreatureGenome::BASE, 100.0, 100.0, None);
    h.u64(id);
    h.u64(probe.creature(id).expect("подсаженное существо").rng.clone().next_u64());
    h.0
}

fn rules(pairs: &[(&str, f64)]) -> Rules {
    pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
}

/// Что проверяется в конфигурации: без этого отпечаток мог бы не задеть ветку.
#[derive(Default)]
struct Seen {
    giant: f64,
    /// Тиков, на которых жили оба варианта стратегии существ.
    both_strategies: u64,
}

struct Case {
    name: &'static str,
    cfg: WorldConfig,
    ticks: u64,
    /// Вмешательство перед тиком: правила на ходу, подсадка.
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
            name: "B: сид 4, гиганты",
            cfg: WorldConfig {
                seed: 4,
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
        // Полоса — явно: записан до форм, а по умолчанию теперь 3:2.
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
        // Форма и табличные профили еды: другой путь выборки растений.
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
        // Каннибализм, как в игре по умолчанию. Отношение ниже стандартного,
        // чтобы поедание случалось и в коротком прогоне.
        Case {
            name: "H: сид 8, каннибализм",
            cfg: WorldConfig {
                seed: 8,
                rules: rules(&[("cannibalism", 1.0), ("cannibal_ratio", 1.5)]),
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
    &[(1, 0x9583afe93ad14fd0), (2, 0xe73e4661c625bef9), (10, 0x48276e5ddc0da59d), (31, 0xf5b0fa93d71b2539), (100, 0xeefe9acca9e1b356), (250, 0xfa084d428d26e220), (500, 0x36c3167f0acc5aca), (1000, 0xfa7bfa7d46817401), (2000, 0xa17cd31bc60b639c), (3000, 0x71101ed69b6b7d91), ],
    &[(1, 0xe389c72a633eba4e), (2, 0xc97c97341e7e0a3d), (10, 0x9e0ec67291e9fbac), (31, 0x7e1c16e121cd68d1), (100, 0x81ee2fdf9960f7dc), (250, 0xc10f320c26e82329), (500, 0x7acb60ff320b52bc), (1000, 0xd00c40db5672280e), (2000, 0xe2bcccc9bdcda4c4), ],
    &[(1, 0xa720529d554e2c74), (2, 0x8a325e9ff94e3635), (10, 0x3d346633736fc28a), (31, 0x4773c959148f05ae), (100, 0xc4a7be2d215489ce), (250, 0x14f8f95bd710d3b9), (500, 0xb5d20a6f2ac690a4), (1000, 0x5a0312d2998d6fa2), (2000, 0x32b0016feb7c88a7), (3000, 0x0a44a4ff3fcea0a6), ],
    &[(1, 0x459c8c5a673c6750), (2, 0x3c4c16da5700b2bf), (10, 0xa4946af3ebada69f), (31, 0x4c07893b75ccf947), (100, 0xb83e12497b2bac07), (250, 0x36e4ad7667d6364d), (500, 0x8710bb9f53e642bd), ],
    &[(1, 0x0bf2e69e3268f2d5), (2, 0x89d8b8cf4b086982), (10, 0x486bd92fd6cec277), (31, 0x1274dbfefe6ecb68), (100, 0xb100f749dbd219d5), (250, 0xed0dcc726d997b16), (500, 0x13185ff029e6d2ed), (1000, 0xf48b4f9b105b0af0), ],
    &[(1, 0x968d76cccae9f411), (2, 0xeea0d48d8c329599), (10, 0x9cb0c82878be08cc), (31, 0x0f8682de9e13e1a3), (100, 0x54d6e38b16b4f429), (250, 0x04ff2cc881dfbb23), (500, 0x2acffc3a166458cf), (1000, 0xdd2f3095d6df35ab), (2000, 0x29b68f6cf80c797c), ],
    &[(1, 0x3dd20ff56a046b60), (2, 0xb26219bfdde4d700), (10, 0xa12e401505cdace4), (31, 0x795a2492586c1410), (100, 0x11312565c973d0f2), (250, 0x6adc3e41977ff22e), (500, 0xbdd1f4accce9ea82), (1000, 0x3df75edf78440e9b), ],
    &[(1, 0x4f3fd3d6d679ec3b), (2, 0xc6c0e17df749578d), (10, 0xa280da7870282b24), (31, 0x342299f1c6ca9a69), (100, 0x9eb31f29c434b3f3), (250, 0x7bed64815349ea6b), (500, 0xdd63de7fa6729957), (1000, 0xe1a5e07c23a1bd1a), (2000, 0xb17beb31e7c5f86b), ],
];

#[cfg(not(windows))]
const GOLDEN: &[&[(u64, u64)]] = &[];

#[test]
fn мир_ведёт_себя_как_при_записи() {
    let mut table = String::new();
    let cases = cases();
    // новый случай без записанных отпечатков не должен проходить молча
    let mut first_mismatch = (!GOLDEN.is_empty() && GOLDEN.len() != cases.len())
        .then(|| format!("отпечатков записано для {} случаев из {}", GOLDEN.len(), cases.len()));
    for (i, case) in cases.iter().enumerate() {
        let (got, w, seen) = run(case);

        // Конфигурация должна задевать то, ради чего она есть.
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

/// Широкая проверка: отпечаток в конце прогона по 50 сидам двух миров.
/// Запускается руками до и после рефакторинга, вывод сравнивается:
/// `cargo test -p life-core --release --test golden -- --ignored --nocapture > до.txt`
#[test]
#[ignore]
fn отпечатки_по_сидам() {
    for seed in 1..=50 {
        for (name, r) in [("умолч", Rules::default()), ("гиганты", rules(&[("size_power", 1.0)]))]
        {
            let mut w = World::new(&WorldConfig { seed, rules: r, ..Default::default() });
            for _ in 0..1500 {
                w.step();
            }
            println!("{seed:>2} {name:<8} {:016x}", digest(&w));
        }
    }
}
