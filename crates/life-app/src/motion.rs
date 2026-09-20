//! Память между кадрами: откуда пришло существо, когда родилось, кто умер.
//!
//! Без неё кружок перескакивает из позиции прошлого кадра в новую, новорождённые
//! вспыхивают в полный рост, а съеденные пропадают за кадр — на экране
//! мельтешение. С ней окно рисует движение между двумя кадрами, рост при
//! рождении и угасание при смерти (это делает шейдер в `render.rs`).
//!
//! Всё — за линейное время от числа видимых существ:
//! - прошлую позицию находит проход двумя указателями (векторы мира
//!   отсортированы по id, значит и прошлый кадр тоже);
//! - время рождения — по кольцу «наибольший id в кадре → время кадра», поиск
//!   не нужен; существо, въехавшее в кадр при панораме, не «рождается» — его
//!   id старый;
//! - растения не двигаются и id не имеют: их узнаём по (тик рождения, x).
//!
//! Живёт в потоке симуляции; у нового мира — новая.

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use life_core::World;
use life_core::config::PLANT_RADIUS;

use crate::frame::{self, Instance, MAX_INSTANCES};

/// Возраст давно родившегося: анимация роста для него давно кончилась.
pub const OLD: f32 = 1.0e6;
/// Призрак хранится, пока не догорит самая долгая смерть — с голоду (шейдер: 0.5 с).
const GHOST_LIFE: Duration = Duration::from_millis(600);
/// Время рождения помним столько; кто старше — давно родился.
const RING: Duration = Duration::from_secs(1);
/// Меньше такой сытости в прошлом кадре — умер с голоду, а не съеден.
const STARVED: f32 = 0.05;
/// Курс поворачивает к направлению сдвига не сразу: зигзаги не дёргают нос.
const TURN: f32 = 0.5;

pub const KIND_PLANT: u32 = 0;
pub const KIND_CREATURE: u32 = 1;
/// Бит `meta`: призрак — существо уже умерло, `age` — время с его смерти.
pub const GHOST: u32 = 1 << 18;
/// Бит `meta`: призрак умер с голоду (сереет), а не съеден (сжимается).
pub const STARVED_BIT: u32 = 1 << 19;

/// Существо прошлого кадра (координаты в мире).
#[derive(Clone, Copy, Debug)]
struct Seen {
    id: u64,
    x: f64,
    y: f64,
    heading: f32,
    r: f32,
    color: u32,
    fullness: f32,
}

/// Растение прошлого кадра: ключ (тик рождения, биты x) и y.
#[derive(Clone, Copy, Debug)]
struct SeenPlant {
    born: u32,
    xbits: u64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct Ghost {
    kind: u32,
    x: f64,
    y: f64,
    r: f32,
    color: u32,
    heading: f32,
    starved: bool,
    died: Instant,
}

/// Кольцо «значение в кадре → время кадра»: по нему видно, в каком кадре
/// что-то появилось. `floor` — всё, что не больше него, появилось давно.
#[derive(Default, Debug)]
struct Ring {
    marks: VecDeque<(u64, Instant)>,
    floor: u64,
}

impl Ring {
    fn push(&mut self, value: u64, now: Instant) {
        self.marks.push_back((value, now));
        while let Some(&(v, t)) = self.marks.front() {
            if now.duration_since(t) <= RING {
                break;
            }
            self.floor = v;
            self.marks.pop_front();
        }
    }

