//! The game screen: the panels around the world, the keys, the intro and keys' cards and the end of
//! a game. The panels are in their own files: the bars (`bars`), the side panel (`side`), the
//! creature's tab (`card`) and the lab (`lab`).

mod bars;
mod card;
mod lab;
mod side;

use eframe::egui::{self, Align2, Key, Vec2};

use crate::app::{LifeApp, SideTab, Tool};
use crate::frame::Ending;
use crate::settings;
use crate::sim::{Command, SPEEDS};
use crate::theme::{self, ACCENT, rgb, spaced};
use crate::view::Click;

/// The speed of panning with the keys, screen points a second.
const PAN_SPEED: f64 = 900.0;

impl LifeApp {
    /// The calm profile is available to an existing game with old settings too.
    pub fn calm_world(&mut self) {
        let Some(f) = &self.view.frame else { return };
        let world_gen = f.world_gen;
        // a price set far past the sliders' range (`--rule size_cost=...`) can overflow the upkeep at ×3
        let rules = match f.rules.with("cost_scale", 3.0) {
            Ok(rules) => rules,
            Err(e) => return self.error(format!("спокойный профиль не применён: {e}")),
        };
        self.lab.take_rules(&rules);
        self.settings.set(settings::Key::CostScale, 3.0);
        self.save_settings();
        self.sim.send(Command::SetRules {
            rules,
            note: "спокойный профиль: цена жизни 150".into(),
            world_gen,
        });
        self.sim.send(Command::SetSpeed(crate::sim::DEFAULT_SPEED));
        self.toast(
            "30 тиков/с · цена жизни 150 — в этой партии и в новых мирах; численность изменится постепенно"
                .into(),
        );
    }

    pub fn game_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.game_keyboard(&ctx);

