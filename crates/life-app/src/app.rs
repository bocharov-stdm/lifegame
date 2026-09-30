//! The application: screens, transitions, settings. The window only draws the last frame and
//! sends commands; everything heavy is in the simulation thread (`sim.rs`).

use std::collections::HashSet;
use std::path::PathBuf;

use eframe::egui;
use life_core::WorldConfig;
use life_core::genome::creature::Gene;

use crate::census::Census;
use crate::frame::{LogEntry, RegionStats};
use crate::history::History;
use crate::settings::{self, Settings, Tab};
use crate::sim::{Command, SimHandle};
use crate::stats::StatsTab;
use crate::theme;
use crate::view::WorldView;

/// We keep no more chronicle entries: the old ones go.
const LOG_LIMIT: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Setup,
    Game,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideTab {
    Charts,
    Log,
    Creature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Spawn,
    /// Drag a region and look at the genome of those inside.
    Area,
}

/// A game under way (not the menu's background world).
pub struct Game {
    /// What the game began with: «Заново» repeats exactly that.
    pub start: WorldConfig,
    /// The rules were changed on the fly: a repeat without a window matches only up to the change.
    pub rules_changed: bool,
}

pub struct LifeApp {
    pub sim: SimHandle,
    pub settings: Settings,
    pub settings_path: Option<PathBuf>,
    pub screen: Screen,
    pub game: Option<Game>,
    pub view: WorldView,
    pub history: History,
    pub log: Vec<LogEntry>,

    // ── interface state ─────────────────────────────────────────────────────
    pub side_open: bool,
    pub side_tab: SideTab,
    /// Which diets have their details open on the panel, in `Diet` order.
    pub diet_open: [bool; 4],
    /// Whether to show the bodies and the surroundings of the world; the history and the card update always.
    pub render_world: bool,
    pub lab_open: bool,
    /// The selected creature's behaviour window (`behaviour.rs`).
    pub behaviour_open: bool,
    /// A draft of the lab's rules on the fly; applied with a button.
    pub lab: Settings,
    /// The lab's tab: rules (`Tab::Lab`) or food (`Tab::Food`).
    pub lab_tab: Tab,
    /// The rules ticked for a joint reset in the lab.
    pub lab_reset_selected: HashSet<settings::Key>,
    /// The «Статистика» window and its tab.
    pub stats_open: bool,
    pub stats_tab: StatsTab,
    /// The last summary of the dragged region; None — there is no region.
    pub region: Option<RegionStats>,
    /// The «Внутри видов» tab: the last census, the (world, tick) it was asked for, the group
    /// looked at (0 everybody, then the diets) and the two characteristics of its scatter.
    pub census: Option<Census>,
    pub census_asked: Option<(u64, u64)>,
    pub census_group: usize,
    pub census_axes: [usize; 2],
    pub tool: Tool,
    pub setup_tab: Tab,
    pub prefs_open: bool,
    pub help_open: bool,
    /// Whether the game was paused when the menu was opened.
    pub paused_before_menu: bool,
    pub toast: Option<(String, f64)>,
    fps: f64,
    applied: (bool, f64),
    /// The threads and fast cores the simulation was last told (`Command::Threads`).
    applied_threads: Option<(usize, bool)>,
}

