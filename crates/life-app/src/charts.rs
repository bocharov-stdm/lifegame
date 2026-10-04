//! The charts of the side panel and of the «Статистика» window: counts, fullness, the genome,
//! where they live. They are drawn straight with egui's brush — lines of a couple of hundred
//! points, a separate charting library is not needed for them. The logic is as in `app/render.py`
//! (tag python-final): every quantity has its own scale, under the cursor are the values at that
//! point, on the right the change since the start of the visible window.

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use life_core::genome::GeneSpec;
use life_sim::observe::{GeneStat, MAX_VARIANTS, Snapshot, Spread};

use crate::census::{self, Census, Group};
use crate::frame::{CREATURE_COLOR, PLANT_COLOR};
use crate::history::{History, Sample};
use crate::theme::{DIET_COLORS, DIET_NAMES, LINE, MUTED, TEXT, rgb, spaced};

/// The index of the point under the cursor (by x), if the cursor is over the chart.
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

/// A chart line: a label, a colour and the value at a point (None — there is no quantity,
/// for example fullness when there are no creatures: the line breaks).
pub struct Line<T> {
    pub label: &'static str,
    pub color: Color32,
    pub value: fn(&T) -> Option<f64>,
    /// With `Scale::Own`, lines marked shared use one scale between them (the diets: a line of three
    /// carnivores must not look as tall as three thousand herbivores).
    pub shared: bool,
}

/// The chart's scale for lines.
#[derive(Clone, Copy, PartialEq)]
pub enum Scale {
    /// Each line has its own, from zero to its maximum on the stretch: otherwise tens of creatures
    /// would lie on zero beside thousands of plants. The labels are numbers.
    Own,
    /// A common 0‒100%: the shares are comparable with one another. The labels are percentages.
    Share,
}

/// A chart of lines by the history's points. Under the chart — the tick and the values at the
/// point under the cursor (or at the last one).
pub fn lines<T>(
    ui: &mut egui::Ui,
    points: &[&T],
    tick: fn(&T) -> u64,
    lines: &[Line<T>],
    scale: Scale,
    height: f32,
) {
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
    let inner = rect.shrink(4.0);
    if scale == Scale::Share {
        // the middle of the scale is a landmark for «more or less than half»
        let y = inner.center().y;
        painter
            .line_segment([Pos2::new(inner.left(), y), Pos2::new(inner.right(), y)], Stroke::new(1.0, LINE));
    }
    let top = |line: &Line<T>| points.iter().filter_map(|p| (line.value)(p)).fold(0.0f64, f64::max).max(1.0);
    let shared = lines.iter().filter(|l| l.shared).map(top).fold(1.0f64, f64::max);
    for line in lines {
        let max = match scale {
            Scale::Own if line.shared => shared,
            Scale::Own => top(line),
            Scale::Share => 1.0,
        };
        // In pieces: where there is no quantity, the line breaks.
        let mut run: Vec<Pos2> = Vec::new();
        for (i, p) in points.iter().enumerate() {
            match (line.value)(p) {
                Some(v) => run.push(Pos2::new(
                    x_at(inner, i, n),
                    inner.bottom() - (v / max).clamp(0.0, 1.0) as f32 * inner.height(),
                )),
                None => draw_run(&painter, &mut run, line.color),
            }
        }
        draw_run(&painter, &mut run, line.color);
    }

    let hovered = hover_index(ui, rect, n);
    let at = points[hovered.unwrap_or(n - 1)];
    if let Some(i) = hovered {
        let x = x_at(inner, i, n);
        painter
            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, MUTED));
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(MUTED, format!("тик {}", spaced(tick(at))));
        for line in lines {
            let value = match ((line.value)(at), scale) {
                (None, _) => "—".into(),
                (Some(v), Scale::Own) => spaced(v.round() as u64),
                (Some(v), Scale::Share) => format!("{:.0}%", v * 100.0),
            };
            // non-breaking spaces: the label does not break by a wrap in the middle
            let text = format!("{} {value}", line.label).replace(' ', "\u{a0}");
            ui.colored_label(line.color, text);
        }
    });
}

