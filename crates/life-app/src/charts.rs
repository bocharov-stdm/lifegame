//! Графики боковой панели: численности и геном. Рисуются прямо кистью egui —
//! линии из пары сотен точек, отдельная библиотека графиков им не нужна.
//! Логика — как в `app/render.py` (тег python-final): у каждой величины своя
//! шкала, под курсором — значения в этой точке, справа — изменение от начала.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use life_core::genome::GeneSpec;
use life_core::genome::vegetarian::{GENES, N};
use life_sim::observe::{GeneStat, MAX_VARIANTS, Spread};

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
/// вида, а полоса его показывает. У гена-выбора (стратегии) — доли вариантов
/// слоями. Справа — значение и изменение от начала.
pub fn genome(ui: &mut egui::Ui, history: &History, whole: bool, row_h: f32) {
    let points: Vec<GenePoint> = history.genes.points(whole);
    if points.is_empty() {
        ui.colored_label(MUTED, "травоядных нет — нет и генома");
        return;
    }
    let rows: Vec<usize> = (0..N).filter(|&g| shown(&GENES[g])).collect();
    let n = points.len();
    let width = ui.available_width();
    let (label_w, value_w) = (118.0, 112.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h * rows.len() as f32), Sense::hover());
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

    for (row_i, &g) in rows.iter().enumerate() {
        let spec = &GENES[g];
        let top = rect.top() + row_h * row_i as f32;
        let row = Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, row_h));
        if row_i > 0 {
            painter.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0, LINE));
        }
        painter.text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            spec.label,
            font.clone(),
            MUTED,
        );
        let spark = Rect::from_min_max(
            Pos2::new(spark_rect.left(), top + 3.0),
            Pos2::new(spark_rect.right(), top + row_h - 3.0),
        );

        let (value, change) = match (at.genes[g], origin[g]) {
            (GeneStat::Number(now), GeneStat::Number(was)) => {
                number_row(&painter, spark, &points, g, spec.is_percent(), color);
                number_text(now.p50, was.p50, spec.is_percent())
            }
            (GeneStat::Shares(now), GeneStat::Shares(was)) => {
                shares_row(&painter, spark, &points, g, spec.variants().unwrap_or_default().len());
                shares_text(spec, &now, &was)
            }
            _ => (String::new(), String::new()),
        };
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

/// Показывать ли ген: ген-выбор с одним вариантом ничего не различает.
pub fn shown(spec: &GeneSpec) -> bool {
    spec.variants().is_none_or(|v| v.len() >= 2)
}

fn spread_at(p: &GenePoint, g: usize) -> Spread {
    p.genes[g].spread().copied().expect("числовой ген")
}

/// Мини-график числового гена: полоса 10‒90% и линия медианы.
fn number_row(
    painter: &egui::Painter,
    spark: Rect,
    points: &[GenePoint],
    g: usize,
    percent: bool,
    color: Color32,
) {
    let n = points.len();
    let (lo, hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
        let s = spread_at(p, g);
        (lo.min(s.p10), hi.max(s.p90))
    });
    let span = (hi - lo).max(if percent { 1.0 } else { hi.abs() * 0.05 + 1e-9 });
    let y = |v: f64| spark.bottom() - ((v - lo) / span) as f32 * spark.height();
    if n < 2 {
        return;
    }
    let mut band: Vec<Pos2> =
        (0..n).map(|i| Pos2::new(x_at(spark, i, n), y(spread_at(&points[i], g).p90))).collect();
    band.extend((0..n).rev().map(|i| Pos2::new(x_at(spark, i, n), y(spread_at(&points[i], g).p10))));
    // полоса — набором четырёхугольников: egui заливает только выпуклые фигуры
    for i in 0..n - 1 {
        let quad = vec![band[i], band[i + 1], band[2 * n - 2 - i], band[2 * n - 1 - i]];
        painter.add(Shape::convex_polygon(quad, color.gamma_multiply(0.18), Stroke::NONE));
    }
    let line: Vec<Pos2> =
        (0..n).map(|i| Pos2::new(x_at(spark, i, n), y(spread_at(&points[i], g).p50))).collect();
    painter.add(Shape::line(line, Stroke::new(1.4, color)));
}

fn number_text(now: f64, was: f64, percent: bool) -> (String, String) {
    let change = if percent {
        format!("{:+.0} п.п.", now - was)
    } else if was > 0.0 {
        format!("{:+.0}%", (now / was - 1.0) * 100.0)
    } else {
        String::new()
    };
    let value = if percent { format!("{now:.0}%") } else { format!("{now:.0}") };
    (value, change)
}

/// Цвета вариантов гена-выбора — по порядку.
const VARIANT_COLORS: [Color32; 5] = [
    Color32::from_rgb(205, 134, 255),
    Color32::from_rgb(245, 197, 66),
    Color32::from_rgb(93, 211, 158),
    Color32::from_rgb(110, 170, 255),
    Color32::from_rgb(239, 99, 81),
];

/// Мини-график гена-выбора: доли вариантов слоями снизу вверх.
fn shares_row(painter: &egui::Painter, spark: Rect, points: &[GenePoint], g: usize, variants: usize) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let share = |i: usize, k: usize| points[i].genes[g].shares().map_or(0.0, |s| s[k]) as f32;
    let mut below = vec![0.0f32; n];
    for k in 0..variants {
        let color = VARIANT_COLORS[k % VARIANT_COLORS.len()].gamma_multiply(0.7);
        let y = |v: f32| spark.bottom() - v * spark.height();
        for i in 0..n - 1 {
            let (a, b) = (share(i, k), share(i + 1, k));
            let quad = vec![
                Pos2::new(x_at(spark, i, n), y(below[i])),
                Pos2::new(x_at(spark, i + 1, n), y(below[i + 1])),
                Pos2::new(x_at(spark, i + 1, n), y(below[i + 1] + b)),
                Pos2::new(x_at(spark, i, n), y(below[i] + a)),
            ];
            painter.add(Shape::convex_polygon(quad, color, Stroke::NONE));
        }
        for (i, b) in below.iter_mut().enumerate() {
            *b += share(i, k);
        }
    }
}

/// Самый частый вариант и изменение его доли от начала.
fn shares_text(spec: &GeneSpec, now: &[f64; MAX_VARIANTS], was: &[f64; MAX_VARIANTS]) -> (String, String) {
    let variants = spec.variants().unwrap_or_default();
    let Some((k, v)) = variants.iter().enumerate().max_by(|a, b| now[a.0].total_cmp(&now[b.0])) else {
        return (String::new(), String::new());
    };
    (format!("{} {:.0}%", v.label, now[k] * 100.0), format!("{:+.0} п.п.", (now[k] - was[k]) * 100.0))
}

/// Цвет-подсказка для полос энергии: голодные — красным.
pub fn energy_color(frac: f64, base: Color32) -> Color32 {
    if frac < 0.25 { crate::theme::DANGER } else { base }
}
