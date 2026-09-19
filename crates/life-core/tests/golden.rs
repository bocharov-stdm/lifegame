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
use life_core::genome::predator::Gene as PredGene;
use life_core::genome::vegetarian::Gene as VegGene;
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
    h.u64(w.migrants);
    h.u64(w.hunting as u64);
    let c = w.counters;
    for v in [
        c.plants_grown,
        c.plants_eaten,
        c.vegetarians_born,
        c.vegetarians_eaten,
        c.vegetarians_starved,
        c.predators_born,
        c.predators_starved,
    ] {
        h.u64(v);
    }
    h.u64(w.plants.len() as u64);
    for p in &w.plants {
        h.f64(p.x);
        h.f64(p.y);
        h.u64(p.alive as u64);
        h.u64(p.born as u64);
    }
    h.u64(w.vegetarians.len() as u64);
    for v in &w.vegetarians {
        h.u64(v.id);
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
    h.u64(w.predators.len() as u64);
    for p in &w.predators {
        h.u64(p.id);
        h.f64(p.x);
        h.f64(p.y);
        h.f64(p.energy);
        for g in p.genome.to_values() {
            h.f64(g);
        }
        h.u64(p.rng.clone().next_u64());
    }
    // поток мира и счётчик номеров: подсадка в копии мира
    let mut probe = w.clone();
    let id = probe.spawn_predator(100.0, 100.0, None);
    h.u64(id);
    h.u64(probe.predator(id).expect("подсаженный хищник").rng.clone().next_u64());
    h.0
}

fn rules(pairs: &[(&str, f64)]) -> Rules {
    pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
}

/// Что проверяется в конфигурации: без этого отпечаток мог бы не задеть ветку.
#[derive(Default)]
struct Seen {
    predator_mutated: bool,
    giant: f64,
    /// Тиков, на которых жили оба варианта стратегии: (травоядные, хищники).
    both_strategies: (u64, u64),
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
                rules: rules(&[
                    ("mutation_sigma", 1.0),
                    ("plant_energy", 80.0),
                    ("predator_divide_chance", 0.5),
                    ("predator_max_energy", 30.0),
                    ("predator_migration", 100.0),
                ]),
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
                400 => w.set_rules(rules(&[
                    ("cost_scale", 2.0),
                    ("size_power", 2.0),
                    ("predator_max_energy", 40.0),
                ])),
                600 => {
                    let g = w.vegetarians.first().map(|v| v.genome).expect("травоядные живы");
                    w.spawn_vegetarian(g, 3000.0, 500.0, None);
                    w.spawn_predator(3000.0, 1500.0, Some(50.0));
                }
                _ => {}
            },
        },
        Case {
            name: "F: сид 5, смесь стратегий",
            cfg: WorldConfig {
                seed: 5,
                vegetarian_strategies: vec![1.0, 1.0],
                predator_strategies: vec![1.0, 1.0],
                ..Default::default()
            },
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
    ]
}

fn run(case: &Case) -> (Vec<(u64, u64)>, World, Seen) {
    let mut w = World::new(&case.cfg);
    let mut seen = Seen::default();
    let mut out = Vec::new();
    for _ in 0..case.ticks {
        (case.before)(&mut w);
        w.step();
        seen.predator_mutated |= w.predators.iter().any(|p| p.pheno.speed != case.cfg.predator_speed);
        seen.giant = w.vegetarians.iter().map(|v| v.pheno.size).fold(seen.giant, f64::max);
        let both = |kinds: &mut dyn Iterator<Item = f64>| {
            let mut seen = [false; 2];
            kinds.for_each(|k| seen[(k != 0.0) as usize] = true);
            (seen[0] && seen[1]) as u64
        };
        seen.both_strategies.0 += both(&mut w.vegetarians.iter().map(|v| v.genome[VegGene::Strategy]));
        seen.both_strategies.1 += both(&mut w.predators.iter().map(|p| p.genome[PredGene::Strategy]));
        if CHECKPOINTS.contains(&w.tick) {
            out.push((w.tick, digest(&w)));
        }
    }
    (out, w, seen)
}