fn draw_run(painter: &egui::Painter, run: &mut Vec<Pos2>, color: Color32) {
    if run.len() >= 2 {
        painter.add(Shape::line(std::mem::take(run), Stroke::new(1.6, color)));
    }
    run.clear();
}

/// Populations: plants on their own scale, the diets on one scale between them.
pub fn populations(ui: &mut egui::Ui, history: &History, height: f32) {
    let points = history.counts.points();
    let diet = |d: usize, value: fn(&Sample) -> Option<f64>| Line {
        label: DIET_NAMES[d],
        color: rgb(DIET_COLORS[d]),
        value,
        shared: true,
    };
    let all = [
        Line {
            label: "растения", color: rgb(PLANT_LINE), value: |s: &Sample| Some(s.plants), shared: false
        },
        diet(0, |s| Some(s.diets[0])),
        diet(1, |s| Some(s.diets[1])),
        diet(2, |s| Some(s.diets[2])),
        diet(3, |s| Some(s.diets[3])),
    ];
    lines(ui, &points, |s| s.tick, &all, Scale::Own, height);
}

/// Plants on the population chart: a pale grey green, apart from the herbivores' green.
const PLANT_LINE: [u8; 3] = [120, 150, 130];

/// Fullness: the mean tank fullness of the creatures and how far the plants have run into the ceiling.
pub fn energy(ui: &mut egui::Ui, snaps: &[&Snapshot], height: f32) {
    let all = [
        Line {
            label: "сытость существ",
            color: rgb(CREATURE_COLOR),
            value: |s: &Snapshot| s.fullness,
            shared: false,
        },
        Line {
            label: "растений от потолка",
            color: rgb(PLANT_COLOR),
            value: |s: &Snapshot| (s.plant_cap > 0).then(|| s.plants as f64 / s.plant_cap as f64),
            shared: false,
        },
    ];
    lines(ui, snaps, |s| s.tick, &all, Scale::Share, height);
}

/// A genome chart's point: the tick and the summary of each of the species' genes.
pub type GenePoint<'a> = (u64, &'a [GeneStat]);

/// The genome: a mini-chart per gene, each with its own scale. The line is the median, the
/// band is where 80% of the population live (10‒90%): the mean hides a split into two kinds,
/// and the band shows it. For a choice gene (a strategy) — the shares of the variants in
/// layers. On the right — the change since the first point of the visible window.
pub fn genome(ui: &mut egui::Ui, table: &[GeneSpec], points: &[GenePoint], color: Color32, row_h: f32) {
    let rows: Vec<usize> = (0..table.len()).filter(|&g| shown(&table[g])).collect();
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
    let (tick, at) = points[hovered.unwrap_or(n - 1)];
    let origin = points[0].1;
    let font = FontId::proportional(12.5);

    for (row_i, &g) in rows.iter().enumerate() {
        let spec = &table[g];
        let top = rect.top() + row_h * row_i as f32;
        let row = Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, row_h));
        if row_i > 0 {
            painter.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0, LINE));
        }
        let mut label = LayoutJob::simple_singleline(spec.label.into(), font.clone(), MUTED);
        label.wrap = TextWrapping {
            max_width: label_w - 6.0,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = painter.layout_job(label);
        painter.galley(Pos2::new(row.left(), row.center().y - galley.size().y / 2.0), galley, MUTED);
        let label_rect = Rect::from_min_size(row.left_top(), Vec2::new(label_w, row_h));
        ui.interact(label_rect, ui.id().with(("ген", row_i)), Sense::hover())
            .on_hover_text(format!("{} — {}", spec.label, spec.about));
        let spark = Rect::from_min_max(
            Pos2::new(spark_rect.left(), top + 3.0),
            Pos2::new(spark_rect.right(), top + row_h - 3.0),
        );

        let (value, value_color, change) = match (at[g], origin[g]) {
            (GeneStat::Number(now), GeneStat::Number(was)) => {
                number_row(&painter, spark, points, g, spec.is_percent(), color);
                let (value, change) = number_text(now.p50, was.p50, spec.is_percent());
                (value, TEXT, change)
            }
            (GeneStat::Shares(now), GeneStat::Shares(_)) => {
                shares_row(&painter, spark, points, g, spec.variants().unwrap_or_default().len());
                shares_text(spec, &now)
            }
            _ => (String::new(), TEXT, String::new()),
        };
        let change = painter.text(
            Pos2::new(rect.right(), row.center().y),
            Align2::RIGHT_CENTER,
            change,
            font.clone(),
            MUTED,
        );
        // The value does not run onto the change on the right: a long one (a variant's name) is cut
        // with an ellipsis.
        let left = rect.right() - value_w + 8.0;
        let mut job = LayoutJob::simple_singleline(value, font.clone(), value_color);
        job.wrap = TextWrapping {
            max_width: (change.left() - 6.0 - left).max(0.0),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = painter.layout_job(job);
        let pos = Pos2::new(left, row.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, value_color);
    }
    if let Some(i) = hovered {
        let x = x_at(spark_rect, i, n);
        painter
            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, MUTED));
    }
    ui.colored_label(
        MUTED,
        format!("тик {} · линия — медиана, полоса — 80% популяции; справа — изменение за окно", spaced(tick)),
    );
}

