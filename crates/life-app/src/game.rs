//! Экран игры: верхняя панель (темп и численности), нижняя (инструменты),
//! мир, боковая панель (графики, хроника, существо), лаборатория на ходу,
//! конец игры.

use eframe::egui::{self, Align2, Key, RichText, Vec2};
use life_core::genome::vegetarian::N;
use life_core::genome::{GeneSpec, predator, vegetarian};
use life_core::{Creature, Rules, WorldConfig};
use life_sim::observe::EventKind;

use crate::app::{LifeApp, SideTab, Tool};
use crate::charts;
use crate::frame::{Ending, PLANT_COLOR, PREDATOR_COLOR, Selected, VEGETARIAN_COLOR};
use crate::settings::{self, FIELDS};
use crate::sim::{Command, SPEEDS};
use crate::theme::{self, ACCENT, DANGER, GOOD, MUTED, TEXT, rgb, spaced};
use crate::view::{Click, creature_id};

/// Скорость панорамы клавишами, точек экрана в секунду.
const PAN_SPEED: f64 = 900.0;

fn speed_label(index: usize) -> String {
    match SPEEDS[index] {
        Some(tps) => format!("{tps:.0} т/с"),
        None => "максимум".into(),
    }
}

/// Команда `life-report`, которая повторяет партию без окна.
pub fn report_command(cfg: &WorldConfig, ticks: u64) -> String {
    let mut cmd =
        format!("cargo run -p life-report --release -- --seed {} --ticks {}", cfg.seed, ticks.max(600));
    if cfg.scale != 1.0 {
        cmd += &format!(" --scale {}", cfg.scale);
    }
    cmd += &format!(" --vegetarians {} --predators {}", cfg.vegetarians_at_start(), cfg.predators_at_start());
    let base = WorldConfig::default();
    if cfg.predator_speed != base.predator_speed {
        cmd += &format!(" --predator-speed {}", cfg.predator_speed);
    }
    if cfg.predator_vision != base.predator_vision {
        cmd += &format!(" --predator-vision {}", cfg.predator_vision);
    }
    let default = Rules::default();
    for key in life_core::rules::RULE_KEYS {
        let (v, d) = (cfg.rules.get(key), default.get(key));
        if let Some(v) = v.filter(|v| Some(*v) != d) {
            cmd += &format!(" --rule {key}={v}");
        }
    }
    cmd
}

impl LifeApp {
    pub fn game_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.game_keyboard(&ctx);

        egui::Panel::top("верх").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("низ").show(ui, |ui| self.bottom_bar(ui));
        if self.side_open {
            egui::Panel::right("сбоку")
                .default_size(400.0)
                .size_range(320.0..=720.0)
                .show(ui, |ui| self.side_panel(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            if let Some(Click::World { x, y, radius }) = self.view.show(ui, rect, &self.sim, true) {
                match self.tool {
                    Tool::Select => {
                        self.sim.send(Command::Pick { x, y, radius });
                        self.side_tab = SideTab::Creature;
                    }
                    Tool::SpawnVegetarian => self.sim.send(Command::Spawn { predator: false, x, y }),
                    Tool::SpawnPredator => self.sim.send(Command::Spawn { predator: true, x, y }),
                }
            }
            if self.tool != Tool::Select {
                ui.painter().text(
                    rect.center_top() + Vec2::new(0.0, 14.0),
                    Align2::CENTER_TOP,
                    "клик по миру — подсадить; Esc — обычный выбор",
                    egui::FontId::proportional(14.0),
                    ACCENT,
                );
            }
        });
        if self.lab_open {
            self.lab_window(&ctx);
        }
        self.ending_window(&ctx);
    }

