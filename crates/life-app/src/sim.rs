//! Поток симуляции. `World` живёт только здесь: окно шлёт команды и забирает
//! готовые кадры, но никогда не ждёт тика. Преемник `app/session.py`.
//!
//! Кадры идут по принципу «последний побеждает»: поток кладёт кадр в слот,
//! только когда окно забрало прошлый, — очереди нет, и медленное окно не
//! копит кадры, а медленный тик не держит окно. Буферы кружков окно отдаёт
//! обратно, чтобы не выделять память на каждый кадр.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use life_core::config::DIVIDE_PERIOD;
use life_core::{CreatureGenome, Rules, World, WorldConfig};
use life_sim::observe::{EventTracker, Snapshot};

use crate::frame::{
    self, Area, CorpseMark, Ending, Frame, Instance, LogEntry, Raster, RegionStats, Selected, ShotTrail,
    Status, ViewRequest,
};
use crate::history::Sample;
use crate::motion::Motion;

/// Скорости, тиков в секунду; None — «максимум», сколько успеет процессор.
pub const SPEEDS: [Option<f64>; 9] = [
    Some(10.0),
    Some(30.0),
    Some(60.0),
    Some(120.0),
    Some(240.0),
    Some(480.0),
    Some(960.0),
    Some(1920.0),
    None,
];
/// Спокойный старт: 30 т/с; ускорение доступно на верхней панели.
pub const DEFAULT_SPEED: usize = 1;

/// Существ на базовую площадь, после которых партия останавливается
/// («взрыв численности»). Как `EXPLOSION_LIMIT` в Python; растёт с площадью.
pub const EXPLOSION_LIMIT: usize = 3000;

/// Точка графика численностей — раз в столько тиков.
pub const GRAPH_EVERY: u64 = 10;
/// Срез для хроники и графика генома — не чаще, чем раз в столько тиков
/// (кратно периоду деления, как в отчёте).
pub const SNAPSHOT_EVERY: u64 = 60;
/// Срез считает перцентили генов (сортировка), и на огромном мире он дорог.
/// Интервал растёт так, чтобы срезы съедали не больше этой доли времени тиков.
const SNAPSHOT_SHARE: f64 = 0.05;

/// Тики без передышки идут не дольше этого: потом поток смотрит команды и
/// отдаёт кадр. Иначе на «максимуме» пауза нажималась бы с задержкой.
const SLICE: Duration = Duration::from_millis(8);
/// Кадры не чаще, чем нужно экрану.
const MIN_FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / 120);
/// Миникарта обновляется пару раз в секунду: ей хватает.
const MINIMAP_INTERVAL: Duration = Duration::from_millis(400);
/// Окно, по которому меряется фактический темп.
const TPS_WINDOW: Duration = Duration::from_millis(500);

pub enum Command {
    #[cfg(test)]
    TestWorld(Box<World>),
    TogglePause,
    SetPaused(bool),
    /// Один тик — только на паузе.
    Step,
    SetSpeed(usize),
    FlockColors(bool),
    /// Не строить графическое содержимое кадра, сохраняя статистику и управление.
    RenderWorld(bool),
    View(ViewRequest),
    /// Выбрать существо у точки мира (клик): ближайшее, до края тела которого
    /// не дальше `radius`. Мимо — выбор снимается.
    Pick {
        x: f64,
        y: f64,
        radius: f64,
    },
    /// Выбрать существо по номеру.
    Select(Option<u64>),
    /// Область для сводки генов (инструмент «Область»); None — снять.
    SetRegion(Option<Area>),
    /// Новые правила посреди партии; `note` — что поменялось, для хроники.
    SetRules {
        rules: Rules,
        note: String,
    },
    /// Подсадить базовое существо в точку мира.
    Spawn {
        x: f64,
        y: f64,
    },
    /// Та же партия с начала: тот же сид и те же стартовые правила.
    Restart,
    /// «Взрыв численности» — продолжить всё равно; больше не останавливаемся.
    KeepGoing,
    NewWorld(WorldConfig),
    Quit,
}

/// Что поток симуляции делает с кадрами после публикации: будит окно.
pub type Waker = Box<dyn Fn() + Send>;

pub struct SimHandle {
    tx: Sender<Command>,
    slot: Arc<Mutex<Option<Frame>>>,
    recycle: Sender<Vec<Instance>>,
    thread: Option<JoinHandle<()>>,
}

