//! Кадр — всё, что окну нужно, чтобы нарисовать мир, собранное потоком
//! симуляции. Окно никогда не читает `World` напрямую: оно рисует последний
//! готовый кадр, поэтому медленный тик не может заморозить интерфейс.
//!
//! Размер кадра ограничен экраном, а не миром: в кадр попадают только видимые
//! существа (отбор по телу, а не по центру — размер травоядного это ген, и
//! крупное торчит в кадр, даже когда центр далеко). Если видимых слишком много,
//! вместо кружков идёт карта плотности — одна картинка размером с экран.

use life_core::genome::vegetarian;
use life_core::{Rules, World};
use life_sim::observe::{EventKind, GeneStat, Snapshot, gene_stats};

use crate::history::Sample;

/// Больше кружков в кадре не шлём: дальше — карта плотности. 32 байта на
/// существо — 8 МБ, это ещё легко заливается в видеокарту каждый кадр; а при
/// таком числе кружки всё равно мельче пикселя.
pub const MAX_INSTANCES: usize = 250_000;

/// Один кружок. Ровно 32 байта — так их и читает шейдер (`render.rs`).
/// Координаты — относительно `Frame::origin`; вид, курс и флаги призрака —
/// в `meta` (см. `motion.rs`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    /// Позиция на тике кадра.
    pub x: f32,
    pub y: f32,
    /// Позиция в прошлом кадре: окно рисует движение между ними.
    pub px: f32,
    pub py: f32,
    /// Радиус тела в единицах мира.
    pub r: f32,
    /// RGB и сытость (0‒255) в последнем байте.
    pub color: u32,
    /// Секунды с рождения на момент сборки кадра, у призрака — со смерти.
    pub age: f32,
    /// Курс u16 | вид << 16 | призрак | умер с голоду.
    pub meta: u32,
}

/// Картинка RGBA, натянутая на прямоугольник мира `rect` (x0, y0, x1, y1).
#[derive(Clone, Debug, Default)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub rect: (f64, f64, f64, f64),
}

/// Какую часть мира окно сейчас показывает и сколько в ней пикселей.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRequest {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub px_w: u32,
    pub px_h: u32,
}

impl ViewRequest {
    /// Видимая область с запасом по половине с каждой стороны: пока новый кадр
    /// не пришёл, камера успевает сдвинуться, и края не должны быть пустыми.
    pub fn padded(&self) -> (f64, f64, f64, f64) {
        let (dx, dy) = ((self.x1 - self.x0) * 0.5, (self.y1 - self.y0) * 0.5);
        (self.x0 - dx, self.y0 - dy, self.x1 + dx, self.y1 + dy)
    }
}

/// Чем закончилась партия.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Extinct,
    Explosion,
}

/// Темп и состояние партии — для верхней панели.
#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    pub paused: bool,
    pub speed_index: usize,
    /// Сколько тиков в секунду получается на самом деле.
    pub tps: f64,
    /// Тик не успевает за выбранной скоростью.
    pub lagging: bool,
    pub ended: Option<Ending>,
}

/// Прямоугольник мира: x0, y0, x1, y1 (x0 < x1, y0 < y1).
pub type Area = (f64, f64, f64, f64);

/// Сводка генов травоядных; None — никого нет.
pub type GeneSummary = Option<[GeneStat; vegetarian::N]>;

/// Сводка по области мира (инструмент «Область»): кто внутри (по центру тела)
/// и какой у них геном — рядом со сводкой по всему миру на том же тике.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionStats {
    pub area: Area,
    pub tick: u64,
    pub plants: usize,
    pub vegetarians: usize,
    /// Средняя заполненность бака травоядных внутри, 0..1.
    pub fullness: Option<f64>,
    pub inside: GeneSummary,
    pub world: GeneSummary,
}

impl RegionStats {
    /// `world_genes` — сводка по всему миру, если она уже посчитана на этом
    /// тике (срез); иначе считается здесь.
    pub fn of(world: &World, area: Area, world_genes: Option<GeneSummary>) -> RegionStats {
        let inside = |x: f64, y: f64| x >= area.0 && x <= area.2 && y >= area.1 && y <= area.3;
        let vegs = world.vegetarians.iter().filter(|v| inside(v.x, v.y));
        let (n, sum) = vegs.clone().fold((0, 0.0), |(n, s), v| (n + 1, s + v.energy / v.pheno.max_energy));
        let world_genes = world_genes
            .unwrap_or_else(|| gene_stats(&vegetarian::GENES, world.vegetarians.iter().map(|v| &v.genome)));
        RegionStats {
            area,
            tick: world.tick,
            plants: world.plants.iter().filter(|p| inside(p.x, p.y)).count(),
            vegetarians: n,
            fullness: (n > 0).then(|| sum / n as f64),
            inside: gene_stats(&vegetarian::GENES, vegs.map(|v| &v.genome)),
            world: world_genes,
        }
    }
}