        let frame = theme::panel_frame(ui.style());
        egui::Panel::top("верх").frame(frame).show(ui, |ui| {
            theme::backdrop(ui, Some(Align2::CENTER_BOTTOM));
            self.top_bar(ui)
        });
        egui::Panel::bottom("низ").frame(frame).show(ui, |ui| {
            theme::backdrop(ui, Some(Align2::CENTER_TOP));
            self.bottom_bar(ui)
        });
        egui::Panel::left("инструменты").frame(frame).resizable(false).exact_size(46.0).show(ui, |ui| {
            theme::backdrop(ui, Some(Align2::RIGHT_CENTER));
            self.tool_rail(ui)
        });
        if self.side_open {
            // the lab wants room for its rules: a wider panel of its own width
            let (id, size, range) = if self.side_tab == SideTab::Lab {
                ("сбоку-лаборатория", 560.0, 420.0..=900.0)
            } else {
                ("сбоку", 340.0, 310.0..=620.0)
            };
            egui::Panel::right(id).frame(frame).default_size(size).size_range(range).show(ui, |ui| {
                theme::backdrop(ui, Some(Align2::LEFT_CENTER));
                self.side_panel(ui)
            });
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            self.world_area = rect;
            self.view.area_mode = self.tool == Tool::Area;
            let click = self.view.show(ui, rect, &self.sim, true);
            // the world the click was aimed at: one replaced since takes no clicks
            let world_gen = self.view.frame.as_ref().map_or(0, |f| f.world_gen);
            match click {
                Some(Click::World { x, y, radius }) => match self.tool {
                    Tool::Select => {
                        let frame = self.view.frame.as_ref().map_or(0, |f| f.number);
                        let k = f64::from(self.view.progress());
                        self.sim.send(Command::Pick { x, y, radius, world_gen, frame, k });
                        // the card opens once the pick comes back with a creature: a miss keeps
                        // the tab and the selection
                        self.pick_pending = 30;
                    }
                    Tool::Spawn => {
                        // the planted creature forks the world's stream: a repeat parts from here
                        // (`Frame::edits`)
                        self.sim.send(Command::Spawn { x, y, world_gen });
                    }
                    Tool::Area => {}
                },
                Some(Click::Area(area)) => self.set_region(area),
                None => {}
            }
            let hint = match self.tool {
                Tool::Select => None,
                Tool::Spawn => Some("клик по миру — подсадить; Esc — обычный выбор"),
                Tool::Area => Some(
                    "протяните мышью прямоугольник — геном тех, кто внутри; двигать мир — WASD и \
                     миникарта; Esc — обычный выбор",
                ),
            };
            if let Some(hint) = hint {
                ui.painter().text(
                    rect.center_top() + Vec2::new(0.0, 14.0),
                    Align2::CENTER_TOP,
                    hint,
                    egui::FontId::proportional(14.0),
                    ACCENT,
                );
            }
        });
        if self.behaviour_open {
            match self.view.frame.as_ref().and_then(|f| f.selected) {
                Some(s) => {
                    crate::behaviour::behaviour_window(&ctx, self.world_area, &mut self.behaviour_open, &s)
                }
                None => self.behaviour_open = false,
            }
        }
        if self.stats_open {
            self.stats_window(&ctx);
        }
        if self.keys_open {
            self.keys_window(&ctx);
        }
        if !self.settings.intro_seen && self.game.is_some() {
            self.intro_window(&ctx);
        }
        self.ending_window(&ctx);
    }

    fn game_keyboard(&mut self, ctx: &egui::Context) {
        // only typing into a field silences the game's keys
        if ctx.text_edit_focused() {
            return;
        }
        // Tab and the arrows are the game's keys here, not egui's moves of the focus, and a button
        // keeps no focus: once one had it every key stayed egui's, and Space pressed it as well as
        // pausing. Called before any widget of the frame, so the move is cancelled in time.
        ctx.memory_mut(|m| {
            m.move_focus(egui::FocusDirection::None);
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        let Some(st) = self.view.frame.as_ref().map(|f| f.status) else { return };
        let (keys, held, dt) = ctx.input(|i| {
            let p = |k| i.key_pressed(k);
            (
                [
                    p(Key::Space),
                    p(Key::ArrowRight),
                    p(Key::Plus) || p(Key::Equals),
                    p(Key::Minus),
                    p(Key::Home),
                    p(Key::F),
                    p(Key::Tab),
                    p(Key::L),
                    p(Key::I),
                    p(Key::Escape),
                    p(Key::B),
                    p(Key::F1),
                ],
                [i.key_down(Key::W), i.key_down(Key::S), i.key_down(Key::A), i.key_down(Key::D)],
                i.stable_dt.min(0.1) as f64,
            )
        });
        let [space, step, faster, slower, home, follow, tab, lab, stats, escape, behaviour, f1] = keys;
        if f1 {
            self.keys_open = !self.keys_open;
        }
        if space {
            self.sim.send(Command::TogglePause);
        }
        if step && st.paused {
            self.sim.send(Command::Step);
        }
        if faster && st.speed_index + 1 < SPEEDS.len() {
            self.sim.send(Command::SetSpeed(st.speed_index + 1));
        }
        if slower && st.speed_index > 0 {
            self.sim.send(Command::SetSpeed(st.speed_index - 1));
        }
        if follow {
            self.view.toggle_follow();
        }
        if tab {
            self.side_open = !self.side_open;
        }
        if lab {
            self.toggle_lab();
        }
        if stats {
            self.stats_open = !self.stats_open;
        }
        if behaviour && self.view.frame.as_ref().is_some_and(|f| f.selected.is_some()) {
            self.behaviour_open = !self.behaviour_open;
        }
        // Esc closes the topmost thing first: a window, then what is on the world, then the
        // selection; only then the menu. Esc that ended typing into a field does nothing more.
        if escape && !self.was_editing {
            let selected = self.view.frame.as_ref().is_some_and(|f| f.selected.is_some());
            if self.help_open {
                self.help_open = false;
            } else if self.prefs_open {
                self.prefs_open = false;
            } else if self.keys_open {
                self.keys_open = false;
            } else if self.lab_shown() {
                self.side_tab = SideTab::Charts;
            } else if self.behaviour_open {
                self.behaviour_open = false;
            } else if self.stats_open {
                self.stats_open = false;
            } else if self.view.area.is_some() {
                self.clear_region();
            } else if self.tool != Tool::Select {
                self.tool = Tool::Select;
                self.view.cancel_area_drag();
            } else if selected || self.lost.is_some() {
                self.unselect();
            } else {
                self.open_menu();
            }
        }
        if let Some(cam) = &mut self.view.camera {
            if home {
                cam.fit();
            }
            let [w, s, a, d] = held;
            let dx = (a as i32 - d as i32) as f64;
            let dy = (w as i32 - s as i32) as f64;
            if dx != 0.0 || dy != 0.0 {
                cam.pan(dx * PAN_SPEED * dt, dy * PAN_SPEED * dt);
                ctx.request_repaint();
            }
        }
    }

    /// Drop the selection, its following and the card of one that died.
    pub fn unselect(&mut self) {
        self.sim.send(Command::Select(None));
        self.lost = None;
        self.refollow = false;
        if let Some(cam) = &mut self.view.camera {
            cam.follow(None, None);
        }
    }

    /// The first game's card: what the world shows and where things are, in four lines. Read once;
    /// the settings can show it again.
    fn intro_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Как здесь всё устроено")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -64.0))
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.horizontal(|ui| {
                    ui.label("Цвет существа — его питание:");
                    for d in 0..4 {
                        ui.colored_label(rgb(theme::DIET_COLORS[d]), theme::DIET_NAMES[d]);
                    }
                });
                for line in [
                    "Вблизи и по форме: у мясоеда край зубчатый, у падальщика ободок пунктиром. \
                     Светлое ядро — сытость: у голодных оно маленькое.",
                    "Пробел — пауза, колесо — приблизить, клик по существу — его карточка справа.",
                    "Слева — инструменты и камера, вверху справа — статистика, поведение, лаборатория; \
                     Tab прячет боковую панель.",
                    "F1 — все клавиши.",
                ] {
                    ui.label(line);
                }
                ui.add_space(theme::GAP);
                if theme::primary_button(ui, "Понятно").clicked() {
                    self.settings.intro_seen = true;
                    self.save_settings();
                }
            });
    }

    /// The keys' card at the top right, under the bar: what each key does, and the whole help.
    fn keys_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Клавиши")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 52.0))
            .show(ctx, |ui| {
                crate::screens::keys_grid(ui);
                ui.add_space(theme::GAP);
                if ui.button("Вся справка").clicked() {
                    self.help_open = true;
                    self.keys_open = false;
                }
            });
        self.keys_open &= open;
    }

    /// The end of a game: what happened and what to do — a new seed (the same one would only end
    /// the same way), planting into an extinct world, going on past an explosion, or just looking.
    fn ending_window(&mut self, ctx: &egui::Context) {
        let Some(f) = self.view.frame.as_ref() else { return };
        if f.status.ended.is_none() {
            // a world revived and ended again asks again
            self.ending_closed = false;
            return;
        }
        let Some(ended) = f.status.ended.filter(|_| !self.ending_closed) else { return };
        let area = f.world_w * f.world_h / (life_core::config::WORLD_WIDTH * life_core::config::WORLD_HEIGHT);
        let limit = (crate::sim::EXPLOSION_LIMIT as f64 * area).round() as u64;
        let (title, text) = match ended {
            Ending::Extinct => (
                "Все вымерли",
                format!(
                    "На тике {} в мире не осталось ни одного существа. Тот же сид кончится так же; \
                     подсаженное существо оживит этот мир.",
                    spaced(f.tick)
                ),
            ),
            Ending::Explosion => (
                "Взрыв численности",
                format!(
                    "Существ {} при пределе {}: мир почти наверняка пошёл вразнос. Можно продолжить — \
                     тик станет медленным — или сделать жизнь дороже.",
                    spaced(f.creatures as u64),
                    spaced(limit)
                ),
            ),
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(text);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "Новый сид")
                        .on_hover_text("Та же партия с другим сидом")
                        .clicked()
                    {
                        self.new_seed();
                    }
                    match ended {
                        Ending::Extinct => {
                            if ui
                                .button("Подсадить существо")
                                .on_hover_text("Клик по миру — подсадить")
                                .clicked()
                            {
                                self.tool = Tool::Spawn;
                                self.ending_closed = true;
                            }
                        }
                        Ending::Explosion => {
                            if ui
                                .button("Спокойнее")
                                .on_hover_text("Цена жизни 150 и 30 тиков/с, и мир идёт дальше")
                                .clicked()
                            {
                                self.calm_world();
                                self.sim.send(Command::KeepGoing);
                            }
                            if ui.button("Продолжить всё равно").clicked() {
                                self.sim.send(Command::KeepGoing);
                            }
                        }
                    }
                    if ui.button("Новый мир").clicked() {
                        self.screen = crate::app::Screen::Setup;
                    }
                    if ui.button("Закрыть").on_hover_text("Посмотреть на мир, каким он кончился").clicked()
                    {
                        self.ending_closed = true;
                    }
                });
            });
    }
}