impl SimHandle {
    pub fn spawn(cfg: WorldConfig, waker: Waker) -> Self {
        let (tx, rx) = mpsc::channel();
        let (recycle, recycled) = mpsc::channel();
        let slot = Arc::new(Mutex::new(None));
        let shared = slot.clone();
        let thread = std::thread::Builder::new()
            .name("симуляция".into())
            .spawn(move || Sim::new(cfg, rx, recycled, shared, waker).run())
            .expect("поток симуляции не запустился");
        SimHandle { tx, slot, recycle, thread: Some(thread) }
    }

    pub fn send(&self, cmd: Command) {
        // поток мог упасть; окно от этого падать не должно
        let _ = self.tx.send(cmd);
    }

    /// Последний готовый кадр, если пришёл новый.
    pub fn take_frame(&self) -> Option<Frame> {
        self.slot.lock().ok()?.take()
    }

    /// Вернуть буфер кружков из отрисованного кадра.
    pub fn recycle(&self, buf: Vec<Instance>) {
        let _ = self.recycle.send(buf);
    }

    pub fn is_alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }
}

impl Drop for SimHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Всё, что копится между кадрами и уходит в кадр приращением.
#[derive(Default)]
struct Pending {
    samples: Vec<Sample>,
    snapshots: Vec<Snapshot>,
    region: Option<RegionStats>,
    log: Vec<LogEntry>,
}

struct Sim {
    cfg: WorldConfig,
    world: World,
    world_gen: u64,
    rx: Receiver<Command>,
    recycled: Receiver<Vec<Instance>>,
    slot: Arc<Mutex<Option<Frame>>>,
    waker: Waker,

    paused: bool,
    speed_index: usize,
    ended: Option<Ending>,
    watch_explosion: bool,
    view: Option<ViewRequest>,
    selected: Option<u64>,
    selected_flock: Option<u64>,
    region: Option<Area>,
    render_world: bool,
    dots: bool,

    // ── наблюдение ──────────────────────────────────────────────────────────
    /// Численности последних `DIVIDE_PERIOD` тиков — для сглаженной точки графика.
    window: VecDeque<[usize; 2]>,
    tracker: EventTracker,
    snapshot_every: u64,
    next_snapshot: u64,
    pending: Pending,

    // ── темп ────────────────────────────────────────────────────────────────
    /// Сколько тиков «задолжали» выбранной скорости.
    due: f64,
    last_time: Instant,
    tps: f64,
    tps_ticks: u64,
    tps_since: Instant,
    lagging: bool,
    /// Средняя цена тика, мс (скользящее среднее).
    tick_ms: f64,
    /// Цена последнего снимка наблюдателя, мс.
    snapshot_ms: f64,

    // ── кадры ───────────────────────────────────────────────────────────────
    /// Мир изменился с прошлого кадра.
    dirty: bool,
    last_frame: Instant,
    frame_interval: Duration,
    /// Окно не забрало прошлый кадр (например, свёрнуто): не крутимся вхолостую.
    blocked: bool,
    last_minimap: Option<Instant>,
    /// Память прошлого кадра: движение, рождения, призраки.
    motion: Motion,
    recent_shots: VecDeque<(ShotTrail, Instant)>,
}

impl Sim {
    fn new(
        cfg: WorldConfig,
        rx: Receiver<Command>,
        recycled: Receiver<Vec<Instance>>,
        slot: Arc<Mutex<Option<Frame>>>,
        waker: Waker,
    ) -> Self {
        let now = Instant::now();
        let mut sim = Sim {
            world: World::new(&cfg),
            world_gen: 0,
            cfg,
            rx,
            recycled,
            slot,
            waker,
            paused: false,
            speed_index: DEFAULT_SPEED,
            ended: None,
            watch_explosion: true,
            view: None,
            selected: None,
            selected_flock: None,
            region: None,
            render_world: true,
            dots: false,
            window: VecDeque::new(),
            tracker: EventTracker::new(),
            snapshot_every: SNAPSHOT_EVERY,
            next_snapshot: 0,
            pending: Pending::default(),
            due: 0.0,
            last_time: now,
            tps: 0.0,
            tps_ticks: 0,
            tps_since: now,
            lagging: false,
            tick_ms: 0.0,
            snapshot_ms: 0.0,
            dirty: true,
            last_frame: now - MIN_FRAME_INTERVAL,
            frame_interval: MIN_FRAME_INTERVAL,
            blocked: false,
            last_minimap: None,
            motion: Motion::default(),
            recent_shots: VecDeque::new(),
        };
        sim.observe_start();
        sim
    }

