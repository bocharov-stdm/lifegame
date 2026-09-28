//! The diets on the side panel: how many of each live now and, on a click, what they are like
//! next to the whole world, how they die and whom they kill. Below the population chart, who kills
//! whom. Everything is read from the world's snapshots (`Snapshot::diets`, `Counters::by_diet`).

use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use life_core::DietCounters;
use life_sim::observe::{DIET_GENES, DietStat, Snapshot, Spread};

use crate::app::LifeApp;
use crate::theme::{ACCENT_TEXT, DIET_COLORS, DIET_HINTS, DIET_NAMES, LINE, MUTED, TEXT, rgb, spaced};

/// Diet names after «убили» and «погибли от» (genitive plural).
const DIET_OF: [&str; 4] = ["травоядных", "всеядных", "падальщиков", "мясоедов"];
/// Short names for the columns of the «кто кого» table.
const DIET_SHORT: [&str; 4] = ["трав.", "всеяд.", "падал.", "мясо."];

/// The genes of `DIET_GENES` as the panel names them, with a plain hint and a way to print them.
/// A gene row: its name, a hint and how to print its value.
type GeneRow = (&'static str, &'static str, fn(f64) -> String);

const GENE_ROWS: [GeneRow; 6] = [
    ("размер", "Какого размера вырастают. Крупным больше еды достаётся, но и тратят они больше.", number),
    ("скорость", "Как быстро плавают. Быстрым проще догнать и убежать, но бег стоит сил.", number),
    ("зрение", "Как далеко видят еду и чужаков. Далёкое зрение тоже стоит сил.", number),
    (
        "хладнокровие",
        "Насколько тело остывает с водой: в холодной глубине дешевле живут, но медленнее плавают.",
        percent,
    ),
    ("рывок", "Во сколько раз быстрее бросаются в погоне и бегстве. Мышцы стоят сил и в покое.", times),
    ("срок жизни", "Сколько тиков живут до смерти от старости.", number),
];

fn number(v: f64) -> String {
    if v < 20.0 { format!("{v:.1}") } else { format!("{v:.0}") }
}

fn times(v: f64) -> String {
    format!("×{v:.1}")
}

fn percent(v: f64) -> String {
    format!("{v:.0}%")
}

fn median(s: &Option<Spread>, show: fn(f64) -> String) -> String {
    s.map_or("—".into(), |s| show(s.p50))
}

/// Flows of the diets over the charts' window (the last 10 000 ticks): the difference between the
/// last snapshot and the first one still in the window.
fn window(snaps: &[&Snapshot]) -> DietCounters {
    match (snaps.first(), snaps.last()) {
        (Some(first), Some(last)) => last.counters.by_diet.since(&first.counters.by_diet),
        _ => DietCounters::default(),
    }
}

