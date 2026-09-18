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
//! `--save-reference`. Тест печатает готовую таблицу для вставки.
//!
//! Математика (`ln`, `cos`, `powf`) — из системной библиотеки, и на Linux
//! последний бит может отличаться: константы записаны на Windows и
//! проверяются только там; на других системах тест печатает отпечатки.

use life_core::{Rules, World, WorldConfig};

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
        // замороженный список: первые семь генов в исходном порядке
        for g in v.genome.to_values().iter().take(7) {
            h.f64(*g);
        }
        h.u64(v.rng.clone().next_u64());
    }
    h.u64(w.predators.len() as u64);
    for p in &w.predators {
        h.u64(p.id);
        h.f64(p.x);
        h.f64(p.y);
        h.f64(p.energy);
        h.f64(p.pheno.speed);
        h.f64(p.pheno.vision);
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
        Case {
            name: "D: сид 2, масштаб 10",
            cfg: WorldConfig { seed: 2, scale: 10.0, ..Default::default() },
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
    &[(1, 0x2dd43d3131fc019b), (2, 0x5f017966591ea525), (10, 0x62e313ed157f344b), (31, 0xccacfbb567ae00bc), (100, 0x3361547e5769a694), (250, 0xbb093550829eb020), (500, 0xbfae313f2fe31ba5), (1000, 0xad6a1b16ed8e5236), (2000, 0x98b37a76f1083ca7), (3000, 0xde86f9750798496f), ],
    // B: сид 4, гиганты
    &[(1, 0x41682d50ebd2741c), (2, 0xcbde8d8d7be4c9ed), (10, 0xd8e21eb12bc92bb0), (31, 0x67f54839009008ad), (100, 0x3e6404f1cc7b1e28), (250, 0x8ad498672b6ec68e), (500, 0x6bf31c11a58ad10b), (1000, 0x69d697b4934fcf52), (2000, 0x36b3aa727b24bdb9), ],
    // C: сид 7, лаборатория
    &[(1, 0xf57c0501ce0ea674), (2, 0x6c6fc8daf370d677), (10, 0x46bf5e408632948d), (31, 0x6218497f953d56da), (100, 0xd9f441afe3404e33), (250, 0x1b69b9c36c864f38), (500, 0xbd817d28270737b8), (1000, 0x38dc2576f911f7f8), (2000, 0xb4dd5e5270d959e7), (3000, 0x5d91e51d52854fab), ],
    // D: сид 2, масштаб 10
    &[(1, 0x72a1250ad5546537), (2, 0x886f5dca6f5cefb5), (10, 0x9dd19b3fa4258856), (31, 0xc49b8b3ec6af6d35), (100, 0xc208feaf60c8042c), (250, 0x8e049e2d7cd324a6), (500, 0x25f6a8ba30b5c99f), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0x38c5c726e73f58b7), (2, 0xddf4c545b8a5b8c6), (10, 0x17ceae07044cbbb5), (31, 0x3cddc0cdd019fd22), (100, 0x478c0402be5ac00c), (250, 0x3aab06ab2d5d25d5), (500, 0x5125617ed218f36d), (1000, 0x647f87b67a6c3bfd), ],
];

#[cfg(not(windows))]
const GOLDEN: &[&[(u64, u64)]] = &[];

#[test]
fn мир_ведёт_себя_как_при_записи() {
    let mut table = String::new();
    let mut first_mismatch = None;
    for (i, case) in cases().iter().enumerate() {
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
