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
    &[(1, 0x76463a29cc1198cf), (2, 0xf3a24dfd4927630a), (10, 0xfe0f31d318dbeca8), (31, 0x5a4386660936eebf), (100, 0xb343821d6bad5c41), (250, 0x15b7c23d70adfb10), (500, 0x8fe821465dba016c), (1000, 0x52468e750cc64d53), (2000, 0xe27ecd8d1cec76cd), (3000, 0x050edf6c6eb5e572), ],
    // B: сид 3, гиганты
    &[(1, 0xb9b3e1598fb799f1), (2, 0xf49ae265608b8de1), (10, 0xedcb31e9bba5b7c2), (31, 0xccecabbb23708ca7), (100, 0x237bebad6a18c41c), (250, 0x2b014d9b910097d7), (500, 0x985909e62428b72c), (1000, 0x6775ef641fa8ee60), (2000, 0xfd0402401f44d02a), ],
    // C: сид 7, лаборатория
    &[(1, 0x02b3ae83052b83dd), (2, 0x10d77735ff46c9a0), (10, 0x3bdf9372e4d43a8a), (31, 0x32cf80d6f5a70c1c), (100, 0x795f11aee032b697), (250, 0x92e68802e783bede), (500, 0x0339d792973403f6), (1000, 0x2baf01e0def2b042), (2000, 0xa044ebf6c9451c03), (3000, 0xdd30f7f5b00cdfa7), ],
    // D: сид 2, масштаб 10
    &[(1, 0xac086a8948368bf6), (2, 0x642b36ad84389c2e), (10, 0x4122c1084c1ec2c2), (31, 0x1887f3194f3c9616), (100, 0x030c9189680c3649), (250, 0x1f3ade96292535f1), (500, 0x993147787f63f81a), ],
    // E: сид 3, правила на ходу и подсадка
    &[(1, 0x45434c31faf1f89d), (2, 0xdedaa72e2570c1cd), (10, 0xdb055fa7893cf2b3), (31, 0x2eed6af143d62db0), (100, 0xb77cc1acb269e701), (250, 0x76259ee75a2730a8), (500, 0x558f74bcf44fa165), (1000, 0x3f2f5527d7663f19), ],
    // F: сид 5, смесь стратегий
    &[(1, 0xfafa91651ec4b441), (2, 0x4a6fe4fc0698c57e), (10, 0x0b546cb18b4e4508), (31, 0x5b9fe5b56c0c4b5f), (100, 0xe7b372c671b49711), (250, 0x514cc10f8f4f7ff6), (500, 0xf4f1e9744df7f9aa), (1000, 0x937e4c2923c85c4f), (2000, 0xb1ac21591635b74a), ],
    // G: сид 6, квадрат x10, еда линейно и волнами
    &[(1, 0x5a48e9cf2ba621a5), (2, 0xf8035414fd0be5ee), (10, 0x2304c4ab1f519bae), (31, 0x4c808577774a2f93), (100, 0x4af00a425bd832f1), (250, 0x3517278fffc7c23a), (500, 0xf15414520ec48c90), (1000, 0x1e75045550b5a58b), ],
    // H: сид 8, бои
    &[(1, 0x40f45dcb21325591), (2, 0x5278f5558073e891), (10, 0x9c0114e39561d175), (31, 0xf6aead7e1617a1a8), (100, 0x2fa25e5d13cf6891), (250, 0x2e7892495af9efbb), (500, 0x048ec47af32a7920), (1000, 0x0a4e38fd41f937b2), (2000, 0x928a4bc13261eca6), ],
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
