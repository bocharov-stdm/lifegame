//! The game screen: the top panel (tempo and counts), the bottom one (tools), the world, the
//! side panel (charts, chronicle, creature), the lab on the fly, the end of the game.

use eframe::egui::{self, Align2, Key, RichText, Vec2};
use life_core::flora::Profile;
use life_core::genome::creature::N;
use life_core::genome::{GeneSpec, creature};
use life_core::{Rules, WorldConfig, units};
use life_sim::observe::{EventKind, GeneStat};

use crate::app::{LifeApp, SideTab, Tool};
use crate::charts;
use crate::frame::{CREATURE_COLOR, Ending, PLANT_COLOR, Selected};
use crate::history::History;
use crate::settings::{self, FIELDS, Tab};
use crate::sim::{Command, SPEEDS};
use crate::theme::{self, ACCENT, DANGER, GOOD, Icon, MUTED, TEXT, rgb, spaced};
use crate::view::Click;
use life_core::profile::Phase;

/// The speed of panning with the keys, screen points a second.
const PAN_SPEED: f64 = 900.0;

/// The tick's dearest phases by share: «фазы тика: решения 62% · стадо 12% · …», the rest as one.
fn phases_line(shares: &[f64; Phase::N]) -> String {
    let mut order: Vec<Phase> = Phase::ALL.to_vec();
    order.sort_by(|a, b| shares[*b as usize].total_cmp(&shares[*a as usize]));
    let (top, rest) = order.split_at(5);
    let mut parts: Vec<String> =
        top.iter().map(|&p| format!("{} {:.0}%", p.label(), shares[p as usize] * 100.0)).collect();
    parts.push(format!("прочее {:.0}%", rest.iter().map(|&p| shares[p as usize]).sum::<f64>() * 100.0));
    format!("фазы тика: {}", parts.join(" · "))
}

fn speed_label(index: usize) -> String {
    match SPEEDS[index] {
        Some(tps) => format!("{tps:.0} тиков/с"),
        None => "максимум".into(),
    }
}

/// What the world eats and how it shoots in the last snapshot: diet shares in % and the share of
/// shooters in %.
pub(crate) fn hunting_summary(history: &History) -> Option<([f64; 4], f64)> {
    let last = history.snapshots.last()?;
    let genes = last.genes.as_ref()?;
    let shares = |gene: creature::Gene| match genes[gene as usize] {
        GeneStat::Shares(shares) => Some(shares),
        _ => None,
    };
    let diets = shares(creature::Gene::Diet)?;
    let diets = [0, 1, 2, 3].map(|k| diets[k] * 100.0);
    Some((diets, last.shooters as f64 / last.creatures as f64 * 100.0))
}

