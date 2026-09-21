//! Ограниченный прогон: баланс и доля разворотов без боя и бегства.
use life_core::{Rules, World, WorldConfig};
use std::collections::BTreeMap;
fn main() {
    let seeds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    assert!((1..=8).contains(&seeds), "проверка рассчитана на 1–8 seed");
    println!("cost,combat,seed,ticks,population,turns,moves,ms");
    for cost in [1.0, 3.0] {
        if std::env::args().nth(2).is_some_and(|s| s.parse::<f64>().ok() != Some(cost)) {
            continue;
        }
        for combat in [0.0, 1.0] {
            let tasks: Vec<_> = (1..=seeds)
                .map(|seed| {
                    std::thread::spawn(move || {
                        let mut w = World::new(&WorldConfig {
                            seed,
                            rules: Rules::default()
                                .with("cost_scale", cost)
                                .unwrap()
                                .with("cannibalism", combat)
                                .unwrap(),
                            ..Default::default()
                        });
                        let start_count = w.creatures.len() as u64;
                        let mut previous = BTreeMap::<u64, (f64, f64)>::new();
                        let (mut turns, mut moves) = (0_u64, 0_u64);
                        let start = std::time::Instant::now();
                        for _ in 0..20000 {
                            let before: BTreeMap<_, _> = w
                                .creatures
                                .iter()
                                .map(|v| (v.id, (v.x, v.y, !v.fleeing() && v.mind.attack.is_none())))
                                .collect();
                            w.step();
                            let mut next = BTreeMap::new();
                            for v in &w.creatures {
                                assert!(
                                    v.x.is_finite()
                                        && v.y.is_finite()
                                        && v.energy.is_finite()
                                        && v.health.is_finite()
                                        && v.health >= 0.0
                                );
                                if let Some(&(x, y, peaceful)) = before.get(&v.id) {
                                    let d = (v.x - x, v.y - y);
                                    if peaceful
                                        && !v.fleeing()
                                        && v.mind.attack.is_none()
                                        && d.0.hypot(d.1) > 1e-6
                                    {
                                        if let Some(&(px, py)) = previous.get(&v.id) {
                                            moves += 1;
                                            turns += (px * d.0 + py * d.1 < 0.0) as u64;
                                        }
                                        next.insert(v.id, d);
                                    }
                                }
                            }
                            previous = next;
                            let c = w.counters;
                            assert_eq!(
                                start_count + c.born - c.starved - c.combat - c.old_age - c.cannibalized,
                                w.creatures.len() as u64
                            );
                        }
                        format!(
                            "{cost},{combat},{seed},{},{},{turns},{moves},{:.0}",
                            w.tick,
                            w.creatures.len(),
                            start.elapsed().as_secs_f64() * 1000.0
                        )
                    })
                })
                .collect();
            for task in tasks {
                println!("{}", task.join().expect("прогон прошёл проверки"));
            }
        }
    }
}