/// Выбранное существо, как оно есть на тике кадра.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    /// Радиус тела.
    pub half: f64,
    pub vision: f64,
    pub speed: f64,
    pub energy: f64,
    pub max_energy: f64,
    /// Расход энергии за тик.
    pub upkeep: f64,
    pub genome: [f64; vegetarian::N],
    /// Слой по глубине (y от и до): где ему можно жить и есть.
    pub layer: (f64, f64),
}

impl Selected {
    pub fn of(world: &World, id: u64) -> Option<Selected> {
        world.vegetarian(id).map(|v| Selected {
            id,
            x: v.x,
            y: v.y,
            half: v.pheno.half,
            vision: v.pheno.vision,
            speed: v.pheno.speed,
            energy: v.energy,
            max_energy: v.pheno.max_energy,
            upkeep: v.pheno.upkeep,
            genome: v.genome.to_values(),
            layer: (v.pheno.layer_lo, v.pheno.layer_hi),
        })
    }
}

/// Запись хроники. `kind` — у событий наблюдателя (те же, что в отчёте);
/// у событий самой игры (правила, подсадка) его нет.
#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub tick: u64,
    pub kind: Option<EventKind>,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Frame {
    /// Номер мира: растёт при «Заново» и новом мире. Окно по нему понимает,
    /// что историю и хронику пора начать с чистого листа.
    pub world_gen: u64,
    pub seed: u64,
    pub scale: f64,
    /// Правила, по которым мир идёт сейчас.
    pub rules: Rules,
    pub tick: u64,
    pub plants: usize,
    pub vegetarians: usize,
    pub world_w: f64,
    pub world_h: f64,
    pub status: Status,
    /// Начало координат кружков в мире (f64): при ×10 000 мир шириной 6·10⁷,
    /// и в f32 абсолютные координаты теряли бы единицы пикселей.
    pub origin: (f64, f64),
    /// Растения, потом травоядные — в таком порядке и рисуются.
    pub instances: Vec<Instance>,
    /// Вместо кружков, когда видимых больше `MAX_INSTANCES`.
    pub density: Option<Raster>,
    /// Весь мир крупными клетками; приходит не в каждом кадре.
    pub minimap: Option<Raster>,
    pub selected: Option<Selected>,
    /// Новое с прошлого кадра: точки графиков и записи хроники. Кадры не
    /// теряются (поток кладёт новый, только когда окно забрало прошлый),
    /// поэтому приращений достаточно.
    pub samples: Vec<Sample>,
    /// Срезы мира — раз в `SNAPSHOT_EVERY` тиков (реже на огромном мире).
    pub snapshots: Vec<Snapshot>,
    /// Сводка по заданной области — когда её пересчитали (при установке и на
    /// каждом срезе).
    pub region: Option<RegionStats>,
    pub log: Vec<LogEntry>,
    /// Когда кадр собран: от этого момента окно отсчитывает возраст кружков.
    pub built: Option<std::time::Instant>,
    /// Сколько времени потока симуляции ушло на сборку кадра, мс.
    pub build_ms: f64,
    /// Средняя цена тика, мс.
    pub tick_ms: f64,
}

// ── цвета (палитра app/theme.py) ────────────────────────────────────────────

pub const WORLD_TOP: [u8; 3] = [31, 38, 47];
pub const WORLD_BOTTOM: [u8; 3] = [15, 18, 23];
pub const PLANT_COLOR: [u8; 3] = [93, 211, 158];
pub const VEGETARIAN_COLOR: [u8; 3] = [205, 134, 255];

pub fn lerp(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
}

/// Цвет и байт в последнем канале (у существ — сытость) одним u32.
pub fn rgba(c: [u8; 3], a: u8) -> u32 {
    u32::from_le_bytes([c[0], c[1], c[2], a])
}

/// Растения тусклее животных: их много, и они не должны спорить с теми, кто движется.
pub fn plant_color() -> [u8; 3] {
    lerp(WORLD_BOTTOM, PLANT_COLOR, 0.6)
}