/// Whether to show a gene: a choice gene with one variant tells nothing apart.
pub fn shown(spec: &GeneSpec) -> bool {
    spec.variants().is_none_or(|v| v.len() >= 2)
}

fn spread_at(p: &GenePoint, g: usize) -> Spread {
    p.1[g].spread().copied().expect("числовой ген")
}

/// A mini-chart of a numeric gene: the 10‒90% band and the median line.
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
    // the band as a set of quadrilaterals: egui fills only convex shapes
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
    // small values (speed, burst, mutability) with tenths, big ones in whole numbers, as the card
    let value = match (percent, now < 20.0) {
        (true, _) => format!("{now:.0}%"),
        (false, true) => format!("{now:.1}"),
        (false, false) => format!("{now:.0}"),
    };
    (value, change)
}

/// The colours of a choice gene's variants — in order.
pub const VARIANT_COLORS: [Color32; 5] = [
    Color32::from_rgb(205, 134, 255),
    Color32::from_rgb(245, 197, 66),
    Color32::from_rgb(93, 211, 158),
    Color32::from_rgb(110, 170, 255),
    Color32::from_rgb(239, 99, 81),
];

/// A mini-chart of a choice gene: the variants' shares in layers from bottom to top.
fn shares_row(painter: &egui::Painter, spark: Rect, points: &[GenePoint], g: usize, variants: usize) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let share = |i: usize, k: usize| points[i].1[g].shares().map_or(0.0, |s| s[k]) as f32;
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

/// The most common variant — by the name of its colour (a legend to the chart's layers) — and its share.
fn shares_text(spec: &GeneSpec, now: &[f64; MAX_VARIANTS]) -> (String, Color32, String) {
    let variants = spec.variants().unwrap_or_default();
    let Some((k, v)) = variants.iter().enumerate().max_by(|a, b| now[a.0].total_cmp(&now[b.0])) else {
        return (String::new(), TEXT, String::new());
    };
    (v.label.to_string(), VARIANT_COLORS[k % VARIANT_COLORS.len()], format!("{:.0}%", now[k] * 100.0))
}

