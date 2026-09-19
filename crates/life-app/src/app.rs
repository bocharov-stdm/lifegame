//! Приложение: экраны, переходы, настройки. Окно только рисует последний кадр
//! и шлёт команды; всё тяжёлое — в потоке симуляции (`sim.rs`).

use std::path::PathBuf;

use eframe::egui;
use life_core::WorldConfig;

use crate::frame::LogEntry;
use crate::history::History;
use crate::settings::{self, Settings, Tab};
use crate::sim::{Command, SimHandle};
use crate::theme;
use crate::view::WorldView;

/// Больше записей хроники не держим: старые уходят.
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
    SpawnVegetarian,
    SpawnPredator,
}

/// Идущая партия (не фоновый мир меню).
pub struct Game {
    /// С чего партия началась: «Заново» повторяет именно его.
    pub start: WorldConfig,
    /// Правила меняли на ходу: повтор без окна совпадёт только до изменения.
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

    // ── состояние интерфейса ────────────────────────────────────────────────
    pub side_open: bool,
    pub side_tab: SideTab,
    /// Графики за всю партию, а не за последнее окно.
    pub whole: bool,
    pub lab_open: bool,
    /// Черновик правил лаборатории на ходу; применяется кнопкой.
    pub lab: Settings,
    /// Вкладка лаборатории: правила (`Tab::Lab`) или еда (`Tab::Food`).
    pub lab_tab: Tab,
    pub tool: Tool,
    pub setup_tab: Tab,
    pub prefs_open: bool,
    pub help_open: bool,
    /// Была ли партия на паузе, когда открыли меню.
    pub paused_before_menu: bool,
    pub toast: Option<(String, f64)>,
    fps: f64,
    applied: (bool, f64),
}

impl LifeApp {
    /// `start` — мир из командной строки: сразу в игру. Иначе — меню с живым
    /// миром ×1 на фоне. `settings_path` — файл настроек; None — не читать и
    /// не писать (тесты не должны трогать настройки игрока).
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
            whole: false,
            lab_open: false,
            lab_tab: Tab::Lab,
            tool: Tool::Select,
            setup_tab: Tab::World,
            prefs_open: false,
            help_open: false,
            paused_before_menu: false,
            toast: None,
            fps: 0.0,
            applied: (false, -1.0),
        }
    }

    /// Забрать новый кадр из потока симуляции, если он есть.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.sim.take_frame() else { return };
        if self.view.frame.as_ref().is_none_or(|old| old.world_gen != f.world_gen) {
            self.history = History::default();
            self.log.clear();
            self.lab.take_rules(&f.rules);
        }
        for s in f.samples.drain(..) {
            self.history.add_sample(s);
        }
        for g in f.gene_points.drain(..) {
            self.history.add_genes(g);
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

    /// Начать новую партию по настройкам экрана «Новый мир».
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

    /// Масштаб интерфейса и полноэкранный режим — когда поменялись.
    fn apply_display(&mut self, ctx: &egui::Context) {
        let want = (self.settings.fullscreen, self.settings.ui_scale);
        if want == self.applied {
            return;
        }
        if want.0 != self.applied.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(want.0));
        }
        // 0 — как в системе: множитель 1 к системному масштабу
        ctx.set_zoom_factor(if want.1 > 0.0 { want.1 as f32 } else { 1.0 });
        self.applied = want;
    }

    pub fn fps(&self) -> f64 {
        self.fps
    }
}

/// Случайный сид от часов: общий генератор движку не нужен.
pub fn random_seed() -> u64 {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    (life_core::rng::mix(nanos as u64) % settings::SEED_MAX) + 1
}

impl eframe::App for LifeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_display(&ctx);
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
        // слежение — каждый кадр окна, а не только когда пришёл кадр мира
        self.view.follow_step(dt);
        if self.view.following() {
            ctx.request_repaint();
        }
    }
}
