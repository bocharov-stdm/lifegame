//! The rules' widgets «Новый мир» and the lab share: a field's input and hint, a tab's grid of
//! fields, the diets' table, the body's formula and the food preview.

use eframe::egui::{self, RichText, Vec2};
use std::sync::Arc;

use life_core::flora::{self, Patch};
use life_core::{Rules, Space};

use crate::frame::PLANT_COLOR;
use crate::settings::{DIET_ROWS, FIELDS, Field, Key, Settings, Tab, field};
use crate::theme::{self, BG, DANGER, MUTED};

/// What the «Вода» tab is about.
pub const WATER_NOTE: &str = "Сверху вода тёплая, ниже термоклина — холодная. Хладнокровным в холоде жить \
                              дешевле, но плавают они медленнее; теплокровным всё равно.";

/// The fields of one tab by the `FIELDS` table — those visible now.
pub fn fields(ui: &mut egui::Ui, s: &mut Settings, tab: Tab) {
    let default = Settings::default();
    egui::Grid::new(("поля", tab as u8)).num_columns(4).spacing([12.0, 8.0]).show(ui, |ui| {
        // the diets' edges are the table's cells, not rows of their own
        for f in FIELDS.iter().filter(|f| f.tab == tab && !matches!(f.key, Key::Diet(..))) {
            if !(f.visible)(s) {
                continue;
            }
            ui.label(f.label).on_hover_text(field_hint(f));
            let mut v = s.get(f.key);
            if field_input(ui, f, &mut v) {
                s.set(f.key, v);
            }
            base_value(ui, f, &default);
            if s.is_default(f.key) {
                ui.label("");
            } else if ui.small_button("↺").on_hover_text("Вернуть значение по умолчанию").clicked()
            {
                s.set(f.key, default.get(f.key));
            }
            ui.end_row();
        }
    });
    if tab == Tab::World {
        // the four diet boxes are weights: what share each diet gets
        let weights = [Key::Herbivores, Key::Omnivores, Key::Scavengers, Key::Carnivores].map(|k| s.get(k));
        let sum: f64 = weights.iter().sum();
        let shares = if sum <= 0.0 {
            "одни травоядные".to_string()
        } else {
            let parts: Vec<String> = weights
                .iter()
                .zip(crate::theme::DIET_NAMES)
                .filter(|(w, _)| **w > 0.0)
                .map(|(w, name)| format!("{name} {:.0}%", w / sum * 100.0))
                .collect();
            parts.join(" · ")
        };
        ui.label(format!("Основатели по питанию: {shares}"));
        ui.colored_label(
            MUTED,
            "Численность основателей — на участок 6000×4000: в большом мире всё в той же плотности.",
        );
    }
}

/// A field from `FIELDS`: a slider or, if the field has variants, a drop-down list. true — the
/// value has changed.
pub fn field_input(ui: &mut egui::Ui, f: &Field, value: &mut f64) -> bool {
    if f.choices.is_empty() {
        // typed or dragged, the value stays inside the field's hard limits
        let mut shown = *value * f.shown;
        // a percent field says so in its box too, not only in its base value; a price shown per
        // 100 of the game's own (life ×2, speed ×0,5) shows the factor the formula uses beside it
        let unit = if f.unit.is_empty() && (f.format)(f.hi).ends_with('%') {
            " %".to_string()
        } else if f.unit.is_empty() && f.shown != 1.0 && f.shown != 100.0 {
            format!(" · ×{}", format!("{:.2}", *value).replace('.', ","))
        } else {
            f.unit.to_string()
        };
        let decimals = f.decimals;
        let response = ui
            .add(
                egui::DragValue::new(&mut shown)
                    .range(f.lo * f.shown..=f.hi * f.shown)
                    .speed(f.step * f.shown)
                    // the decimal comma, as in every text of the game; a point is taken too
                    .custom_formatter(move |v, _| format!("{v:.decimals$}").replace('.', ","))
                    .custom_parser(|text| {
                        text.trim().trim_end_matches('%').trim().replace(',', ".").parse().ok()
                    })
                    .suffix(unit),
            )
            .on_hover_text(field_hint(f));
        if response.changed() {
            *value = shown / f.shown;
        }
        return response.changed();
    }
    let mut k = (*value as usize).min(f.choices.len() - 1);
    let before = k;
    egui::ComboBox::from_id_salt(f.label)
        .selected_text(f.choices[k])
        .width(150.0)
        .show_ui(ui, |ui| {
            for (i, name) in f.choices.iter().enumerate() {
                ui.selectable_value(&mut k, i, *name);
            }
        })
        .response
        .on_hover_text(f.hint);
    *value = k as f64;
    k != before
}