    /// Сколько секунд назад появилось то, что впервые попало в кадр, где
    /// значение стало не меньше `key`.
    fn age(&self, key: u64, now: Instant) -> f32 {
        if key <= self.floor {
            return OLD;
        }
        let i = self.marks.partition_point(|&(v, _)| v < key);
        self.marks.get(i).map_or(0.0, |&(_, t)| now.duration_since(t).as_secs_f32())
    }
}

#[derive(Default, Debug)]
pub struct Motion {
    pub flock_colors: bool,
    started: bool,
    creatures: Vec<Seen>,
    /// Отсортированы по (born, xbits).
    plants: Vec<SeenPlant>,
    ids: Ring,
    ticks: Ring,
    max_id: u64,
    ghosts: Vec<Ghost>,
}

/// Угол курса в u16: полный круг — 65 536.
fn pack_heading(a: f32) -> u32 {
    ((a.rem_euclid(TAU) / TAU * 65536.0) as u32) & 0xFFFF
}

fn meta(kind: u32, heading: f32) -> u32 {
    pack_heading(heading) | kind << 16
}

/// Курс, пока существо не сдвинулось: у каждого свой, чтобы новорождённые
/// не смотрели все в одну сторону.
fn initial_heading(id: u64) -> f32 {
    (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 / (1u64 << 24) as f32 * TAU
}

/// Повернуть курс `from` к `to` на долю `k` кратчайшим путём.
fn turn(from: f32, to: f32, k: f32) -> f32 {
    let d = (to - from + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
    from + d * k
}

impl Motion {
    /// Кружки видимой части мира в `out` (растения, потом существа — в таком
    /// порядке и рисуются; призраки — в конце своего вида). false —
    /// видимых больше `MAX_INSTANCES`: нужна карта плотности, память кадра сброшена.
    pub fn collect(&mut self, world: &World, rect: (f64, f64, f64, f64), out: &mut Vec<Instance>) -> bool {
        let now = Instant::now();
        let max_id = world.creatures.last().map_or(0, |v| v.id).max(self.max_id);
        self.max_id = max_id;
        if !self.started {
            // Первый кадр мира: всё, что есть, было всегда.
            self.started = true;
            self.ids.floor = max_id;
            self.ticks.floor = world.tick;
        }
        self.ids.push(max_id, now);
        self.ticks.push(world.tick, now);
        self.ghosts.retain(|g| now.duration_since(g.died) < GHOST_LIFE);

        out.clear();
        let full =
            self.collect_plants(world, rect, now, out) && self.collect_vegetarians(world, rect, now, out);
        if !full {
            // Кадр недособран: сопоставлять следующий не с чем.
            self.creatures.clear();
            self.plants.clear();
            self.ghosts.clear();
        }
        full
    }

    fn collect_plants(
        &mut self,
        world: &World,
        rect: (f64, f64, f64, f64),
        now: Instant,
        out: &mut Vec<Instance>,
    ) -> bool {
        let (x0, y0, x1, y1) = rect;
        let r = PLANT_RADIUS;
        let visible = |x: f64, y: f64| x + r >= x0 && x - r <= x1 && y + r >= y0 && y - r <= y1;
        let color = frame::rgba(frame::plant_color(), 255);
        let mut seen = Vec::with_capacity(self.plants.len() + 64);
        for p in &world.plants {
            if !visible(p.x, p.y) {
                continue;
            }
            let (x, y) = ((p.x - x0) as f32, (p.y - y0) as f32);
            out.push(Instance {
                x,
                y,
                px: x,
                py: y,
                r: r as f32,
                color,
                age: self.ticks.age(p.born as u64 + 1, now),
                meta: meta(KIND_PLANT, 0.0),
            });
            if out.len() > MAX_INSTANCES {
                return false;
            }
            seen.push(SeenPlant { born: p.born, xbits: p.x.to_bits(), y: p.y });
        }
        // Внутри одного тика рождения порядок по x — свой, но одинаковый в обоих
        // кадрах; растения идут по тику рождения, так что сортировка почти даром.
        seen.sort_unstable_by_key(|s| (s.born, s.xbits));

        // Растение прошлого кадра, которое лежит в новом прямоугольнике, но в
        // кадр не попало, — съедено.
        let mut j = 0;
        for p in &self.plants {
            while j < seen.len() && (seen[j].born, seen[j].xbits) < (p.born, p.xbits) {
                j += 1;
            }
            let found = j < seen.len() && (seen[j].born, seen[j].xbits) == (p.born, p.xbits);
            let x = f64::from_bits(p.xbits);
            if !found && visible(x, p.y) {
                self.ghosts.push(Ghost {
                    kind: KIND_PLANT,
                    x,
                    y: p.y,
                    r: r as f32,
                    color,
                    heading: 0.0,
                    starved: false,
                    died: now,
                });
            }
        }
        self.plants = seen;
        self.push_ghosts(KIND_PLANT, rect, now, out)
    }

    fn collect_vegetarians(
        &mut self,
        world: &World,
        rect: (f64, f64, f64, f64),
        now: Instant,
        out: &mut Vec<Instance>,
    ) -> bool {
        let kind = KIND_CREATURE;
        let (x0, y0, x1, y1) = rect;
        let visible =
            |x: f64, y: f64, half: f64| x + half >= x0 && x - half <= x1 && y + half >= y0 && y - half <= y1;
        let prev = std::mem::take(&mut self.creatures);
        let mut seen = Vec::with_capacity(prev.len() + 16);
        let alive = |id: u64| world.creature(id).is_some();
        let mut ghosts = Vec::new();
        let mut j = 0;
        // Прошлое существо без пары: если его нет в мире — умерло.
        let mut gone = |s: &Seen| {
            if !alive(s.id) {
                ghosts.push(Ghost {
                    kind,
                    x: s.x,
                    y: s.y,
                    r: s.r,
                    color: s.color,
                    heading: s.heading,
                    starved: s.fullness < STARVED,
                    died: now,
                });
            }
        };

        let mut body = |id: u64, x: f64, y: f64, half: f64, color: u32, fullness: f32| -> bool {
            if !visible(x, y, half) {
                return true;
            }
            while j < prev.len() && prev[j].id < id {
                gone(&prev[j]);
                j += 1;
            }
            let before = (j < prev.len() && prev[j].id == id).then(|| prev[j]);
            let (px, py, heading) = match before {
                Some(b) => {
                    j += 1;
                    let (dx, dy) = ((x - b.x) as f32, (y - b.y) as f32);
                    let heading =
                        if dx != 0.0 || dy != 0.0 { turn(b.heading, dy.atan2(dx), TURN) } else { b.heading };
                    (b.x, b.y, heading)
                }
                None => (x, y, initial_heading(id)),
            };
            out.push(Instance {
                x: (x - x0) as f32,
                y: (y - y0) as f32,
                px: (px - x0) as f32,
                py: (py - y0) as f32,
                r: half as f32,
                color,
                age: self.ids.age(id, now),
                meta: meta(kind, heading),
            });
            seen.push(Seen { id, x, y, heading, r: half as f32, color, fullness });
            out.len() <= MAX_INSTANCES
        };

        for v in &world.creatures {
            let fullness = (v.energy / v.pheno.max_energy).clamp(0.0, 1.0) as f32;
            let color = frame::rgba(
                frame::creature_color(world, v.flock, self.flock_colors),
                (fullness * 255.0) as u8,
            );
            if !body(v.id, v.x, v.y, v.pheno.half, color, fullness) {
                return false;
            }
        }
        for s in &prev[j..] {
            gone(s);
        }
        self.ghosts.extend(ghosts);
        self.creatures = seen;
        self.push_ghosts(kind, rect, now, out)
    }

    fn push_ghosts(
        &self,
        kind: u32,
        rect: (f64, f64, f64, f64),
        now: Instant,
        out: &mut Vec<Instance>,
    ) -> bool {
        let (x0, y0, ..) = rect;
        for g in self.ghosts.iter().filter(|g| g.kind == kind) {
            let (x, y) = ((g.x - x0) as f32, (g.y - y0) as f32);
            let mut m = meta(kind, g.heading) | GHOST;
            if g.starved {
                m |= STARVED_BIT;
            }
            out.push(Instance {
                x,
                y,
                px: x,
                py: y,
                r: g.r,
                color: g.color,
                age: now.duration_since(g.died).as_secs_f32(),
                meta: m,
            });
        }
        out.len() <= MAX_INSTANCES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::genome::creature::Gene;
    use life_core::{CreatureGenome, WorldConfig};

    fn empty_world() -> World {
        let mut w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        w.plants.clear();
        w
    }

    const ALL: (f64, f64, f64, f64) = (0.0, 0.0, 6000.0, 4000.0);
    const BASE: CreatureGenome = CreatureGenome::BASE;

    fn kind(i: &Instance) -> u32 {
        (i.meta >> 16) & 3
    }

    #[test]
    fn в_кадр_попадают_только_видимые_по_телу() {
        let mut world = empty_world();
        // Центр за левым краем, но тело крупное — торчит в кадр.
        let big = BASE.with(Gene::Size, 400.0).with(Gene::MinY, 0.0);
        world.spawn(big, 1000.0 - 150.0, 2000.0, None);
        world.spawn(big, 100.0, 2000.0, None); // далеко слева
        world.spawn(BASE, 1500.0, 2000.0, None);
        let mut out = Vec::new();
        assert!(Motion::default().collect(&world, (1000.0, 0.0, 2000.0, 4000.0), &mut out));
        assert_eq!(out.len(), 2, "крупное существо у края и мелкое в середине");
        assert!(out[0].x < 0.0, "координаты — от начала видимой области");
    }

    #[test]
    fn кружок_помнит_прошлую_позицию_и_курс() {
        let mut world = empty_world();
        let id = world.spawn(BASE, 3000.0, 2000.0, None);
        let mut m = Motion::default();
        let mut out = Vec::new();
        m.collect(&world, ALL, &mut out);
        let before = out[0];
        assert_eq!((before.px, before.py), (before.x, before.y), "в первом кадре идти неоткуда");
        assert_eq!(before.age, OLD, "существа первого кадра были всегда");

        world.creatures[0].x += 10.0;
        m.collect(&world, ALL, &mut out);
        let after = out[0];
        assert_eq!((after.px, after.py), (before.x, before.y), "прошлая позиция — из прошлого кадра");
        assert_eq!(after.x, before.x + 10.0);
        // курс повернул к востоку (угол 0) от начального
        let heading = (after.meta & 0xFFFF) as f32 / 65536.0 * TAU;
        let expected = turn(initial_heading(id), 0.0, TURN).rem_euclid(TAU);
        assert!((heading - expected).abs() < 1e-3, "курс {heading}, ожидался {expected}");
    }

    #[test]
    fn цвета_стай_согласованы_и_не_меняют_мир() {
        let mut world = empty_world();
        for x in [1000.0, 1500.0, 2000.0] {
            world.spawn(BASE, x, 2000.0, None);
        }
        world.creatures[1].flock = world.creatures[0].flock;
        world.step(); // обновляет состав стай
        world.plants.clear(); // этот сценарий проверяет только кружки существ
        let tag = world.creatures[0].flock;
        let mut motion = Motion { flock_colors: true, ..Default::default() };
        let mut out = Vec::new();
        motion.collect(&world, ALL, &mut out);
        let colors: Vec<_> = out.iter().map(|v| v.color & 0xFFFFFF).collect();
        assert_eq!(colors[0], colors[1]);
        assert_ne!(colors[0], colors[2]);
        motion.collect(&world, ALL, &mut out);
        assert_eq!(colors, out.iter().map(|v| v.color & 0xFFFFFF).collect::<Vec<_>>());
        assert_eq!(world.tick, 1);
        assert_eq!(world.creatures[0].flock, tag);
        motion.flock_colors = false;
        motion.collect(&world, ALL, &mut out);
        assert!(out.iter().all(|v| v.color & 0xFFFFFF == frame::rgba(frame::CREATURE_COLOR, 0)));
    }

    #[test]
    fn новорождённый_растёт_а_въехавший_в_кадр_нет() {
        let mut world = empty_world();
        world.spawn(BASE, 500.0, 2000.0, None);
        let mut m = Motion::default();
        let mut out = Vec::new();
        // первый кадр видит только левую половину; старое существо справа за краем
        world.spawn(BASE, 5000.0, 2000.0, None);
        m.collect(&world, (0.0, 0.0, 3000.0, 4000.0), &mut out);
        assert_eq!(out.len(), 1);

        // новое существо и сдвиг вида на весь мир
        world.spawn(BASE, 1000.0, 2000.0, None);
        m.collect(&world, ALL, &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].age, OLD);
        assert_eq!(out[1].age, OLD, "въехало в кадр при панораме — не рождалось");
        assert!(out[2].age < 0.1, "новорождённое растёт с нуля: {}", out[2].age);
    }

    #[test]
    fn умершие_становятся_призраками_и_догорают() {
        let mut world = empty_world();
        let fed = world.spawn(BASE, 1000.0, 2000.0, None);
        let hungry = world.spawn(BASE, 2000.0, 2000.0, Some(0.1));
        world.plants.push(life_core::plant::Plant::at(4000.0, 2000.0));
        let mut m = Motion::default();
        let mut out = Vec::new();
        m.collect(&world, ALL, &mut out);
        assert_eq!(out.len(), 3);

        // оба существ пропали из мира: одного съели, другой умер с голоду
        world.creatures.retain(|v| v.id != fed && v.id != hungry);
        m.collect(&world, ALL, &mut out);
        let ghosts: Vec<&Instance> = out.iter().filter(|i| i.meta & GHOST != 0).collect();
        assert_eq!(ghosts.len(), 2);
        assert!(ghosts.iter().all(|g| kind(g) == KIND_CREATURE));
        assert_eq!(ghosts.iter().filter(|g| g.meta & STARVED_BIT != 0).count(), 1, "с голоду — один");
        // растение — до существ: порядок рисования не нарушен
        assert_eq!(kind(&out[0]), KIND_PLANT);

        std::thread::sleep(GHOST_LIFE + Duration::from_millis(20));
        m.collect(&world, ALL, &mut out);
        assert!(out.iter().all(|i| i.meta & GHOST == 0), "призраки догорели");
    }

    #[test]
    fn съеденное_растение_призрак_а_ушедшее_за_край_нет() {
        use life_core::plant::Plant;
        let mut world = empty_world();
        for (x, born) in [(100.0, 3), (2000.0, 3), (2100.0, 3), (5000.0, 4)] {
            let mut p = Plant::at(x, 1000.0);
            p.born = born;
            world.plants.push(p);
        }
        let mut m = Motion::default();
        let mut out = Vec::new();
        m.collect(&world, ALL, &mut out);
        assert_eq!(out.len(), 4);

        // растение на 2000 съели; вид сдвинулся — растение на 100 ушло за край
        world.plants.remove(1);
        m.collect(&world, (1000.0, 0.0, 6000.0, 4000.0), &mut out);
        let ghosts: Vec<&Instance> = out.iter().filter(|i| i.meta & GHOST != 0).collect();
        assert_eq!(ghosts.len(), 1, "призрак только у съеденного");
        assert_eq!(ghosts[0].x, 1000.0, "на месте съеденного (2000 − начало вида 1000)");
    }

    #[test]
    fn новые_растения_растут_старые_нет() {
        let mut world = World::new(&WorldConfig { seed: 3, ..Default::default() });
        let mut m = Motion::default();
        let mut out = Vec::new();
        for _ in 0..30 {
            world.step();
        }
        m.collect(&world, ALL, &mut out);
        let old = world.plants.len();
        assert!(out.iter().filter(|i| kind(i) == KIND_PLANT).all(|i| i.age == OLD));
        for _ in 0..10 {
            world.step();
        }
        m.collect(&world, ALL, &mut out);
        let young =
            out.iter().filter(|i| kind(i) == KIND_PLANT && i.meta & GHOST == 0 && i.age < 0.1).count();
        let grown = world.plants.iter().filter(|p| p.born >= 30).count();
        assert!(
            grown > 0 && young == grown,
            "растут ровно выросшие за 10 тиков: {young} из {grown} (было {old})"
        );
    }
}
