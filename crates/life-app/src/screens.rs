//! The menu, «Новый мир», the screen's settings and the help.

use eframe::egui::{self, Align2, RichText, Vec2};
use std::sync::Arc;

use life_core::flora::{self, Patch};
use life_core::genome::creature;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space};

use crate::app::{LifeApp, Screen};
use crate::frame::PLANT_COLOR;
use crate::settings::{
    DIET_ROWS, FIELDS, Field, Key, PRESETS, SEED_MAX, Settings, Tab, UI_SCALES, auto_threads, cpu_threads,
    field,
};
use crate::theme::{self, ACCENT, BG, DANGER, GOOD, MUTED, VEIL, spaced};

/// An estimate of a big world: how many creatures at the start and how fast a tick will go.
/// A tick's cost is taken from the measurement of the world running now (the menu's background
/// or a game), rescaled to the area: a tick grows with the number of creatures, and that with
/// the area. It is a measurement on this machine, not an invented formula. Rules the world would
/// refuse are said instead.
pub fn estimate(settings: &Settings, measured: Option<(f64, f64)>) -> (String, egui::Color32) {
    let cfg = match settings.world_config(0) {
        Ok(cfg) => cfg,
        Err(e) => return (format!("мир не создастся: {e}"), DANGER),
    };
    let start = format!("на старте {} существ", spaced(cfg.creatures_at_start() as u64));
    let Some((tick_ms, scale)) = measured.filter(|(ms, _)| *ms > 0.0) else {
        return (format!("{start}; скорость оценим, когда мир пойдёт"), MUTED);
    };
    let ms = tick_ms / scale * settings.scale;
    let tps = 1000.0 / ms.max(1e-6);
    let (verdict, color) = if tps >= 60.0 {
        ("пойдёт плавно", GOOD)
    } else if tps >= 15.0 {
        ("медленнее обычного", ACCENT)
    } else {
        ("очень медленно", DANGER)
    };
    let speed = if tps >= 1000.0 { "больше 1000 т/с".into() } else { format!("до {tps:.0} т/с") };
    (format!("{start}; тик ~{ms:.2} мс, {speed} — {verdict}"), color)
}

