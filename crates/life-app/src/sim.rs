//! The simulation thread. `World` lives only here: the window sends commands and takes ready
//! frames, but never waits for a tick. The successor of `app/session.py`.
//!
//! Frames follow the «last one wins» principle: the thread puts a frame in the slot only when
//! the window has taken the previous one — there is no queue, and a slow window does not pile up
//! frames, and a slow tick does not hold the window. The window hands the buffers of circles
//! back, so as not to allocate memory for every frame.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use life_core::config::DIVIDE_PERIOD;
use life_core::par::Threads;
use life_core::profile::{Phase, PhaseTimes};
use life_core::{CreatureGenome, Rules, World, WorldConfig};
use life_sim::observe::{EventTracker, Snapshot};

use crate::census::Census;
use crate::frame::{
    self, Area, CorpseMark, Ending, Frame, Instance, LogEntry, Raster, RegionStats, Selected, ShotTrail,
    Status, ViewRequest,
};
use crate::history::Sample;
use crate::motion::Motion;

/// The speeds, ticks a second; None — «maximum», as many as the processor manages.
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
/// A calm start: 30 t/s; a speed-up is available on the top panel.
pub const DEFAULT_SPEED: usize = 1;

/// Creatures per base area after which the game stops («a population explosion»). Like
/// `EXPLOSION_LIMIT` in Python; grows with the area.
pub const EXPLOSION_LIMIT: usize = 3000;

/// A point of the population chart — once in this many ticks.
pub const GRAPH_EVERY: u64 = 10;
/// A sample for the chronicle and the genome chart — no oftener than once in this many ticks
/// (a multiple of the division period, as in the report).
pub const SNAPSHOT_EVERY: u64 = 60;
/// A sample computes the genes' percentiles (a sort), and on a huge world it is dear. The
/// interval grows so that the samples eat no more than this share of the ticks' time.
const SNAPSHOT_SHARE: f64 = 0.05;

/// Ticks without a breather go on no longer than this: then the thread looks at the commands
/// and hands out a frame. Otherwise at «maximum» a pause would be pressed with a delay.
const SLICE: Duration = Duration::from_millis(8);
/// Frames no oftener than the screen needs.
const MIN_FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / 120);
/// The minimap updates a couple of times a second: it is enough.
const MINIMAP_INTERVAL: Duration = Duration::from_millis(400);
/// The window over which the actual tempo is measured.
const TPS_WINDOW: Duration = Duration::from_millis(500);

pub enum Command {
    #[cfg(test)]
    TestWorld(Box<World>),
    TogglePause,
    SetPaused(bool),
    /// One tick — only on pause.
    Step,
    SetSpeed(usize),
    /// Do not build the frame's graphical content, keeping the statistics and the control.
    RenderWorld(bool),
    View(ViewRequest),
    /// Pick a creature at a world point (a click): the nearest one, no farther than `radius` from
    /// the edge of its body. A miss — the selection is dropped.
    Pick {
        x: f64,
        y: f64,
        radius: f64,
    },
    /// Select a creature by number.
    Select(Option<u64>),
    /// A region for the genes' summary (the «Область» tool); None — clear.
    SetRegion(Option<Area>),
    /// A census of the creatures (the «Внутри видов» tab) — taken only while the world stands.
    Census,
    /// Threads for the creatures' decisions (1 — none but its own) and whether the simulation
    /// thread keeps to the fast cores. The world goes the same either way.
    Threads {
        threads: usize,
        fast_cores: bool,
    },
    /// New rules in the middle of a game; `note` — what has changed, for the chronicle.
    SetRules {
        rules: Rules,
        note: String,
    },
    /// Plant a base creature at a world point.
    Spawn {
        x: f64,
        y: f64,
    },
    /// The same game from the start: the same seed and the same starting rules.
    Restart,
    /// «A population explosion» — go on anyway; we no longer stop.
    KeepGoing,
    NewWorld(WorldConfig),
    Quit,
}

/// What the simulation thread does with frames after publishing: wakes the window.
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
        // the thread may have crashed; the window must not crash because of it
        let _ = self.tx.send(cmd);
    }

    /// The last ready frame, if a new one has come.
    pub fn take_frame(&self) -> Option<Frame> {
        self.slot.lock().ok()?.take()
    }

    /// Return the circle buffer from a drawn frame.
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