    fn game_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
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
                    p(Key::Escape),
                ],
                [i.key_down(Key::W), i.key_down(Key::S), i.key_down(Key::A), i.key_down(Key::D)],
                i.stable_dt.min(0.1) as f64,
            )
        });
        let [space, step, faster, slower, home, follow, tab, lab, escape] = keys;
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
            self.lab_open = !self.lab_open;
        }
        if escape {
            if self.tool != Tool::Select {
                self.tool = Tool::Select;
            } else if self.lab_open {
                self.lab_open = false;
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

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.view.frame else {
            ui.label("Создаём мир…");
            return;
        };
        let (st, tick, counts) = (f.status, f.tick, [f.plants, f.vegetarians, f.predators]);
        let (tick_ms, build_ms) = (f.tick_ms, f.build_ms);
        ui.horizontal(|ui| {
            let (icon, hint) = if st.paused {
                ("▶", "Пуск (Пробел)")
            } else {
                ("⏸", "Пауза (Пробел)")
            };
            if ui.button(icon).on_hover_text(hint).clicked() {
                self.sim.send(Command::TogglePause);
            }
            if ui.add_enabled(st.paused, egui::Button::new("⏭")).on_hover_text("Один тик (→)").clicked()
            {
                self.sim.send(Command::Step);
            }
            if ui
                .add_enabled(st.speed_index > 0, egui::Button::new("−"))
                .on_hover_text("Медленнее (−)")
                .clicked()
            {
                self.sim.send(Command::SetSpeed(st.speed_index - 1));
            }
            ui.label(speed_label(st.speed_index));
            if ui
                .add_enabled(st.speed_index + 1 < SPEEDS.len(), egui::Button::new("+"))
                .on_hover_text("Быстрее (+)")
                .clicked()
            {
                self.sim.send(Command::SetSpeed(st.speed_index + 1));
            }
            ui.separator();
            ui.label(format!("тик {}", spaced(tick)));
            ui.colored_label(rgb(PLANT_COLOR), format!("растения {}", spaced(counts[0] as u64)));
            ui.colored_label(rgb(VEGETARIAN_COLOR), format!("травоядные {}", spaced(counts[1] as u64)));
            ui.colored_label(rgb(PREDATOR_COLOR), format!("хищники {}", spaced(counts[2] as u64)));
            ui.separator();
            if st.lagging {
                let target = SPEEDS[st.speed_index].unwrap_or(0.0);
                ui.colored_label(DANGER, format!("отстаёт: {:.0} из {target:.0} т/с", st.tps)).on_hover_text(
                    "Тик не успевает за выбранной скоростью: мир большой или скорость высокая. \
                     Окно при этом не тормозит.",
                );
            } else if st.paused {
                ui.colored_label(MUTED, "пауза");
            } else {
                ui.colored_label(MUTED, format!("{:.0} т/с", st.tps));
            }
            if self.settings.show_fps {
                ui.colored_label(
                    MUTED,
                    format!("{:.0} к/с · тик {tick_ms:.1} мс · кадр мира {build_ms:.1} мс", self.fps()),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("☰ Меню").on_hover_text("Меню (Esc)").clicked() {
                    self.open_menu();
                }
            });
        });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tool, Tool::Select, "Выбор")
                .on_hover_text("Клик по существу — выбрать");
            ui.selectable_value(&mut self.tool, Tool::SpawnVegetarian, "+ травоядное")
                .on_hover_text("Подсадить базовое травоядное кликом по миру");
            ui.selectable_value(&mut self.tool, Tool::SpawnPredator, "+ хищник")
                .on_hover_text("Подсадить хищника кликом по миру");
            ui.separator();
            if ui.button("Весь мир").on_hover_text("Показать весь мир (Home)").clicked()
                && let Some(cam) = &mut self.view.camera
            {
                cam.fit();
            }
            ui.toggle_value(&mut self.lab_open, "Лаборатория").on_hover_text("Правила мира на ходу (L)");
            ui.toggle_value(&mut self.side_open, "Панель").on_hover_text("Графики, хроника, существо (Tab)");
            if ui
                .button("Заново")
                .on_hover_text("Та же партия с начала: тот же сид и стартовые правила")
                .clicked()
            {
                self.restart();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(f) = &self.view.frame {
                    ui.colored_label(MUTED, format!("сид {} · масштаб ×{}", f.seed, f.scale));
                }
            });
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.side_tab, SideTab::Charts, "Графики");
            ui.selectable_value(&mut self.side_tab, SideTab::Log, "Хроника");
            ui.selectable_value(&mut self.side_tab, SideTab::Creature, "Существо");
        });
        ui.separator();
        match self.side_tab {
            SideTab::Charts => self.charts_tab(ui),
            SideTab::Log => self.log_tab(ui),
            SideTab::Creature => self.creature_tab(ui),
        }
    }

    fn charts_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.whole, false, "Недавнее");
                ui.selectable_value(&mut self.whole, true, "Вся партия");
            });
            ui.label(RichText::new("Численность").strong());
            charts::populations(ui, &self.history, self.whole, 150.0);
            ui.add_space(8.0);
            ui.label(RichText::new("Геном травоядных").strong());
            charts::genome(ui, &self.history, self.whole, 30.0);
            ui.add_space(8.0);
            self.research(ui);
        });
    }

    /// Для исследователя: как повторить партию без окна.
    fn research(&mut self, ui: &mut egui::Ui) {
        let (Some(game), Some(f)) = (&self.game, &self.view.frame) else { return };
        ui.collapsing("Повторить без окна", |ui| {
            let cmd = report_command(&game.start, f.tick);
            ui.label(RichText::new(&cmd).monospace().size(11.5));
            if game.rules_changed {
                ui.colored_label(
                    DANGER,
                    "Правила меняли на ходу: повтор совпадёт только до первого изменения.",
                );
            }
            if ui.button("Скопировать команду").clicked() {
                ui.ctx().copy_text(cmd);
                self.toast = Some(("команда скопирована".into(), 2.0));
            }
        });
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        if self.log.is_empty() {
            ui.colored_label(
                MUTED,
                "Пока ничего не случилось. Здесь появятся обвалы и подъёмы численности, \
                 вымирания, сдвиги генов, мигранты и ваши вмешательства.",
            );
            return;
        }
        ui.colored_label(MUTED, "Клик по записи — пауза.");
        let mut pause = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for e in self.log.iter().rev() {
                let color = match e.kind {
                    Some(EventKind::VegetariansCrash | EventKind::PredatorsCrash)
                    | Some(EventKind::VegetariansExtinct | EventKind::PredatorsExtinct) => DANGER,
                    Some(
                        EventKind::VegetariansRise | EventKind::PredatorsRise | EventKind::PredatorsReturn,
                    ) => GOOD,
                    Some(EventKind::GeneShift | EventKind::StrategyShift) => rgb(VEGETARIAN_COLOR),
                    Some(EventKind::PredatorGeneShift) => rgb(PREDATOR_COLOR),
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

    fn creature_tab(&mut self, ui: &mut egui::Ui) {
        let Some(s) = self.view.frame.as_ref().and_then(|f| f.selected) else {
            ui.colored_label(MUTED, "Никто не выбран. Кликните по существу в мире.");
            return;
        };
        let avg = self.history.counts.last().and_then(|p| p.genom);
        creature_card(ui, &s, avg);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let following = self.view.following();
            if ui.selectable_label(following, "Следить (F)").clicked() {
                self.view.toggle_follow();
            }
            if ui.button("Снять выбор").clicked() {
                self.sim.send(Command::Select(None));
                if let Some(cam) = &mut self.view.camera {
                    cam.follow(None, None);
                }
            }
        });
    }

    fn lab_window(&mut self, ctx: &egui::Context) {
        let Some(current) = self.view.frame.as_ref().map(|f| f.rules.clone()) else { return };
        let mut open = true;
        egui::Window::new("Лаборатория")
            .open(&mut open)
            .resizable(false)
            .default_pos(ctx.content_rect().right_top() + Vec2::new(-460.0, 60.0))
            .show(ctx, |ui| {
                ui.colored_label(
                    MUTED,
                    "Правила мира прямо в партии. Живые существа сразу платят по новым ценам.",
                );
                ui.add_space(4.0);
                egui::Grid::new("лаборатория").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    for f in FIELDS.iter().filter(|f| f.live()) {
                        ui.label(f.label).on_hover_text(f.hint);
                        let mut v = self.lab.get(f.key);
                        let slider = egui::Slider::new(&mut v, f.lo..=f.hi)
                            .step_by(f.step)
                            .custom_formatter(|v, _| (f.format)(v));
                        if ui.add(slider).on_hover_text(f.hint).changed() {
                            self.lab.set(f.key, v);
                        }
                        ui.end_row();
                    }
                });
                let mut now = self.lab.clone();
                now.take_rules(&current);
                let change = settings::describe_change(&now, &self.lab);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(change.is_some(), theme::primary("Применить")).clicked()
                        && let Some(note) = change.clone()
                    {
                        self.sim.send(Command::SetRules { rules: self.lab.rules(), note });
                        if let Some(g) = &mut self.game {
                            g.rules_changed = true;
                        }
                    }
                    if ui.add_enabled(change.is_some(), egui::Button::new("Отменить")).clicked() {
                        self.lab.take_rules(&current);
                    }
                    if ui.button("По умолчанию").clicked() {
                        self.lab.take_rules(&Rules::default());
                    }
                });
            });
        self.lab_open &= open;
    }

    fn ending_window(&mut self, ctx: &egui::Context) {
        let Some(ended) = self.view.frame.as_ref().and_then(|f| f.status.ended) else { return };
        let (title, text) = match ended {
            Ending::Extinct => ("Все вымерли", "В мире не осталось ни травоядных, ни хищников."),
            Ending::Explosion => (
                "Взрыв численности",
                "Существ стало так много, что мир почти наверняка пошёл вразнос. \
                 Можно продолжить — но тик станет медленным.",
            ),
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(text);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary("Заново")).clicked() {
                        self.restart();
                    }
                    if ended == Ending::Explosion && ui.button("Продолжить всё равно").clicked()
                    {
                        self.sim.send(Command::KeepGoing);
                    }
                    if ui.button("Новый мир").clicked() {
                        self.screen = crate::app::Screen::Setup;
                    }
                });
            });
    }
}