impl LifeApp {
    pub fn menu_screen(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            self.view.show(ui, rect, &self.sim, false);
            ui.painter().rect_filled(rect, 0.0, VEIL);
        });
        let started = self.game.is_some();
        egui::Area::new("меню".into()).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style()).inner_margin(28.0).show(ui, |ui| {
                ui.set_width(300.0);
                ui.vertical_centered_justified(|ui| {
                    ui.label(RichText::new("lifegame").size(34.0).strong());
                    ui.colored_label(MUTED, "эволюция растений и существ");
                    ui.add_space(18.0);
                    let big = |t: &str| RichText::new(t).size(17.0);
                    if started && ui.add(theme::primary_rich(big("Продолжить"))).clicked() {
                        self.resume();
                    }
                    let new = if started {
                        egui::Button::new(big("Новый мир"))
                    } else {
                        theme::primary_rich(big("Новый мир"))
                    };
                    if ui.add(new).clicked() {
                        self.screen = Screen::Setup;
                    }
                    if ui.button(big("Настройки")).clicked() {
                        self.prefs_open = true;
                    }
                    if ui.button(big("Справка")).clicked() {
                        self.help_open = true;
                    }
                    if ui.button(big("Выход")).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && started && !self.prefs_open && !self.help_open {
            self.resume();
        }
    }

    pub fn setup_screen(&mut self, ui: &mut egui::Ui) {
        let measured = self.view.frame.as_ref().map(|f| (f.tick_ms, f.scale));
        // The buttons are at the bottom, outside the scroll: the tabs differ in height, and «Начать»
        // («Start») must always be visible.
        egui::Panel::bottom("кнопки нового мира").show(ui, |ui| {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(760.0);
                ui.horizontal(|ui| {
                    if ui.button("Назад").clicked() {
                        self.save_settings();
                        self.screen = Screen::Menu;
                    }
                    if ui.button("По умолчанию").on_hover_text("Вернуть значения этой вкладки").clicked()
                    {
                        self.settings.reset(self.setup_tab);
                    }
                    if ui.add(theme::primary("Начать")).clicked() {
                        self.start_game();
                    }
                });
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(760.0);
                    ui.label(RichText::new("Новый мир").size(26.0).strong());
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for (tab, name) in std::iter::once((Tab::World, "Мир")).chain(Tab::RULES) {
                            ui.selectable_value(&mut self.setup_tab, tab, RichText::new(name).size(16.0));
                        }
                    });
                    ui.separator();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        match self.setup_tab {
                            Tab::World => self.world_tab(ui, measured),
                            _ => {
                                ui.colored_label(
                                    MUTED,
                                    "Правила мира. Их можно менять и посреди партии — окном «Лаборатория». \
                                     Наведите на название, чтобы узнать, что это; справа — значение по умолчанию.",
                                );
                                ui.add_space(4.0);
                            }
                        }
                        if self.setup_tab == Tab::Body {
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| fields(ui, &mut self.settings, Tab::Body));
                                ui.add_space(12.0);
                                body_formula(ui, &self.settings.rules());
                            });
                        } else if self.setup_tab == Tab::Food {
                            let space = Space::new(self.settings.scale, self.settings.shape);
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| fields(ui, &mut self.settings, Tab::Food));
                                ui.add_space(12.0);
                                rules_preview(ui, &self.settings.rules(), space, self.settings.seed);
                            });
                        } else if self.setup_tab == Tab::Diets {
                            diet_table(ui, &mut self.settings);
                        } else {
                            fields(ui, &mut self.settings, self.setup_tab);
                        }
                    });
                });
            });
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.save_settings();
            self.screen = Screen::Menu;
        }
    }

    fn world_tab(&mut self, ui: &mut egui::Ui, measured: Option<(f64, f64)>) {
        let s = &mut self.settings;
        ui.label(RichText::new("Масштаб").strong());
        ui.horizontal_wrapped(|ui| {
            for (name, scale) in PRESETS {
                let label = format!("{name} ×{}", spaced(scale as u64));
                if ui.selectable_label(s.scale == scale, label).clicked() {
                    s.scale = scale;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Свой:");
            ui.add(
                egui::Slider::new(&mut s.scale, MIN_SCALE..=MAX_SCALE)
                    .logarithmic(true)
                    .custom_formatter(|v, _| format!("×{}", spaced(v.round() as u64)))
                    .custom_parser(|t| {
                        t.trim().trim_start_matches('×').replace(['\u{202F}', ' '], "").parse().ok()
                    }),
            )
            .on_hover_text("Во сколько раз мир больше базового 6000×4000 по площади.");
            s.scale = s.scale.round().clamp(MIN_SCALE, MAX_SCALE);
        });
        let (text, color) = estimate(s, measured);
        ui.colored_label(color, text);
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Форма").strong());
            for shape in Shape::ALL {
                let hint = match shape {
                    Shape::Strip => "Высота всегда 4000, большой мир растёт только вширь — длинной лентой.",
                    _ => "Большой мир растёт в обе стороны и держит пропорции.",
                };
                ui.selectable_value(&mut s.shape, shape, shape.label()).on_hover_text(hint);
            }
        });
        let space = Space::new(s.scale, s.shape);
        ui.colored_label(
            MUTED,
            format!(
                "мир {} × {}; глубина и еда по ней — в процентах, так что баланс от формы не зависит",
                spaced(space.width.round() as u64),
                spaced(space.height.round() as u64)
            ),
        );
        ui.add_space(8.0);

        ui.label(RichText::new("Сид").strong());
        ui.horizontal(|ui| {
            ui.add_enabled(!s.random_seed, egui::DragValue::new(&mut s.seed).range(1..=SEED_MAX));
            if ui.button("🎲").on_hover_text("Случайный сид").clicked() {
                s.seed = crate::app::random_seed();
                s.random_seed = false;
            }
            ui.checkbox(&mut s.random_seed, "новый каждый раз");
        });
        ui.colored_label(MUTED, "Один сид — один и тот же мир: партию можно повторить.");
        ui.add_space(8.0);
    }

    pub fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_open {
            return;
        }
        let mut open = true;
        let before = self.settings.clone();
        // what the simulation really runs on now (the last frame tells)
        let now = self.view.frame.as_ref().map(|f| (f.threads, f.fast_cores));
        egui::Window::new("Настройки").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let s = &mut self.settings;
            ui.label(RichText::new("Экран").strong());
            ui.checkbox(&mut s.fullscreen, "Во весь экран");
            ui.horizontal(|ui| {
                ui.label("Масштаб интерфейса");
                egui::ComboBox::from_id_salt("масштаб интерфейса")
                    .selected_text(ui_scale_label(s.ui_scale))
                    .show_ui(ui, |ui| {
                        for v in UI_SCALES {
                            ui.selectable_value(&mut s.ui_scale, v, ui_scale_label(v));
                        }
                    });
            });
            ui.checkbox(&mut s.show_fps, "Показывать кадры в секунду и цену тика");
            ui.add_space(6.0);
            ui.separator();
            computation(ui, s, now);
        });
        self.prefs_open = open;
        if self.settings != before {
            self.save_settings();
        }
    }

    pub fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Справка").open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                ui.label(RichText::new("Что происходит").strong());
                ui.label(
                    "Растения по умолчанию растут гуще у поверхности (вверху). Существа едят их, растут, \
                     делятся и мутируют; питание — ген: травоядные едят растения, всеядные — всё, \
                     мясоеды охотятся на тех, кто заметно мельче, падальщики едят гниль, которая \
                     опускается на дно. Умершие оставляют трупы. Отбор никто не задаёт: выживают те, \
                     чей геном окупается.",
                );
                ui.label(
                    "Светлое ядро существа — сколько у него энергии: у голодных оно маленькое. Глазок \
                     смотрит туда, куда существо идёт. Рамка на миникарте — то, что сейчас на экране.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Гены").strong());
                egui::Grid::new("гены").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for spec in creature::GENES.iter().filter(|s| crate::charts::shown(s)) {
                        ui.label(spec.label);
                        ui.label(spec.about);
                        ui.end_row();
                        // for a choice gene — what each variant means
                        for v in spec.variants().unwrap_or_default() {
                            ui.colored_label(MUTED, format!("  {}", v.label));
                            ui.label(v.about);
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Управление").strong());
                egui::Grid::new("клавиши").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (k, what) in [
                        ("Пробел", "пауза"),
                        ("→", "один тик на паузе"),
                        ("+ / −", "быстрее / медленнее"),
                        ("колесо", "приблизить к курсору"),
                        ("перетаскивание, WASD", "двигать камеру"),
                        ("Home", "весь мир"),
                        ("клик", "выбрать существо"),
                        ("F", "следить за выбранным"),
                        ("B", "поведение выбранного: блок-схема его программы"),
                        ("Tab", "боковая панель"),
                        ("L", "лаборатория: правила на ходу"),
                        ("I", "статистика: сытость, где живут, область"),
                        ("Esc", "меню"),
                    ] {
                        ui.label(RichText::new(k).strong());
                        ui.label(what);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Масштаб и форма").strong());
                ui.label(
                    "Масштаб — во сколько раз мир больше по площади; плотность жизни та же. Форма — его \
                     пропорции: квадрат, 3:2, 2:1 или полоса, которая растёт только вширь. Если тик не успевает за \
                     скоростью, вверху появляется «отстаёт»: мир идёт медленнее, но окно не тормозит.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Еда").strong());
                ui.label(
                    "Где растут растения, задаётся по глубине и по ширине отдельно: равномерно, линейно, \
                     экспонентой, логарифмом или волнами-полосами. Гены слоя под еду не подстраиваются — \
                     существа сами ищут, на какой глубине выгоднее. Профиль можно менять и посреди партии: \
                     выросшее остаётся, новое растёт по-новому.",
                );
            });
        });
        self.help_open = open;
    }
}