/// Everything that piles up between frames and goes into a frame as an increment.
#[derive(Default)]
struct Pending {
    samples: Vec<Sample>,
    snapshots: Vec<Snapshot>,
    region: Option<RegionStats>,
    census: Option<Census>,
    log: Vec<LogEntry>,
}

struct Sim {
    cfg: WorldConfig,
    world: World,
    world_gen: u64,
    /// `Frame::edits`.
    edits: u64,
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
    region: Option<Area>,
    render_world: bool,
    dots: bool,

    // ── observation ─────────────────────────────────────────────────────────
    /// Plants, creatures and creatures by diet over the last `DIVIDE_PERIOD` ticks: a smoothed point.
    window: VecDeque<[usize; 6]>,
    tracker: EventTracker,
    snapshot_every: u64,
    next_snapshot: u64,
    pending: Pending,

    // ── tempo ───────────────────────────────────────────────────────────────
    /// How many ticks are «owed» to the chosen speed.
    due: f64,
    last_time: Instant,
    tps: f64,
    tps_ticks: u64,
    tps_since: Instant,
    lagging: bool,
    /// The mean price of a tick, ms (a moving average).
    tick_ms: f64,
    /// Each phase's share of a tick, smoothed like `tick_ms`, and the world's times it was last
    /// read at.
    phases: [f64; Phase::N],
    phases_seen: PhaseTimes,
    /// Where the decisions run (`Command::Threads`), how many threads that is, and whether this
    /// thread keeps to the fast cores.
    pool: Threads,
    threads: usize,
    fast_cores: bool,
    /// The price of the last observer snapshot, ms.
    snapshot_ms: f64,