/// A field's hint with its limits: «…\n\nМожно от 0,04 до 20. Больше 50 в тик — …».
pub fn field_hint(f: &Field) -> String {
    if !f.choices.is_empty() {
        return f.hint.to_string();
    }
    let why = if f.limit.is_empty() { String::new() } else { format!(" {}", f.limit) };
    format!(
        "{}\n\nМожно от {} до {}.{why} Тяните мышью влево-вправо или щёлкните и впишите число.",
        f.hint,
        (f.format)(f.lo),
        (f.format)(f.hi)
    )
}

/// «база 2.5»: the default value, muted, beside the input.
pub fn base_value(ui: &mut egui::Ui, f: &Field, default: &Settings) {
    let base = if f.choices.is_empty() {
        (f.format)(default.get(f.key))
    } else {
        f.choices[(default.get(f.key) as usize).min(f.choices.len() - 1)].to_string()
    };
    ui.colored_label(MUTED, format!("база {base}")).on_hover_text("Значение по умолчанию");
}

/// The «Питание» tab: a row per edge, a column per diet, the bases of the row beside it. Hover a
/// row's name for what it means, a cell for its limits.
pub fn diet_table(ui: &mut egui::Ui, s: &mut Settings) {
    let default = Settings::default();
    let cell = |d: usize, e: usize| Key::Diet(d as u8, e as u8);
    ui.colored_label(
        MUTED,
        "Сильные стороны каждого питания. Доли усвоения не больше 100%: энергия берётся только из растений \
         и дальше лишь переходит по цепочке.",
    );
    ui.add_space(4.0);
    egui::Grid::new("бонусы питаний").num_columns(7).spacing([8.0, 6.0]).striped(true).show(ui, |ui| {
        ui.label("");
        for d in 0..4 {
            ui.colored_label(theme::rgb(theme::DIET_COLORS[d]), theme::DIET_NAMES[d])
                .on_hover_text(theme::DIET_HINTS[d]);
        }
        ui.colored_label(MUTED, "база").on_hover_text("Значения по умолчанию, по порядку столбцов");
        ui.label("");
        ui.end_row();
        for (e, (label, hint)) in DIET_ROWS.iter().enumerate() {
            ui.label(*label).on_hover_text(*hint);
            for d in 0..4 {
                let f = field(cell(d, e));
                let mut v = s.get(f.key);
                if field_input(ui, f, &mut v) {
                    s.set(f.key, v);
                }
            }
            let bases: Vec<String> =
                (0..4).map(|d| (field(cell(d, e)).format)(default.get(cell(d, e)))).collect();
            ui.colored_label(MUTED, bases.join(" · "));
            if (0..4).all(|d| s.is_default(cell(d, e))) {
                ui.label("");
            } else if ui.small_button("↺").on_hover_text("Вернуть строку по умолчанию").clicked()
            {
                for d in 0..4 {
                    s.set(cell(d, e), default.get(cell(d, e)));
                }
            }
            ui.end_row();
        }
    });
}