/// The `life-report` command that repeats a game without a window.
pub fn report_command(cfg: &WorldConfig, ticks: u64) -> String {
    let mut cmd =
        format!("cargo run -p life-report --release -- --seed {} --ticks {}", cfg.seed, ticks.max(600));
    let base = WorldConfig::default();
    if cfg.scale != 1.0 {
        cmd += &format!(" --scale {}", cfg.scale);
    }
    if cfg.shape != base.shape {
        cmd += &format!(" --shape {}", cfg.shape.key());
    }
    cmd += &format!(" --creatures {}", cfg.creatures_at_start());
    if !cfg.strategies.is_empty() {
        let shares: Vec<String> = cfg.strategies.iter().map(|s| s.to_string()).collect();
        cmd += &format!(" --mix {}", shares.join(" "));
    }
    if cfg.diets != base.diets {
        let shares: Vec<String> = cfg.diets.iter().map(|s| s.to_string()).collect();
        cmd += &format!(" --diet-mix {}", shares.join(" "));
    }
    if cfg.meat_founder_size != base.meat_founder_size {
        cmd += &format!(" --meat-founders {}", cfg.meat_founder_size);
    }
    let default = Rules::default();
    for key in life_core::rules::RULE_KEYS {
        let (v, d) = (cfg.rules.get(key), default.get(key));
        if let Some(v) = v.filter(|v| Some(*v) != d) {
            // the food profile by name: plant_width_profile=waves is clearer than =4
            match life_core::flora::split_key(key) {
                Some((_, "profile")) => cmd += &format!(" --rule {key}={}", Profile::of(v).key()),
                _ => cmd += &format!(" --rule {key}={v}"),
            }
        }
    }
    cmd
}

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
                Some(s) => crate::behaviour::behaviour_window(&ctx, &mut self.behaviour_open, &s),
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

    /// The top bar: the menu, time (pause, a step, the speed), the counts, what state the world is
    /// in (chips), and on the right the windows: statistics, behaviour, lab, help, the side panel.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.view.frame else {
            ui.label("Создаём мир…");
            return;
        };
        let (st, tick, counts) = (f.status, f.tick, [f.plants, f.creatures]);
        let (seed, scale, edited, selected) = (f.seed, f.scale, f.edits > 0, f.selected.is_some());
        let (tick_ms, snapshot_ms, build_ms, draw_ms) =
            (f.tick_ms, f.snapshot_ms, f.build_ms, self.view.draw_ms);
        let phases = f.phases;
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            if theme::icon_button(ui, Icon::Menu, "Меню (Esc)", false).clicked() {
                self.open_menu();
            }
            ui.add_space(theme::GAP);
            let (icon, hint) = if st.paused { (Icon::Play, "Пуск (Пробел)") } else { (Icon::Pause, "Пауза (Пробел)") };
            if theme::icon_button(ui, icon, hint, false).clicked() {
                self.sim.send(Command::TogglePause);
            }
            if ui.add_enabled_ui(st.paused, |ui| theme::icon_button(ui, Icon::Step, "Один тик (→)", false)).inner.clicked()
            {
                self.sim.send(Command::Step);
            }
            if ui
                .add_enabled_ui(st.speed_index > 0, |ui| theme::icon_button(ui, Icon::Slower, "Медленнее (−)", false))
                .inner
                .clicked()
            {
                self.sim.send(Command::SetSpeed(st.speed_index - 1));
            }
            ui.label(RichText::new(speed_label(st.speed_index)).monospace()).on_hover_text("Выбранная скорость");
            if ui
                .add_enabled_ui(st.speed_index + 1 < SPEEDS.len(), |ui| {
                    theme::icon_button(ui, Icon::Faster, "Быстрее (+)", false)
                })
                .inner
                .clicked()
            {
                self.sim.send(Command::SetSpeed(st.speed_index + 1));
            }
            ui.add_space(theme::GAP);
            ui.label(RichText::new(format!("тик {}", spaced(tick))).monospace())
                .on_hover_text(format!("Сид {seed}, мир ×{scale}"));
            ui.label(RichText::new(format!("● {}", spaced(counts[0] as u64))).monospace().color(rgb(PLANT_COLOR)))
                .on_hover_text("Растений");
            ui.label(RichText::new(format!("● {}", spaced(counts[1] as u64))).monospace().color(rgb(CREATURE_COLOR)))
                .on_hover_text("Существ");
            if st.lagging {
                let target = SPEEDS[st.speed_index].unwrap_or(0.0);
                theme::chip(ui, &format!("отстаёт: {:.0} из {target:.0}", st.tps), DANGER).on_hover_text(
                    "Тик не успевает за выбранной скоростью: мир большой или скорость высокая. \
                     Окно при этом не тормозит.",
                );
            } else if st.paused {
                theme::chip(ui, "пауза", theme::WARN).on_hover_text("Пробел — пуск");
            }
            if edited {
                theme::chip(ui, "мир изменён", MUTED).on_hover_text(
                    "Правила или существ меняли на ходу: повтор этой партии без окна совпадёт только до \
                     первого изменения.",
                );
            }
            if self.settings.show_fps {
                ui.colored_label(
                    MUTED,
                    format!(
                        "{:.0} к/с · {:.0} тиков/с · тик {tick_ms:.1} · срез {snapshot_ms:.1} · сборка {build_ms:.1} · рисунок {draw_ms:.1} мс",
                        self.fps(),
                        st.tps
                    ),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::icon_button(ui, Icon::Panel, "Боковая панель (Tab)", self.side_open).clicked() {
                    self.side_open = !self.side_open;
                }
                if theme::icon_button(ui, Icon::Help, "Клавиши и справка (F1)", self.keys_open).clicked() {
                    self.keys_open = !self.keys_open;
                }
                if theme::icon_button(ui, Icon::Lab, "Лаборатория: правила на ходу (L)", self.lab_shown()).clicked()
                {
                    self.toggle_lab();
                }
                if ui
                    .add_enabled_ui(selected, |ui| {
                        theme::icon_button(ui, Icon::Behaviour, "Поведение выбранного (B)", self.behaviour_open)
                    })
                    .inner
                    .clicked()
                {
                    self.behaviour_open = !self.behaviour_open;
                }
                if theme::icon_button(ui, Icon::Stats, "Статистика (I)", self.stats_open).clicked() {
                    self.stats_open = !self.stats_open;
                }
            });
        });
        if self.settings.show_fps && phases.iter().any(|&s| s > 0.0) {
            ui.colored_label(MUTED, phases_line(&phases)).on_hover_text(
                "Куда уходит время тика, по фазам: решения существ, снимок стада, сетки, еда, бой. \
                 Сглажено, как цена тика.",
            );
        }
        ui.add_space(2.0);
    }

    /// The tool rail at the left: what a click on the world does, and the camera.
    fn tool_rail(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            for (tool, icon, label) in [
                (Tool::Select, Icon::Select, "Выбор: клик по существу"),
                (Tool::Spawn, Icon::Spawn, "Подсадить существо кликом"),
                (Tool::Area, Icon::Area, "Область: протянуть рамку — геном тех, кто внутри"),
            ] {
                if theme::icon_button(ui, icon, label, self.tool == tool).clicked() {
                    self.tool = tool;
                }
            }
            ui.add_space(theme::GAP);
            if theme::icon_button(ui, Icon::Fit, "Весь мир (Home)", false).clicked()
                && let Some(cam) = &mut self.view.camera
            {
                cam.fit();
            }
            let selected = self.view.frame.as_ref().is_some_and(|f| f.selected.is_some());
            if ui
                .add_enabled_ui(selected, |ui| {
                    theme::icon_button(ui, Icon::Follow, "Следить за выбранным (F)", self.view.following())
                })
                .inner
                .clicked()
            {
                self.view.toggle_follow();
            }
            ui.add_space(theme::GAP);
            let area = self.view.area.is_some();
            if ui
                .add_enabled_ui(area, |ui| theme::icon_button(ui, Icon::Area, "Снять область (Esc)", area))
                .inner
                .clicked()
            {
                self.clear_region();
            }
        });
    }

    /// The bottom strip: the world's menu, the creatures over the charts' span with the
    /// chronicle's marks (a click on a mark pauses), and the region, if one is set.
    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.menu_button("Мир ▾", |ui| {
                if ui
                    .button("Спокойнее")
                    .on_hover_text(
                        "30 тиков/с и цена жизни 150 (обычная — 100): численность постепенно снижается, \
                         но падальщики почти не держатся. Сохраняется и для новых миров.",
                    )
                    .clicked()
                {
                    self.calm_world();
                    ui.close();
                }
                if ui
                    .button("Заново…")
                    .on_hover_text("Та же партия с начала: тот же сид и стартовые правила")
                    .clicked()
                {
                    self.confirm = Some(crate::app::Confirm::Restart);
                    ui.close();
                }
                if ui.button("Новый сид").on_hover_text("Та же партия с другим сидом").clicked()
                {
                    self.new_seed();
                    ui.close();
                }
                if ui.button("Новый мир…").clicked() {
                    self.screen = crate::app::Screen::Setup;
                    ui.close();
                }
                ui.separator();
                let was_rendering = self.render_world;
                ui.checkbox(&mut self.render_world, "Рисовать мир")
                    .on_hover_text("Без рисунка мир считается быстрее; графики и карточка работают");
                if was_rendering != self.render_world {
                    self.sim.send(Command::RenderWorld(self.render_world));
                }
            });
            let width = ui.available_width();
            if let Some(tick) = charts::timeline(ui, &self.history, &self.log, width, 26.0) {
                self.sim.send(Command::SetPaused(true));
                self.toast(format!("пауза у события тика {}", spaced(tick)));
            }
        });
        ui.add_space(2.0);
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 4.0;
        // the tabs first, always in the same place
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.side_tab, SideTab::Charts, "Обзор");
            ui.selectable_value(&mut self.side_tab, SideTab::Creature, "Существо");
            ui.selectable_value(&mut self.side_tab, SideTab::Log, "Хроника");
            ui.selectable_value(&mut self.side_tab, SideTab::Lab, "Лаборатория");
        });
        ui.add_space(3.0);
        // the overview lists the diets itself; the creature and the chronicle keep one line of them
        if matches!(self.side_tab, SideTab::Creature | SideTab::Log) {
            self.diets_line(ui);
            self.highlight_toggles(ui);
            ui.add_space(3.0);
        }
        match self.side_tab {
            SideTab::Charts => self.charts_tab(ui),
            SideTab::Log => self.log_tab(ui),
            SideTab::Creature => self.creature_tab(ui),
            SideTab::Lab => self.lab_page(ui),
        }
    }

    /// Hunting and shooting: minor facts, folded away.
    fn other_facts(&self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Прочее").default_open(false).show(ui, |ui| {
            if let Some((_, shooters)) = hunting_summary(&self.history) {
                ui.label(format!(
                    "Умеют стрелять {shooters:.1}% · выстрелов за 10 000 тиков {}",
                    spaced(self.history.shots_in_window())
                ))
                .on_hover_text("Выстрел слабее удара вблизи и дорого стоит, поэтому стрелков мало.");
            }
        });
    }

    fn charts_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.diets_block(ui);
            ui.add_space(6.0);
            ui.label(crate::theme::strong("Численность").color(ACCENT)).on_hover_text(
                "Растения — в своей шкале, питания — в одной общей, чтобы их можно было сравнивать. \
                 Наведите на график — под ним будут числа в этой точке.",
            );
            ui.colored_label(MUTED, self.history.span_label());
            charts::populations(ui, &self.history, 112.0);
            ui.add_space(4.0);
            self.kills_table(ui);
            egui::CollapsingHeader::new("Геном всех существ").default_open(false).show(ui, |ui| {
                ui.colored_label(
                    MUTED,
                    "Линия — медиана, полоса — где 80% существ. Справа — сейчас и изменение за окно.",
                );
                let snaps = self.history.snapshots.points();
                let points: Vec<charts::GenePoint> =
                    snaps.iter().filter_map(|s| Some((s.tick, &s.genes.as_ref()?[..]))).collect();
                if points.is_empty() {
                    ui.colored_label(MUTED, "существ нет — нет и генома");
                } else {
                    charts::genome(ui, &creature::GENES, &points, rgb(CREATURE_COLOR), 23.0);
                }
            });
            self.other_facts(ui);
            if self.settings.details {
                self.research(ui);
            }
        });
    }

    /// For a researcher: how to repeat a game without a window.
    fn research(&mut self, ui: &mut egui::Ui) {
        let (Some(game), Some(f)) = (&self.game, &self.view.frame) else { return };
        let (cmd, edited) = (report_command(&game.start, f.tick), f.edits > 0);
        let mut copied = false;
        ui.collapsing("Повторить без окна", |ui| {
            ui.label(RichText::new(&cmd).monospace().size(11.5));
            // the edits the simulation thread took, not the ones the window sent
            if edited {
                ui.colored_label(
                    ACCENT,
                    "Мир меняли на ходу (правила, подсадка): повтор совпадёт только до первого изменения.",
                );
            }
            if ui.button("Скопировать команду").clicked() {
                ui.ctx().copy_text(cmd.clone());
                copied = true;
            }
        });
        if copied {
            self.toast("команда скопирована".into());
        }
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        if self.log.is_empty() {
            ui.colored_label(
                MUTED,
                "Пока ничего не случилось. Здесь появятся обвалы и подъёмы численности, \
                 вымирания, сдвиги генов и ваши вмешательства.",
            );
            return;
        }
        ui.colored_label(MUTED, "Клик по записи — пауза.");
        let mut pause = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for e in self.log.iter().rev() {
                let color = match e.kind {
                    Some(EventKind::CreaturesCrash | EventKind::CreaturesExtinct) => DANGER,
                    Some(EventKind::CreaturesRise) => GOOD,
                    Some(EventKind::GeneShift | EventKind::StrategyShift) => rgb(CREATURE_COLOR),
                    Some(_) => TEXT,
                    None => ACCENT,
                };
                let text = RichText::new(format!("{}  ", spaced(e.tick))).color(MUTED).monospace();
                let resp = ui
                    .horizontal_wrapped(|ui| {
                        ui.label(text);
                        ui.add(
                            egui::Label::new(RichText::new(&e.text).color(color)).sense(egui::Sense::click()),
                        )
                    })
                    .inner;
                pause |= resp.clicked();
                ui.add_space(2.0);
            }
        });
        if pause {
            self.sim.send(Command::SetPaused(true));
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
                    "Светлое ядро — сытость: у голодных оно маленькое.",
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

    /// Drop the selection, its following and the card of one that died.
    pub fn unselect(&mut self) {
        self.sim.send(Command::Select(None));
        self.lost = None;
        self.refollow = false;
        if let Some(cam) = &mut self.view.camera {
            cam.follow(None, None);
        }
    }

    fn creature_tab(&mut self, ui: &mut egui::Ui) {
        let Some((s, rules, world_h)) =
            self.view.frame.as_ref().and_then(|f| Some((f.selected?, f.rules.clone(), f.world_h)))
        else {
            if let Some(lost) = self.lost {
                self.lost_card(ui, lost);
            } else {
                ui.colored_label(MUTED, "Никто не выбран. Кликните по существу в мире.");
            }
            return;
        };
        // its own diet's medians: a carnivore is compared with carnivores, not with the herbivores
        let diet = (s.genome[creature::Gene::Diet as usize] as usize).min(3);
        let mut own = [None; N];
        if let Some(genes) = self.history.snapshots.last().and_then(|snap| snap.diets[diet].genes) {
            for (spread, gene) in genes.iter().zip(life_sim::observe::DIET_GENES) {
                own[gene as usize] = Some(spread.p50);
            }
        }
        let following = self.view.following();
        let mut act = CardAction::None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            act = creature_card(ui, &s, &own, &rules, world_h, self.behaviour_open, following);
        });
        match act {
            CardAction::None => {}
            CardAction::Behaviour => self.behaviour_open = !self.behaviour_open,
            CardAction::Follow => self.view.toggle_follow(),
            CardAction::Unselect => self.unselect(),
        }
    }

    /// The card of the selected creature that died: how and when, and who to look at next.
    fn lost_card(&mut self, ui: &mut egui::Ui, lost: crate::frame::Lost) {
        let diet = lost.diet.min(3);
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(rgb(theme::DIET_COLORS[diet])).size(18.0));
            ui.label(theme::heading("Погибло", theme::HEADING));
            ui.colored_label(MUTED, format!("№ {}", lost.id));
        });
        ui.label(format!(
            "{} · {} на тике {} в возрасте {:.0}",
            creature::DIET_VARIANTS[diet].label,
            crate::sim::death_words(lost.cause),
            spaced(lost.tick),
            lost.age
        ));
        ui.add_space(6.0);
        let world_gen = self.view.frame.as_ref().map_or(0, |f| f.world_gen);
        ui.horizontal_wrapped(|ui| {
            let nearest = |diet| Command::SelectNearest { x: lost.x, y: lost.y, diet, world_gen };
            if ui
                .button(format!("Ближайший: {}", theme::DIET_NAMES[diet]))
                .on_hover_text("Выбрать живое существо того же питания, ближайшее к месту гибели")
                .clicked()
            {
                self.sim.send(nearest(Some(diet)));
            }
            if ui.button("Ближайший любой").clicked() {
                self.sim.send(nearest(None));
            }
            if ui.button("Закрыть").on_hover_text("Esc").clicked() {
                self.lost = None;
                self.refollow = false;
            }
        });
    }

    /// The lab's rows of one tab's live rules: a box to mark for a joint reset, the name, the
    /// value, the default, and ↺ when it differs.
    fn lab_grid(&mut self, ui: &mut egui::Ui, tab: Tab, default: &settings::Settings) {
        ui.add_space(5.0);
        egui::Grid::new(("правила лаборатории", tab as u8)).num_columns(5).spacing([7.0, 4.0]).show(
            ui,
            |ui| {
                let rows = FIELDS
                    .iter()
                    .filter(|f| f.live() && f.tab == tab && !matches!(f.key, settings::Key::Diet(..)));
                for f in rows {
                    if !(f.visible)(&self.lab) {
                        continue;
                    }
                    let mut marked = self.lab_reset_selected.contains(&f.key);
                    if ui.checkbox(&mut marked, "").on_hover_text("Отметить для общего сброса").changed()
                    {
                        if marked {
                            self.lab_reset_selected.insert(f.key);
                        } else {
                            self.lab_reset_selected.remove(&f.key);
                        }
                    }
                    ui.label(f.label).on_hover_text(crate::screens::field_hint(f));
                    let mut v = self.lab.get(f.key);
                    if crate::screens::field_input(ui, f, &mut v) {
                        self.lab.set(f.key, v);
                    }
                    crate::screens::base_value(ui, f, default);
                    if self.lab.is_default(f.key) {
                        ui.label("");
                    } else if ui.small_button("↺").on_hover_text("Вернуть значение по умолчанию").clicked()
                    {
                        self.lab.set(f.key, default.get(f.key));
                        self.lab_reset_selected.remove(&f.key);
                    }
                    ui.end_row();
                }
            },
        );
    }

    /// The lab on the side panel: the world's rules by topic, applied with a button, saved for new
    /// worlds with another.
    fn lab_page(&mut self, ui: &mut egui::Ui) {
        let Some((current, space, seed, world_gen)) = self.view.frame.as_ref().map(|f| {
            (f.rules.clone(), life_core::Space { width: f.world_w, height: f.world_h }, f.seed, f.world_gen)
        }) else {
            return;
        };
        // what «Применить» would send — the world's rules with the moved sliders over them, which
        // keep a rule past the sliders' range — is what the body formula and the food preview show
        let applied = |lab: &settings::Settings| {
            let mut taken = lab.clone();
            taken.take_rules(&current);
            lab.rules_over(&current, &taken)
        };
        {
            {
                let body = self.lab_tab == Tab::Body;
                let diets = self.lab_tab == Tab::Diets;
                ui.horizontal_wrapped(|ui| {
                    for (tab, name) in Tab::RULES {
                        ui.selectable_value(&mut self.lab_tab, tab, name);
                    }
                });
                let food = self.lab_tab == Tab::Food;
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Правила действуют после «Применить»; гены существ не меняются. Наведите на \
                             название — что это и в каких пределах; справа — значение по умолчанию.",
                        )
                        .color(MUTED),
                    )
                    .wrap(),
                );
                let default = settings::Settings::default();
                // the buttons stay in sight under the scrolled rules; the diets' table scrolls across
                let height = (ui.available_height() - 76.0).max(120.0);
                egui::ScrollArea::both().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                    if diets {
                        ui.add_space(5.0);
                        crate::screens::diet_table(ui, &mut self.lab);
                        ui.add_space(theme::GAP_WIDE);
                        ui.label(theme::strong("Как меняется питание у потомков"));
                        self.lab_grid(ui, Tab::Diets, &default);
                        return;
                    }
                    // the body's formula under its fields, so the panel keeps its width
                    ui.vertical(|ui| {
                        if self.lab_tab == Tab::Water {
                            ui.label(crate::screens::WATER_NOTE);
                        }
                        for (i, (tab, heading)) in self.lab_tab.pages().iter().enumerate() {
                            if let Some(heading) = heading.filter(|_| i > 0) {
                                ui.add_space(theme::GAP_WIDE);
                                ui.label(theme::strong(heading));
                            }
                            self.lab_grid(ui, *tab, &default);
                        }
                        if body && self.settings.details {
                            ui.add_space(8.0);
                            crate::screens::body_formula(ui, &applied(&self.lab));
                        }
                    });
                    if food {
                        ui.add_space(6.0);
                        crate::screens::rules_preview(ui, &applied(&self.lab), space, seed);
                    }
                });
                let mut now = self.lab.clone();
                now.take_rules(&current);
                let change = settings::describe_change(&now, &self.lab);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled_ui(change.is_some(), |ui| theme::primary_button(ui, "Применить"))
                        .inner
                        .clicked()
                        && let Some(note) = change.clone()
                    {
                        match self.lab.rules_over(&current, &now) {
                            Ok(rules) => {
                                self.sim.send(Command::SetRules { rules, note, world_gen });
                                self.toast("правила применены к этому миру".into());
                            }
                            Err(e) => self.error(format!("правила не применены: {e}")),
                        }
                    }
                    if ui
                        .add_enabled(change.is_some(), egui::Button::new("Вернуть как в мире"))
                        .on_hover_text("Забыть несохранённые правки: как сейчас в мире")
                        .clicked()
                    {
                        self.lab.take_rules(&current);
                    }
                    if ui
                        .add_enabled(
                            !self.lab_reset_selected.is_empty(),
                            egui::Button::new("Сбросить отмеченные"),
                        )
                        .clicked()
                    {
                        for key in self.lab_reset_selected.drain() {
                            self.lab.set(key, default.get(key));
                        }
                    }
                    // «Применить» changes this world only; new worlds start from the settings
                    let saved =
                        FIELDS.iter().all(|f| !f.live() || self.settings.get(f.key) == self.lab.get(f.key));
                    if ui
                        .add_enabled(!saved, egui::Button::new("Сохранить для новых миров"))
                        .on_hover_text(
                            "Записать эти правила в настройки «Нового мира»: с ними начнутся новые \
                             партии. «Заново» и «Новый сид» повторяют начало этой.",
                        )
                        .clicked()
                    {
                        for f in FIELDS.iter().filter(|f| f.live()) {
                            self.settings.set(f.key, self.lab.get(f.key));
                        }
                        self.save_settings();
                        self.toast("правила сохранены для новых миров".into());
                    }
                });
            }
        }
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

