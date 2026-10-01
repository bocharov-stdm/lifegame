//! The «Статистика» window: what the side panel does not show. Fullness, where the creatures live
//! and where the food grows, and a summary of the region dragged over the world. The data are
//! samples of the world (`Snapshot`), which the simulation thread takes anyway for the chronicle.

use eframe::egui::{self, RichText, Vec2};
use life_core::creature::Action;
use life_core::flora::Profile;
use life_core::genome::{GeneSpec, creature};
use life_sim::observe::GeneStat;

use crate::app::{LifeApp, Tool};
use crate::frame::CREATURE_COLOR;
use crate::sim::Command;
use crate::theme::{DANGER, DIET_COLORS, DIET_NAMES, GOOD, MUTED, TEXT, rgb, spaced};
use crate::{census, charts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsTab {
    Energy,
    Where,
    Region,
    /// How the characteristics spread within each diet: a census, taken only on pause.
    Species,
}

impl LifeApp {
    pub fn stats_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Статистика")
            .open(&mut open)
            .resizable(true)
            .default_size(Vec2::new(640.0, 460.0))
            .min_width(420.0)
            .default_pos(ctx.content_rect().left_top() + Vec2::new(40.0, 60.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.stats_tab, StatsTab::Energy, "Энергия");
                    ui.selectable_value(&mut self.stats_tab, StatsTab::Where, "Где живут");
                    ui.selectable_value(&mut self.stats_tab, StatsTab::Region, "Область");
                    ui.selectable_value(&mut self.stats_tab, StatsTab::Species, "Внутри видов");
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.stats_tab {
                    StatsTab::Energy => self.energy_tab(ui),
                    StatsTab::Where => self.where_tab(ui),
                    StatsTab::Region => self.region_tab(ui),
                    StatsTab::Species => self.species_tab(ui),
                });
            });
        self.stats_open &= open;
    }

    fn energy_tab(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(MUTED, "Последние 10 000 тиков");
        self.diets_line(ui);
        let snaps = self.history.snapshots.points();
        if let Some(s) = snaps.last() {
            ui.label(format!("Молодых {:.0}%", 100.0 * s.juveniles as f64 / s.creatures.max(1) as f64));
            if let Some(first) = snaps.first() {
                ui.label(life_sim::observe::describe_flows(&s.counters.since(&first.counters)));
            }
        }
        ui.label(RichText::new("Сытость").strong());
        charts::energy(ui, &snaps, 190.0);
        ui.add_space(4.0);
        ui.colored_label(
            MUTED,
            "Сытость — насколько в среднем полон бак. Растения у потолка — еды больше, чем успевают \
             съесть: существ держит не голод.",
        );
    }

    fn where_tab(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(MUTED, "Последние 10 000 тиков");
        let snaps = self.history.snapshots.points();
        ui.label(RichText::new("Глубина существ во времени").strong());
        let hovered = charts::depth_map(ui, &snaps, 170.0);
        let Some(&at) = hovered.and_then(|i| snaps.get(i)).or(snaps.last()) else { return };
        let layer = at.depth.map_or("существ нет".into(), |d| {
            format!("80% существ на глубине {:.0}‒{:.0}%, медиана {:.0}%", d.p10, d.p90, d.p50)
        });
        ui.colored_label(MUTED, format!("тик {} · {layer}; верх — поверхность", spaced(at.tick)));
        ui.add_space(8.0);
        ui.label(RichText::new("Растения и существа по глубине").strong());
        ui.colored_label(MUTED, "слева — доля растений, справа — доля существ");
        charts::bands(ui, &at.plants_by_depth, &at.creatures_by_depth, ("поверхность", "дно"));
        // across the width there is something to look at only if the food is uneven along it
        let uneven = self.view.frame.as_ref().is_some_and(|f| f.rules.plant_width.kind() != Profile::Uniform);
        if uneven {
            ui.add_space(8.0);
            ui.label(RichText::new("По ширине").strong());
            charts::bands(ui, &at.plants_by_width, &at.creatures_by_width, ("слева", "справа"));
        }
    }

    fn region_tab(&mut self, ui: &mut egui::Ui) {
        let Some(r) = self.region.clone() else {
            ui.colored_label(
                MUTED,
                "Выберите внизу инструмент «Область» и протяните мышью прямоугольник по миру: \
                 здесь появится средний геном тех, кто внутри, рядом со средним по всему миру.",
            );
            if self.tool != Tool::Area && ui.button("Взять инструмент «Область»").clicked()
            {
                self.tool = Tool::Area;
            }
            return;
        };
        let (w, h) = (r.area.2 - r.area.0, r.area.3 - r.area.1);
        let depth = self.view.frame.as_ref().map_or(1.0, |f| f.world_h).max(1.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Область {w:.0}×{h:.0}, глубина {:.0}‒{:.0}% · тик {}",
                r.area.1 / depth * 100.0,
                r.area.3 / depth * 100.0,
                spaced(r.tick)
            ));
            if ui.button("Снять область").clicked() {
                self.clear_region();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("растений {}", spaced(r.plants as u64)));
            ui.colored_label(rgb(CREATURE_COLOR), format!("существ {}", spaced(r.creatures as u64)));
            if let Some(f) = r.fullness {
                ui.colored_label(MUTED, format!("сытость {:.0}%", f * 100.0));
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Геном существ").strong());
        compare(ui, "область-существа", &creature::GENES, r.inside.as_ref(), r.world.as_ref());
        ui.add_space(6.0);
        ui.colored_label(MUTED, "Сводка обновляется на каждом срезе мира и сразу, когда область задана.");
    }

    fn species_tab(&mut self, ui: &mut egui::Ui) {
        let Some((world_gen, tick, edits, still)) = self
            .view
            .frame
            .as_ref()
            .map(|f| (f.world_gen, f.tick, f.edits, f.status.paused || f.status.ended.is_some()))
        else {
            return;
        };
        if !still {
            ui.colored_label(
                MUTED,
                "Распределения признаков внутри видов считаются только на паузе: перепись всех существ \
                 не должна отнимать время у тиков.",
            );
            if ui.button("Поставить на паузу").clicked() {
                self.sim.send(Command::SetPaused(true));
            }
            return;
        }
        // a creature planted or new rules on pause change the world without a tick: counted again
        let key = (world_gen, tick, edits);
        let Some(census) = self.census.as_ref().filter(|c| (c.world_gen, c.tick, c.edits) == key) else {
            if self.census_asked != Some(key) {
                self.census_asked = Some(key);
                self.sim.send(Command::Census);
            }
            ui.colored_label(MUTED, "считаю…");
            return;
        };

        ui.horizontal_wrapped(|ui| {
            for g in 0..census::GROUPS {
                let (name, color) = match g {
                    0 => ("все", rgb(CREATURE_COLOR)),
                    d => (DIET_NAMES[d - 1], rgb(DIET_COLORS[d - 1])),
                };
                let text =
                    RichText::new(format!("{name} {}", spaced(census.groups[g].count as u64))).color(color);
                if ui.selectable_label(self.census_group == g, text).clicked() {
                    self.census_group = g;
                }
            }
        });
        let g = self.census_group;
        let group = &census.groups[g];
        let color = match g {
            0 => rgb(CREATURE_COLOR),
            d => rgb(DIET_COLORS[d - 1]),
        };
        ui.colored_label(
            MUTED,
            format!("тик {} · перепись на паузе, шаг вперёд — новая", spaced(census.tick)),
        );
        if group.count == 0 {
            ui.colored_label(MUTED, "таких существ сейчас нет");
            return;
        }

        ui.add_space(4.0);
        ui.label(RichText::new("Признаки").strong());
        let columns: Vec<usize> = census::numeric().collect();
        charts::histograms(ui, group, &columns, color);
        for c in census::choices() {
            charts::shares(ui, &creature::GENES[c], &group.columns[c]);
        }
        ui.colored_label(
            MUTED,
            "Столбик — сколько существ с таким значением, черта — медиана, полоска внизу — где 80%. \
             Два горба — два подвида.",
        );

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Два признака").strong());
            for (k, axis) in ["по горизонтали", "по вертикали"].into_iter().enumerate()
            {
                egui::ComboBox::from_id_salt(("перепись-ось", k))
                    .selected_text(census::label(self.census_axes[k]))
                    .show_ui(ui, |ui| {
                        for c in census::numeric() {
                            ui.selectable_value(&mut self.census_axes[k], c, census::label(c));
                        }
                    })
                    .response
                    .on_hover_text(axis);
            }
        });
        charts::scatter(ui, census, g, self.census_axes, 170.0);
        ui.colored_label(
            MUTED,
            "Отдельные облака — отдельные подвиды; цвет — питание, яркость — сколько их там.",
        );

        ui.add_space(8.0);
        ui.label(RichText::new("Поведение взрослых").strong());
        let n = group.count as f64;
        for b in &group.behaviours {
            behaviour_row(ui, b.count as f64 / n, &chain(&b.chain), color);
        }
        let rest = group.count - group.behaviours.iter().map(|b| b.count).sum::<usize>();
        if rest > 0 {
            behaviour_row(ui, rest as f64 / n, "прочие", MUTED);
        }
        ui.colored_label(
            MUTED,
            "Существа с одинаковыми решающими блоками взрослой программы в одном порядке — \
             условия и числа у них могут различаться.",
        );
    }

    /// Clear the region: both the frame in the world and the summary.
    pub fn clear_region(&mut self) {
        self.region = None;
        self.view.area = None;
        self.view.cancel_area_drag();
        self.tool = Tool::Select;
        let world_gen = self.view.frame.as_ref().map_or(0, |f| f.world_gen);
        self.sim.send(Command::SetRegion { area: None, world_gen });
    }

    /// A new region has been dragged: the thread will compute the summary, the window opens on it.
    pub fn set_region(&mut self, area: crate::frame::Area) {
        self.view.area = Some(area);
        self.region = None;
        self.tool = Tool::Select;
        let world_gen = self.view.frame.as_ref().map_or(0, |f| f.world_gen);
        self.sim.send(Command::SetRegion { area: Some(area), world_gen });
        self.stats_open = true;
        self.stats_tab = StatsTab::Region;
    }
}