/// Карточка выбранного существа: вид, энергия, состояние, гены.
fn creature_card(ui: &mut egui::Ui, s: &Selected, avg: Option<[f64; N]>) {
    let is_veg = matches!(s.creature, Creature::Vegetarian(_));
    let color = rgb(if is_veg { VEGETARIAN_COLOR } else { PREDATOR_COLOR });
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color).size(18.0));
        ui.label(RichText::new(if is_veg { "Травоядное" } else { "Хищник" }).strong().size(17.0));
        ui.colored_label(MUTED, format!("№ {}", creature_id(s.creature)));
    });
    let frac = (s.energy / s.max_energy).clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        ui.colored_label(MUTED, "энергия");
        ui.label(format!("{:.0} / {:.0}", s.energy.max(0.0), s.max_energy));
        ui.colored_label(MUTED, format!("расход {:.2} в тик", s.upkeep));
    });
    ui.add(egui::ProgressBar::new(frac as f32).fill(charts::energy_color(frac, color)).desired_height(6.0));
    let state = if s.fleeing {
        Some(("убегает от хищника", DANGER))
    } else if !is_veg {
        Some(if s.hungry {
            ("голоден — охотится", DANGER)
        } else {
            ("сыт — бродит", GOOD)
        })
    } else {
        None
    };
    if let Some((text, c)) = state {
        ui.colored_label(c, text);
    }
    ui.add_space(6.0);

    egui::Grid::new("карточка").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
        match (s.genome, s.predator_genome) {
            (Some(g), _) => gene_rows(ui, &vegetarian::GENES, &g, avg.as_ref()),
            (None, Some(g)) => gene_rows(ui, &predator::GENES, &g, None),
            (None, None) => {}
        }
    });
}