/// What the card's buttons asked for.
enum CardAction {
    None,
    Behaviour,
    Follow,
    Unselect,
}

/// The card of the selected creature: who it is and the buttons first, then how it is doing
/// (fullness, health, life lived) as bars, what it eats and how it behaves, and its genes folded.
/// `own`: its diet's median of each gene the panel follows (`DIET_GENES`), None for the rest.
fn creature_card(
    ui: &mut egui::Ui,
    s: &Selected,
    own: &[Option<f64>; N],
    rules: &Rules,
    world_h: f64,
    behaviour: bool,
    following: bool,
) -> CardAction {
    let mut act = CardAction::None;
    let diet_index = (s.genome[creature::Gene::Diet as usize] as usize).min(3);
    let diet = &creature::DIET_VARIANTS[diet_index];
    let color = rgb(theme::DIET_COLORS[diet_index]);
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color).size(18.0));
        ui.label(theme::heading(format!("Существо № {}", s.id), theme::HEADING));
    });
    ui.colored_label(MUTED, s.state);
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(behaviour, "Поведение (B)").on_hover_text("Блок-схема его программ").clicked()
        {
            act = CardAction::Behaviour;
        }
        if ui.selectable_label(following, "Следить (F)").on_hover_text("Камера идёт за ним").clicked()
        {
            act = CardAction::Follow;
        }
        if ui.button("Снять выбор").on_hover_text("Esc").clicked() {
            act = CardAction::Unselect;
        }
    });
    ui.add_space(4.0);

    let lifespan = s.genome[creature::Gene::Lifespan as usize].max(1.0);
    let fullness = (s.energy / s.max_energy).clamp(0.0, 1.0);
    let health = (s.health / s.max_health.max(1e-9)).clamp(0.0, 1.0);
    let lived = (s.age / lifespan).clamp(0.0, 1.0);
    let bar = |ui: &mut egui::Ui, frac: f64, fill: egui::Color32, text: String, hint: &str| {
        ui.add(egui::ProgressBar::new(frac as f32).fill(fill).desired_height(16.0).text(text))
            .on_hover_text(hint);
    };
    bar(
        ui,
        fullness,
        charts::energy_color(fullness, color),
        format!("сытость {:.0}% · расход {:.2} в тик", fullness * 100.0, s.upkeep),
        "Сколько энергии в баке от полного. Пустой бак — смерть от голода.",
    );
    bar(
        ui,
        health,
        charts::energy_color(health, GOOD),
        format!("здоровье {:.0} из {:.0}", s.health.max(0.0), s.max_health),
        "Удары отнимают здоровье; лечится по своей программе.",
    );
    bar(
        ui,
        lived,
        rgb(CREATURE_COLOR).gamma_multiply(0.7),
        format!("прожито {:.0}% · возраст {:.0} тиков", lived * 100.0, s.age),
        "Доля срока жизни. С 70% тело слабеет, на 100% — смерть от старости.",
    );
    ui.add_space(4.0);

    let program = &s.programs[s.stage];
    // how much smaller its prey, by the program it lives by now; only who digests fresh meat hunts
    let prey = match program.hunt_ratio() {
        Some(ratio) if rules.diets[diet_index].digestion[1] > 0.0 => {
            format!(" · добыча мельче в {} раза", format!("{ratio:.1}").replace('.', ","))
        }
        _ => String::new(),
    };
    ui.colored_label(color, format!("Питание: {}{prey}", diet.label)).on_hover_text(diet.about);
    ui.colored_label(MUTED, diet_bonuses(s.genome[creature::Gene::Diet as usize], rules));
    if let Some(food) = s.eating {
        use life_core::corpse::Stage;
        use life_core::creature::Morsel;
        ui.label(match food {
            Morsel::Plant => "ест растение",
            Morsel::Corpse { stage: Stage::Fresh } => "ест свежее мясо",
            Morsel::Corpse { stage: Stage::Rot } => "ест гниль",
            Morsel::Corpse { stage: Stage::Bones } => "грызёт кости",
        });
    }
    let template = life_core::creature::strategy::VARIANTS
        .get(s.genome[creature::Gene::Strategy as usize] as usize)
        .map_or("?", |v| v.label);
    let track = if s.stage == life_core::creature::JUVENILE { "детская" } else { "взрослая" };
    ui.colored_label(
        MUTED,
        format!(
            "Шаблон {template} · дорожка {track} · мутаций {} · блоков {}",
            program.changes,
            program.blocks().len()
        ),
    )
    .on_hover_text(
        "Две программы поведения: детская — пока растёт, взрослая — когда вырос. Шаблон — с какой \
         программы начинал род. Блок-схемы — по кнопке «Поведение (B)».",
    );
    ui.colored_label(
        MUTED,
        format!(
            "тело {:.1} из {:.1} · глубина {:.0}%",
            s.half * 2.0,
            s.genome[creature::Gene::Size as usize],
            s.y / world_h.max(1.0) * 100.0
        ),
    )
    // the same in real units (`life_core::units`): the life clock is compressed
    .on_hover_text(format!(
        "Растёт до размера своего гена. В настоящих единицах: {:.0} см, {:.1} года, глубина {:.0} м \
         (базовое существо — рыба в 20 см; тик жизни ≈ 9 часов).",
        units::cm(s.half * 2.0),
        units::years(s.age),
        units::metres(s.y)
    ));
    ui.add_space(4.0);
    egui::CollapsingHeader::new("Гены").default_open(false).show(ui, |ui| {
        ui.colored_label(MUTED, format!("справа — к медиане: {}", theme::DIET_NAMES[diet_index]));
        egui::Grid::new("карточка").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
            gene_rows(ui, &creature::GENES, &s.genome, own);
        });
    });
    act
}