/// «Скорость расчёта»: how many threads compute the world and whether it keeps to the fast cores.
/// Nothing here can spoil a game: the world goes the same on any threads, only faster or slower.
fn computation(ui: &mut egui::Ui, s: &mut Settings, now: Option<(usize, bool)>) {
    let (cpu, auto) = (cpu_threads(), auto_threads());
    ui.label(RichText::new("Скорость расчёта").strong());
    ui.colored_label(
        MUTED,
        "Сколько потоков процессора считает мир. На сам мир это не влияет: партия идёт точно так же, \
         меняется только скорость.",
    );
    // 0 — auto, 1 — one thread, 2 — by hand
    let mut mode = s.threads.min(2);
    ui.radio_value(&mut mode, 0, format!("Авто: {auto} из {cpu} (рекомендуется)")).on_hover_text(
        "Все потоки процессора, кроме двух: одним рисуется окно, другим ведётся мир. \
         Самый быстрый выбор для игры.",
    );
    ui.radio_value(&mut mode, 1, "Один поток: медленнее, зато процессор свободен").on_hover_text(
        "Мир считается одним потоком, как раньше. Пригодится, если параллельно работает что-то тяжёлое \
         или ноутбук сильно греется.",
    );
    // with two threads or fewer there is nothing to choose by hand
    if cpu > 2 {
        ui.radio_value(&mut mode, 2, "Вручную").on_hover_text("Сколько потоков отдать миру — от 2 до всех.");
    }
    s.threads = match mode {
        0 => 0,
        1 => 1,
        // just switched to by hand: start from what auto gave, not from a sudden slowdown
        _ if s.threads < 2 => auto.clamp(2, cpu),
        _ => s.threads.min(cpu),
    };
    if mode == 2 {
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let word = threads_word(s.threads);
            ui.add(egui::Slider::new(&mut s.threads, 2..=cpu).text(word));
        });
        if s.threads + 1 >= cpu {
            ui.colored_label(DANGER, "Окну почти не остаётся ядер: картинка может подтормаживать.");
        }
    }
    ui.add_space(4.0);
    let hybrid = fast_cores_exist();
    ui.add_enabled(
        hybrid,
        egui::Checkbox::new(&mut s.fast_cores, "Держать расчёт на быстрых ядрах (рекомендуется)"),
    )
    .on_hover_text(
        "У процессора есть быстрые и экономичные ядра. Без этого Windows иногда уводит расчёт мира \
         на медленное ядро, и тик идёт до полутора раз дольше.",
    );
    if !hybrid {
        ui.colored_label(MUTED, "У этого процессора все ядра одинаковые: эта галочка ничего не меняет.");
    }
    ui.horizontal(|ui| {
        let default = Settings::default();
        let is_default = s.threads == default.threads && s.fast_cores == default.fast_cores;
        if ui.add_enabled(!is_default, egui::Button::new("Как по умолчанию")).clicked() {
            (s.threads, s.fast_cores) = (default.threads, default.fast_cores);
        }
        if let Some((threads, fast)) = now {
            let place = if fast {
                "на быстрых ядрах"
            } else {
                "ядра выбирает система"
            };
            ui.colored_label(MUTED, format!("Сейчас: {threads} {} · {place}", threads_word(threads)));
        }
    });
}