/// Histograms of a census group, two in a row: bars of how many have each value, the median as a
/// line and the middle 80% as a lighter strip under the bars. Two humps in a histogram are two
/// kinds within one diet. Under the cursor — the bar's range and share.
pub fn histograms(ui: &mut egui::Ui, group: &Group, columns: &[usize], color: Color32) {
    let (gap, cell_h) = (14.0, 62.0);
    let width = ui.available_width();
    let col_w = ((width - gap) / 2.0).max(60.0);
    let rows = columns.len().div_ceil(2);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, cell_h * rows as f32), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let font = FontId::proportional(12.0);
    let small = FontId::proportional(10.5);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mut hint = None;
    for (k, &c) in columns.iter().enumerate() {
        let column = &group.columns[c];
        let min =
            Pos2::new(rect.left() + (k % 2) as f32 * (col_w + gap), rect.top() + (k / 2) as f32 * cell_h);
        let cell = Rect::from_min_size(min, Vec2::new(col_w, cell_h - 6.0));
        painter.text(cell.left_top(), Align2::LEFT_TOP, census::label(c), font.clone(), MUTED);
        let label_rect = Rect::from_min_size(cell.left_top(), Vec2::new(col_w * 0.5, 15.0));
        ui.interact(label_rect, ui.id().with(("признак", c)), Sense::hover()).on_hover_text(format!(
            "{} — {}",
            census::label(c),
            census::about(c)
        ));
        if group.count == 0 {
            continue;
        }
        painter.text(
            cell.right_top(),
            Align2::RIGHT_TOP,
            format!("медиана {}", census::number(c, column.p50)),
            font.clone(),
            TEXT,
        );
        let bars = Rect::from_min_max(
            Pos2::new(cell.left(), cell.top() + 17.0),
            cell.right_bottom() - Vec2::new(0.0, 12.0),
        );
        let x = |v: f64| {
            let t = if column.hi > column.lo { (v - column.lo) / (column.hi - column.lo) } else { 0.5 };
            bars.left() + t.clamp(0.0, 1.0) as f32 * bars.width()
        };
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x(column.p10), bars.bottom()),
                Pos2::new(x(column.p90).max(x(column.p10) + 1.0), bars.bottom() + 3.0),
            ),
            1.0,
            color.gamma_multiply(0.5),
        );
        let top = column.bins.iter().copied().max().unwrap_or(0).max(1) as f32;
        let bar_w = bars.width() / census::BINS as f32;
        for (b, &n) in column.bins.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let h = (n as f32 / top).sqrt().max(0.08) * bars.height();
            let left = bars.left() + b as f32 * bar_w;
            let r = Rect::from_min_max(
                Pos2::new(left + 0.5, bars.bottom() - h),
                Pos2::new(left + bar_w - 0.5, bars.bottom()),
            );
            painter.rect_filled(r, 0.0, color.gamma_multiply(0.8));
        }
        let xm = x(column.p50);
        painter
            .line_segment([Pos2::new(xm, bars.top()), Pos2::new(xm, bars.bottom())], Stroke::new(1.2, TEXT));
        painter.text(
            bars.left_bottom() + Vec2::new(0.0, 3.0),
            Align2::LEFT_TOP,
            census::number(c, column.lo),
            small.clone(),
            MUTED,
        );
        painter.text(
            bars.right_bottom() + Vec2::new(0.0, 3.0),
            Align2::RIGHT_TOP,
            census::number(c, column.hi),
            small.clone(),
            MUTED,
        );
        if let Some(p) = pointer.filter(|p| bars.contains(*p)) {
            let b = (((p.x - bars.left()) / bar_w) as usize).min(census::BINS - 1);
            let step = (column.hi - column.lo) / census::BINS as f64;
            let (from, to) = (column.lo + step * b as f64, column.lo + step * (b + 1) as f64);
            let share = column.bins[b] as f64 / group.count as f64 * 100.0;
            painter.rect_stroke(
                Rect::from_min_max(
                    Pos2::new(bars.left() + b as f32 * bar_w, bars.top()),
                    Pos2::new(bars.left() + (b + 1) as f32 * bar_w, bars.bottom()),
                ),
                0.0,
                Stroke::new(1.0, MUTED),
                egui::StrokeKind::Inside,
            );
            hint = Some(format!(
                "{} {}‒{}: {} существ, {share:.0}%\n80% — от {} до {}",
                census::label(c),
                census::number(c, from),
                census::number(c, to),
                spaced(u64::from(column.bins[b])),
                census::number(c, column.p10),
                census::number(c, column.p90),
            ));
        }
    }
    if let Some(text) = hint {
        ui.interact(rect, ui.id().with("гистограммы"), Sense::hover()).on_hover_text(text);
    }
}