impl LifeApp {
    /// `start` — a world from the command line: straight into the game. Otherwise the menu with a
    /// live ×1 world in the background. `settings_path` — the settings file; None — neither read
    /// nor write (the tests must not touch the player's settings).
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        start: Option<WorldConfig>,
        settings_path: Option<PathBuf>,
    ) -> Self {
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            crate::render::init(rs);
        }
        theme::apply(&cc.egui_ctx);
        let settings = settings_path.as_deref().map(Settings::load).unwrap_or_default();

        let (cfg, screen, game) = match start {
            Some(cfg) => (cfg.clone(), Screen::Game, Some(Game { start: cfg, rules_changed: false })),
            None => {
                let demo = Settings { scale: 1.0, ..settings.clone() };
                (demo.world_config(random_seed()), Screen::Menu, None)
            }
        };
        let ctx = cc.egui_ctx.clone();
        let sim = SimHandle::spawn(cfg, Box::new(move || ctx.request_repaint()));
        LifeApp {
            sim,
            lab: settings.clone(),
            settings,
            settings_path,
            screen,
            game,
            view: WorldView::default(),
            history: History::default(),
            log: Vec::new(),
            side_open: true,
            side_tab: SideTab::Charts,
            diet_open: [false; 4],
            render_world: true,
            lab_open: false,
            behaviour_open: false,
            lab_tab: Tab::Food,
            lab_reset_selected: HashSet::new(),
            stats_open: false,
            stats_tab: StatsTab::Energy,
            region: None,
            census: None,
            census_asked: None,
            census_group: 0,
            census_axes: [Gene::Size as usize, Gene::Speed as usize],
            tool: Tool::Select,
            setup_tab: Tab::World,
            prefs_open: false,
            help_open: false,
            paused_before_menu: false,
            toast: None,
            fps: 0.0,
            applied: (false, -1.0),
            applied_threads: None,
        }
    }

    /// Take a new frame from the simulation thread, if there is one.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.sim.take_frame() else { return };
        if self.view.frame.as_ref().is_none_or(|old| old.world_gen != f.world_gen) {
            self.history = History::default();
            self.log.clear();
            self.lab.take_rules(&f.rules);
            self.lab_reset_selected.clear();
            // the region is from the previous world; the thread has already forgotten it
            self.region = None;
            self.view.area = None;
            self.census = None;
            self.census_asked = None;
        }
        if let Some(r) = f.region.take().filter(|r| self.view.area == Some(r.area)) {
            self.region = Some(r);
        }
        if let Some(c) = f.census.take() {
            self.census = Some(c);
        }
        for s in f.samples.drain(..) {
            self.history.add_sample(s);
        }
        for s in f.snapshots.drain(..) {
            self.history.add_snapshot(s);
        }
        self.log.append(&mut f.log);
        if self.log.len() > LOG_LIMIT {
            self.log.drain(..self.log.len() - LOG_LIMIT);
        }
        self.view.accept(ctx, &self.sim, f);
    }

    pub fn save_settings(&mut self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.toast(format!("настройки не сохранились: {e}"));
        }
    }

    pub fn toast(&mut self, text: String) {
        self.toast = Some((text, 3.0));
    }

    /// Start a new game by the settings of the «Новый мир» screen.
    pub fn start_game(&mut self) {
        if self.settings.random_seed {
            self.settings.seed = random_seed();
        }
        let cfg = self.settings.world_config(self.settings.seed);
        self.save_settings();
        self.sim.send(Command::NewWorld(cfg.clone()));
        self.sim.send(Command::SetPaused(false));
        self.game = Some(Game { start: cfg, rules_changed: false });
        self.lab = self.settings.clone();
        self.tool = Tool::Select;
        self.screen = Screen::Game;
    }

    pub fn restart(&mut self) {
        self.sim.send(Command::Restart);
        if let Some(g) = &mut self.game {
            g.rules_changed = false;
        }
    }

    pub fn open_menu(&mut self) {
        self.paused_before_menu = self.view.frame.as_ref().is_some_and(|f| f.status.paused);
        if self.game.is_some() {
            self.sim.send(Command::SetPaused(true));
        }
        self.lab_open = false;
        self.screen = Screen::Menu;
    }

    pub fn resume(&mut self) {
        if self.game.is_some() {
            self.sim.send(Command::SetPaused(self.paused_before_menu));
            self.screen = Screen::Game;
        }
    }

    /// The interface scale and full-screen mode — when they have changed.
    fn apply_display(&mut self, ctx: &egui::Context) {
        let want = (self.settings.fullscreen, self.settings.ui_scale);
        if want == self.applied {
            return;
        }
        if want.0 != self.applied.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(want.0));
        }
        // 0 — as in the system: a factor of 1 to the system scale
        ctx.set_zoom_factor(if want.1 > 0.0 { want.1 as f32 } else { 1.0 });
        self.applied = want;
    }

    /// The settings' threads and fast cores to the simulation — when they have changed.
    fn apply_threads(&mut self) {
        let want = (self.settings.threads_in_use(), self.settings.fast_cores);
        if self.applied_threads != Some(want) {
            self.sim.send(Command::Threads { threads: want.0, fast_cores: want.1 });
            self.applied_threads = Some(want);
        }
    }

    pub fn fps(&self) -> f64 {
        self.fps
    }
}

/// A random seed from the clock: the engine needs no shared generator.
pub fn random_seed() -> u64 {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    (life_core::rng::mix(nanos as u64) % settings::SEED_MAX) + 1
}

impl eframe::App for LifeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_display(&ctx);
        self.apply_threads();
        self.receive(&ctx);
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + 0.1 / dt;
        }
        if let Some((_, left)) = &mut self.toast {
            *left -= dt;
            if *left <= 0.0 {
                self.toast = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        match self.screen {
            Screen::Game => self.game_screen(ui),
            Screen::Menu => self.menu_screen(ui),
            Screen::Setup => self.setup_screen(ui),
        }
        self.prefs_window(&ctx);
        self.help_window(&ctx);

        if let Some((text, _)) = &self.toast {
            egui::Area::new("тост".into()).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -60.0)).show(
                &ctx,
                |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text.as_str()));
                },
            );
        }
        if !self.sim.is_alive() {
            egui::Window::new("Симуляция остановилась").collapsible(false).show(&ctx, |ui| {
                ui.label("Поток симуляции завершился с ошибкой. Подробности — в консоли.");
            });
        }
        if self.screen != Screen::Game {
            return;
        }
        // following runs every window frame, not only when a world frame has come
        self.view.follow_step(dt);
        if self.view.following() {
            ctx.request_repaint();
        }
    }
}
