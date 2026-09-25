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
        // Combat, as in the game by default.
        Case {
            name: "H: сид 8, каннибализм",
            cfg: WorldConfig { seed: 8, rules: rules(&[("cannibalism", 1.0)]), ..Default::default() },
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
    &[(1, 0x6a44268cc40e7957), (2, 0xde61030706504c2c), (10, 0x805a5935600d54dd), (31, 0xbbc7fc083e40c913), (100, 0x940ee78e2b208d6e), (250, 0x7c5a59de4bfacd88), (500, 0xa82fdf05003edef3), (1000, 0x57b134f46bf79e0f), (2000, 0x3e9fb58db6efa5ff), (3000, 0x38863beb7074af26), ],
    // B: сид 4, гиганты
    &[(1, 0x843b6fbd160bafc8), (2, 0xe520e60216a5526b), (10, 0x1d7996b8c2a99348), (31, 0x2594e68a7c8410af), (100, 0x8d3b06c8f890b6f7), (250, 0xb8401f24c8051b29), (500, 0xec21ef9a278f30ee), (1000, 0xa16a3950896f5693), (2000, 0x5b7c35942fab4fdd), ],
    // C: сид 7, лаборатория
    &[(1, 0xb8a99c80a06c03f2), (2, 0x98a0331e01e1c1f7), (10, 0xd169d6fc46fe9eaa), (31, 0xa974a6e7edb8c28e), (100, 0x6b81e0bd69f1277b), (250, 0x2707e5ed766bed7d), (500, 0xfd7479df4928ac21), (1000, 0xccb2876636019ffc), (2000, 0x4594442741830857), (3000, 0x1933cd1e813ecb3c), ],
    // D: сид 2, масштаб 10
    &[(1, 0xa70ea00e91281b04), (2, 0x0f3647084bf20d25), (10, 0x412931f24cd98b1b), (31, 0x449acbf8a9a301d7), (100, 0x1eed87f19b477d7f), (250, 0xd29a76b86339b220), (500, 0x16b69648c079731b), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0xc1e145c08f7ad4a3), (2, 0xbf546af80aca701b), (10, 0x84e8c6f53a201d72), (31, 0x3165daf040e4debb), (100, 0x2113d0f807364fd6), (250, 0x25103d4752e44ac9), (500, 0x7272538152f11b3f), (1000, 0x14077938857557a3), ],
    // F: сид 5, смесь стратегий
    &[(1, 0x1a2b4fb529fcacea), (2, 0x881fbbcf6be5d624), (10, 0xdff9526f3abdd866), (31, 0xe1b978b43ae8da67), (100, 0xcffb6033b4577926), (250, 0x7d2ee858442ae955), (500, 0x5fccb609f78cd8a5), (1000, 0xb1de61e73be73efc), (2000, 0xed074a0516b9ff1f), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0x3a10ffd149a45476), (2, 0x7d227a9865ab11aa), (10, 0x279a9b5412accbba), (31, 0x5e0f5a270d5ab556), (100, 0x055ffb5ddfa843ef), (250, 0xd69a75ecce01a3ad), (500, 0x449144cdf5ecd64a), (1000, 0x634b2a444c7489b2), ],
    // H: сид 8, каннибализм
    &[(1, 0x16c36fc84fe0ef9b), (2, 0x7a51b3f36783765e), (10, 0x90d34619d5fa10c0), (31, 0x0ef50906e6da2c05), (100, 0x9edb3db4c3cb8399), (250, 0x618f34462f6666ba), (500, 0x63cc166655335736), (1000, 0x7abdfb48ef218b65), (2000, 0x5ab7d4bd1a68f0ce), ],
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
