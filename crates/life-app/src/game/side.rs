//! The side panel: its tabs, the overview (populations, kills, the genome), the chronicle and the
//! command that repeats a game without a window.

use eframe::egui::{self, RichText};
use life_core::flora::Profile;
use life_core::genome::creature;
use life_core::{Rules, WorldConfig};
use life_sim::observe::EventKind;

use crate::app::{LifeApp, SideTab};
use crate::charts;
use crate::frame::CREATURE_COLOR;
use crate::history::History;
use crate::sim::Command;
use crate::theme::{ACCENT, DANGER, GOOD, MUTED, TEXT, rgb, spaced};

/// The share of shooters in the last snapshot, %; None while its genes are unknown (no creatures).
pub(crate) fn shooters_share(history: &History) -> Option<f64> {
    let last = history.snapshots.last()?;
    last.genes.as_ref()?;
    Some(last.shooters as f64 / last.creatures as f64 * 100.0)
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
    if cfg.terrain != base.terrain {
        cmd += &format!(" --terrain {}", cfg.terrain.key());
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
    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui) {
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
            if let Some(shooters) = shooters_share(&self.history) {
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
            charts::populations(ui, &self.history);
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