/// A creature's genes by the table, against `own` (its diet's median, where known): who is this
/// one — bigger, more far-sighted than its kind? A choice gene with one variant is not shown.
fn gene_rows(ui: &mut egui::Ui, genes: &[GeneSpec], g: &[f64], own: &[Option<f64>; N]) {
    for (i, spec) in genes.iter().enumerate().filter(|(_, spec)| charts::shown(spec)) {
        ui.colored_label(MUTED, spec.label);
        if let Some(variants) = spec.variants() {
            ui.label(variants.get(g[i] as usize).map_or("?", |v| v.label));
            ui.label("");
            ui.end_row();
            continue;
        }
        let percent = spec.is_percent();
        // small values (speed) with tenths, big ones in whole numbers
        ui.label(match (percent, g[i] < 20.0) {
            (true, _) => format!("{:.0}%", g[i]),
            (false, true) => format!("{:.1}", g[i]),
            (false, false) => format!("{:.0}", g[i]),
        });
        let delta = own[i].map(|m| {
            if percent {
                format!("{:+.0} пунктов", g[i] - m)
            } else if m > 0.0 {
                format!("{:+.0}%", (g[i] / m - 1.0) * 100.0)
            } else {
                String::new()
            }
        });
        ui.colored_label(MUTED, delta.unwrap_or_default());
        ui.end_row();
    }
}

