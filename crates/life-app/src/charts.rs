//! Графики боковой панели и окна «Статистика»: численности, сытость, геном,
//! где живут. Рисуются прямо кистью egui — линии из пары сотен точек,
//! отдельная библиотека графиков им не нужна. Логика — как в `app/render.py`
//! (тег python-final): у каждой величины своя шкала, под курсором — значения в
//! этой точке, справа — изменение от начала.

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use life_core::genome::GeneSpec;
use life_sim::observe::{GeneStat, MAX_VARIANTS, Snapshot, Spread};

use crate::frame::{PLANT_COLOR, PREDATOR_COLOR, VEGETARIAN_COLOR};
use crate::history::{History, Sample};
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

/// Линия графика: подпись, цвет и значение в точке (None — величины нет,
/// например сытости хищников, когда хищников нет: линия прерывается).
pub struct Line<T> {
    pub label: &'static str,
    pub color: Color32,
    pub value: fn(&T) -> Option<f64>,
}

/// Шкала графика линиями.
#[derive(Clone, Copy, PartialEq)]
pub enum Scale {
    /// У каждой линии своя, от нуля до её максимума на участке: иначе хищники
    /// лежали бы на нуле рядом с тысячами растений. Подписи — числами.
    Own,
    /// Общая 0‒100%: доли сравнимы между собой. Подписи — процентами.
    Share,
}

/// График линиями по точкам истории. Под графиком — тик и значения в точке
/// под курсором (или в последней).
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
        // середина шкалы — ориентир для «больше или меньше половины»
        let y = inner.center().y;
        painter
            .line_segment([Pos2::new(inner.left(), y), Pos2::new(inner.right(), y)], Stroke::new(1.0, LINE));
    }
    for line in lines {
        let max = match scale {
            Scale::Own => points.iter().filter_map(|p| (line.value)(p)).fold(0.0f64, f64::max).max(1.0),
            Scale::Share => 1.0,
        };
        // Кусками: где величины нет, линия рвётся.
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
            // неразрывные пробелы: подпись не рвётся переносом посередине
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

/// Численности: растения, травоядные, хищники — каждая в своей шкале.
/// `predators` — рисовать ли хищников: без них в партии линия лежала бы на нуле.
pub fn populations(ui: &mut egui::Ui, history: &History, whole: bool, height: f32, predators: bool) {
    let points = history.counts.points(whole);
    let all = [
        Line { label: "растения", color: rgb(PLANT_COLOR), value: |s: &Sample| Some(s.plants) },
        Line {
            label: "травоядные", color: rgb(VEGETARIAN_COLOR), value: |s: &Sample| Some(s.vegetarians)
        },
        Line { label: "хищники", color: rgb(PREDATOR_COLOR), value: |s: &Sample| Some(s.predators) },
    ];
    let shown = if predators { &all[..] } else { &all[..2] };
    lines(ui, &points, |s| s.tick, shown, Scale::Own, height);
}

/// Сытость и голод: средняя заполненность бака травоядных и хищников, доля
/// голодных (охотящихся) хищников и насколько растения упёрлись в потолок.
pub fn energy(ui: &mut egui::Ui, snaps: &[&Snapshot], predators: bool, height: f32) {
    let vegetarians = rgb(VEGETARIAN_COLOR);
    let hunters = rgb(PREDATOR_COLOR);
    let all = [
        Line {
            label: "сытость травоядных", color: vegetarians, value: |s: &Snapshot| s.vegetarian_fullness
        },
        Line {
            label: "растений от потолка",
            color: rgb(PLANT_COLOR),
            value: |s: &Snapshot| (s.plant_cap > 0).then(|| s.plants as f64 / s.plant_cap as f64),
        },
        Line {
            label: "сытость хищников", color: hunters, value: |s: &Snapshot| s.predator_fullness
        },
        Line {
            label: "голодных хищников",
            color: hunters.gamma_multiply(0.55),
            value: |s: &Snapshot| s.predators_hungry,
        },
    ];
    let shown = if predators { &all[..] } else { &all[..2] };
    lines(ui, snaps, |s| s.tick, shown, Scale::Share, height);
}

/// Точка графика генома: тик и сводка каждого гена вида.
pub type GenePoint<'a> = (u64, &'a [GeneStat]);

