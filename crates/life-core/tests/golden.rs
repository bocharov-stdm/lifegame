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
        h.u64(v.circle.is_some() as u64);
        if let Some(c) = v.circle {
            for n in [c.x, c.y, c.radius] {
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
        if let Some(c) = f.circle {
            for n in [c.x, c.y, c.radius, f.target.0, f.target.1] {
                h.f64(n);
            }
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
    // A: сид 1, по умолчанию
    &[(1, 0xdfcfa0e655f829ff), (2, 0x5babd0ff32f0a185), (10, 0xf32e1cb1470a1a45), (31, 0x49e048c04a760c4b), (100, 0xdc44304b4cbfffcf), (250, 0x3bba235bcc36f11a), (500, 0xbbdc7c82e8d6a1fc), (1000, 0x314f641c7cf0af2f), (2000, 0xc69c405b5943984c), (3000, 0x63320332be1d1856), ],
    // B: сид 3, гиганты
    &[(1, 0x68152cc55e73e53a), (2, 0xe502cceeadd10c8f), (10, 0x6beaf262826a360b), (31, 0x943a1d1755a27b28), (100, 0x030ec390d98fe77e), (250, 0x7bbf852bbd2fa344), (500, 0x28c6634a5d5ff341), (1000, 0x2002d13a81ebac58), (2000, 0x744a5073f3f48eaa), ],
    // C: сид 7, лаборатория
    &[(1, 0x24484f087e2286cf), (2, 0x0805b9e015bab99e), (10, 0x3e2fef2e92469e67), (31, 0x5e7c11ebb9dde05f), (100, 0x6e4a1ea87bb9c5c4), (250, 0x09a0f6314af1f117), (500, 0xef9d9d30fe025e67), (1000, 0x5572e78003d6e994), (2000, 0x986f215ff05b5591), (3000, 0x34d5de7e588f8ea9), ],
    // D: сид 2, масштаб 10
    &[(1, 0xa809f2ad5a74e76d), (2, 0xe17bf0569266d86c), (10, 0x206f38f6f0c166b8), (31, 0x0ce728b342ed60c6), (100, 0xc068d1c4011d5ef3), (250, 0xc4cee27589e9bf66), (500, 0x0aff169a744a119f), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0x68152cc55e73e53a), (2, 0xe502cceeadd10c8f), (10, 0x6beaf262826a360b), (31, 0x715c4dd517128c9b), (100, 0x3da974fc44f8c75e), (250, 0x0e3f5228cc361734), (500, 0x8257e0de9e9d4f8f), (1000, 0x5ae32fc69437b106), ],
    // F: сид 5, смесь стратегий
    &[(1, 0xded39bdab65fb9ce), (2, 0x305ddc760efbf188), (10, 0x134df36dbcce2267), (31, 0xcc057c1af7637bba), (100, 0xb244f4370451f99d), (250, 0x95e490f76ee7aea9), (500, 0xcb84582fea5a0f94), (1000, 0xc4e6781a4f17e590), (2000, 0xc1c06f65e98e5745), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0x65444fbaa2b1777e), (2, 0xf6d80b9ec995f42b), (10, 0x106d9a19a0c2922e), (31, 0xce01403cbac0883a), (100, 0xde09ef11d6e9ad16), (250, 0x6722251c57bb804d), (500, 0x8ca4844ee97df402), (1000, 0xf4a0f24fb8ce0c86), ],
    // H: сид 8, бои
    &[(1, 0x19a43174d0008123), (2, 0xf9d0fa268eaf7b26), (10, 0x22f5f295e5c8e221), (31, 0x9e5ba28e0b8bc0ac), (100, 0x1695254aee114714), (250, 0xbbb1cebe8e76a3d8), (500, 0xa9dac0371da96d05), (1000, 0x8762fc4477d1b7db), (2000, 0x1bfb38a0cc70c3da), ],
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