/// A choice gene's variants in a census group, as one bar of shares with a legend.
pub fn shares(ui: &mut egui::Ui, spec: &GeneSpec, column: &census::Column) {
    let total = column.bins.iter().sum::<u32>().max(1) as f32;
    let variants = spec.variants().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add_sized(
            [118.0, 16.0],
            egui::Label::new(egui::RichText::new(spec.label).color(MUTED)).truncate(),
        )
        .on_hover_text(format!("{} — {}", spec.label, spec.about));
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width().min(260.0), 12.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let mut left = rect.left();
        for (k, &n) in column.bins.iter().enumerate() {
            let w = n as f32 / total * rect.width();
            let r = Rect::from_min_max(Pos2::new(left, rect.top()), Pos2::new(left + w, rect.bottom()));
            painter.rect_filled(r, 0.0, VARIANT_COLORS[k % VARIANT_COLORS.len()].gamma_multiply(0.8));
            left += w;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(126.0);
        for (k, v) in variants.iter().enumerate() {
            let share = column.bins.get(k).copied().unwrap_or(0) as f32 / total * 100.0;
            ui.colored_label(VARIANT_COLORS[k % VARIANT_COLORS.len()], format!("{} {share:.0}%", v.label));
        }
    });
}

/// The side of a scatter's cell, in points.
const SCATTER_CELL: f32 = 5.0;

/// Two characteristics of a census group against each other, as a density: separate clouds are
/// separate kinds. Everybody is coloured by diet; a cell's brightness is how many are there.
pub fn scatter(ui: &mut egui::Ui, census: &Census, group: usize, axes: [usize; 2], height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
    let columns = &census.groups[group].columns;
    let ([cx, cy], [ax, ay]) = (axes, [&columns[axes[0]], &columns[axes[1]]]);
    let inner = rect.shrink2(Vec2::new(6.0, 6.0));
    // square cells: dashes would read as a trend
    let w = ((inner.width() / SCATTER_CELL).round() as usize).max(1);
    let h = ((inner.height() / SCATTER_CELL).round() as usize).max(1);
    let cell = |v: f32, lo: f64, hi: f64, n: usize| {
        if hi > lo {
            ((f64::from(v) - lo) / (hi - lo) * n as f64).floor().clamp(0.0, (n - 1) as f64) as usize
        } else {
            n / 2
        }
    };
    let mut counts = vec![[0u32; 4]; w * h];
    for (row, &diet) in census.rows.iter().zip(&census.diets) {
        if group != 0 && usize::from(diet) != group - 1 {
            continue;
        }
        let (i, j) = (cell(row[cx], ax.lo, ax.hi, w), cell(row[cy], ay.lo, ay.hi, h));
        counts[(h - 1 - j) * w + i][usize::from(diet)] += 1;
    }
    let (cw, ch) = (inner.width() / w as f32, inner.height() / h as f32);
    for (k, c) in counts.iter().enumerate() {
        let n: u32 = c.iter().sum();
        if n == 0 {
            continue;
        }
        let mix: [f32; 3] = std::array::from_fn(|ch| {
            (0..4).map(|d| c[d] as f32 * f32::from(DIET_COLORS[d][ch])).sum::<f32>() / n as f32
        });
        let a = (0.35 + (n as f32).log2() / 8.0).min(1.0);
        let color = Color32::from_rgb(mix[0] as u8, mix[1] as u8, mix[2] as u8).gamma_multiply(a);
        let min = Pos2::new(inner.left() + (k % w) as f32 * cw, inner.top() + (k / w) as f32 * ch);
        painter.rect_filled(Rect::from_min_size(min, Vec2::new(cw + 0.5, ch + 0.5)), 0.0, color);
    }
    // the axes' ranges in the corners where each axis starts: up the left side, along the bottom
    let small = FontId::proportional(10.5);
    let range = |c: usize, col: &census::Column| {
        format!("{} {}‒{}", census::label(c), census::number(c, col.lo), census::number(c, col.hi))
    };
    painter.text(
        rect.right_bottom() + Vec2::new(-4.0, -2.0),
        Align2::RIGHT_BOTTOM,
        format!("{} →", range(cx, ax)),
        small.clone(),
        MUTED,
    );
    painter.text(
        rect.left_top() + Vec2::new(4.0, 2.0),
        Align2::LEFT_TOP,
        format!("↑ {}", range(cy, ay)),
        small,
        MUTED,
    );
    if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| inner.contains(*p)) {
        let (i, j) = (
            (((p.x - inner.left()) / cw) as usize).min(w - 1),
            (((p.y - inner.top()) / ch) as usize).min(h - 1),
        );
        let at = |t: usize, n: usize, lo: f64, hi: f64| lo + (hi - lo) * (t as f64 + 0.5) / n as f64;
        let n: u32 = counts[j * w + i].iter().sum();
        if n == 0 {
            return;
        }
        ui.interact(rect, ui.id().with("разброс"), Sense::hover()).on_hover_text(format!(
            "{} ≈ {}, {} ≈ {}: {} существ",
            census::label(cx),
            census::number(cx, at(i, w, ax.lo, ax.hi)),
            census::label(cy),
            census::number(cy, at(h - 1 - j, h, ay.lo, ay.hi)),
            spaced(u64::from(n)),
        ));
    }
}