/// A behaviour group's line: its share as a bar and a number, then its chain of actions.
fn behaviour_row(ui: &mut egui::Ui, share: f64, text: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 10.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, MUTED.gamma_multiply(0.25));
        let filled =
            egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * share as f32, rect.height()));
        ui.painter().rect_filled(filled, 2.0, color.gamma_multiply(0.8));
        ui.add_sized(
            [34.0, 16.0],
            egui::Label::new(
                RichText::new(if share < 0.005 { "<1%".into() } else { format!("{:.0}%", share * 100.0) })
                    .color(TEXT),
            ),
        );
        ui.add(egui::Label::new(RichText::new(text).color(MUTED)).wrap());
    });
}

/// The deciding actions in order: «дать сдачи → убегать → …».
fn chain(actions: &[Action]) -> String {
    if actions.is_empty() {
        return "ничего не решает — стоит".into();
    }
    actions.iter().map(|a| a.label()).collect::<Vec<_>>().join(" → ")
}

/// The gene table: the mean in the region, the mean over the world and the difference. For a
/// choice gene, the share of the variant most common in the region.
fn compare<const N: usize>(
    ui: &mut egui::Ui,
    id: &str,
    table: &[GeneSpec; N],
    inside: Option<&[GeneStat; N]>,
    world: Option<&[GeneStat; N]>,
) {
    let Some(inside) = inside else {
        ui.colored_label(MUTED, "в области никого нет");
        return;
    };
    egui::Grid::new(id).num_columns(4).spacing([16.0, 4.0]).striped(true).show(ui, |ui| {
        for head in ["ген", "в области", "по миру", "разница"] {
            ui.colored_label(MUTED, head);
        }
        ui.end_row();
        for (g, spec) in table.iter().enumerate().filter(|(_, s)| charts::shown(s)) {
            ui.label(spec.label);
            let (here, there, diff) = row(spec, &inside[g], world.map(|w| &w[g]));
            ui.label(RichText::new(here).color(TEXT));
            ui.colored_label(MUTED, there);
            let color = match diff.chars().next() {
                Some('+') => GOOD,
                Some('−') | Some('-') => DANGER,
                _ => MUTED,
            };
            ui.colored_label(color, diff);
            ui.end_row();
        }
    });
}