    fn running(&self) -> bool {
        !self.paused && self.ended.is_none()
    }

    fn target_tps(&self) -> Option<f64> {
        SPEEDS[self.speed_index]
    }

    fn run(mut self) {
        loop {
            // ── команды: ждём их, только если делать больше нечего ──────────
            let wait = self.idle_wait();
            let first = if wait.is_zero() {
                self.rx.try_recv().ok()
            } else {
                match self.rx.recv_timeout(wait) {
                    Ok(cmd) => Some(cmd),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            if let Some(cmd) = first
                && !self.apply(cmd)
            {
                return;
            }
            while let Ok(cmd) = self.rx.try_recv() {
                if !self.apply(cmd) {
                    return;
                }
            }

            self.advance();
            self.publish();
        }
    }

    /// Сколько можно спать до следующего дела: тика по расписанию или кадра.
    fn idle_wait(&self) -> Duration {
        let frame_wait = if self.dirty && self.blocked {
            Duration::from_millis(5)
        } else if self.dirty {
            self.frame_interval.saturating_sub(self.last_frame.elapsed())
        } else {
            Duration::from_millis(250)
        };
        if !self.running() {
            return frame_wait;
        }
        match self.target_tps() {
            None => Duration::ZERO,
            Some(_) if self.due >= 1.0 => Duration::ZERO,
            Some(tps) => frame_wait.min(Duration::from_secs_f64((1.0 - self.due) / tps)),
        }
    }

    /// false — пора выходить.
    fn apply(&mut self, cmd: Command) -> bool {
        match cmd {
            #[cfg(test)]
            Command::TestWorld(world) => {
                self.world = *world;
                self.recent_shots.clear();
                for shot in &self.world.shots {
                    self.recent_shots
                        .push_back((ShotTrail { from: shot.from, to: shot.to, age: 0.0 }, Instant::now()));
                }
                self.world_gen += 1;
                self.selected = None;
                self.selected_flock = None;
                self.paused = true;
                self.ended = None;
                self.motion = Motion::default();
                self.motion.flock_colors = true;
                self.last_minimap = None;
                self.dirty = true;
            }
            Command::TogglePause => self.set_paused(!self.paused),
            Command::SetPaused(p) => self.set_paused(p),
            Command::Step => {
                if self.paused && self.ended.is_none() {
                    self.tick();
                }
            }
            Command::SetSpeed(i) => {
                self.speed_index = i.min(SPEEDS.len() - 1);
                self.due = 0.0;
                self.reset_tps();
                self.dirty = true;
            }
            Command::FlockColors(enabled) => {
                if !enabled {
                    self.selected_flock = None;
                }
                self.motion.flock_colors = enabled;
                self.last_minimap = None;
                self.dirty = true;
            }
            Command::RenderWorld(enabled) => {
                if self.render_world != enabled {
                    let colored = self.motion.flock_colors;
                    self.motion = Motion::default();
                    self.motion.flock_colors = colored;
                    self.dots = false;
                    self.recent_shots.clear();
                }
                self.render_world = enabled;
                if enabled {
                    self.last_minimap = None;
                }
                self.dirty = true;
            }
            Command::View(v) => {
                if self.view != Some(v) {
                    self.view = Some(v);
                    self.dirty = true;
                }
            }
            Command::Pick { x, y, radius } => {
                self.selected = self.world.pick(x, y, 0.0);
                self.selected_flock = if self.selected.is_none() && self.motion.flock_colors {
                    frame::flock_areas(&self.world)
                        .into_iter()
                        .filter(|s| {
                            (s.x - x).hypot(s.y - y) <= s.radius.max(s.territory_radius).max(radius * 1.2)
                        })
                        .min_by(|a, b| {
                            (a.x - x)
                                .hypot(a.y - y)
                                .total_cmp(&(b.x - x).hypot(b.y - y))
                                .then(a.id.cmp(&b.id))
                        })
                        .map(|s| s.id)
                } else {
                    None
                };
                if self.selected.is_none() && self.selected_flock.is_none() {
                    self.selected = self.world.pick(x, y, radius);
                }
                self.dirty = true;
            }
            Command::Select(c) => {
                self.selected = c;
                self.selected_flock = None;
                self.dirty = true;
            }
            Command::SetRegion(area) => {
                self.region = area;
                // сразу, а не на следующем срезе: на паузе срезов нет
                self.pending.region = area.map(|a| RegionStats::of(&self.world, a, None));
                self.dirty = true;
            }
            Command::SetRules { rules, note } => {
                self.world.set_rules(rules);
                self.log(None, note);
            }
            Command::Spawn { x, y } => {
                self.world.spawn(CreatureGenome::BASE, x, y, None);
                self.log(None, "подсажено существо".into());
                // Подсадка в вымерший мир его оживляет.
                if self.ended == Some(Ending::Extinct) {
                    self.ended = None;
                }
            }
            Command::Restart => self.restart(self.cfg.clone()),
            Command::NewWorld(cfg) => self.restart(cfg),
            Command::KeepGoing => {
                if self.ended == Some(Ending::Explosion) {
                    self.ended = None;
                    self.watch_explosion = false;
                    self.dirty = true;
                }
            }
            Command::Quit => return false,
        }
        true
    }

    fn log(&mut self, kind: Option<life_sim::observe::EventKind>, text: String) {
        self.pending.log.push(LogEntry { tick: self.world.tick, kind, text });
        self.dirty = true;
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.due = 0.0;
        self.last_time = Instant::now();
        self.reset_tps();
        self.dirty = true;
    }

    fn reset_tps(&mut self) {
        self.tps_ticks = 0;
        self.tps_since = Instant::now();
        self.lagging = false;
    }

    fn restart(&mut self, cfg: WorldConfig) {
        // Большой мир строится заметное время — но в этом потоке, окно живёт.
        self.world = World::new(&cfg);
        self.cfg = cfg;
        self.world_gen += 1;
        self.ended = None;
        self.watch_explosion = true;
        self.selected = None;
        self.selected_flock = None;
        self.region = None;
        self.due = 0.0;
        self.last_time = Instant::now();
        self.last_minimap = None;
        let flock_colors = self.motion.flock_colors;
        self.motion = Motion::default();
        self.motion.flock_colors = flock_colors;
        self.recent_shots.clear();
        self.tick_ms = 0.0;
        self.snapshot_ms = 0.0;
        self.reset_tps();
        self.pending = Pending::default();
        self.observe_start();
        self.dirty = true;
    }

    /// Начало наблюдения за новым миром: первая точка графика и первый срез.
    fn observe_start(&mut self) {
        self.window.clear();
        self.tracker = EventTracker::new();
        self.snapshot_every = SNAPSHOT_EVERY;
        self.record();
        self.snapshot();
    }

    fn tick(&mut self) {
        let start = Instant::now();
        self.world.step();
        let engine_ms = start.elapsed().as_secs_f64() * 1000.0;
        let now = Instant::now();
        if self.render_world {
            for shot in self.world.shots.iter().filter(|shot| shot.tick == self.world.tick) {
                self.recent_shots.push_back((ShotTrail { from: shot.from, to: shot.to, age: 0.0 }, now));
            }
            self.recent_shots.retain(|(_, at)| now.duration_since(*at).as_secs_f32() < 0.25);
            while self.recent_shots.len() > 512 {
                self.recent_shots.pop_front();
            }
        }
        self.tick_ms = if self.tick_ms == 0.0 { engine_ms } else { self.tick_ms * 0.95 + engine_ms * 0.05 };
        self.tps_ticks += 1;
        self.dirty = true;

        self.record();
        if self.world.tick >= self.next_snapshot {
            self.snapshot();
        }
        if let Some(id) = self.selected
            && Selected::of(&self.world, id).is_none()
        {
            self.selected = None;
            self.log(None, "выбранное существо погибло".into());
        }

        let n = self.world.creatures.len();
        if n == 0 {
            self.ended = Some(Ending::Extinct);
        } else if self.watch_explosion && n > self.world.space.per_area(EXPLOSION_LIMIT) {
            self.ended = Some(Ending::Explosion);
        }
    }

    /// Каждый тик — численности в окно сглаживания; раз в `GRAPH_EVERY` — точка графика.
    fn record(&mut self) {
        let w = &self.world;
        if self.window.len() == DIVIDE_PERIOD as usize {
            self.window.pop_front();
        }
        self.window.push_back([w.plants.len(), w.creatures.len()]);
        if !w.tick.is_multiple_of(GRAPH_EVERY) {
            return;
        }
        let n = self.window.len() as f64;
        let avg = |k: usize| self.window.iter().map(|c| c[k] as f64).sum::<f64>() / n;
        let stats = w.stats();
        self.pending.samples.push(Sample {
            tick: w.tick,
            plants: avg(0),
            creatures: avg(1),
            shots: w.counters.ranged_shots,
            genom: stats.avg_genom,
        });
    }

    /// Срез мира: хроника и график генома. На большом мире срез дорог, и
    /// интервал растёт, чтобы наблюдение не отнимало время у тиков.
    fn snapshot(&mut self) {
        let start = Instant::now();
        let snap = Snapshot::of(&self.world);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.snapshot_ms = ms;
        if self.tick_ms > 0.0 {
            let ticks = (ms / (SNAPSHOT_SHARE * self.tick_ms)).ceil() as u64;
            self.snapshot_every = ticks.div_ceil(SNAPSHOT_EVERY).max(1) * SNAPSHOT_EVERY;
        }
        self.next_snapshot = self.world.tick + self.snapshot_every;

        let mut events = Vec::new();
        self.tracker.observe(&snap, &mut events);
        for e in events {
            self.log(Some(e.kind), e.text);
        }
        if let Some(area) = self.region {
            self.pending.region = Some(RegionStats::of(&self.world, area, Some(snap.genes)));
        }
        self.pending.snapshots.push(snap);
    }

    /// Тики по расписанию: не больше, чем задолжали скорости, и не дольше `SLICE`.
    fn advance(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64();
        self.last_time = now;
        if self.running() {
            if let Some(tps) = self.target_tps() {
                // Отставание не копится: догонять секунды тиков рывком — это тот
                // же фриз, только в симуляции. Долг не больше десятой доли секунды.
                self.due = (self.due + dt * tps).min(tps * 0.1 + 1.0);
            }
            let start = Instant::now();
            while self.running()
                && (self.target_tps().is_none() || self.due >= 1.0)
                && start.elapsed() < SLICE
            {
                self.tick();
                self.due = (self.due - 1.0).max(0.0);
            }
        }

        let window = self.tps_since.elapsed();
        if window >= TPS_WINDOW {
            self.tps = self.tps_ticks as f64 / window.as_secs_f64();
            self.lagging = self.running() && self.target_tps().is_some_and(|t| self.tps < t * 0.85);
            self.tps_ticks = 0;
            self.tps_since = Instant::now();
            self.dirty = true;
        }
    }

    /// Отдать кадр, если окно забрало прошлый и пора.
    fn publish(&mut self) {
        if !self.dirty || self.last_frame.elapsed() < self.frame_interval {
            return;
        }
        let Ok(slot) = self.slot.lock() else { return };
        self.blocked = slot.is_some(); // окно ещё не забрало прошлый кадр
        if self.blocked {
            return;
        }
        drop(slot);

        let start = Instant::now();
        let frame = self.build_frame();
        let build = start.elapsed();
        // Сборка кадра не должна съедать больше трети времени потока: на
        // огромном мире кадры просто идут реже, а тики — с прежней скоростью.
        self.frame_interval = MIN_FRAME_INTERVAL.max(build * 2);

        if let Ok(mut s) = self.slot.lock() {
            *s = Some(frame);
        }
        self.dirty = false;
        self.last_frame = Instant::now();
        (self.waker)();
    }

    fn build_frame(&mut self) -> Frame {
        let start = Instant::now();
        let w = &self.world;
        let mut instances = self.recycled.try_iter().last().unwrap_or_default();
        let mut density = None;
        let mut origin = (0.0, 0.0);
        if self.render_world
            && let Some(view) = self.view
        {
            let rect = view.padded();
            origin = (rect.0, rect.1);
            let mean_size =
                w.creatures.iter().map(|v| v.pheno.size).sum::<f64>() / w.creatures.len().max(1) as f64;
            let px_size = mean_size * view.px_w as f64 / (view.x1 - view.x0).max(1.0);
            if self.dots {
                if px_size >= 3.5 {
                    self.dots = false;
                    let colored = self.motion.flock_colors;
                    self.motion = Motion::default();
                    self.motion.flock_colors = colored;
                }
            } else if px_size <= 2.5 {
                self.dots = true;
            }
            let collected = if self.dots {
                frame::dots_colored(w, rect, &mut instances, self.motion.flock_colors)
            } else {
                self.motion.collect(w, rect, &mut instances)
            };
            if !collected {
                instances.clear();
                // Карта плотности ровно по видимой области, клетка — пара пикселей.
                let (dw, dh) =
                    ((view.px_w as usize / 2).clamp(1, 1024), (view.px_h as usize / 2).clamp(1, 1024));
                density = Some(frame::density_colored(
                    w,
                    (view.x0, view.y0, view.x1, view.y1),
                    dw,
                    dh,
                    Raster::default(),
                    self.motion.flock_colors,
                ));
            }
        } else {
            instances.clear();
        }
        let minimap =
            if self.render_world && self.last_minimap.is_none_or(|t| t.elapsed() >= MINIMAP_INTERVAL) {
                self.last_minimap = Some(Instant::now());
                let (mw, mh) = frame::minimap_size(w.space.width, w.space.height);
                Some(frame::density_colored(
                    w,
                    (0.0, 0.0, w.space.width, w.space.height),
                    mw,
                    mh,
                    Raster::default(),
                    self.motion.flock_colors,
                ))
            } else {
                None
            };
        let pending = std::mem::take(&mut self.pending);
        let mut flock_areas =
            if self.render_world && (self.motion.flock_colors || self.selected_flock.is_some()) {
                frame::flock_areas(w)
            } else {
                Vec::new()
            };
        let selected_flock = self.selected_flock.and_then(|id| {
            flock_areas
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.details.clone())
                .or_else(|| life_core::flock::summary(w, id))
        });
        if let Some(view) = self.view.filter(|_| self.motion.flock_colors) {
            let (x0, y0, x1, y1) = view.padded();
            flock_areas.retain(|s| {
                let radius = s.radius.max(s.territory_radius);
                s.x + radius >= x0 && s.x - radius <= x1 && s.y + radius >= y0 && s.y - radius <= y1
            });
        } else {
            flock_areas.clear();
        }
        let corpses = if self.render_world {
            self.view
                .map(|v| {
                    let (x0, y0, x1, y1) = v.padded();
                    w.corpses
                        .iter()
                        .filter(|c| {
                            c.x + c.size >= x0
                                && c.x - c.size <= x1
                                && c.y + c.size >= y0
                                && c.y - c.size <= y1
                        })
                        .take(10_000)
                        .map(|c| CorpseMark {
                            x: c.x,
                            y: c.y,
                            size: c.size,
                            fullness: if c.initial > 0.0 {
                                (c.remaining / c.initial).clamp(0.0, 1.0)
                            } else {
                                0.0
                            },
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let shots = if self.render_world {
            self.recent_shots
                .iter()
                .filter(|(_, at)| at.elapsed().as_secs_f32() < 0.25)
                .map(|(s, at)| ShotTrail { age: at.elapsed().as_secs_f32(), ..*s })
                .collect()
        } else {
            Vec::new()
        };
        Frame {
            world_gen: self.world_gen,
            seed: self.cfg.seed,
            scale: self.cfg.scale,
            rules: w.rules.clone(),
            tick: w.tick,
            plants: w.plants.len(),
            creatures: w.creatures.len(),
            world_w: w.space.width,
            world_h: w.space.height,
            status: Status {
                paused: self.paused,
                speed_index: self.speed_index,
                tps: self.tps,
                lagging: self.lagging,
                ended: self.ended,
            },
            render_world: self.render_world,
            dots: self.dots && self.render_world,
            origin,
            instances,
            flock_areas,
            corpses,
            shots,
            density,
            minimap,
            selected: self.selected.and_then(|id| Selected::of(w, id)),
            selected_flock,
            samples: pending.samples,
            snapshots: pending.snapshots,
            region: pending.region,
            log: pending.log,
            built: Some(Instant::now()),
            build_ms: start.elapsed().as_secs_f64() * 1000.0,
            tick_ms: self.tick_ms,
            snapshot_ms: self.snapshot_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn выключенный_рендер_сохраняет_карточку_стаи_без_областей() {
        let cfg = WorldConfig { n_creatures: Some(2), ..Default::default() };
        let (_tx, rx) = mpsc::channel();
        let (_recycle_tx, recycled) = mpsc::channel();
        let mut sim = Sim::new(cfg, rx, recycled, Arc::new(Mutex::new(None)), Box::new(|| {}));
        let id = sim.world.creatures[0].flock;
        sim.world.creatures[1].flock = id;
        sim.selected_flock = Some(id);
        sim.render_world = false;
        let frame = sim.build_frame();
        assert!(frame.flock_areas.is_empty());
        assert_eq!(frame.selected_flock.unwrap().members, 2);
    }

    /// Диагностика разделяет цену среза и построения графического кадра.
    /// Запускается вручную: цифры зависят от машины и не являются порогом теста.
    #[test]
    #[ignore]
    fn замер_среза_и_режимов_сборки_кадра_4000_4000() {
        use life_core::rng::Rng;
        fn measure(sim: &mut Sim, count: usize) -> (f64, Frame) {
            let start = Instant::now();
            let mut last = Frame::default();
            for _ in 0..count {
                last = sim.build_frame();
            }
            (start.elapsed().as_secs_f64() * 1000.0 / count as f64, last)
        }
        let cfg = WorldConfig { n_creatures: Some(4_000), ..Default::default() };
        let (_tx, rx) = mpsc::channel();
        let (_recycle_tx, recycled) = mpsc::channel();
        let mut sim = Sim::new(cfg, rx, recycled, Arc::new(Mutex::new(None)), Box::new(|| {}));
        let flora = sim.world.flora().clone();
        let mut rng = Rng::new(17);
        sim.world.plants = (0..4_000).map(|_| flora.plant(&mut rng)).collect();
        sim.view = Some(ViewRequest {
            x0: 0.0,
            y0: 0.0,
            x1: sim.world.space.width,
            y1: sim.world.space.height,
            px_w: 1600,
            px_h: 900,
        });
        let start = Instant::now();
        let _snapshot = Snapshot::of(&sim.world);
        let snapshot_ms = start.elapsed().as_secs_f64() * 1000.0;
        let (normal_ms, normal) = measure(&mut sim, 20);
        assert_eq!(normal.instances.len(), 8_000);
        sim.view.as_mut().unwrap().px_w = 300;
        let (dots_ms, dots) = measure(&mut sim, 20);
        assert!(dots.dots);
        assert_eq!(dots.instances.len(), 8_000);
        sim.render_world = false;
        sim.selected = Some(1);
        let (off_ms, off) = measure(&mut sim, 20);
        assert!(off.instances.is_empty() && off.density.is_none() && off.minimap.is_none());
        assert!(off.selected.is_some());
        eprintln!(
            "срез {snapshot_ms:.3} мс · сборка тел {normal_ms:.3} · квадраты {dots_ms:.3} · выкл {off_ms:.3} мс"
        );
    }

    /// Кадры до первого подходящего; ожидание ограничено: 500 попыток по 10 мс.
    fn frames_until(h: &SimHandle, until: impl Fn(&Frame) -> bool) -> Vec<Frame> {
        let mut seen = Vec::new();
        for _ in 0..500 {
            if let Some(f) = h.take_frame() {
                let done = until(&f);
                seen.push(f);
                if done {
                    return seen;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("кадр не пришёл за 5 с");
    }

    fn wait_frame(h: &SimHandle, until: impl Fn(&Frame) -> bool) -> Frame {
        frames_until(h, until).pop().expect("кадр есть")
    }

    fn cfg() -> WorldConfig {
        WorldConfig { seed: 3, ..Default::default() }
    }

    fn paused(cfg: WorldConfig) -> SimHandle {
        let h = SimHandle::spawn(cfg, Box::new(|| {}));
        h.send(Command::SetPaused(true));
        h.send(Command::Restart); // с тика 0: до паузы мир мог успеть шагнуть
        h
    }

    #[test]
    fn шаг_на_паузе_ровно_один_тик() {
        let h = paused(cfg());
        wait_frame(&h, |f| f.status.paused && f.world_gen == 1);
        h.send(Command::Step);
        h.send(Command::Step);
        let f = wait_frame(&h, |f| f.tick >= 2);
        assert_eq!(f.tick, 2);
    }

    #[test]
    fn заново_повторяет_партию() {
        let h = paused(cfg());
        let mut runs = Vec::new();
        for world_gen in 2..=3 {
            // команды применяются по порядку: шаги идут уже в новом мире
            h.send(Command::Restart);
            (0..50).for_each(|_| h.send(Command::Step));
            runs.push(wait_frame(&h, |f| f.world_gen == world_gen && f.tick == 50));
        }
        let (a, b) = (&runs[0], &runs[1]);
        assert_eq!((a.plants, a.creatures), (b.plants, b.creatures));
        // и с тем же движком без окна
        let mut w = World::new(&cfg());
        (0..50).for_each(|_| w.step());
        assert_eq!((w.plants.len(), w.creatures.len()), (a.plants, a.creatures));
    }

    #[test]
    fn мир_без_жизни_заканчивает_партию() {
        let empty = WorldConfig { n_creatures: Some(0), ..cfg() };
        let h = SimHandle::spawn(empty, Box::new(|| {}));
        let f = wait_frame(&h, |f| f.status.ended.is_some());
        assert_eq!(f.status.ended, Some(Ending::Extinct));
    }

    #[test]
    fn видимая_область_даёт_кружки_и_миникарту() {
        let h = SimHandle::spawn(cfg(), Box::new(|| {}));
        h.send(Command::View(ViewRequest { x0: 0.0, y0: 0.0, x1: 6000.0, y1: 4000.0, px_w: 900, px_h: 600 }));
        let f = wait_frame(&h, |f| !f.instances.is_empty());
        assert!(f.instances.len() >= 20, "20 существ на старте");
        assert!(f.density.is_none());
    }

    #[test]
    fn приращения_истории_и_хроники_не_теряются() {
        let h = paused(cfg());
        let ticks = 1200;
        (0..ticks).for_each(|_| h.send(Command::Step));
        let frames = frames_until(&h, |f| f.world_gen == 1 && f.tick == ticks);
        let fresh: Vec<&Frame> = frames.iter().filter(|f| f.world_gen == 1).collect();
        let samples: Vec<u64> = fresh.iter().flat_map(|f| f.samples.iter().map(|s| s.tick)).collect();
        let expected: Vec<u64> = (0..=ticks).step_by(GRAPH_EVERY as usize).collect();
        assert_eq!(samples, expected, "точка графика на каждый GRAPH_EVERY-й тик, без пропусков");
        let genes: Vec<u64> = fresh.iter().flat_map(|f| f.snapshots.iter().map(|s| s.tick)).collect();
        assert_eq!(genes.first(), Some(&0));
        assert!(genes.windows(2).all(|w| w[1] > w[0] && (w[1] - w[0]).is_multiple_of(SNAPSHOT_EVERY)));

        // хроника та же, что у отчёта на тех же срезах
        let log: Vec<String> = fresh
            .iter()
            .flat_map(|f| f.log.iter().filter(|e| e.kind.is_some()).map(|e| e.text.clone()))
            .collect();
        let mut w = World::new(&cfg());
        let mut snaps = vec![Snapshot::of(&w)];
        for _ in 0..ticks {
            w.step();
            // срезы игры — на тех же тиках, что точки генома (существа в этом сиде живы)
            if genes.contains(&w.tick) {
                snaps.push(Snapshot::of(&w));
            }
        }
        let report: Vec<String> = life_sim::observe::events(&snaps).into_iter().map(|e| e.text).collect();
        assert_eq!(log, report);
    }

    #[test]
    fn выбранное_существо_видно_в_кадре_и_погибает_с_записью() {
        let h = paused(cfg());
        wait_frame(&h, |f| f.world_gen == 1);
        // первое существо мира — id 1; его координаты узнаем из кадра выбора
        h.send(Command::Select(Some(1)));
        let f = wait_frame(&h, |f| f.selected.is_some());
        let s = f.selected.unwrap();
        assert_eq!(s.id, 1);
        // клик в пустоту снимает выбор
        h.send(Command::Pick { x: -1e6, y: -1e6, radius: 1.0 });
        wait_frame(&h, |f| f.selected.is_none());
        // клик точно в центр — выбирает
        h.send(Command::Pick { x: s.x, y: s.y, radius: 1.0 });
        let f = wait_frame(&h, |f| f.selected.is_some());
        assert_eq!(f.selected.unwrap().id, 1);
    }

    #[test]
    fn правила_на_ходу_и_подсадка_пишутся_в_хронику() {
        let h = paused(cfg());
        wait_frame(&h, |f| f.world_gen == 1);
        let rules = Rules::default().with("plant_energy", 80.0).unwrap();
        h.send(Command::SetRules {
            rules: rules.clone(), note: "энергия растения 50 → 80".into()
        });
        h.send(Command::Spawn { x: 3000.0, y: 2000.0 });
        let frames = frames_until(&h, |f| f.creatures == 21);
        let f = frames.last().unwrap();
        assert_eq!(f.rules, rules);
        let log: Vec<&str> = frames.iter().flat_map(|f| f.log.iter().map(|e| e.text.as_str())).collect();
        assert!(log.contains(&"энергия растения 50 → 80") && log.contains(&"подсажено существо"), "{log:?}");
    }
}
