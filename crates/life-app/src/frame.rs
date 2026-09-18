//! Кадр — всё, что окну нужно, чтобы нарисовать мир, собранное потоком
//! симуляции. Окно никогда не читает `World` напрямую: оно рисует последний
//! готовый кадр, поэтому медленный тик не может заморозить интерфейс.
//!
//! Размер кадра ограничен экраном, а не миром: в кадр попадают только видимые
//! существа (отбор по телу, а не по центру — размер травоядного это ген, и
//! крупное торчит в кадр, даже когда центр далеко). Если видимых слишком много,
//! вместо кружков идёт карта плотности — одна картинка размером с экран.

use life_core::config::PLANT_RADIUS;
use life_core::predator::Predator;
use life_core::{Creature, Rules, World};
use life_sim::observe::EventKind;

use crate::history::{GenePoint, Sample};

/// Больше кружков в кадре не шлём: дальше — карта плотности. 16 байт на
/// существо — 6,4 МБ, это ещё легко заливается в видеокарту каждый кадр.
pub const MAX_INSTANCES: usize = 400_000;

/// Один кружок: центр относительно `Frame::origin`, радиус в единицах мира,
/// цвет RGBA. Ровно 16 байт — так их и читает шейдер.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub color: u32,
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

/// Выбранное существо, как оно есть на тике кадра.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected {
    pub creature: Creature,
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
    /// Геном травоядного; у хищника генома нет.
    pub genom: Option<[f64; 7]>,
    /// Слой травоядного по глубине (y от и до): где ему можно жить и есть.
    pub layer: Option<(f64, f64)>,
    pub fleeing: bool,
    pub hungry: bool,
}

impl Selected {
    pub fn of(world: &World, c: Creature) -> Option<Selected> {
        match c {
            Creature::Vegetarian(id) => world.vegetarian(id).map(|v| Selected {
                creature: c,
                x: v.x,
                y: v.y,
                half: v.half,
                vision: v.vision,
                speed: v.speed,
                energy: v.energy,
                max_energy: v.max_energy,
                upkeep: v.upkeep,
                genom: Some(v.genom.to_array()),
                layer: Some((v.layer_lo, v.layer_hi)),
                fleeing: v.flee_ticks > 0,
                hungry: false,
            }),
            Creature::Predator(id) => world.predator(id).map(|p| Selected {
                creature: c,
                x: p.x,
                y: p.y,
                half: Predator::DIAM / 2.0,
                vision: p.vision,
                speed: p.speed,
                energy: p.energy,
                max_energy: p.max_energy,
                upkeep: p.upkeep,
                genom: None,
                layer: None,
                fleeing: false,
                hungry: p.hungry(),
            }),
        }
    }
}

/// Запись хроники. `kind` — у событий наблюдателя (те же, что в отчёте);
/// у событий самой игры (мигранты, правила, подсадка) его нет.
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
    pub predators: usize,
    pub world_w: f64,
    pub world_h: f64,
    pub status: Status,
    /// Начало координат кружков в мире (f64): при ×10 000 мир шириной 6·10⁷,
    /// и в f32 абсолютные координаты теряли бы единицы пикселей.
    pub origin: (f64, f64),
    /// Растения, потом травоядные, потом хищники — в таком порядке и рисуются.
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
    pub gene_points: Vec<GenePoint>,
    pub log: Vec<LogEntry>,
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
pub const PREDATOR_COLOR: [u8; 3] = [255, 110, 94];

pub fn lerp(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
}

fn pack(c: [u8; 3]) -> u32 {
    u32::from_le_bytes([c[0], c[1], c[2], 255])
}

/// Травоядное тем ярче, чем полнее его бак: голодающих видно сразу.
const SHADES: usize = 6;

struct Palette {
    plant: u32,
    vegetarian: [u32; SHADES],
    predator: u32,
}

impl Palette {
    fn new() -> Self {
        Palette {
            plant: pack(lerp(WORLD_BOTTOM, PLANT_COLOR, 0.8)),
            vegetarian: std::array::from_fn(|i| {
                pack(lerp(WORLD_BOTTOM, VEGETARIAN_COLOR, 0.45 + 0.55 * i as f64 / (SHADES - 1) as f64))
            }),
            predator: pack(PREDATOR_COLOR),
        }
    }

    fn vegetarian(&self, energy: f64, max_energy: f64) -> u32 {
        let shade = (energy / max_energy * SHADES as f64).clamp(0.0, (SHADES - 1) as f64);
        self.vegetarian[shade as usize]
    }
}

