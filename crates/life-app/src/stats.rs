//! Окно «Статистика»: то, чего не видно на боковой панели. Сытость и голод,
//! где живут травоядные и где растёт еда, геном хищников и сводка по
//! области, протянутой по миру. Данные — срезы мира (`Snapshot`), которые поток
//! симуляции и так снимает для хроники.

use eframe::egui::{self, RichText, Vec2};
use life_core::flora::Profile;
use life_core::genome::{GeneSpec, predator, vegetarian};
use life_sim::observe::GeneStat;

use crate::app::{LifeApp, Tool};
use crate::charts::{self, GenePoint};
use crate::frame::{PREDATOR_COLOR, VEGETARIAN_COLOR};
use crate::sim::Command;
use crate::theme::{DANGER, GOOD, MUTED, TEXT, rgb, spaced};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsTab {
    Energy,
    Where,
    Predators,
    Region,
}

impl LifeApp {
    pub fn stats_window(&mut self, ctx: &egui::Context) {
        let predators = self.predators_in_game();
        if self.stats_tab == StatsTab::Predators && !predators {
            self.stats_tab = StatsTab::Energy;
        }
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
                    if predators {
                        ui.selectable_value(&mut self.stats_tab, StatsTab::Predators, "Хищники");
                    }
                    ui.selectable_value(&mut self.stats_tab, StatsTab::Region, "Область");
                    if self.stats_tab != StatsTab::Region {
                        ui.separator();
                        ui.selectable_value(&mut self.whole, false, "Недавнее");
                        ui.selectable_value(&mut self.whole, true, "Вся партия");
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.stats_tab {
                    StatsTab::Energy => self.energy_tab(ui, predators),
                    StatsTab::Where => self.where_tab(ui),
                    StatsTab::Predators => self.predators_tab(ui),
                    StatsTab::Region => self.region_tab(ui, predators),
                });
            });
        self.stats_open &= open;
    }

    fn energy_tab(&mut self, ui: &mut egui::Ui, predators: bool) {
        let snaps = self.history.snapshots.points(self.whole);
        ui.label(RichText::new("Сытость и голод").strong());
        charts::energy(ui, &snaps, predators, 190.0);
        ui.add_space(4.0);
        ui.colored_label(
            MUTED,
            "Сытость — насколько в среднем полон бак. Растения у потолка — еды больше, чем успевают \
             съесть: травоядных держит не голод.",
        );
    }

    fn where_tab(&mut self, ui: &mut egui::Ui) {
        let snaps = self.history.snapshots.points(self.whole);
        ui.label(RichText::new("Глубина травоядных во времени").strong());
        let hovered = charts::depth_map(ui, &snaps, 170.0);
        let Some(&at) = hovered.and_then(|i| snaps.get(i)).or(snaps.last()) else { return };
        let layer = at.vegetarian_depth.map_or("травоядных нет".into(), |d| {
            format!("80% травоядных на глубине {:.0}‒{:.0}%, медиана {:.0}%", d.p10, d.p90, d.p50)
        });
        ui.colored_label(MUTED, format!("тик {} · {layer}; верх — поверхность", spaced(at.tick)));
        ui.add_space(8.0);
        ui.label(RichText::new("Растения и травоядные по глубине").strong());
        ui.colored_label(MUTED, "слева — доля растений, справа — доля травоядных");
        charts::bands(ui, &at.plants_by_depth, &at.vegetarians_by_depth, ("поверхность", "дно"));
        // по ширине смотреть есть на что, только если еда по ней неравномерна
        let uneven = self.view.frame.as_ref().is_some_and(|f| f.rules.plant_width.kind() != Profile::Uniform);
        if uneven {
            ui.add_space(8.0);
            ui.label(RichText::new("По ширине").strong());
            charts::bands(ui, &at.plants_by_width, &at.vegetarians_by_width, ("слева", "справа"));
        }
    }

    fn predators_tab(&mut self, ui: &mut egui::Ui) {
        let snaps = self.history.snapshots.points(self.whole);
        let points: Vec<GenePoint> =
            snaps.iter().filter_map(|s| Some((s.tick, &s.predator_genes.as_ref()?[..]))).collect();
        ui.label(RichText::new("Геном хищников").strong());
        if points.is_empty() {
            ui.colored_label(MUTED, "хищников нет — нет и генома");
            return;
        }
        let origin = self.history.predator_origin.as_ref().map(|o| &o[..]);
        charts::genome(ui, &predator::GENES, &points, origin, rgb(PREDATOR_COLOR), 30.0);
    }

    fn region_tab(&mut self, ui: &mut egui::Ui, predators: bool) {
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
            ui.colored_label(rgb(VEGETARIAN_COLOR), format!("травоядных {}", spaced(r.vegetarians as u64)));
            if predators {
                ui.colored_label(rgb(PREDATOR_COLOR), format!("хищников {}", spaced(r.predators as u64)));
            }
            if let Some(f) = r.fullness {
                ui.colored_label(MUTED, format!("сытость {:.0}%", f * 100.0));
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Геном травоядных").strong());
        compare(ui, "область-травоядные", &vegetarian::GENES, r.inside.0.as_ref(), r.world.0.as_ref());
        if predators {
            ui.add_space(6.0);
            ui.label(RichText::new("Геном хищников").strong());
            compare(ui, "область-хищники", &predator::GENES, r.inside.1.as_ref(), r.world.1.as_ref());
        }
        ui.add_space(6.0);
        ui.colored_label(MUTED, "Сводка обновляется на каждом срезе мира и сразу, когда область задана.");
    }

    /// Снять область: и рамку в мире, и сводку.
    pub fn clear_region(&mut self) {
        self.region = None;
        self.view.area = None;
        self.sim.send(Command::SetRegion(None));
    }

    /// Протянута новая область: сводку посчитает поток, окно откроется на ней.
    pub fn set_region(&mut self, area: crate::frame::Area) {
        self.view.area = Some(area);
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
    use life_core::genome::vegetarian::Gene;
    use life_core::{World, WorldConfig};
    use life_sim::observe::Snapshot;

    /// Сводка по области — ровно по тем, кто внутри, а по всему миру — по всем.
    #[test]
    fn область_считает_только_тех_кто_внутри() {
        let mut w = World::new(&WorldConfig { seed: 2, n_predators: Some(0), ..Default::default() });
        for _ in 0..300 {
            w.step();
        }
        let (x, y) = (w.space.width / 2.0, w.space.height / 2.0);
        let area = (0.0, 0.0, x, y);
        let r = RegionStats::of(&w, area, None);
        let inside: Vec<_> = w.vegetarians.iter().filter(|v| v.x <= x && v.y <= y).collect();
        assert_eq!(r.vegetarians, inside.len());
        assert_eq!(r.plants, w.plants.iter().filter(|p| p.x <= x && p.y <= y).count());
        assert_eq!(r.predators, 0);
        let size = |s: &Option<[GeneStat; vegetarian::N]>| {
            s.as_ref().and_then(|g| g[Gene::Size as usize].spread().map(|s| s.mean))
        };
        let mean = inside.iter().map(|v| v.genome[Gene::Size]).sum::<f64>() / inside.len().max(1) as f64;
        if !inside.is_empty() {
            assert!((size(&r.inside.0).unwrap() - mean).abs() < 1e-9);
        }
        let all =
            w.vegetarians.iter().map(|v| v.genome[Gene::Size]).sum::<f64>() / w.vegetarians.len() as f64;
        assert!((size(&r.world.0).unwrap() - all).abs() < 1e-9);
        // весь мир — та же сводка внутри и снаружи
        let whole = RegionStats::of(&w, (0.0, 0.0, w.space.width, w.space.height), None);
        assert_eq!(whole.inside, whole.world);
        assert_eq!(whole.vegetarians, w.vegetarians.len());
    }

    #[test]
    fn строка_таблицы_сравнивает_среднее_и_доли() {
        use life_sim::observe::Spread;
        let spec = &vegetarian::GENES[Gene::Size as usize];
        let at = |mean| GeneStat::Number(Spread { p10: mean, p50: mean, p90: mean, mean });
        assert_eq!(row(spec, &at(60.0), Some(&at(40.0))), ("60".into(), "40".into(), "+50%".into()));
        let layer = &vegetarian::GENES[Gene::MinY as usize];
        assert_eq!(row(layer, &at(30.0), Some(&at(40.0))).2, "-10 п.п.");
        let strategy = &vegetarian::GENES[Gene::Strategy as usize];
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
    fn снимок_без_травоядных_не_роняет_окно() {
        // пустой мир — срез без генов; вкладки должны это пережить (проверяется
        // в ui_tests), а сводка по области — пустая
        let w =
            World::new(&WorldConfig { n_vegetarians: Some(0), n_predators: Some(0), ..Default::default() });
        let r = RegionStats::of(&w, (0.0, 0.0, 100.0, 100.0), None);
        assert!(r.inside.0.is_none() && r.world.0.is_none() && r.fullness.is_none());
        let s = Snapshot::of(&w);
        assert!(s.genes.is_none());
    }
}
