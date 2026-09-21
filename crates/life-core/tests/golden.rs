//! Золотой тест: мир ведёт себя ровно так же, как в момент записи констант.
//!
//! Нужен для реорганизаций без смены поведения (геном таблицей, чувства,
//! стратегии): они не должны сдвинуть ни одного случайного числа и ни одной
//! формулы. Отпечаток мира на контрольных тиках — FNV-1a по битам того, что
//! переживает любой рефакторинг: координаты, энергия, номера, гены и проба
//! генератора каждого существа (лишний или пропущенный розыгрыш меняет пробу
//! сразу, а не через сотню тиков). Производное (расход, цели) не берём: его
//! ошибка всё равно всплывёт в координатах на следующих тиках.
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
    for byte in format!("{:?}{:?}", w.split_watches, w.social_counts).bytes() {
        h.u64(byte as u64);
    }
    let c = w.counters;
    for v in [c.plants_grown, c.plants_eaten, c.born, c.starved, c.cannibalized, c.old_age, c.combat] {
        h.u64(v);
    }
    h.u64(w.plants.len() as u64);
    for p in &w.plants {
        h.f64(p.x);
        h.f64(p.y);
        h.u64(p.alive as u64);
        h.u64(p.born as u64);
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
            cfg: WorldConfig { seed: 4, rules: rules(&[("size_power", 1.0)]), ..Default::default() },
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
    &[(1, 0x320e6a9bc8ef4e7e), (2, 0xfdd3126113f19142), (10, 0x1993f210be7a5bf2), (31, 0x07c27b341b890886), (100, 0xacc3f89389c779f7), (250, 0xfa69adc88d38bddf), (500, 0x2e35093ce2412c2a), (1000, 0x79c7f10cad9947bc), (2000, 0x51cb32f585724f5c), (3000, 0xc5e9a6c114da6632), ],
    &[(1, 0x5efb722c30434fa2), (2, 0xf3d1feddff6cb3e8), (10, 0x8bb79cb008e8da91), (31, 0x38a848de2e64590d), (100, 0xdf17b659f1b8158a), (250, 0xf72d1d967be370be), (500, 0xe2d9749ede8c8adf), (1000, 0x88250fdb029f9176), (2000, 0x64e59995157a29a7), ],
    &[(1, 0xf4adbcaa32abeced), (2, 0x994ad21bbb024e68), (10, 0xabec7e82680e55ca), (31, 0xb20b06c9b1b1c4b3), (100, 0xe5e17fe3499c05db), (250, 0xc9efd5e81b925e7a), (500, 0x2c5ecde2f42180df), (1000, 0x8d003da81749f439), (2000, 0xeda0efd30bf18596), (3000, 0xe69c4e7e6240a666), ],
    &[(1, 0x3758396b8d55b22a), (2, 0x46b71421dc4d024c), (10, 0x4fbb025c50f2de72), (31, 0x059bf17607241a75), (100, 0x1c4db22e1152cd97), (250, 0x1bd1b6dc5148dd09), (500, 0x8b4e04fc0f6f34f6), ],
    &[(1, 0xc227d2d1f605608d), (2, 0x7adc7007d150e33f), (10, 0x77f5ad8b88c00cbf), (31, 0x8a522a70fec690b4), (100, 0x5523eb70a1d189b3), (250, 0xc5a928392f3212c5), (500, 0x980b8989101f5dc4), (1000, 0x35d3b88411afca74), ],
    &[(1, 0x82e1e93117590419), (2, 0x05828c22e762df75), (10, 0x6ec94e73eae69ff9), (31, 0x56b17941339b3e55), (100, 0xd422e34745e615ab), (250, 0x7a9a27c16100cc4b), (500, 0x0a0004b9a5568351), (1000, 0x67affed9fe5946ac), (2000, 0x62f97c87918ec230), ],
    &[(1, 0x6b0377ae6e681f43), (2, 0xc8ec1ed1c99fff2c), (10, 0xa506685705005cbc), (31, 0x840701d39c4cbe2c), (100, 0xd4db20701f5fb55a), (250, 0x0b73e65f55fd0764), (500, 0x7e6e4c8b3ed00d2c), (1000, 0x08f3b45502ed13cd), ],
    &[(1, 0x222a7c06d9caadb7), (2, 0x5af52f89d9905c90), (10, 0x2f06452be9156121), (31, 0x4198fe86250bfe50), (100, 0x4439296bfab2ab91), (250, 0xb1e2907bcf0cf7f6), (500, 0x73350457e4f4f8c7), (1000, 0xa48095b9a0b32aab), (2000, 0xd42178291141b111), ],
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