/// Карта плотности: сколько растений и травоядных в каждой клетке
/// прямоугольника мира, в цвете. Считается за один проход по миру, поэтому
/// её цена не зависит от того, сколько существ видно.
pub fn density(world: &World, rect: (f64, f64, f64, f64), w: usize, h: usize, out: Raster) -> Raster {
    let (x0, y0, x1, y1) = rect;
    let (sx, sy) = (w as f64 / (x1 - x0), h as f64 / (y1 - y0));
    let mut counts = vec![[0u32; 2]; w * h];
    let mut add = |x: f64, y: f64, kind: usize| {
        let (cx, cy) = ((x - x0) * sx, (y - y0) * sy);
        if cx >= 0.0 && cy >= 0.0 && (cx as usize) < w && (cy as usize) < h {
            counts[cy as usize * w + cx as usize][kind] += 1;
        }
    };
    world.plants.iter().for_each(|p| add(p.x, p.y, 0));
    world.vegetarians.iter().for_each(|v| add(v.x, v.y, 1));

    let colors = [PLANT_COLOR, VEGETARIAN_COLOR];
    let mut rgba = out.rgba;
    rgba.clear();
    rgba.reserve(w * h * 4);
    for c in &counts {
        // Яркость по логарифму: одинокое существо видно, а скопление не слепит.
        let k = c.map(|n| if n == 0 { 0.0 } else { (0.6 + (n as f64).log2() / 10.0).min(1.0) });
        // Травоядные поверх растений.
        let mut px = [0.0f64; 3];
        let mut alpha = 0.0f64;
        for (kind, &a) in k.iter().enumerate() {
            for (ch, v) in px.iter_mut().enumerate() {
                *v = *v * (1.0 - a) + colors[kind][ch] as f64 * a;
            }
            alpha = alpha * (1.0 - a) + a;
        }
        // Премультиплицированный цвет: так его смешивает egui.
        rgba.extend([px[0] as u8, px[1] as u8, px[2] as u8, (alpha * 255.0) as u8]);
    }
    Raster { w, h, rgba, rect }
}

/// Размер миникарты в клетках: 480 по ширине, по высоте — по пропорциям мира,
/// но не меньше 8 строк (при ×1000 мир в 1500 раз шире, чем выше).
pub fn minimap_size(world_w: f64, world_h: f64) -> (usize, usize) {
    let w = 480;
    let h = ((w as f64 * world_h / world_w).round() as usize).clamp(8, 320);
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use life_core::WorldConfig;

    #[test]
    fn кружок_ровно_32_байта() {
        assert_eq!(std::mem::size_of::<Instance>(), 32);
    }

    /// Страж скорости кадра: 200 тыс. видимых существ собираются в кадр
    /// быстро — вместе с сопоставлением с прошлым кадром (`motion.rs`), — а мир
    /// ×400 целиком (600 тыс. растений) уходит в карту плотности, и кадр весит
    /// не больше пары мегабайт, а не десятки.
    #[test]
    fn кадр_огромного_мира_быстрый_и_лёгкий() {
        use crate::motion::Motion;
        use life_core::rng::Rng;

        let mut world =
            World::new(&WorldConfig { scale: 400.0, n_vegetarians: Some(0), ..Default::default() });
        let mut rng = Rng::new(9);
        let flora = world.flora().clone();
        world.plants =
            (0..world.space.per_area(life_core::config::PLANT_MAX)).map(|_| flora.plant(&mut rng)).collect();
        let (w, h) = (world.space.width, world.space.height);

        // вид на часть мира: ~200 тыс. растений в кадре; все родились на одном
        // тике — худший случай для сопоставления растений (одна большая группа)
        let part = (0.0, 0.0, w / 3.0, h);
        let mut out = Vec::new();
        let mut motion = Motion::default();
        assert!(motion.collect(&world, part, &mut out));
        world.plants.retain(|p| p.x.to_bits() % 7 != 0); // часть съели
        let start = std::time::Instant::now();
        assert!(motion.collect(&world, part, &mut out));
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("  [кадр] {} кружков за {ms:.1} мс", out.len());
        assert!(out.len() > 150_000);
        assert!(ms < 60.0, "сборка кадра {ms:.1} мс — окно получало бы кадры редко");

        // весь мир: кружков больше потолка — карта плотности размером с экран
        assert!(!motion.collect(&world, (0.0, 0.0, w, h), &mut out));
        let start = std::time::Instant::now();
        let r = density(&world, (0.0, 0.0, w, h), 960, 300, Raster::default());
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("  [плотность] {}x{} за {ms:.1} мс", r.w, r.h);
        assert!(r.rgba.len() <= 2 * 1024 * 1024);
        assert!(ms < 60.0, "карта плотности {ms:.1} мс");
    }

    #[test]
    fn плотность_считает_всех() {
        let world = World::new(&WorldConfig { scale: 10.0, ..Default::default() });
        let (w, h) = minimap_size(world.space.width, world.space.height);
        let r = density(&world, (0.0, 0.0, world.space.width, world.space.height), w, h, Raster::default());
        assert_eq!(r.rgba.len(), w * h * 4);
        let lit = r.rgba.chunks(4).filter(|p| p[3] > 0).count();
        assert!(lit >= 1, "стартовые существа видны на миникарте");
    }
}