/// The signed difference, in whole numbers: no «-0» when there is no difference.
fn signed(v: f64, unit: &str) -> String {
    let v = v.round();
    if v == 0.0 { format!("0{unit}") } else { format!("{v:+.0}{unit}") }
}

/// A table row: (in the region, over the world, the difference).
fn row(spec: &GeneSpec, inside: &GeneStat, world: Option<&GeneStat>) -> (String, String, String) {
    let percent = spec.is_percent();
    let number = |v: f64| match (percent, v.abs() < 20.0) {
        (true, _) => format!("{v:.0}%"),
        (false, true) => format!("{v:.1}"),
        (false, false) => format!("{v:.0}"),
    };
    match (inside, world) {
        (GeneStat::Number(a), world) => {
            let b = world.and_then(|w| w.spread()).map(|s| s.mean);
            let diff = match b {
                Some(b) if percent => signed(a.mean - b, " п.п."),
                Some(b) if b > 0.0 => signed((a.mean / b - 1.0) * 100.0, "%"),
                _ => String::new(),
            };
            (number(a.mean), b.map(number).unwrap_or_default(), diff)
        }
        (GeneStat::Shares(a), world) => {
            let variants = spec.variants().unwrap_or_default();
            let Some(k) = (0..variants.len()).max_by(|&i, &j| a[i].total_cmp(&a[j])) else {
                return Default::default();
            };
            let b = world.and_then(|w| w.shares()).map(|s| s[k]);
            let diff = b.map(|b| signed((a[k] - b) * 100.0, " п.п.")).unwrap_or_default();
            (
                format!("{} {:.0}%", variants[k].label, a[k] * 100.0),
                b.map(|b| format!("{:.0}%", b * 100.0)).unwrap_or_default(),
                diff,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::RegionStats;
    use life_core::genome::creature::Gene;
    use life_core::{World, WorldConfig};
    use life_sim::observe::Snapshot;

    /// The region's summary counts exactly those inside, and the world's counts all.
    #[test]
    fn область_считает_только_тех_кто_внутри() {
        let mut w = World::new(&WorldConfig { seed: 2, ..Default::default() });
        for _ in 0..300 {
            w.step();
        }
        let (x, y) = (w.space.width / 2.0, w.space.height / 2.0);
        let area = (0.0, 0.0, x, y);
        let r = RegionStats::of(&w, area, None);
        let inside: Vec<_> = w.creatures.iter().filter(|v| v.x <= x && v.y <= y).collect();
        assert_eq!(r.creatures, inside.len());
        assert_eq!(r.plants, w.plants.iter().filter(|p| p.x <= x && p.y <= y).count());
        let size = |s: &Option<[GeneStat; creature::N]>| {
            s.as_ref().and_then(|g| g[Gene::Size as usize].spread().map(|s| s.mean))
        };
        let mean = inside.iter().map(|v| v.genome[Gene::Size]).sum::<f64>() / inside.len().max(1) as f64;
        if !inside.is_empty() {
            assert!((size(&r.inside).unwrap() - mean).abs() < 1e-9);
        }
        let all = w.creatures.iter().map(|v| v.genome[Gene::Size]).sum::<f64>() / w.creatures.len() as f64;
        assert!((size(&r.world).unwrap() - all).abs() < 1e-9);
        // the whole world — the same summary inside and outside
        let whole = RegionStats::of(&w, (0.0, 0.0, w.space.width, w.space.height), None);
        assert_eq!(whole.inside, whole.world);
        assert_eq!(whole.creatures, w.creatures.len());
    }

    #[test]
    fn строка_таблицы_сравнивает_среднее_и_доли() {
        use life_sim::observe::Spread;
        let spec = &creature::GENES[Gene::Size as usize];
        let at = |mean| GeneStat::Number(Spread { p10: mean, p50: mean, p90: mean, mean });
        assert_eq!(row(spec, &at(60.0), Some(&at(40.0))), ("60".into(), "40".into(), "+50%".into()));
        let maturation = &creature::GENES[Gene::Maturation as usize];
        assert_eq!(row(maturation, &at(30.0), Some(&at(40.0))).2, "-10 п.п.");
        let strategy = &creature::GENES[Gene::Strategy as usize];
        let mut a = [0.0; life_sim::observe::MAX_VARIANTS];
        a[1] = 0.75;
        a[0] = 0.25;
        let mut b = a;
        b[1] = 0.5;
        let (here, there, diff) = row(strategy, &GeneStat::Shares(a), Some(&GeneStat::Shares(b)));
        assert!(here.ends_with("75%") && there == "50%" && diff == "+25 п.п.", "{here} {there} {diff}");
        assert_eq!(row(spec, &at(40.1), Some(&at(40.0))).2, "0%", "без «-0» и «+0»");
    }

    #[test]
    fn снимок_без_существ_не_роняет_окно() {
        // an empty world — a sample without genes; the tabs must survive it (checked in ui_tests),
        // and the region's summary is empty
        let w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let r = RegionStats::of(&w, (0.0, 0.0, 100.0, 100.0), None);
        assert!(r.inside.is_none() && r.world.is_none() && r.fullness.is_none());
        let s = Snapshot::of(&w);
        assert!(s.genes.is_none());
    }
}