#[cfg(windows)]
#[rustfmt::skip]
const GOLDEN: &[&[(u64, u64)]] = &[
    // A: сид 1, по умолчанию
    &[(1, 0x207ec6e8b64c0b5f), (2, 0xb0dad16047f72dcd), (10, 0x9bd2bc3cf961554f), (31, 0xf20fefed4f4bf38f), (100, 0x744922ef1948e828), (250, 0xea3bbfb8ec0419ea), (500, 0x1392ba2b76a2ae73), (1000, 0x9726f62b4bb5a46a), (2000, 0x381a990faed3f578), (3000, 0x50e7b7511907d667), ],
    // B: сид 4, гиганты
    &[(1, 0xb6b61eb038789e1c), (2, 0xb52b07d2f3738e29), (10, 0x38799d9324ec5c1c), (31, 0xd6811b6236033dff), (100, 0x0aea77ec812274fb), (250, 0x26e64fd3e43fb3e2), (500, 0x0f57209a5b7faf30), (1000, 0xeaa37f099b875324), (2000, 0x1f33f8056c743cab), ],
    // C: сид 7, лаборатория
    &[(1, 0xb42190921b6f9b03), (2, 0x21ceb984efe41f8d), (10, 0xcef5176dd9c9a4ba), (31, 0x6109ff6398993b8c), (100, 0x29ce2cc9e3686b47), (250, 0x84acd31793184161), (500, 0x1e8a8ad005a0992b), (1000, 0xd96045d727b487a1), (2000, 0xafd462c773c8ccf6), (3000, 0x5a214bb69d97a793), ],
    // D: сид 2, масштаб 10
    &[(1, 0x2e0244fd56c469e3), (2, 0xe449e7a4c049179c), (10, 0x8adaeaf913554ec9), (31, 0xb6c119fbe6b6eeff), (100, 0xe3b50af0fe5c2595), (250, 0xcd8c8057cdb23ca6), (500, 0x4a848524719b8f82), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0xd23ba569ff2d042f), (2, 0x35bb6c7c7332e2c2), (10, 0xb5e43bd792e50381), (31, 0xb5a41475b6bfc7ce), (100, 0x6c3ca7d6f89d0f33), (250, 0xcf98ecc26510109b), (500, 0xef3637f135abe7b7), (1000, 0x141882a01c8970d3), ],
    // F: сид 5, смесь стратегий
    &[(1, 0xde0bb929593aa08a), (2, 0x64e67ad2587c4e7e), (10, 0xa5349d20ed7e2462), (31, 0x269c0eb7fc8832c0), (100, 0x63d04781559c558b), (250, 0x7910678ec4141c30), (500, 0x36613cd815b48e5c), (1000, 0xf9ef06d8dfee56c4), (2000, 0x0cbb0d77662594e7), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0x21556637b8421fd1), (2, 0x55f42659e998a364), (10, 0x2e91b3ea94f27e27), (31, 0x47bc58e251f6113c), (100, 0x135d88b7b35de4f1), (250, 0xd27f6d9e5f94f649), (500, 0x893b96d102bf7f3d), (1000, 0xd0fc8974c86fe1a4), ],
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
            0 => {
                assert!(
                    c.vegetarians_eaten > 0 && c.predators_born > 0,
                    "{}: охота и деление хищников",
                    case.name
                );
                assert!(seen.predator_mutated, "{}: скорость хищников мутировала", case.name);
            }
            1 => assert!(seen.giant > 100.0, "{}: гиганты выросли ({:.0})", case.name, seen.giant),
            2 => assert!(w.migrants > 0, "{}: мигранты пришли", case.name),
            5 => {
                let (veg, pred) = seen.both_strategies;
                assert!(
                    veg >= case.ticks / 2 && pred >= case.ticks / 2,
                    "{}: обе стратегии живут вместе хотя бы полпрогона (травоядные {veg}, хищники {pred} тиков)",
                    case.name
                );
            }
            6 => assert!(
                !w.vegetarians.is_empty() && !w.predators.is_empty() && c.vegetarians_eaten > 0,
                "{}: жизнь идёт — травоядные едят, хищники охотятся",
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