/// Геном: по мини-графику на ген, у каждого своя шкала. Линия — медиана,
/// полоса — где живут 80% популяции (10‒90%): среднее прячет раскол на два
/// вида, а полоса его показывает. У гена-выбора (стратегии) — доли вариантов
/// слоями. Справа — значение и изменение от начала (`origin` — первая сводка
/// партии; None — первая точка ряда). Таблица генов — вида, чей это геном.
pub fn genome(
    ui: &mut egui::Ui,
    table: &[GeneSpec],
    points: &[GenePoint],
    origin: Option<&[GeneStat]>,
    color: Color32,
    row_h: f32,
) {
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
    let origin = origin.unwrap_or(points[0].1);
    let font = FontId::proportional(12.5);

    for (row_i, &g) in rows.iter().enumerate() {
        let spec = &table[g];
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
        // Значение не заходит на изменение справа: длинное (имя варианта)
        // обрезается многоточием.
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
        format!(
            "тик {} · линия — медиана, полоса — 80% популяции; справа — изменение от начала",
            spaced(tick)
        ),
    );
}

/// Показывать ли ген: ген-выбор с одним вариантом ничего не различает.
pub fn shown(spec: &GeneSpec) -> bool {
    spec.variants().is_none_or(|v| v.len() >= 2)
}

fn spread_at(p: &GenePoint, g: usize) -> Spread {
    p.1[g].spread().copied().expect("числовой ген")
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
pub const VARIANT_COLORS: [Color32; 5] = [
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

/// Самый частый вариант — именем его цвета (легенда к слоям графика) — и его доля.
fn shares_text(spec: &GeneSpec, now: &[f64; MAX_VARIANTS]) -> (String, Color32, String) {
    let variants = spec.variants().unwrap_or_default();
    let Some((k, v)) = variants.iter().enumerate().max_by(|a, b| now[a.0].total_cmp(&now[b.0])) else {
        return (String::new(), TEXT, String::new());
    };
    (v.label.to_string(), VARIANT_COLORS[k % VARIANT_COLORS.len()], format!("{:.0}%", now[k] * 100.0))
}

/// Цвет-подсказка для полос энергии: голодные — красным.
pub fn energy_color(frac: f64, base: Color32) -> Color32 {
    if frac < 0.25 { crate::theme::DANGER } else { base }
}

/// Где живут травоядные во времени: по x — срезы, по y — полосы глубины
/// (верх — поверхность), яркость — доля травоядных в полосе. Поверх — медиана
/// глубины и границы слоя, где живут 80% (10‒90%). Возвращает срез под
/// курсором, чтобы гистограмма рядом показала именно его.
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
    let color = rgb(VEGETARIAN_COLOR);
    let bands = snaps[0].vegetarians_by_depth.len();
    let (col_w, band_h) = (inner.width() / (n - 1) as f32, inner.height() / bands as f32);
    for (i, s) in snaps.iter().enumerate() {
        let total: usize = s.vegetarians_by_depth.iter().sum();
        if total == 0 {
            continue;
        }
        let x = x_at(inner, i, n);
        for (b, &count) in s.vegetarians_by_depth.iter().enumerate() {
            if count == 0 {
                continue;
            }
            // корень: и полоса с десятой долей стада должна быть видна
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
            match &s.vegetarian_depth {
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

/// Растения и травоядные по полосам — глубины (сверху вниз) или ширины (слева
/// направо, тоже строками сверху вниз). Слева от середины — доля растений,
/// справа — доля травоядных, каждая от своего вида: видно, живут ли там, где
/// еда. `edges` — подписи первой и последней полосы.
pub fn bands(ui: &mut egui::Ui, plants: &[usize], vegetarians: &[usize], edges: (&str, &str)) {
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
    let (ps, vs) = (share(plants), share(vegetarians));
    // шкала — по самой густой полосе обоих видов, чтобы полосы были сравнимы
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
        bar(mid, mid + vs[i] / max * half, rgb(VEGETARIAN_COLOR).gamma_multiply(0.8));
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