/// A hint colour for the energy bands: the hungry in red.
pub fn energy_color(frac: f64, base: Color32) -> Color32 {
    if frac < 0.25 { crate::theme::DANGER } else { base }
}

/// Where the creatures live in time: along x — samples, along y — bands of depth (the top is
/// the surface), the brightness is the share of creatures in a band. On top — the median depth
/// and the borders of the layer where 80% live (10‒90%). Returns the sample under the cursor,
/// so that the histogram beside it shows exactly that one.
pub fn depth_map(ui: &mut egui::Ui, snaps: &[&Snapshot], height: f32) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
    let n = snaps.len();
    if n < 2 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "история копится…",
            FontId::proportional(13.0),
            MUTED,
        );
        return None;
    }
    let inner = rect.shrink(2.0);
    let color = rgb(CREATURE_COLOR);
    let bands = snaps[0].creatures_by_depth.len();
    let (col_w, band_h) = (inner.width() / (n - 1) as f32, inner.height() / bands as f32);
    for (i, s) in snaps.iter().enumerate() {
        let total: usize = s.creatures_by_depth.iter().sum();
        if total == 0 {
            continue;
        }
        let x = x_at(inner, i, n);
        for (b, &count) in s.creatures_by_depth.iter().enumerate() {
            if count == 0 {
                continue;
            }
            // the root: a band with a tenth of the herd must be visible too
            let share = (count as f32 / total as f32).sqrt();
            let top = inner.top() + band_h * b as f32;
            let cell = Rect::from_min_max(
                Pos2::new((x - col_w / 2.0).max(inner.left()), top),
                Pos2::new((x + col_w / 2.0 + 0.5).min(inner.right()), top + band_h + 0.5),
            );
            painter.rect_filled(cell, 0.0, color.gamma_multiply(0.85 * share));
        }
    }
    let y = |pct: f64| inner.top() + (pct / 100.0).clamp(0.0, 1.0) as f32 * inner.height();
    type Pick = fn(&Spread) -> f64;
    let marks: [(Pick, f32); 3] = [(|d| d.p10, 1.0), (|d| d.p50, 1.8), (|d| d.p90, 1.0)];
    for (pick, width) in marks {
        let mut run = Vec::new();
        for (i, s) in snaps.iter().enumerate() {
            match &s.depth {
                Some(d) => run.push(Pos2::new(x_at(inner, i, n), y(pick(d)))),
                None => draw_line(&painter, &mut run, width),
            }
        }
        draw_line(&painter, &mut run, width);
    }
    let hovered = hover_index(ui, rect, n);
    if let Some(i) = hovered {
        let x = x_at(inner, i, n);
        painter
            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, MUTED));
    }
    hovered
}

