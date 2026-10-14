//! The lab: the world's rules on the fly, applied to this world or saved for new ones.

use eframe::egui::{self, RichText};

use crate::app::LifeApp;
use crate::settings::{self, FIELDS, Tab};
use crate::sim::Command;
use crate::theme::{self, MUTED};

impl LifeApp {
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
                    ui.label(f.label).on_hover_text(crate::rules_ui::field_hint(f));
                    let mut v = self.lab.get(f.key);
                    if crate::rules_ui::field_input(ui, f, &mut v) {
                        self.lab.set(f.key, v);
                    }
                    crate::rules_ui::base_value(ui, f, default);
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
    pub(super) fn lab_page(&mut self, ui: &mut egui::Ui) {
        let Some((current, space, seed, world_gen)) = self.view.frame.as_ref().map(|f| {
            (
                f.rules.clone(),
                life_core::Space { width: f.world_w, height: f.world_h, terrain: f.terrain },
                f.seed,
                f.world_gen,
            )
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
        let body = self.lab_tab == Tab::Body;
        let diets = self.lab_tab == Tab::Diets;
        ui.horizontal_wrapped(|ui| {
            for (tab, name) in Tab::RULES {
                let name = crate::rules_ui::tab_name(ui.ctx(), tab, name);
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
                crate::rules_ui::diet_table(ui, &mut self.lab);
                ui.add_space(theme::GAP_WIDE);
                ui.label(theme::strong("Как меняется питание у потомков"));
                self.lab_grid(ui, Tab::Diets, &default);
                return;
            }
            // the body's formula under its fields, so the panel keeps its width
            ui.vertical(|ui| {
                if self.lab_tab == Tab::Water {
                    ui.label(crate::rules_ui::water_note(ui.ctx()));
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
                    crate::rules_ui::body_formula(ui, &applied(&self.lab));
                }
            });
            if food {
                ui.add_space(6.0);
                crate::rules_ui::rules_preview(ui, &applied(&self.lab), space, seed);
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
                .add_enabled(!self.lab_reset_selected.is_empty(), egui::Button::new("Сбросить отмеченные"))
                .clicked()
            {
                for key in self.lab_reset_selected.drain() {
                    self.lab.set(key, default.get(key));
                }
            }
            // «Применить» changes this world only; new worlds start from the settings
            let saved = FIELDS.iter().all(|f| !f.live() || self.settings.get(f.key) == self.lab.get(f.key));
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