/// Гены существа по таблице вида. `avg` — средний геном популяции: кто этот —
/// крупнее, дальнозорче? Ген-выбор с одним вариантом не показывается.
fn gene_rows(ui: &mut egui::Ui, genes: &[GeneSpec], g: &[f64], avg: Option<&[f64; N]>) {
    for (i, spec) in genes.iter().enumerate().filter(|(_, spec)| charts::shown(spec)) {
        ui.colored_label(MUTED, spec.label);
        if let Some(variants) = spec.variants() {
            ui.label(variants.get(g[i] as usize).map_or("?", |v| v.label));
            ui.label("");
            ui.end_row();
            continue;
        }
        let percent = spec.is_percent();
        // мелкие величины (скорость) — с десятыми, крупные — целыми
        ui.label(match (percent, g[i] < 20.0) {
            (true, _) => format!("{:.0}%", g[i]),
            (false, true) => format!("{:.1}", g[i]),
            (false, false) => format!("{:.0}", g[i]),
        });
        let delta = avg.map(|a| {
            if percent {
                format!("{:+.0} п.п. к среднему", g[i] - a[i])
            } else if a[i] > 0.0 {
                format!("{:+.0}% к среднему", (g[i] / a[i] - 1.0) * 100.0)
            } else {
                String::new()
            }
        });
        ui.colored_label(MUTED, delta.unwrap_or_default());
        ui.end_row();
    }
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
        assert!(cmd.contains("--vegetarians 200 --predators 60"));
        assert!(cmd.contains("--rule plant_energy=80"));
        assert!(!cmd.contains("mutation_sigma"), "правила по умолчанию не пишутся");
        assert!(!cmd.contains("--predator-speed"));
    }
}