/// A diet's edges in the world's current rules (the lab may have changed them): «удар ×1,5 ·
/// скорость дешевле на 20%».
fn diet_bonuses(gene: f64, rules: &Rules) -> String {
    let e = &rules.diets[(gene.max(0.0) as usize).min(3)];
    let times = |x: f64| format!("{}", (x * 100.0).round() / 100.0).replace('.', ",");
    let cheaper = |name: &str, x: f64| {
        if x < 1.0 {
            format!("{name} дешевле на {:.0}%", (1.0 - x) * 100.0)
        } else {
            format!("{name} дороже на {:.0}%", (x - 1.0) * 100.0)
        }
    };
    let mut parts = Vec::new();
    if e.strike != 1.0 {
        parts.push(format!("удар ×{}", times(e.strike)));
    }
    if e.health != 1.0 {
        parts.push(format!("здоровье ×{}", times(e.health)));
    }
    if e.size_upkeep != 1.0 {
        parts.push(cheaper("размер", e.size_upkeep));
    }
    if e.speed_upkeep != 1.0 {
        parts.push(cheaper("скорость", e.speed_upkeep));
    }
    if e.smell != 1.0 {
        parts.push(format!("нюх ×{}", times(e.smell)));
    }
    if e.young_plants > e.digestion[0] {
        parts.push(format!("растения в детстве {:.0}%", e.young_plants * 100.0));
    }
    // bones feed only the scavenger by default: its own niche
    if e.digestion[3] > 0.0 {
        parts.push(format!("кости {:.0}%", e.digestion[3] * 100.0));
    }
    if parts.is_empty() { "без особых сил".into() } else { parts.join(" · ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn команда_отчёта_повторяет_мир() {
        let mut cfg = WorldConfig { seed: 42, scale: 10.0, ..Default::default() };
        cfg.rules = cfg.rules.with("plant_energy", 80.0).unwrap();
        let cmd = report_command(&cfg, 5000);
        assert!(cmd.contains("--seed 42"));
        assert!(cmd.contains("--ticks 5000"));
        assert!(cmd.contains("--scale 10"));
        assert!(!cmd.contains("--shape"), "форма по умолчанию не пишется");
        assert!(cmd.contains("--creatures 200"));
        assert!(cmd.contains("--rule plant_energy=80"));
        assert!(!cmd.contains("mutation_sigma"), "правила по умолчанию не пишутся");
        assert!(!cmd.contains("-mix"));

        let cfg = WorldConfig {
            shape: life_core::Shape::Square,
            strategies: vec![70.0, 30.0],
            rules: Rules::default().with("plant_width_profile", Profile::Waves.index()).unwrap(),
            ..Default::default()
        };
        let cmd = report_command(&cfg, 600);
        assert!(cmd.contains("--shape 1:1"));
        assert!(cmd.contains("--rule plant_width_profile=waves"));
        assert!(!cmd.contains("plant_depth"), "профиль по глубине не трогали");
        assert!(cmd.contains("--mix 70 30"));
    }
}