/// Кружки видимой части мира в `out`. false — видимых больше `MAX_INSTANCES`,
/// и `out` недособран: тогда нужна карта плотности.
pub fn collect_instances(world: &World, rect: (f64, f64, f64, f64), out: &mut Vec<Instance>) -> bool {
    let palette = Palette::new();
    let (x0, y0, x1, y1) = rect;
    out.clear();
    let visible =
        |x: f64, y: f64, half: f64| x + half >= x0 && x - half <= x1 && y + half >= y0 && y - half <= y1;
    let mut push = |x: f64, y: f64, r: f64, color: u32| {
        out.push(Instance { x: (x - x0) as f32, y: (y - y0) as f32, r: r as f32, color });
        out.len() <= MAX_INSTANCES
    };
    for p in &world.plants {
        if visible(p.x, p.y, PLANT_RADIUS) && !push(p.x, p.y, PLANT_RADIUS, palette.plant) {
            return false;
        }
    }
    for v in &world.vegetarians {
        if visible(v.x, v.y, v.half) && !push(v.x, v.y, v.half, palette.vegetarian(v.energy, v.max_energy)) {
            return false;
        }
    }
    let half = Predator::DIAM / 2.0;
    for p in &world.predators {
        if visible(p.x, p.y, half) && !push(p.x, p.y, half, palette.predator) {
            return false;
        }
    }
    true
}

/// Карта плотности: сколько растений, травоядных и хищников в каждой клетке
/// прямоугольника мира, в цвете. Считается за один проход по миру, поэтому
/// её цена не зависит от того, сколько существ видно.
pub fn density(world: &World, rect: (f64, f64, f64, f64), w: usize, h: usize, out: Raster) -> Raster {
    let (x0, y0, x1, y1) = rect;
    let (sx, sy) = (w as f64 / (x1 - x0), h as f64 / (y1 - y0));
    let mut counts = vec![[0u32; 3]; w * h];
    let mut add = |x: f64, y: f64, kind: usize| {
        let (cx, cy) = ((x - x0) * sx, (y - y0) * sy);
        if cx >= 0.0 && cy >= 0.0 && (cx as usize) < w && (cy as usize) < h {
            counts[cy as usize * w + cx as usize][kind] += 1;
        }
    };
    world.plants.iter().for_each(|p| add(p.x, p.y, 0));
    world.vegetarians.iter().for_each(|v| add(v.x, v.y, 1));
    world.predators.iter().for_each(|p| add(p.x, p.y, 2));

    let colors = [PLANT_COLOR, VEGETARIAN_COLOR, PREDATOR_COLOR];
    let mut rgba = out.rgba;
    rgba.clear();
    rgba.reserve(w * h * 4);
    for c in &counts {
        // Яркость по логарифму: одинокое существо видно, а скопление не слепит.
        let k = c.map(|n| if n == 0 { 0.0 } else { (0.6 + (n as f64).log2() / 10.0).min(1.0) });
        // Хищники поверх травоядных, травоядные поверх растений.
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
    fn кружок_ровно_16_байт() {
        assert_eq!(std::mem::size_of::<Instance>(), 16);
    }

    #[test]
    fn в_кадр_попадают_только_видимые_по_телу() {
        let mut world =
            World::new(&WorldConfig { n_vegetarians: Some(0), n_predators: Some(0), ..Default::default() });
        world.plants.clear();
        // Центр за левым краем, но тело крупное — торчит в кадр.
        let big = life_core::Genom::from_array([400.0, 10.0, 400.0, 70.0, 30.0, 0.0, 100.0]);
        world.spawn_vegetarian(big, 1000.0 - 150.0, 2000.0, None);
        world.spawn_vegetarian(big, 100.0, 2000.0, None); // далеко слева
        world.spawn_predator(1500.0, 2000.0, None);
        let mut out = Vec::new();
        assert!(collect_instances(&world, (1000.0, 0.0, 2000.0, 4000.0), &mut out));
        assert_eq!(out.len(), 2, "крупное травоядное у края и хищник");
        assert!(out[0].x < 0.0, "координаты — от начала видимой области");
    }

    /// Страж скорости кадра: 200 тыс. видимых существ собираются в кадр
    /// быстро, а мир ×400 целиком (600 тыс. растений) уходит в карту
    /// плотности, и кадр весит не больше пары мегабайт, а не десятки.
    #[test]
    fn кадр_огромного_мира_быстрый_и_лёгкий() {
        use life_core::plant::Plant;
        use life_core::rng::Rng;

        let mut world = World::new(&WorldConfig {
            scale: 400.0,
            n_vegetarians: Some(0),
            n_predators: Some(0),
            ..Default::default()
        });
        let mut rng = Rng::new(9);
        world.plants = (0..world.space.per_area(life_core::config::PLANT_MAX))
            .map(|_| Plant::random(&world.space, &mut rng))
            .collect();
        let (w, h) = (world.space.width, world.space.height);

        // вид на часть мира: ~200 тыс. растений в кадре
        let part = (0.0, 0.0, w / 3.0, h);
        let mut out = Vec::new();
        let start = std::time::Instant::now();
        assert!(collect_instances(&world, part, &mut out));
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("  [кадр] {} кружков за {ms:.1} мс", out.len());
        assert!(out.len() > 150_000);
        assert!(ms < 60.0, "сборка кадра {ms:.1} мс — окно получало бы кадры редко");

        // весь мир: кружков больше потолка — карта плотности размером с экран
        assert!(!collect_instances(&world, (0.0, 0.0, w, h), &mut out));
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