/// The body's upkeep in words and a chart: the formula with the rules' numbers, what a body twice
/// the base costs, and each term's price against its stat.
pub fn body_formula(ui: &mut egui::Ui, rules: &Result<Rules, String>) {
    let r = match rules {
        Ok(r) => r,
        Err(e) => {
            ui.colored_label(DANGER, format!("правила не сходятся: {e}"));
            return;
        }
    };
    // what each term costs the base genome by config (the three are equal by design)
    let base = life_core::config::SIZE_ENERGY_COEF * 40f64.powf(life_core::config::SIZE_ENERGY_POWER);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            ui.label(crate::theme::strong("Сколько стоит тело"));
            ui.label("Каждый тик существо тратит энергию на своё тело:");
            let formula = format!(
                "трата = {:.2} × (
    {:.3} × р^{:.2}
  + {:.3} × с^{:.2} × р^{:.2}
  + {:.3} × з^{:.2} )",
                r.cost_scale,
                base * r.size_cost,
                r.size_power,
                base * r.speed_cost,
                r.speed_power,
                r.speed_mass_power,
                base * r.sight_cost,
                r.sight_power,
            );
            ui.label(RichText::new(formula).monospace()).on_hover_text(
                "р, с, з — размер, скорость и зрение в долях базовых (40, 10 и 400): у базового существа \
                 каждое равно 1. За скорость платят по шагу этого тика: стоя — ничего, кроме мышц \
                 рывка. Первое число — общая цена жизни множителем (её 100 в настройках — ×2; цена \
                 скорости 100 — ×0,5), числа перед буквами — цены без поправок питания, степени — как \
                 быстро дорожает стат выше базового. Для сравнения: полный бак базового существа — 100.",
            );
            let twice = |power: f64| 2f64.powf(power);
            ui.colored_label(
                MUTED,
                format!(
                    "Базовое существо на полном ходу платит {:.3} в тик. Вдвое крупнее — тело дороже в {:.1} раза, \
                     вдвое быстрее — бег в {:.1}, вдвое зорче — зрение в {:.1}.",
                    r.upkeep(40.0, 10.0, 400.0),
                    twice(r.size_power),
                    twice(r.speed_power),
                    twice(r.sight_power)
                ),
            );
            cost_chart(ui, [r.size_power, r.speed_power, r.sight_power]);
        });
    });
}

/// How each term's price grows with its stat, from half the base to three times it.
fn cost_chart(ui: &mut egui::Ui, powers: [f64; 3]) {
    let size = Vec2::new(ui.available_width().min(300.0), 90.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, theme::LINE), egui::StrokeKind::Inside);
    let inner = rect.shrink(6.0);
    let (x0, x1, top) = (0.5f64, 3.0f64, 10.0f64);
    let at = |x: f64, y: f64| {
        egui::pos2(
            inner.left() + ((x - x0) / (x1 - x0)) as f32 * inner.width(),
            inner.bottom() - (y.min(top) / top) as f32 * inner.height(),
        )
    };
    // the base: stat 1, price 1
    let grid = egui::Stroke::new(1.0, theme::LINE);
    painter.line_segment([at(1.0, 0.0), at(1.0, top)], grid);
    painter.line_segment([at(x0, 1.0), at(x1, 1.0)], grid);
    let names = ["размер", "скорость", "зрение"];
    let colors = theme::STAT_COLORS;
    for (power, color) in powers.iter().zip(colors) {
        let points: Vec<egui::Pos2> =
            (0..=60).map(|i| x0 + (x1 - x0) * i as f64 / 60.0).map(|x| at(x, x.powf(*power))).collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.6, color)));
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(MUTED, "цена от стата (от 0,5 до 3 базовых, шкала до ×10):")
            .on_hover_text("Серый крест — базовое существо: стат 1, цена 1.");
        for (name, color) in names.iter().zip(colors) {
            ui.colored_label(color, *name);
        }
    });
}

/// The food preview of the sliders' rules, or why the rules are refused.
pub fn rules_preview(ui: &mut egui::Ui, rules: &Result<Rules, String>, space: Space, seed: u64) {
    match rules {
        Ok(rules) => food_preview(ui, rules, space, seed),
        Err(e) => {
            ui.colored_label(DANGER, format!("правила не сходятся: {e}"));
        }
    }
}