fn draw_line(painter: &egui::Painter, run: &mut Vec<Pos2>, width: f32) {
    if run.len() >= 2 {
        painter.add(Shape::line(std::mem::take(run), Stroke::new(width, TEXT.gamma_multiply(0.8))));
    }
    run.clear();
}

/// Plants and creatures by bands — of depth (top to bottom) or of width (left to right, also
/// in rows from top to bottom). Left of the middle — the share of plants, on the right — the
/// share of creatures, each of its own kind: one sees whether they live where the food is.
/// `edges` — the labels of the first and last band.
pub fn bands(ui: &mut egui::Ui, plants: &[usize], creatures: &[usize], edges: (&str, &str)) {
    let row_h = 17.0;
    let n = plants.len();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h * n as f32), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let label_w = 86.0;
    let mid = rect.left() + label_w + (rect.width() - label_w) / 2.0;
    let half = (rect.right() - mid - 4.0).max(10.0);
    let share = |xs: &[usize]| {
        let total: usize = xs.iter().sum();
        xs.iter().map(|&x| if total > 0 { x as f32 / total as f32 } else { 0.0 }).collect::<Vec<f32>>()
    };
    let (ps, vs) = (share(plants), share(creatures));
    // the scale is by the densest band of both kinds, so that the bands are comparable
    let max = ps.iter().chain(&vs).fold(0.01f32, |m, &v| m.max(v));
    let font = FontId::proportional(11.5);
    for i in 0..n {
        let top = rect.top() + row_h * i as f32;
        let cy = top + row_h / 2.0;
        let label = match i {
            0 => format!("{} 0‒{}%", edges.0, 100 / n),
            _ if i == n - 1 => format!("{} {}‒100%", edges.1, 100 * i / n),
            _ => format!("{}‒{}%", 100 * i / n, 100 * (i + 1) / n),
        };
        painter.text(Pos2::new(rect.left(), cy), Align2::LEFT_CENTER, label, font.clone(), MUTED);
        let bar = |from: f32, to: f32, color: Color32| {
            let r = Rect::from_min_max(
                Pos2::new(from.min(to), top + 3.0),
                Pos2::new(from.max(to), top + row_h - 3.0),
            );
            painter.rect_filled(r, 2.0, color);
        };
        bar(mid, mid - ps[i] / max * half, rgb(PLANT_COLOR).gamma_multiply(0.8));
        bar(mid, mid + vs[i] / max * half, rgb(CREATURE_COLOR).gamma_multiply(0.8));
        for (v, x, align) in
            [(ps[i], mid - 4.0, Align2::RIGHT_CENTER), (vs[i], mid + 4.0, Align2::LEFT_CENTER)]
        {
            if v >= 0.005 {
                painter.text(Pos2::new(x, cy), align, format!("{:.0}%", v * 100.0), font.clone(), TEXT);
            }
        }
    }
    painter.line_segment([Pos2::new(mid, rect.top()), Pos2::new(mid, rect.bottom())], Stroke::new(1.0, LINE));
}

#[cfg(test)]
mod tests {
    use super::number_text;

    /// The genome chart shows a median as the card does: small values with tenths (a burst of
    /// 1.45 is no «1», a mutability of 0.4 no «0»), big ones whole, percents as percents.
    #[test]
    fn the_genome_chart_shows_small_values_with_tenths() {
        assert_eq!(number_text(1.45, 1.0, false).0, "1.4");
        assert_eq!(number_text(0.4, 1.0, false).0, "0.4");
        assert_eq!(number_text(42.4, 40.0, false).0, "42");
        assert_eq!(number_text(37.6, 30.0, true), ("38%".into(), "+8 п.п.".into()));
    }
}
