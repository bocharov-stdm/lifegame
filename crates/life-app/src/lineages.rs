//! The «Виды» tab of the statistics: the species — groups of creatures that could mate
//! (`life_sim::species`) — as a count over time and as a tree. Time runs to the right; each species
//! is a branch from its rise to its end (or now) in its diet's colour, as thick as it was at most,
//! joined to the one it split from. The data come with the frames now and then
//! (`sim::SPECIES_EVERY`).

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};
use life_sim::species::{SPECIES_MIN, Species, View};

use crate::theme::{DIET_COLORS, DIET_NAMES, MUTED, TEXT, rgb, spaced};

/// The height of a branch's row in the tree, and the most rows drawn: the biggest species first.
const ROW: f32 = 7.0;
const ROWS_MAX: usize = 160;

pub fn tab(ui: &mut egui::Ui, view: Option<&View>) {
    let Some(view) = view.filter(|v| !v.history.is_empty()) else {
        ui.colored_label(MUTED, "Виды появятся после первой выборки мира.");
        return;
    };
    let last = view.history.last().expect("a sample");
    let arisen = view.species.iter().filter(|s| s.shown()).count();
    let new_shapes: usize = view.history.iter().skip(1).map(|p| p.new_shapes).sum();
    ui.label(format!(
        "Видов сейчас {}, возникло всего {} · программ разной формы {}, новых за всё время {} · рождений от пары {:.0}%",
        last.species,
        arisen,
        last.shapes,
        spaced(new_shapes as u64),
        last.mated_share * 100.0
    ));
    ui.colored_label(
        MUTED,
        format!(
            "Вид — те, кто мог бы спариться: одно питание и близкие гены (порог — «Близость для \
             спаривания»). Видом считается группа, в которой хоть раз было {SPECIES_MIN}+ существ."
        ),
    );
    ui.add_space(6.0);
    ui.label(crate::theme::strong("Число видов"));
    count_chart(ui, view);
    ui.add_space(8.0);
    ui.label(crate::theme::strong("Древо видов"));
    ui.colored_label(MUTED, "слева — начало мира, справа — сейчас; толщина — наибольшая численность");
    tree(ui, view, last.tick);
}

/// The species alive at each sample, and the groups of any size fainter.
fn count_chart(ui: &mut egui::Ui, view: &View) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 90.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, MUTED.gamma_multiply(0.4)), egui::StrokeKind::Inside);
    let points = &view.history;
    let (t0, t1) = (points[0].tick as f32, points.last().map_or(1.0, |p| p.tick as f32).max(1.0));
    let top = points.iter().map(|p| p.groups.max(p.species)).max().unwrap_or(1).max(1) as f32;
    let at = |tick: u64, v: usize| {
        let x = rect.left() + (tick as f32 - t0) / (t1 - t0).max(1.0) * rect.width();
        Pos2::new(x, rect.bottom() - 4.0 - v as f32 / top * (rect.height() - 8.0))
    };
    let line = |pts: Vec<Pos2>, color: Color32| {
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, color)));
        }
    };
    line(points.iter().map(|p| at(p.tick, p.groups)).collect(), MUTED.gamma_multiply(0.5));
    line(points.iter().map(|p| at(p.tick, p.species)).collect(), TEXT);
    let font = FontId::monospace(10.5);
    painter.text(
        rect.left_top() + Vec2::new(4.0, 2.0),
        Align2::LEFT_TOP,
        format!("до {}", top as usize),
        font,
        MUTED,
    );
}

/// The tree: the biggest shown species, each placed under the one it split from.
fn tree(ui: &mut egui::Ui, view: &View, now: u64) {
    let mut shown: Vec<&Species> = view.species.iter().filter(|s| s.shown()).collect();
    if shown.is_empty() {
        ui.colored_label(MUTED, "видов ещё нет");
        return;
    }
    // the biggest first, then their order by descent: every child right under its line
    shown.sort_by_key(|s| std::cmp::Reverse(s.peak));
    shown.truncate(ROWS_MAX);
    let ids: Vec<u32> = shown.iter().map(|s| s.id).collect();
    let parent = |s: &Species| view.shown_parent(s.id).filter(|p| ids.contains(p));
    let mut order: Vec<&Species> = Vec::with_capacity(shown.len());
    let mut roots: Vec<&Species> = shown.iter().copied().filter(|s| parent(s).is_none()).collect();
    roots.sort_by_key(|s| (s.born, s.id));
    fn place<'a>(
        s: &'a Species,
        all: &[&'a Species],
        parent: &dyn Fn(&Species) -> Option<u32>,
        out: &mut Vec<&'a Species>,
    ) {
        out.push(s);
        let mut children: Vec<&Species> = all.iter().copied().filter(|c| parent(c) == Some(s.id)).collect();
        children.sort_by_key(|c| (c.born, c.id));
        for c in children {
            place(c, all, parent, out);
        }
    }
    for r in roots {
        place(r, &shown, &parent, &mut order);
    }
    let rows = order.len();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), rows as f32 * ROW + 6.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let span = now.max(1) as f32;
    let x = |tick: u64| rect.left() + 4.0 + tick as f32 / span * (rect.width() - 8.0);
    let y = |row: usize| rect.top() + 3.0 + (row as f32 + 0.5) * ROW;
    let peak = order.iter().map(|s| s.peak).max().unwrap_or(1) as f32;
    let row_of = |id: u32| order.iter().position(|s| s.id == id);
    for (row, s) in order.iter().enumerate() {
        let color = rgb(DIET_COLORS[s.diet.min(3)]);
        let color = if s.died.is_some() { color.gamma_multiply(0.45) } else { color };
        let width = 1.0 + 4.0 * ((s.peak as f32).ln() / peak.ln().max(1.0)).clamp(0.0, 1.0);
        let (a, b) = (x(s.born), x(s.died.unwrap_or(now)).max(x(s.born) + 1.0));
        painter.line_segment([Pos2::new(a, y(row)), Pos2::new(b, y(row))], Stroke::new(width, color));
        if let Some(p) = parent(s).and_then(row_of) {
            painter.line_segment(
                [Pos2::new(a, y(p)), Pos2::new(a, y(row))],
                Stroke::new(1.0, color.gamma_multiply(0.6)),
            );
        }
    }
    if let Some(at) = response.hover_pos() {
        let row = ((at.y - rect.top() - 3.0) / ROW).floor();
        if row >= 0.0 && (row as usize) < rows {
            let s = order[row as usize];
            let from =
                parent(s).map_or("от основателей".into(), |p| format!("отделился от №{p}"));
            let end = s.died.map_or("жив".into(), |d| format!("вымер на тике {}", spaced(d)));
            response.on_hover_text(format!(
                "Вид №{} — {}\nс тика {}, {from}; {end}\nсейчас {}, больше всего {}",
                s.id,
                DIET_NAMES[s.diet.min(3)],
                spaced(s.born),
                spaced(s.count as u64),
                spaced(s.peak as u64)
            ));
        }
    }
}
