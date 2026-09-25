//! Окно «Статистика»: то, чего не видно на боковой панели. Сытость, где живут
//! существа и где растёт еда, и сводка по области, протянутой по миру. Данные — срезы мира (`Snapshot`), которые поток
//! симуляции и так снимает для хроники.

use eframe::egui::{self, RichText, Vec2};
use life_core::flora::Profile;
use life_core::genome::{GeneSpec, creature};
use life_sim::observe::GeneStat;

use crate::app::{LifeApp, Tool};
use crate::charts;
use crate::frame::CREATURE_COLOR;
use crate::sim::Command;
use crate::theme::{DANGER, GOOD, MUTED, TEXT, rgb, spaced};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsTab {
    Energy,
    Where,
    Region,
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
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.stats_tab {
                    StatsTab::Energy => self.energy_tab(ui),
                    StatsTab::Where => self.where_tab(ui),
                    StatsTab::Region => self.region_tab(ui),
                });
            });
        self.stats_open &= open;
    }

    fn energy_tab(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(MUTED, "Последние 10 000 тиков");
        self.predator_status(ui);
        let snaps = self.history.snapshots.points();
        if let Some(s) = snaps.last() {
            ui.label(format!(
                "Молодых {:.0}% · стай {}",
                100.0 * s.juveniles as f64 / s.creatures.max(1) as f64,
                s.flocks
            ));
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
        // по ширине смотреть есть на что, только если еда по ней неравномерна
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

    /// Снять область: и рамку в мире, и сводку.
    pub fn clear_region(&mut self) {
        self.region = None;
        self.view.area = None;
        self.view.cancel_area_drag();
        self.tool = Tool::Select;
        self.sim.send(Command::SetRegion(None));
    }

    /// Протянута новая область: сводку посчитает поток, окно откроется на ней.
    pub fn set_region(&mut self, area: crate::frame::Area) {
        self.view.area = Some(area);
        self.region = None;
        self.tool = Tool::Select;
        self.sim.send(Command::SetRegion(Some(area)));
        self.stats_open = true;
        self.stats_tab = StatsTab::Region;
    }
}

/// Таблица генов: среднее в области, среднее по миру и разница. У гена-выбора
/// — доля самого частого в области варианта.
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

/// Разница со знаком, целыми: без «-0», когда разницы нет.
fn signed(v: f64, unit: &str) -> String {
    let v = v.round();
    if v == 0.0 { format!("0{unit}") } else { format!("{v:+.0}{unit}") }
}

/// Строка таблицы: (в области, по миру, разница).
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

    /// Сводка по области — ровно по тем, кто внутри, а по всему миру — по всем.
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
        // весь мир — та же сводка внутри и снаружи
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
        let layer = &creature::GENES[Gene::MinY as usize];
        assert_eq!(row(layer, &at(30.0), Some(&at(40.0))).2, "-10 п.п.");
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
        // пустой мир — срез без генов; вкладки должны это пережить (проверяется
        // в ui_tests), а сводка по области — пустая
        let w = World::new(&WorldConfig { n_creatures: Some(0), ..Default::default() });
        let r = RegionStats::of(&w, (0.0, 0.0, 100.0, 100.0), None);
        assert!(r.inside.is_none() && r.world.is_none() && r.fullness.is_none());
        let s = Snapshot::of(&w);
        assert!(s.genes.is_none());
    }
}
