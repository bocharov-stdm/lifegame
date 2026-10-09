//! The application: screens, transitions, settings. The window only draws the last frame and
//! sends commands; everything heavy is in the simulation thread (`sim.rs`).

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use eframe::egui;
use life_core::WorldConfig;
use life_core::genome::creature::Gene;

use crate::census::Census;
use crate::frame::{LogEntry, Lost, RegionStats};
use crate::history::History;
use crate::settings::{self, Settings, Tab};
use crate::sim::{Command, SimHandle};
use crate::stats::StatsTab;
use crate::theme;
use crate::view::WorldView;

/// We keep no more chronicle entries: the old ones go.
pub const LOG_LIMIT: usize = 5000;

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

/// A short message at the bottom of the window. An error stays until it is closed.
pub struct Toast {
    pub text: String,
    /// Seconds left; None — an error, until closed.
    pub left: Option<f64>,
}

/// Messages shown at once; an older one goes first.
const TOASTS: usize = 3;

/// An action that loses what was there, asked about first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// «Заново»: the same game from its start.
    Restart,
    /// «Начать» while a game is under way: it is replaced.
    NewGame,
    /// «Сбросить вкладку» on «Новый мир».
    ResetTab(Tab),
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
    /// What the game began with: «Заново» repeats exactly that. Whether the world was changed on
    /// the fly (rules, a planted creature) the frame tells (`Frame::edits`): only the simulation
    /// thread knows whether it took an edit or refused it as built for a world replaced since.
    pub start: WorldConfig,
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
    /// The «Внутри видов» tab: the last census, the (world, tick, edits) it was asked for, the
    /// group looked at (0 everybody, then the diets) and the two characteristics of its scatter.
    pub census: Option<Census>,
    pub census_asked: Option<(u64, u64, u64)>,
    pub census_group: usize,
    pub census_axes: [usize; 2],
    pub tool: Tool,
    pub setup_tab: Tab,
    pub prefs_open: bool,
    pub help_open: bool,
    /// The keys' card over the game (F1, «?»).
    pub keys_open: bool,
    /// Whether the game was paused when the menu was opened.
    pub paused_before_menu: bool,
    pub toasts: VecDeque<Toast>,
    /// The selected creature that died, shown on the card until another is selected.
    pub lost: Option<Lost>,
    /// Window frames left for a click's pick to come back: a creature it selects opens the card.
    pub pick_pending: u32,
    /// Following was on when the selected one died: follow the next one selected.
    pub refollow: bool,
    /// A text field had the keyboard in the last frame: Esc, which egui takes the focus away with
    /// before a frame starts, ended the typing and must not also close or leave anything.
    pub was_editing: bool,
    /// The action waiting for «Да» in a modal question.
    pub confirm: Option<Confirm>,
    /// The ending's window was closed for this world (the world stays as it ended).
    pub ending_closed: bool,
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
            Some(cfg) => (cfg.clone(), Screen::Game, Some(Game { start: cfg })),
            None => {
                let seed = random_seed();
                let demo = |s: &Settings| Settings { scale: 1.0, ..s.clone() }.world_config(seed);
                // a settings file whose rules are refused shows the default world behind the menu
                let cfg =
                    demo(&settings).unwrap_or_else(|_| demo(&Settings::default()).expect("the defaults fit"));
                (cfg, Screen::Menu, None)
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
            keys_open: false,
            paused_before_menu: false,
            toasts: VecDeque::new(),
            lost: None,
            pick_pending: 0,
            refollow: false,
            was_editing: false,
            confirm: None,
            ending_closed: false,
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
            self.lost = None;
            self.pick_pending = 0;
            self.refollow = false;
            self.ending_closed = false;
        }
        let before = self.view.frame.as_ref().and_then(|f| f.selected).map(|s| s.id);
        let now = f.selected.map(|s| (s.id, s.x, s.y));
        if let Some(lost) = f.lost.take() {
            self.refollow = self.view.following();
            let diet = theme::DIET_NAMES[lost.diet.min(3)];
            self.toast(format!("№ {} ({diet}) погибло {}", lost.id, crate::sim::death_words(lost.cause)));
            self.lost = Some(lost);
        }
        self.pick_pending = self.pick_pending.saturating_sub(1);
        if let Some((id, x, y)) = now.filter(|n| Some(n.0) != before) {
            self.lost = None;
            // a click's pick opens the card; a selection made elsewhere (the card's own buttons)
            // keeps the tab the player is on
            if self.pick_pending > 0 {
                self.side_tab = SideTab::Creature;
                self.side_open = true;
                self.pick_pending = 0;
            }
            if std::mem::take(&mut self.refollow)
                && let Some(cam) = &mut self.view.camera
            {
                cam.follow(Some(id), Some((x, y)));
            }
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
            self.error(format!("настройки не сохранились: {e}"));
        }
    }

    /// A message for a few seconds.
    pub fn toast(&mut self, text: String) {
        self.push_toast(Toast { text, left: Some(4.0) });
    }

    /// An error: it stays until the player closes it.
    pub fn error(&mut self, text: String) {
        self.push_toast(Toast { text, left: None });
    }

    fn push_toast(&mut self, toast: Toast) {
        self.toasts.retain(|t| t.text != toast.text);
        self.toasts.push_back(toast);
        while self.toasts.len() > TOASTS {
            self.toasts.pop_front();
        }
    }

    fn toasts_area(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        let mut closed = None;
        egui::Area::new("тост".into()).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -60.0)).show(
            ctx,
            |ui| {
                for (i, t) in self.toasts.iter().enumerate() {
                    let error = t.left.is_none();
                    let frame = egui::Frame::popup(ui.style());
                    let frame =
                        if error { frame.stroke(egui::Stroke::new(1.5, theme::DANGER)) } else { frame };
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if error {
                                ui.colored_label(theme::DANGER, t.text.as_str());
                                if ui.small_button("✕").on_hover_text("Закрыть").clicked() {
                                    closed = Some(i);
                                }
                            } else {
                                ui.label(t.text.as_str());
                            }
                        });
                    });
                }
            },
        );
        if let Some(i) = closed {
            self.toasts.remove(i);
        }
    }

    /// Start a new game by the settings of the «Новый мир» screen.
    pub fn start_game(&mut self) {
        if self.settings.random_seed {
            self.settings.seed = random_seed();
        }
        let cfg = match self.settings.world_config(self.settings.seed) {
            Ok(cfg) => cfg,
            Err(e) => return self.error(format!("мир не создан: {e}")),
        };
        self.save_settings();
        self.sim.send(Command::NewWorld(cfg.clone()));
        self.sim.send(Command::SetPaused(false));
        self.game = Some(Game { start: cfg });
        self.lab = self.settings.clone();
        self.tool = Tool::Select;
        self.screen = Screen::Game;
    }

    pub fn restart(&mut self) {
        self.sim.send(Command::Restart);
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

    /// The simulation thread stopped with an error: what it said, and a way on without restarting
    /// the program.
    fn crash_window(&mut self, ctx: &egui::Context) {
        let failure = self.sim.failure().unwrap_or_else(|| "поток завершился без объяснения".into());
        egui::Window::new("Симуляция остановилась")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label("Поток симуляции завершился с ошибкой:");
                ui.label(egui::RichText::new(&failure).monospace().color(theme::DANGER));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "Запустить заново")
                        .on_hover_text("Та же партия с начала")
                        .clicked()
                    {
                        let cfg = match &self.game {
                            Some(game) => game.start.clone(),
                            None => Settings { scale: 1.0, ..self.settings.clone() }
                                .world_config(random_seed())
                                .unwrap_or_default(),
                        };
                        let waker = ctx.clone();
                        self.sim = SimHandle::spawn(cfg, Box::new(move || waker.request_repaint()));
                        self.applied_threads = None;
                    }
                    if ui.button("Скопировать текст").clicked() {
                        ctx.copy_text(failure.clone());
                    }
                });
            });
    }

    /// The same game anew, with a new seed: a world that ended would only end the same way again.
    pub fn new_seed(&mut self) {
        let Some(game) = &mut self.game else { return };
        game.start.seed = random_seed();
        self.sim.send(Command::NewWorld(game.start.clone()));
        self.sim.send(Command::SetPaused(false));
    }

    /// The modal question of `confirm`; «Да» does the action, Esc or a click outside cancels.
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(what) = self.confirm else { return };
        let (title, text, yes) = match what {
            Confirm::Restart => (
                "Начать партию заново?",
                "Мир вернётся к своему началу: тот же сид и стартовые правила. Всё, что выросло, пропадёт.",
                "Заново",
            ),
            Confirm::NewGame => (
                "Заменить текущую партию?",
                "Идущий мир закроется, вместо него начнётся новый с настройками этого экрана.",
                "Начать новый",
            ),
            Confirm::ResetTab(_) => (
                "Сбросить вкладку?",
                "Все значения этой вкладки вернутся к значениям по умолчанию.",
                "Сбросить",
            ),
        };
        let modal = egui::Modal::new(egui::Id::new("подтверждение")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(theme::heading(title, theme::HEADING));
            ui.add_space(4.0);
            ui.label(text);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if theme::danger_button(ui, yes).clicked() {
                    match what {
                        Confirm::Restart => self.restart(),
                        Confirm::NewGame => self.start_game(),
                        Confirm::ResetTab(tab) => self.settings.reset(tab),
                    }
                    self.confirm = None;
                }
                if ui.button("Отмена").clicked() {
                    self.confirm = None;
                }
            });
        });
        if modal.should_close() {
            self.confirm = None;
        }
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
        // the window's ✕ ends the program past «Назад» and «Начать»: what was changed on the
        // «Новый мир» screen is kept as they keep it
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_settings();
        }
        self.receive(&ctx);
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + 0.1 / dt;
        }
        for t in &mut self.toasts {
            if let Some(left) = &mut t.left {
                *left -= dt;
            }
        }
        self.toasts.retain(|t| t.left.is_none_or(|left| left > 0.0));
        if self.toasts.iter().any(|t| t.left.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        match self.screen {
            Screen::Game => self.game_screen(ui),
            Screen::Menu => self.menu_screen(ui),
            Screen::Setup => self.setup_screen(ui),
        }
        self.prefs_window(&ctx);
        self.help_window(&ctx);
        self.confirm_window(&ctx);

        self.was_editing = ctx.text_edit_focused();
        self.toasts_area(&ctx);
        if !self.sim.is_alive() {
            self.crash_window(&ctx);
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