/// The food preview: the world in its own proportions, painted by the plants' density by the
/// rules — brighter where thicker — and the thickets of seed `seed`. The top is the surface.
fn food_preview(ui: &mut egui::Ui, rules: &Rules, space: Space, seed: u64) {
    let aspect = (space.width / space.height) as f32;
    let (max_w, max_h) = (ui.available_width().clamp(120.0, 280.0), 170.0);
    // a very long strip is still visible at least as a 14-point strip
    let size = if max_w / aspect <= max_h {
        Vec2::new(max_w, (max_w / aspect).max(14.0))
    } else {
        Vec2::new(max_h * aspect, max_h)
    };
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let (nx, ny) = (((size.x / 4.0) as usize).clamp(8, 70), ((size.y / 4.0) as usize).clamp(3, 42));
        let cell = Vec2::new(size.x / nx as f32, size.y / ny as f32);
        let plant = theme::rgb(PLANT_COLOR);
        let patches =
            rules.plant_patches > 0.0 && rules.plant_patches * space.area_ratio() <= PREVIEW_PATCHES;
        for j in 0..ny {
            for i in 0..nx {
                let d = flora::density(rules, (i as f64 + 0.5) / nx as f64, (j as f64 + 0.5) / ny as f64);
                let min = rect.min + Vec2::new(i as f32 * cell.x, j as f32 * cell.y);
                // the cells with a half-point margin: otherwise seams are visible between them
                let r = egui::Rect::from_min_size(min, cell + Vec2::splat(0.5)).intersect(rect);
                let shade = if patches { 0.35 } else { 1.0 };
                painter.rect_filled(r, 0.0, BG.lerp_to_gamma(plant, shade * d.sqrt() as f32));
            }
        }
        if patches {
            let (sx, sy) = (size.x as f64 / space.width, size.y as f64 / space.height);
            for p in preview_patches(ui, rules, space, seed).iter() {
                let center = rect.min + Vec2::new((p.x * sx) as f32, (p.y * sy) as f32);
                let points = (0..16)
                    .map(|i| {
                        let (sin, cos) = (i as f64 / 16.0 * std::f64::consts::TAU).sin_cos();
                        let dx = cos * if cos < 0.0 { p.left } else { p.right } * sx;
                        let dy = sin * if sin < 0.0 { p.up } else { p.down } * sy;
                        center + Vec2::new(dx as f32, dy as f32)
                    })
                    .collect();
                painter.add(egui::Shape::convex_polygon(
                    points,
                    plant.gamma_multiply(0.75),
                    egui::Stroke::NONE,
                ));
            }
        }
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, MUTED), egui::StrokeKind::Outside);
        let caption = if patches {
            "вверху поверхность; пятна — заросли этого сида"
        } else if rules.plant_patches > 0.0 {
            "вверху поверхность; ярче — гуще (зарослей слишком много, чтобы их рисовать)"
        } else {
            "вверху поверхность; ярче — гуще"
        };
        ui.colored_label(MUTED, caption);
        // the world in real units (`life_core::units`): the base fish is 20 cm
        ui.colored_label(
            MUTED,
            format!(
                "≈ {:.0} × {:.0} м, базовая рыба 20 см",
                life_core::units::metres(space.width),
                life_core::units::metres(space.height)
            ),
        );
    });
}

/// Patches the preview still draws: more would be specks of a pixel.
const PREVIEW_PATCHES: f64 = 3000.0;

/// The patch layout of these rules, world and seed; kept between frames, since the preview is
/// redrawn every frame and the layout changes only with the sliders.
fn preview_patches(ui: &egui::Ui, rules: &Rules, space: Space, seed: u64) -> Arc<[Patch]> {
    let id = egui::Id::new(("заросли", format!("{rules:?}{space:?}"), seed));
    let cached: Option<Arc<[Patch]>> = ui.ctx().data(|d| d.get_temp(id));
    cached.unwrap_or_else(|| {
        let patches: Arc<[Patch]> = Arc::from(flora::Flora::new(rules, &space, seed).patches());
        ui.ctx().data_mut(|d| d.insert_temp(id, patches.clone()));
        patches
    })
}