impl LifeApp {
    /// A row per diet: its colour, name, number and share, and a bar. A click opens its details.
    pub(crate) fn diets_block(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Кто живёт").strong()).on_hover_text(
            "Сколько сейчас существ каждого питания. Нажмите на строку — откроется, какие они, \
             отчего умирают и кого убивают.",
        );
        self.highlight_toggles(ui);
        let snaps = self.history.snapshots.points();
        let Some(last) = snaps.last().copied() else {
            ui.colored_label(MUTED, "Сводка по питанию появится через пару секунд игры.");
            return;
        };
        let flows = window(&snaps);
        let total = last.creatures.max(1) as f64;
        for d in 0..4 {
            let n = last.diets[d].creatures;
            let open = self.diet_open[d];
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, DIET_NAMES[d]));
            let painter = ui.painter_at(rect);
            if response.hovered() {
                painter.rect_filled(rect, 3.0, LINE);
            }
            let color = rgb(DIET_COLORS[d]);
            let font = FontId::proportional(14.0);
            let y = rect.center().y;
            painter.text(
                Pos2::new(rect.left() + 2.0, y),
                Align2::LEFT_CENTER,
                if open { "▾" } else { "▸" },
                font.clone(),
                MUTED,
            );
            painter.circle_filled(Pos2::new(rect.left() + 20.0, y), 5.0, color);
            painter.text(
                Pos2::new(rect.left() + 32.0, y),
                Align2::LEFT_CENTER,
                DIET_NAMES[d],
                font.clone(),
                TEXT,
            );
            let share = n as f64 / total;
            // the bar takes the right part of the row; the number and share stand before it
            let bar = Rect::from_min_max(
                Pos2::new(rect.right() - rect.width() * 0.3, y - 4.0),
                Pos2::new(rect.right() - 4.0, y + 4.0),
            );
            painter.rect_stroke(bar, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
            let mut fill = bar;
            fill.set_width(bar.width() * share as f32);
            painter.rect_filled(fill, 2.0, color);
            painter.text(
                Pos2::new(bar.left() - 8.0, y),
                Align2::RIGHT_CENTER,
                format!("{} · {:.0}%", spaced(n as u64), share * 100.0),
                font,
                if n == 0 { MUTED } else { TEXT },
            );
            let response = response.on_hover_text(format!(
                "{}\n\nНажмите, чтобы {} подробности.",
                DIET_HINTS[d],
                if open { "скрыть" } else { "открыть" }
            ));
            if response.clicked() {
                self.diet_open[d] = !open;
            }
            if self.diet_open[d] {
                ui.indent(("питание", d), |ui| details(ui, d, last, &flows));
                ui.add_space(4.0);
            }
        }
    }

    /// Who kills whom over the window: kills, and strikes behind them.
    pub(crate) fn kills_table(&self, ui: &mut egui::Ui) {
        let snaps = self.history.snapshots.points();
        let flows = window(&snaps);
        egui::CollapsingHeader::new("Кто кого").default_open(true).show(ui, |ui| {
            ui.colored_label(
                MUTED,
                "Строка — кто нападал, столбец — на кого. Число — убийства, за косой — удары.",
            )
            .on_hover_text(
                "Считается за последние 10 000 тиков. Убийство засчитывается тому, кто в смертельный \
                     тик ударил сильнее всех. Удары — и вблизи, и выстрелы.",
            );
            egui::Grid::new("кто-кого").striped(true).spacing(Vec2::new(10.0, 3.0)).show(ui, |ui| {
                ui.label("");
                for (short, color) in DIET_SHORT.iter().zip(DIET_COLORS) {
                    ui.colored_label(rgb(color), *short);
                }
                ui.end_row();
                for d in 0..4 {
                    ui.colored_label(rgb(DIET_COLORS[d]), DIET_SHORT[d]).on_hover_text(DIET_NAMES[d]);
                    for (v, victims) in DIET_OF.iter().enumerate() {
                        let (kills, strikes) = (flows.kills[d][v], flows.strikes[d][v]);
                        let text = format!("{} / {}", spaced(kills), spaced(strikes));
                        let color = if kills + strikes == 0 { MUTED } else { TEXT };
                        ui.colored_label(color, text).on_hover_text(format!(
                            "{} убили {} {} и ударили {} раз",
                            capital(DIET_NAMES[d]),
                            spaced(kills),
                            victims,
                            spaced(strikes)
                        ));
                    }
                    ui.end_row();
                }
            });
        });
    }

    /// «Подсветить в мире»: a toggle per diet. Highlighted diets are drawn bright, bigger and with a
    /// halo at any zoom; everyone else fades.
    pub(crate) fn highlight_toggles(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.colored_label(MUTED, "Подсветить:").on_hover_text(
                "Отмеченные питания в мире рисуются ярко, с ореолом своего цвета и не пропадают \
                 даже издалека, а все остальные тускнеют. Можно отметить несколько. \
                 Нажмите ещё раз, чтобы снять.",
            );
            for d in 0..4 {
                let bit = 1u32 << d;
                let on = self.view.highlight & bit != 0;
                // on the selected (accent) fill the diet's colour would fade: dark text there
                let color = if on { ACCENT_TEXT } else { rgb(DIET_COLORS[d]) };
                let text = RichText::new(format!("● {}", DIET_SHORT[d])).color(color);
                if ui
                    .selectable_label(on, text)
                    .on_hover_text(format!("Подсветить {} в мире", DIET_OF[d]))
                    .clicked()
                {
                    self.view.highlight ^= bit;
                }
            }
        });
    }

    /// One line for places without room for the rows: «травоядные 441 · всеядные 123 · …».
    pub(crate) fn diets_line(&self, ui: &mut egui::Ui) {
        let Some(last) = self.history.snapshots.last() else { return };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for d in 0..4 {
                ui.colored_label(
                    rgb(DIET_COLORS[d]),
                    format!("{} {}", DIET_NAMES[d], spaced(last.diets[d].creatures as u64))
                        .replace(' ', "\u{a0}"),
                )
                .on_hover_text(DIET_HINTS[d]);
            }
        });
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// A diet next to the whole world, then how it lived over the window.
fn details(ui: &mut egui::Ui, d: usize, last: &Snapshot, flows: &DietCounters) {
    let (me, all): (&DietStat, &DietStat) = (&last.diets[d], &last.all);
    if me.creatures == 0 {
        ui.colored_label(MUTED, "Сейчас ни одного.");
    } else {
        egui::Grid::new(("питание-сводка", d)).spacing(Vec2::new(14.0, 2.0)).show(ui, |ui| {
            ui.label("");
            ui.colored_label(rgb(DIET_COLORS[d]), "эти");
            ui.colored_label(MUTED, "весь мир");
            ui.end_row();
            let fullness = |s: &DietStat| s.fullness.map_or("—".into(), |f| format!("{:.0}%", f * 100.0));
            row(
                ui,
                "сытость",
                "Насколько в среднем полон бак. Голодные чаще умирают и меньше рожают.",
                fullness(me),
                fullness(all),
            );
            row(
                ui,
                "возраст",
                "Медианный возраст в тиках: половина моложе, половина старше.",
                median(&me.age, |v| format!("{v:.0}")),
                median(&all.age, |v| format!("{v:.0}")),
            );
            row(
                ui,
                "глубина",
                "Где живёт половина из них: 0% — поверхность, 100% — дно.",
                median(&me.depth, percent),
                median(&all.depth, percent),
            );
            for (i, (label, hint, show)) in GENE_ROWS.iter().enumerate() {
                let value = |s: &DietStat| s.genes.map_or("—".into(), |g| show(g[i].p50));
                row(ui, label, hint, value(me), value(all));
            }
        });
        debug_assert_eq!(GENE_ROWS.len(), DIET_GENES.len());
    }
    ui.add_space(3.0);
    ui.colored_label(MUTED, "За последние 10 000 тиков:");
    let [starved, old, combat] = flows.deaths[d];
    ui.label(format!(
        "родилось {} · умерли от голода {}, от старости {}, в бою {}",
        spaced(flows.born[d]),
        spaced(starved),
        spaced(old),
        spaced(combat)
    ))
    .on_hover_text("Основатели на старте рождениями не считаются.");
    ui.label(format!("убили: {}", list((0..4).map(|v| (v, flows.kills[d][v])))))
        .on_hover_text("Кого эти существа убили в бою, по питанию жертвы.");
    ui.label(format!("погибли от: {}", list((0..4).map(|k| (k, flows.kills[k][d])))))
        .on_hover_text("Кто убил их в бою, по питанию убийцы.");
}

fn row(ui: &mut egui::Ui, label: &str, hint: &str, me: String, all: String) {
    ui.colored_label(MUTED, label).on_hover_text(hint);
    ui.label(me);
    ui.colored_label(MUTED, all);
    ui.end_row();
}

/// «травоядных 12, мясоедов 3» or «никого».
fn list(counts: impl Iterator<Item = (usize, u64)>) -> String {
    let parts: Vec<String> =
        counts.filter(|&(_, n)| n > 0).map(|(k, n)| format!("{} {}", DIET_OF[k], spaced(n))).collect();
    if parts.is_empty() { "никого".into() } else { parts.join(", ") }
}
