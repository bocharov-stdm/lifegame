//! The bars around the world: the top one (time, counts, the windows' buttons), the tool rail at
//! the left and the bottom strip (the world's menu and the timeline).

use eframe::egui::{self, RichText};
use life_core::profile::Phase;

use crate::app::{LifeApp, Tool};
use crate::charts;
use crate::frame::{CREATURE_COLOR, PLANT_COLOR};
use crate::sim::{Command, SPEEDS};
use crate::theme::{self, DANGER, Icon, MUTED, rgb, spaced};

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

impl LifeApp {
    /// The top bar: the menu, time (pause, a step, the speed), the counts, what state the world is
    /// in (chips), and on the right the windows: statistics, behaviour, lab, help, the side panel.
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
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
            } else if st.ended.is_none() {
                // the speed beside the arrows is the chosen one; this is what comes out
                ui.add_space(theme::GAP);
                ui.label(RichText::new(format!("{:.0} тиков/с", st.tps)).monospace().color(MUTED))
                    .on_hover_text("Сколько тиков в секунду мир делает на самом деле");
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
                        "{:.0} к/с · тик {tick_ms:.1} · срез {snapshot_ms:.1} · сборка {build_ms:.1} · рисунок {draw_ms:.1} мс",
                        self.fps()
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
    pub(super) fn tool_rail(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn bottom_bar(&mut self, ui: &mut egui::Ui) {
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
            if let Some(tick) = charts::timeline(ui, &self.history, &self.log, width) {
                self.sim.send(Command::SetPaused(true));
                self.toast(format!("пауза у события тика {}", spaced(tick)));
            }
        });
        ui.add_space(2.0);
    }
}
