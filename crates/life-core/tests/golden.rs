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
use life_core::genome::VegetarianGenome;
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
    let c = w.counters;
    for v in [
        c.plants_grown,
        c.plants_eaten,
        c.vegetarians_born,
        c.vegetarians_starved,
        c.vegetarians_cannibalized,
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
    // поток мира и счётчик номеров: подсадка в копии мира
    let mut probe = w.clone();
    let id = probe.spawn_vegetarian(VegetarianGenome::BASE, 100.0, 100.0, None);
    h.u64(id);
    h.u64(probe.vegetarian(id).expect("подсаженное травоядное").rng.clone().next_u64());
    h.0
}

fn rules(pairs: &[(&str, f64)]) -> Rules {
    pairs.iter().fold(Rules::default(), |r, &(k, v)| r.with(k, v).expect("правило"))
}

/// Что проверяется в конфигурации: без этого отпечаток мог бы не задеть ветку.
#[derive(Default)]
struct Seen {
    giant: f64,
    /// Тиков, на которых жили оба варианта стратегии травоядных.
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
                    let g = w.vegetarians.first().map(|v| v.genome).expect("травоядные живы");
                    w.spawn_vegetarian(g, 3000.0, 500.0, None);
                }
                _ => {}
            },
        },
        Case {
            name: "F: сид 5, смесь стратегий",
            cfg: WorldConfig { seed: 5, vegetarian_strategies: vec![1.0, 1.0], ..Default::default() },
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
        seen.giant = w.vegetarians.iter().map(|v| v.pheno.size).fold(seen.giant, f64::max);
        let both = |kinds: &mut dyn Iterator<Item = f64>| {
            let mut seen = [false; 2];
            kinds.for_each(|k| seen[(k != 0.0) as usize] = true);
            (seen[0] && seen[1]) as u64
        };
        seen.both_strategies += both(&mut w.vegetarians.iter().map(|v| v.genome[VegGene::Strategy]));
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
    &[(1, 0x3b44c17be97da83c), (2, 0x379472af7567f72a), (10, 0x0443743d6da10915), (31, 0xdb9734875812dd71), (100, 0x9a4a567a352e2083), (250, 0xe1b7eb0520828760), (500, 0xb3f2294caab14a7f), (1000, 0x647cf10b5724d920), (2000, 0x51947d2b25b9c6a1), (3000, 0x556d79f2484d6ad4), ],
    // B: сид 4, гиганты
    &[(1, 0xa913efd85ccdd5c4), (2, 0x0b397a7d656c41d1), (10, 0xfd91c35f6e7500be), (31, 0xc35b9810dce9753d), (100, 0x03182f8397d10693), (250, 0xe004caca8c189a44), (500, 0x3a19407c5307e695), (1000, 0xf88ce0feceac6f9f), (2000, 0x52c86f72797fd549), ],
    // C: сид 7, лаборатория
    &[(1, 0x4d6dd5cb1b810180), (2, 0x29ce9c9c3f306d93), (10, 0xe2b2697801db84d6), (31, 0x08e60df497e6d33a), (100, 0x1178090fd5a1ecbd), (250, 0x3de1341425e28bd0), (500, 0x2d355d53eaf29f23), (1000, 0x9dde273052316417), (2000, 0x04c10546352eddfe), (3000, 0xe0a50fa2f5b909f1), ],
    // D: сид 2, масштаб 10
    &[(1, 0xce8336c8cd08e944), (2, 0xf8fd6698caa0e348), (10, 0x4f1b3bf9a8ce891b), (31, 0x9e5d35c05155965a), (100, 0xd9857fe66564ba0b), (250, 0xde6e1e43b886825e), (500, 0xad781c1b90952c92), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0x92e3997a0550a9a3), (2, 0x493e0592c02fc42e), (10, 0xf0b8b9b1b9873bb4), (31, 0x73cbcfd46ff26bf5), (100, 0xa4f075b3a2373a56), (250, 0xf5a912eed6c459fa), (500, 0xfbad887a17f8edf8), (1000, 0xa5248d0fc4dbb7d4), ],
    // F: сид 5, смесь стратегий
    &[(1, 0x89b8030aee40f225), (2, 0x4dc081f9d6c203e6), (10, 0x99e93cb6da032a1b), (31, 0x89c78924c39ee5f0), (100, 0x1dc5d8cf3bcf3c5a), (250, 0x641fcba4bfbe4339), (500, 0x63f69d4ce75dce6f), (1000, 0x7e222e329e2d9f64), (2000, 0xfd4282be274dd1bc), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0xe5516ae536fc6e82), (2, 0x42dbcccd5fee28b0), (10, 0x0bb8f8ba4f4a5bcc), (31, 0x0cae41c19514fa53), (100, 0x304ebc38a4cbe31f), (250, 0x659b2110f9a53671), (500, 0x4711667a866dc148), (1000, 0x54237efd759e01e6), ],
    // H: сид 8, каннибализм
    &[(1, 0x8004cef2da5711f9), (2, 0x804072b0cc049f3a), (10, 0x88f073899c3c6482), (31, 0x8cea17195bcc4d8e), (100, 0xd7868680f3f34215), (250, 0x85319629685cf1c6), (500, 0x5c31700e34588653), (1000, 0x93b23247896784ad), (2000, 0x09ceed3ea083a658), ],
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
                !w.vegetarians.is_empty() && c.plants_eaten > 0 && c.vegetarians_born > 0,
                "{}: жизнь идёт — травоядные едят и делятся",
                case.name
            ),
            7 => assert!(c.vegetarians_cannibalized > 0, "{}: сородичей едят", case.name),
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
