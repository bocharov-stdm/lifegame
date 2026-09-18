//! Графики боковой панели: численности и геном. Рисуются прямо кистью egui —
//! линии из пары сотен точек, отдельная библиотека графиков им не нужна.
//! Логика — как в `app/render.py` (тег python-final): у каждой величины своя
//! шкала, под курсором — значения в этой точке, справа — изменение от начала.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use life_core::genome::{GENE_LABELS, PERCENT};

use crate::frame::{PLANT_COLOR, PREDATOR_COLOR, VEGETARIAN_COLOR};
use crate::history::{GenePoint, History, Sample};
use crate::theme::{LINE, MUTED, TEXT, rgb, spaced};

/// Индекс точки под курсором (по x), если курсор над графиком.
fn hover_index(ui: &egui::Ui, rect: Rect, n: usize) -> Option<usize> {
    let p = ui.input(|i| i.pointer.hover_pos())?;
    if !rect.contains(p) || n == 0 {
        return None;
    }
    let t = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    Some(((n - 1) as f32 * t).round() as usize)
}

fn x_at(rect: Rect, i: usize, n: usize) -> f32 {
    if n <= 1 { rect.right() } else { rect.left() + rect.width() * i as f32 / (n - 1) as f32 }
}

/// Численности: растения, травоядные, хищники — каждая в своей шкале от нуля
/// до своего максимума на участке, иначе хищники лежали бы на нуле.
pub fn populations(ui: &mut egui::Ui, history: &History, whole: bool, height: f32) {
    let points = history.counts.points(whole);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
    if points.len() < 2 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "история копится…",
            FontId::proportional(13.0),
            MUTED,
        );
        return;
    }
    let n = points.len();
    type Value = fn(&Sample) -> f64;
    let series: [(Value, [u8; 3]); 3] = [
        (|s| s.plants, PLANT_COLOR),
        (|s| s.vegetarians, VEGETARIAN_COLOR),
        (|s| s.predators, PREDATOR_COLOR),
    ];
    let inner = rect.shrink(4.0);
    for (value, color) in series {
        let max = points.iter().map(value).fold(0.0f64, f64::max).max(1.0);
        let line: Vec<Pos2> = points
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Pos2::new(x_at(inner, i, n), inner.bottom() - (value(s) / max) as f32 * inner.height())
            })
            .collect();
        painter.add(Shape::line(line, Stroke::new(1.6, rgb(color))));
    }

    let hovered = hover_index(ui, rect, n);
    let at = &points[hovered.unwrap_or(n - 1)];
    if let Some(i) = hovered {
        let x = x_at(inner, i, n);
        painter
            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, MUTED));
    }
    let label = |v: f64| spaced(v.round() as u64);
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(MUTED, format!("тик {}", spaced(at.tick)));
        ui.colored_label(rgb(PLANT_COLOR), format!("растения {}", label(at.plants)));
        ui.colored_label(rgb(VEGETARIAN_COLOR), format!("травоядные {}", label(at.vegetarians)));
        ui.colored_label(rgb(PREDATOR_COLOR), format!("хищники {}", label(at.predators)));
    });
}

/// Геном: по мини-графику на ген, у каждого своя шкала. Линия — медиана,
/// полоса — где живут 80% популяции (10‒90%): среднее прячет раскол на два
/// вида, а полоса его показывает. Справа — значение и изменение от начала.
pub fn genome(ui: &mut egui::Ui, history: &History, whole: bool, row_h: f32) {
    let points: Vec<GenePoint> = history.genes.points(whole);
    if points.is_empty() {
        ui.colored_label(MUTED, "травоядных нет — нет и генома");
        return;
    }
    let n = points.len();
    let width = ui.available_width();
    let (label_w, value_w) = (118.0, 112.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h * 7.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let spark_rect = Rect::from_min_max(
        Pos2::new(rect.left() + label_w, rect.top()),
        Pos2::new(rect.right() - value_w, rect.bottom()),
    );
    let hovered = hover_index(ui, spark_rect, n);
    let at = &points[hovered.unwrap_or(n - 1)];
    let origin = history.gene_origin.unwrap_or(points[0].genes);
    let font = FontId::proportional(12.5);
    let color = rgb(VEGETARIAN_COLOR);

    for g in 0..7 {
        let top = rect.top() + row_h * g as f32;
        let row = Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, row_h));
        if g > 0 {
            painter.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0, LINE));
        }
        painter.text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            GENE_LABELS[g],
            font.clone(),
            MUTED,
        );

        let spark = Rect::from_min_max(
            Pos2::new(spark_rect.left(), top + 3.0),
            Pos2::new(spark_rect.right(), top + row_h - 3.0),
        );
        let (lo, hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.genes[g].p10), hi.max(p.genes[g].p90))
        });
        let span = (hi - lo).max(if PERCENT[g] { 1.0 } else { hi.abs() * 0.05 + 1e-9 });
        let y = |v: f64| spark.bottom() - ((v - lo) / span) as f32 * spark.height();
        if n >= 2 {
            let mut band: Vec<Pos2> =
                (0..n).map(|i| Pos2::new(x_at(spark, i, n), y(points[i].genes[g].p90))).collect();
            band.extend((0..n).rev().map(|i| Pos2::new(x_at(spark, i, n), y(points[i].genes[g].p10))));
            // полоса — набором четырёхугольников: egui заливает только выпуклые фигуры
            for i in 0..n - 1 {
                let quad = vec![band[i], band[i + 1], band[2 * n - 2 - i], band[2 * n - 1 - i]];
                painter.add(Shape::convex_polygon(quad, color.gamma_multiply(0.18), Stroke::NONE));
            }
            let line: Vec<Pos2> =
                (0..n).map(|i| Pos2::new(x_at(spark, i, n), y(points[i].genes[g].p50))).collect();
            painter.add(Shape::line(line, Stroke::new(1.4, color)));
        }

        let now = at.genes[g].p50;
        let was = origin[g].p50;
        let change = if PERCENT[g] {
            format!("{:+.0} п.п.", now - was)
        } else if was > 0.0 {
            format!("{:+.0}%", (now / was - 1.0) * 100.0)
        } else {
            String::new()
        };
        let value = if PERCENT[g] { format!("{now:.0}%") } else { format!("{now:.0}") };
        painter.text(
            Pos2::new(rect.right() - value_w + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            value,
            font.clone(),
            TEXT,
        );
        painter.text(
            Pos2::new(rect.right(), row.center().y),
            Align2::RIGHT_CENTER,
            change,
            font.clone(),
            MUTED,
        );
    }
    if let Some(i) = hovered {
        let x = x_at(spark_rect, i, n);
        painter
            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, MUTED));
    }
    ui.colored_label(
        MUTED,
        format!(
            "тик {} · линия — медиана, полоса — 80% популяции; справа — изменение от начала",
            spaced(at.tick)
        ),
    );
}

/// Цвет-подсказка для полос энергии: голодные — красным.
pub fn energy_color(frac: f64, base: Color32) -> Color32 {
    if frac < 0.25 { crate::theme::DANGER } else { base }
}