/// «поток», «потока», «потоков» by the number.
fn threads_word(n: usize) -> &'static str {
    match (n % 10, n % 100) {
        (1, h) if h != 11 => "поток",
        (2..=4, h) if !(12..=14).contains(&h) => "потока",
        _ => "потоков",
    }
}

/// The processor has fast and economical cores; asked of the system once.
fn fast_cores_exist() -> bool {
    static HYBRID: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HYBRID.get_or_init(life_sim::cores::has_fast_cores)
}

fn ui_scale_label(v: f64) -> String {
    if v == 0.0 { "как в системе".into() } else { format!("{:.0}%", v * 100.0) }
}

/// A field from `FIELDS`: a slider or, if the field has variants, a drop-down list. true — the
/// value has changed.
pub fn field_input(ui: &mut egui::Ui, f: &Field, value: &mut f64) -> bool {
    if f.choices.is_empty() {
        // typed or dragged, the value stays inside the field's hard limits
        let mut shown = *value * f.shown;
        let response = ui
            .add(
                egui::DragValue::new(&mut shown)
                    .range(f.lo * f.shown..=f.hi * f.shown)
                    .speed(f.step * f.shown)
                    .fixed_decimals(f.decimals)
                    .suffix(f.unit),
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
        "{}\n\nМожно от {} до {}.{why} Тяните мышью или щёлкните дважды и впишите число.",
        f.hint,
        (f.format)(f.lo),
        (f.format)(f.hi)
    )
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

/// The fields of one tab by the `FIELDS` table — those visible now.
fn fields(ui: &mut egui::Ui, s: &mut Settings, tab: Tab) {
    let default = Settings::default();
    egui::Grid::new(("поля", tab as u8)).num_columns(4).spacing([12.0, 8.0]).show(ui, |ui| {
        for f in FIELDS.iter().filter(|f| f.tab == tab) {
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
        ui.colored_label(
            MUTED,
            "Численность и плотность энергии — на участок 6000×4000: в большом мире всё в той же плотности.",
        );
    }
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
            let [r, g, b] = theme::DIET_COLORS[d];
            ui.colored_label(egui::Color32::from_rgb(r, g, b), theme::DIET_NAMES[d])
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

/// «база 2.5»: the default value, muted, beside the input.
pub fn base_value(ui: &mut egui::Ui, f: &Field, default: &Settings) {
    let base = if f.choices.is_empty() {
        (f.format)(default.get(f.key))
    } else {
        f.choices[(default.get(f.key) as usize).min(f.choices.len() - 1)].to_string()
    };
    ui.colored_label(MUTED, format!("база {base}")).on_hover_text("Значение по умолчанию");
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
            ui.label(RichText::new("Сколько стоит тело").strong());
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
                 каждое равно 1, и каждое слагаемое стоит свою цену. Первое число — общая цена жизни \
                 множителем (её 100 в настройках — ×2; цена скорости 100 — ×0,5), числа перед буквами — цены, степени — как быстро дорожает \
                 стат выше базового. \
                 Для сравнения: полный бак базового существа — 100.",
            );
            let twice = |power: f64| 2f64.powf(power);
            ui.colored_label(
                MUTED,
                format!(
                    "Базовое существо платит {:.3} в тик. Вдвое крупнее — тело дороже в {:.1} раза, \
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
    let colors = [
        egui::Color32::from_rgb(235, 170, 90),
        egui::Color32::from_rgb(110, 190, 235),
        egui::Color32::from_rgb(190, 140, 235),
    ];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn оценка_растёт_с_масштабом() {
        let small = Settings { scale: 1.0, ..Default::default() };
        let big = Settings { scale: 1000.0, ..Default::default() };
        let (a, ca) = estimate(&small, Some((0.5, 1.0)));
        let (b, cb) = estimate(&big, Some((0.5, 1.0)));
        assert!(a.contains("20 существ") && a.contains("плавно"), "{a}");
        assert!(b.contains("20\u{202F}000 существ") && b.contains("очень медленно"), "{b}");
        assert_ne!(ca, cb);
        let (c, _) = estimate(&small, None);
        assert!(c.contains("оценим"));
    }

    #[test]
    fn слово_поток_склоняется_по_числу() {
        let words: Vec<&str> = [1, 2, 4, 5, 11, 12, 18, 21, 22, 25].map(threads_word).to_vec();
        assert_eq!(
            words,
            [
                "поток",
                "потока",
                "потока",
                "потоков",
                "потоков",
                "потоков",
                "потоков",
                "поток",
                "потока",
                "потоков"
            ]
        );
    }
}