    // ── frames ──────────────────────────────────────────────────────────────
    /// The world has changed since the previous frame.
    dirty: bool,
    last_frame: Instant,
    frame_interval: Duration,
    /// The window has not taken the previous frame (for example, it is minimised): we do not spin idle.
    blocked: bool,
    last_minimap: Option<Instant>,
    /// The memory of the previous frame: motion, births, ghosts.
    motion: Motion,
    recent_shots: VecDeque<(ShotTrail, Instant)>,
    /// Tick of the last built frame: sinking corpses are drawn from where they lay then.
    last_frame_tick: u64,
    /// The food patches changed since the last frame: a new world or new rules.
    patches_due: bool,
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
            edits: 0,
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
            phases: [0.0; Phase::N],
            phases_seen: PhaseTimes::default(),
            pool: Threads::One,
            threads: 1,
            fast_cores: false,
            snapshot_ms: 0.0,
            dirty: true,
            last_frame: now - MIN_FRAME_INTERVAL,
            frame_interval: MIN_FRAME_INTERVAL,
            blocked: false,
            last_minimap: None,
            motion: Motion::default(),
            recent_shots: VecDeque::new(),
            last_frame_tick: 0,
            patches_due: true,
        };
        // the settings' defaults until the window sends its own
        sim.set_threads(crate::settings::auto_threads(), true);
        sim.observe_start();
        sim
    }

    /// A pool of `threads` for the decisions (1: none), and this thread on the fast cores or
    /// anywhere. Runs on the simulation thread itself, so it is the one held.
    fn set_threads(&mut self, threads: usize, fast_cores: bool) {
        let threads = threads.clamp(1, crate::settings::cpu_threads());
        if threads != self.threads {
            let pool = (threads > 1).then(|| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|i| format!("расчёт {i}"))
                    .build()
            });
            (self.pool, self.threads) = match pool {
                Some(Ok(pool)) => (Threads::Pool(Arc::new(pool)), threads),
                // no pool (one thread asked, or the system gave none): the decisions go on here
                _ => (Threads::One, 1),
            };
        }
        self.fast_cores = life_sim::cores::keep_on_fast_cores(fast_cores) && fast_cores;
        self.dirty = true;
    }

    fn running(&self) -> bool {
        !self.paused && self.ended.is_none()
    }

    fn target_tps(&self) -> Option<f64> {
        SPEEDS[self.speed_index]
    }

    fn run(mut self) {
        loop {
            // ── commands: we wait for them only if there is nothing else to do ───
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

    /// How long we may sleep until the next job: a scheduled tick or a frame.
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

    /// false — time to exit.
    fn apply(&mut self, cmd: Command) -> bool {
        match cmd {
            #[cfg(test)]
            Command::TestWorld(world) => {
                self.world = *world;
                self.patches_due = true;
                self.recent_shots.clear();
                for shot in &self.world.shots {
                    self.recent_shots
                        .push_back((ShotTrail { from: shot.from, to: shot.to, age: 0.0 }, Instant::now()));
                }
                self.world_gen += 1;
                self.selected = None;
                self.paused = true;
                self.ended = None;
                self.motion = Motion::default();
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
            Command::RenderWorld(enabled) => {
                if self.render_world != enabled {
                    self.motion = Motion::default();
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
                self.selected = self.world.pick(x, y, 0.0).or_else(|| self.world.pick(x, y, radius));
                self.dirty = true;
            }
            Command::Select(c) => {
                self.selected = c;
                self.dirty = true;
            }
            Command::SetRegion(area) => {
                self.region = area;
                // at once, not at the next sample: there are no samples on pause
                self.pending.region = area.map(|a| RegionStats::of(&self.world, a, None));
                self.dirty = true;
            }
            Command::Threads { threads, fast_cores } => self.set_threads(threads, fast_cores),
            Command::Census => {
                // a running world is never counted: the census would take time from the ticks
                if !self.running() {
                    self.pending.census = Some(Census::of(&self.world, self.world_gen, self.edits));
                    self.dirty = true;
                }
            }
            Command::SetRules { rules, note } => {
                self.world.set_rules(rules);
                self.edits += 1;
                self.patches_due = true;
                self.log(None, note);
            }
            Command::Spawn { x, y } => {
                self.world.spawn(CreatureGenome::BASE, x, y, None);
                self.edits += 1;
                self.log(None, "подсажено существо".into());
                // Planting into an extinct world revives it.
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
        // A big world takes a noticeable time to build — but in this thread, the window lives.
        self.world = World::new(&cfg);
        self.patches_due = true;
        self.cfg = cfg;
        self.world_gen += 1;
        self.ended = None;
        self.watch_explosion = true;
        self.selected = None;
        self.region = None;
        self.due = 0.0;
        self.last_time = Instant::now();
        self.last_minimap = None;
        self.motion = Motion::default();
        self.recent_shots.clear();
        self.tick_ms = 0.0;
        self.phases = [0.0; Phase::N];
        self.snapshot_ms = 0.0;
        self.reset_tps();
        self.pending = Pending::default();
        self.observe_start();
        self.dirty = true;
    }

    /// The start of observing a new world: the first chart point and the first sample.
    fn observe_start(&mut self) {
        self.window.clear();
        self.tracker = EventTracker::new();
        self.snapshot_every = SNAPSHOT_EVERY;
        self.record();
        self.snapshot();
    }

    fn tick(&mut self) {
        // the phases are measured always: a dozen clock reads a tick, and the world goes the same
        self.world.set_profiling(true);
        self.world.set_threads(self.pool.clone());
        let start = Instant::now();
        self.world.step();
        let engine_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.measure_phases();
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

    /// This tick's phases into their smoothed shares. A new world starts its times anew.
    fn measure_phases(&mut self) {
        let Some(now) = self.world.phase_times() else { return };
        if now.ticks < self.phases_seen.ticks {
            self.phases_seen = PhaseTimes::default();
        }
        let spent = |t: &PhaseTimes, p: Phase| t.nanos[p as usize];
        let total = now.total_nanos().saturating_sub(self.phases_seen.total_nanos());
        if total > 0 {
            let fresh = self.phases.iter().all(|&s| s == 0.0);
            for p in Phase::ALL {
                let share = spent(now, p).saturating_sub(spent(&self.phases_seen, p)) as f64 / total as f64;
                let s = &mut self.phases[p as usize];
                *s = if fresh { share } else { *s * 0.95 + share * 0.05 };
            }
        }
        self.phases_seen = now.clone();
    }

    /// Every tick — the counts into the smoothing window; once in `GRAPH_EVERY` — a chart point.
    fn record(&mut self) {
        let w = &self.world;
        if self.window.len() == DIVIDE_PERIOD as usize {
            self.window.pop_front();
        }
        let mut counts = [w.plants.len(), w.creatures.len(), 0, 0, 0, 0];
        for v in &w.creatures {
            counts[2 + v.pheno.diet as usize] += 1;
        }
        self.window.push_back(counts);
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
            diets: std::array::from_fn(|d| avg(2 + d)),
            shots: w.counters.ranged_shots,
            genom: stats.avg_genom,
        });
    }

    /// A sample of the world: the chronicle and the genome chart. On a big world a sample is dear,
    /// and the interval grows so that observing does not take time from the ticks.
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

    /// Ticks on schedule: no more than the speed is owed, and no longer than `SLICE`.
    fn advance(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64();
        self.last_time = now;
        if self.running() {
            if let Some(tps) = self.target_tps() {
                // The lag does not pile up: catching up seconds of ticks in a jerk is the same freeze, only
                // in
                // the simulation. The debt is no more than a tenth of a second.
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

    /// Hand out a frame if the window has taken the previous one and it is time.
    fn publish(&mut self) {
        if !self.dirty || self.last_frame.elapsed() < self.frame_interval {
            return;
        }
        let Ok(slot) = self.slot.lock() else { return };
        self.blocked = slot.is_some(); // the window has not yet taken the previous frame
        if self.blocked {
            return;
        }
        drop(slot);

        let start = Instant::now();
        let frame = self.build_frame();
        let build = start.elapsed();
        // Building a frame must not eat more than a third of the thread's time: on a huge world frames
        // just come less often, and ticks at the former speed.
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
        let patches = std::mem::take(&mut self.patches_due).then(|| Arc::from(self.world.flora().patches()));
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
                    self.motion = Motion::default();
                }
            } else if px_size <= 2.5 {
                self.dots = true;
            }
            let collected = if self.dots {
                frame::dots(w, rect, &mut instances)
            } else {
                self.motion.collect(w, rect, &mut instances)
            };
            if !collected {
                instances.clear();
                // The density map is exactly over the visible area, a cell is a couple of pixels.
                let (dw, dh) =
                    ((view.px_w as usize / 2).clamp(1, 1024), (view.px_h as usize / 2).clamp(1, 1024));
                density =
                    Some(frame::density(w, (view.x0, view.y0, view.x1, view.y1), dw, dh, Raster::default()));
            }
        } else {
            instances.clear();
        }
        let minimap =
            if self.render_world && self.last_minimap.is_none_or(|t| t.elapsed() >= MINIMAP_INTERVAL) {
                self.last_minimap = Some(Instant::now());
                let (mw, mh) = frame::minimap_size(w.space.width, w.space.height);
                Some(frame::density(w, (0.0, 0.0, w.space.width, w.space.height), mw, mh, Raster::default()))
            } else {
                None
            };
        let pending = std::mem::take(&mut self.pending);
        let prev_tick = self.last_frame_tick.min(w.tick);
        self.last_frame_tick = w.tick;
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
                            py: c.y_at(prev_tick.max(c.born)),
                            rot: c.stage(w.tick) == life_core::corpse::Stage::Rot,
                            size: c.size,
                            fullness: match c.skeleton {
                                Some(s) if s.store > 0.0 => (c.remaining / s.store).clamp(0.0, 1.0),
                                None if c.initial > 0.0 => (c.remaining / c.initial).clamp(0.0, 1.0),
                                _ => 0.0,
                            },
                            skeleton: c.skeleton.is_some(),
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
            edits: self.edits,
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
            patches,
            corpses,
            shots,
            density,
            minimap,
            selected: self.selected.and_then(|id| Selected::of(w, id)),
            samples: pending.samples,
            snapshots: pending.snapshots,
            region: pending.region,
            census: pending.census,
            log: pending.log,
            built: Some(Instant::now()),
            build_ms: start.elapsed().as_secs_f64() * 1000.0,
            tick_ms: self.tick_ms,
            snapshot_ms: self.snapshot_ms,
            phases: self.phases,
            threads: self.threads,
            fast_cores: self.fast_cores,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A diagnostic separating the price of a sample from the building of a graphical frame.
    /// Run by hand: the numbers depend on the machine and are no threshold of the test.
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

    /// The frames up to the first suitable one; the wait is limited: 500 attempts of 10 ms.
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
        h.send(Command::Restart); // from tick 0: before the pause the world might have managed a step
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

    /// A census is taken on pause, of that very tick, and never while the world runs.
    #[test]
    fn перепись_только_на_паузе() {
        let h = paused(cfg());
        (0..5).for_each(|_| h.send(Command::Step));
        h.send(Command::Census);
        let f = wait_frame(&h, |f| f.census.is_some());
        let c = f.census.as_ref().unwrap();
        assert_eq!((c.world_gen, c.tick, c.rows.len()), (f.world_gen, f.tick, f.creatures));
        h.send(Command::SetPaused(false));
        wait_frame(&h, |f| !f.status.paused);
        h.send(Command::Census);
        let running = frames_until(&h, |f| f.tick >= c.tick + 60);
        assert!(running.iter().all(|f| f.census.is_none()), "идущий мир не переписывают");
    }

    /// A creature planted on pause changes the world without a tick: the frame says so, and the
    /// census taken after it counts the new one.
    #[test]
    fn подсадка_на_паузе_требует_новой_переписи() {
        let h = paused(cfg());
        h.send(Command::Census);
        let before = wait_frame(&h, |f| f.census.is_some());
        let old = before.census.as_ref().unwrap();
        h.send(Command::Spawn { x: 1000.0, y: 1000.0 });
        let after = wait_frame(&h, |f| f.edits > before.edits);
        assert_eq!((after.tick, after.creatures), (before.tick, before.creatures + 1));
        h.send(Command::Census);
        let f = wait_frame(&h, |f| f.census.is_some());
        let c = f.census.as_ref().unwrap();
        assert_eq!((c.tick, c.edits), (old.tick, after.edits));
        assert_eq!(c.rows.len(), old.rows.len() + 1);
    }

    #[test]
    fn заново_повторяет_партию() {
        let h = paused(cfg());
        let mut runs = Vec::new();
        for world_gen in 2..=3 {
            // the commands apply in order: the steps already go in the new world
            h.send(Command::Restart);
            (0..50).for_each(|_| h.send(Command::Step));
            runs.push(wait_frame(&h, |f| f.world_gen == world_gen && f.tick == 50));
        }
        let (a, b) = (&runs[0], &runs[1]);
        assert_eq!((a.plants, a.creatures), (b.plants, b.creatures));
        // and with the same engine without a window
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

        // the chronicle is the report's on the same samples
        let log: Vec<String> = fresh
            .iter()
            .flat_map(|f| f.log.iter().filter(|e| e.kind.is_some()).map(|e| e.text.clone()))
            .collect();
        let mut w = World::new(&cfg());
        let mut snaps = vec![Snapshot::of(&w)];
        for _ in 0..ticks {
            w.step();
            // the game's samples are at the same ticks as the genome points (the creatures in this seed are
            // alive)
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
        // the world's first creature is id 1; we learn its coordinates from the selection frame
        h.send(Command::Select(Some(1)));
        let f = wait_frame(&h, |f| f.selected.is_some());
        let s = f.selected.unwrap();
        assert_eq!(s.id, 1);
        // a click into the void drops the selection
        h.send(Command::Pick { x: -1e6, y: -1e6, radius: 1.0 });
        wait_frame(&h, |f| f.selected.is_none());
        // a click exactly in the centre selects
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

    /// «Скорость расчёта» on the fly: the pool is rebuilt to the threads asked, never more than the
    /// processor has, and one thread is no pool at all; the world, split among however many and
    /// timed every tick, goes bit for bit as one thread steps it.
    #[test]
    fn threads_change_on_the_fly_and_the_world_goes_the_same() {
        let cfg = WorldConfig { seed: 3, scale: 10.0, shape: life_core::Shape::R2x1, ..Default::default() };
        let (_tx, rx) = mpsc::channel();
        let (_recycle_tx, recycled) = mpsc::channel();
        let mut sim = Sim::new(cfg.clone(), rx, recycled, Arc::new(Mutex::new(None)), Box::new(|| {}));
        let mut plain = World::new(&cfg);
        plain.set_threads(Threads::One);
        let cpus = crate::settings::cpu_threads();
        let mut most = 0;
        for (threads, expected) in [(3, 3.min(cpus)), (1, 1), (1000, cpus), (2, 2.min(cpus))] {
            sim.apply(Command::Threads { threads, fast_cores: false });
            assert_eq!(sim.threads, expected, "{threads} asked");
            assert_eq!(matches!(sim.pool, Threads::One), expected == 1, "{threads} asked");
            for _ in 0..75 {
                sim.tick();
                plain.step();
                most = most.max(sim.world.creatures.len());
            }
        }
        assert!(most > life_core::config::PARALLEL_MIN, "big enough to be split among the threads: {most}");
        assert!(sim.phases.iter().sum::<f64>() > 0.0, "the phases were timed");
        let state = |w: &World| format!("{} {:?} {:?} {:?}", w.tick, w.creatures, w.plants, w.corpses);
        assert!(state(&sim.world) == state(&plain), "the world differs from one thread's");
    }
}
