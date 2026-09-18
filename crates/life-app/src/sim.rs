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

use life_core::config::{DIVIDE_PERIOD, VEGETARIAN_BASE_GENOM};
use life_core::{Creature, Genom, Rules, World, WorldConfig};
use life_sim::observe::{EventTracker, Snapshot};

use crate::frame::{self, Ending, Frame, Instance, LogEntry, Raster, Selected, Status, ViewRequest};
use crate::history::{GenePoint, Sample};

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
/// 60 т/с — как один тик за кадр в Python-версии.
pub const DEFAULT_SPEED: usize = 2;

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
    TogglePause,
    SetPaused(bool),
    /// Один тик — только на паузе.
    Step,
    SetSpeed(usize),
    View(ViewRequest),
    /// Выбрать существо у точки мира (клик): ближайшее, до края тела которого
    /// не дальше `radius`. Мимо — выбор снимается.
    Pick {
        x: f64,
        y: f64,
        radius: f64,
    },
    Select(Option<Creature>),
    /// Новые правила посреди партии; `note` — что поменялось, для хроники.
    SetRules {
        rules: Rules,
        note: String,
    },
    /// Подсадить базовое травоядное или хищника в точку мира.
    Spawn {
        predator: bool,
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
    gene_points: Vec<GenePoint>,
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
    selected: Option<Creature>,

    // ── наблюдение ──────────────────────────────────────────────────────────
    /// Численности последних `DIVIDE_PERIOD` тиков — для сглаженной точки графика.
    window: VecDeque<[usize; 3]>,
    tracker: EventTracker,
    snapshot_every: u64,
    next_snapshot: u64,
    migrants: u64,
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

    // ── кадры ───────────────────────────────────────────────────────────────
    /// Мир изменился с прошлого кадра.
    dirty: bool,
    last_frame: Instant,
    frame_interval: Duration,
    /// Окно не забрало прошлый кадр (например, свёрнуто): не крутимся вхолостую.
    blocked: bool,
    last_minimap: Option<Instant>,
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
            window: VecDeque::new(),
            tracker: EventTracker::new(),
            snapshot_every: SNAPSHOT_EVERY,
            next_snapshot: 0,
            migrants: 0,
            pending: Pending::default(),
            due: 0.0,
            last_time: now,
            tps: 0.0,
            tps_ticks: 0,
            tps_since: now,
            lagging: false,
            tick_ms: 0.0,
            dirty: true,
            last_frame: now - MIN_FRAME_INTERVAL,
            frame_interval: MIN_FRAME_INTERVAL,
            blocked: false,
            last_minimap: None,
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
            Command::View(v) => {
                if self.view != Some(v) {
                    self.view = Some(v);
                    self.dirty = true;
                }
            }
            Command::Pick { x, y, radius } => {
                self.selected = self.world.pick(x, y, radius);
                self.dirty = true;
            }
            Command::Select(c) => {
                self.selected = c;
                self.dirty = true;
            }
            Command::SetRules { rules, note } => {
                self.world.set_rules(rules);
                self.log(None, note);
            }
            Command::Spawn { predator, x, y } => {
                let text = if predator {
                    self.world.spawn_predator(x, y, None);
                    "подсажен хищник"
                } else {
                    self.world.spawn_vegetarian(Genom::from_array(VEGETARIAN_BASE_GENOM), x, y, None);
                    "подсажено травоядное"
                };
                self.log(None, text.into());
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
        self.due = 0.0;
        self.last_time = Instant::now();
        self.last_minimap = None;
        self.tick_ms = 0.0;
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
        self.migrants = self.world.migrants;
        self.record();
        self.snapshot();
    }

    fn tick(&mut self) {
        let start = Instant::now();
        self.world.step();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.tick_ms = if self.tick_ms == 0.0 { ms } else { self.tick_ms * 0.95 + ms * 0.05 };
        self.tps_ticks += 1;
        self.dirty = true;

        self.record();
        if self.world.tick >= self.next_snapshot {
            self.snapshot();
        }
        if self.world.migrants != self.migrants {
            let n = self.world.migrants - self.migrants;
            self.migrants = self.world.migrants;
            self.log(None, format!("с края мира пришли хищники-мигранты: {n}"));
        }
        if let Some(c) = self.selected
            && Selected::of(&self.world, c).is_none()
        {
            self.selected = None;
            let who =
                if matches!(c, Creature::Vegetarian(_)) { "травоядное" } else { "хищник" };
            self.log(None, format!("выбранное {who} погибло"));
        }

        let (veg, pred) = (self.world.vegetarians.len(), self.world.predators.len());
        if veg == 0 && pred == 0 {
            self.ended = Some(Ending::Extinct);
        } else if self.watch_explosion && veg + pred > self.world.space.per_area(EXPLOSION_LIMIT) {
            self.ended = Some(Ending::Explosion);
        }
    }

    /// Каждый тик — численности в окно сглаживания; раз в `GRAPH_EVERY` — точка графика.
    fn record(&mut self) {
        let w = &self.world;
        if self.window.len() == DIVIDE_PERIOD as usize {
            self.window.pop_front();
        }
        self.window.push_back([w.plants.len(), w.vegetarians.len(), w.predators.len()]);
        if !w.tick.is_multiple_of(GRAPH_EVERY) {
            return;
        }
        let n = self.window.len() as f64;
        let avg = |k: usize| self.window.iter().map(|c| c[k] as f64).sum::<f64>() / n;
        let stats = w.stats();
        self.pending.samples.push(Sample {
            tick: w.tick,
            plants: avg(0),
            vegetarians: avg(1),
            predators: avg(2),
            genom: stats.avg_genom,
        });
    }

    /// Срез мира: хроника и график генома. На большом мире срез дорог, и
    /// интервал растёт, чтобы наблюдение не отнимало время у тиков.
    fn snapshot(&mut self) {
        let start = Instant::now();
        let snap = Snapshot::of(&self.world);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
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
        if let Some(genes) = snap.genes {
            self.pending.gene_points.push(GenePoint { tick: snap.tick, genes });
        }
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
        if let Some(view) = self.view {
            let rect = view.padded();
            origin = (rect.0, rect.1);
            if !frame::collect_instances(w, rect, &mut instances) {
                instances.clear();
                // Карта плотности ровно по видимой области, клетка — пара пикселей.
                let (dw, dh) =
                    ((view.px_w as usize / 2).clamp(1, 1024), (view.px_h as usize / 2).clamp(1, 1024));
                density =
                    Some(frame::density(w, (view.x0, view.y0, view.x1, view.y1), dw, dh, Raster::default()));
            }
        }
        let minimap = if self.last_minimap.is_none_or(|t| t.elapsed() >= MINIMAP_INTERVAL) {
            self.last_minimap = Some(Instant::now());
            let (mw, mh) = frame::minimap_size(w.space.width, w.space.height);
            Some(frame::density(w, (0.0, 0.0, w.space.width, w.space.height), mw, mh, Raster::default()))
        } else {
            None
        };
        let pending = std::mem::take(&mut self.pending);
        Frame {
            world_gen: self.world_gen,
            seed: self.cfg.seed,
            scale: self.cfg.scale,
            rules: w.rules.clone(),
            tick: w.tick,
            plants: w.plants.len(),
            vegetarians: w.vegetarians.len(),
            predators: w.predators.len(),
            world_w: w.space.width,
            world_h: w.space.height,
            status: Status {
                paused: self.paused,
                speed_index: self.speed_index,
                tps: self.tps,
                lagging: self.lagging,
                ended: self.ended,
            },
            origin,
            instances,
            density,
            minimap,
            selected: self.selected.and_then(|c| Selected::of(w, c)),
            samples: pending.samples,
            gene_points: pending.gene_points,
            log: pending.log,
            build_ms: start.elapsed().as_secs_f64() * 1000.0,
            tick_ms: self.tick_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!((a.plants, a.vegetarians, a.predators), (b.plants, b.vegetarians, b.predators));
        // и с тем же движком без окна
        let mut w = World::new(&cfg());
        (0..50).for_each(|_| w.step());
        assert_eq!((w.plants.len(), w.vegetarians.len()), (a.plants, a.vegetarians));
    }

    #[test]
    fn мир_без_жизни_заканчивает_партию() {
        let empty = WorldConfig { n_vegetarians: Some(0), n_predators: Some(0), ..cfg() };
        let h = SimHandle::spawn(empty, Box::new(|| {}));
        let f = wait_frame(&h, |f| f.status.ended.is_some());
        assert_eq!(f.status.ended, Some(Ending::Extinct));
    }

    #[test]
    fn видимая_область_даёт_кружки_и_миникарту() {
        let h = SimHandle::spawn(cfg(), Box::new(|| {}));
        h.send(Command::View(ViewRequest { x0: 0.0, y0: 0.0, x1: 6000.0, y1: 4000.0, px_w: 900, px_h: 600 }));
        let f = wait_frame(&h, |f| !f.instances.is_empty());
        assert!(f.instances.len() >= 26, "20 травоядных и 6 хищников на старте");
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
        let genes: Vec<u64> = fresh.iter().flat_map(|f| f.gene_points.iter().map(|g| g.tick)).collect();
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
            // срезы игры — на тех же тиках, что точки генома (травоядные в этом сиде живы)
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
        // первое травоядное мира — id 1; его координаты узнаем из кадра выбора
        h.send(Command::Select(Some(Creature::Vegetarian(1))));
        let f = wait_frame(&h, |f| f.selected.is_some());
        let s = f.selected.unwrap();
        assert_eq!(s.creature, Creature::Vegetarian(1));
        assert!(s.genom.is_some() && s.layer.is_some());
        // клик в пустоту снимает выбор
        h.send(Command::Pick { x: -1e6, y: -1e6, radius: 1.0 });
        wait_frame(&h, |f| f.selected.is_none());
        // клик точно в центр — выбирает
        h.send(Command::Pick { x: s.x, y: s.y, radius: 1.0 });
        let f = wait_frame(&h, |f| f.selected.is_some());
        assert_eq!(f.selected.unwrap().creature, Creature::Vegetarian(1));
    }

    #[test]
    fn правила_на_ходу_и_подсадка_пишутся_в_хронику() {
        let h = paused(cfg());
        wait_frame(&h, |f| f.world_gen == 1);
        let rules = Rules::default().with("plant_energy", 80.0).unwrap();
        h.send(Command::SetRules {
            rules: rules.clone(), note: "энергия растения 50 → 80".into()
        });
        h.send(Command::Spawn { predator: true, x: 3000.0, y: 2000.0 });
        let frames = frames_until(&h, |f| f.predators == 7);
        let f = frames.last().unwrap();
        assert_eq!(f.rules, rules);
        let log: Vec<&str> = frames.iter().flat_map(|f| f.log.iter().map(|e| e.text.as_str())).collect();
        assert!(log.contains(&"энергия растения 50 → 80") && log.contains(&"подсажен хищник"), "{log:?}");
    }
}
