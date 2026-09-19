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
    &[(1, 0xb531d57a49a8647b), (2, 0xe1e5f91e6bb26fc5), (10, 0x4e856ca8ddfec74b), (31, 0x2a9cd1eb30ccef6e), (100, 0xbdb0431fa963ce5e), (250, 0x4f19db7db51cbc92), (500, 0x8f9a1d8487c4f322), (1000, 0x649cfffcedfa8758), (2000, 0xcfb89223a9e82bbb), (3000, 0x1d103e0f7913b436), ],
    // B: сид 4, гиганты
    &[(1, 0x88936696b40731fc), (2, 0x7df80ab1bddd27cd), (10, 0x9ae698f06c8366f0), (31, 0x9953843e41c38b77), (100, 0xa5b0dde06597b74a), (250, 0xf1e5d94615f91cfa), (500, 0x7b6c1b68bfd66127), (1000, 0x9583ed9f6c6c8535), (2000, 0xdae667be42d7477c), ],
    // C: сид 7, лаборатория
    &[(1, 0x18e785976a24318b), (2, 0x86619dd67e7d4075), (10, 0x6785aa79d8a10d87), (31, 0x351d5685dba1d20a), (100, 0xdc56cfda540cefb4), (250, 0x5e0b5f2c8f425ad9), (500, 0x034e8d1aaa11b375), (1000, 0xdba8305612af7f75), (2000, 0x21e3112a92a37e7e), (3000, 0x1554cd1ee1ab972b), ],
    // D: сид 2, масштаб 10
    &[(1, 0x48e21c3735a5e2ae), (2, 0x5f4f62b22f9736b5), (10, 0x3833bf99e8a86b6c), (31, 0xa13fa942fddeae51), (100, 0x274a152f41c3aab8), (250, 0x648f906d4e7757ab), (500, 0xce837febeaa5fa6d), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0x316ae041f66e9497), (2, 0xcde320a742b5ee26), (10, 0x2a34c6e3462bf435), (31, 0xb792326ea3542dfb), (100, 0x7afa623114d83a5c), (250, 0x0406007e3cc76bad), (500, 0x3186ce3e980c5116), (1000, 0xed20f5edc4897dd1), ],
    // F: сид 5, смесь стратегий
    &[(1, 0xad3cc43fe3d33d77), (2, 0xa5bb3b7ee9c55b33), (10, 0x13a980e8230df73f), (31, 0x9a2e321eabf395c6), (100, 0xf291a3c5567ea00b), (250, 0x3935fac9e2db78d9), (500, 0xc90a553d76af1598), (1000, 0xc8e704856647930e), (2000, 0x7a52686f9fa7da29), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0x82f4dfbdcc7ae9e9), (2, 0xe2a1085dd9b13250), (10, 0x35250327912b46de), (31, 0x7db6b09749b88043), (100, 0x3151615edb9f73bb), (250, 0x7cf23452346a0165), (500, 0x2479cbb9173c7541), (1000, 0xca0da278866df1ad), ],
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
