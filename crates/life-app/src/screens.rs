//! Меню, «Новый мир», настройки экрана и справка.

use eframe::egui::{self, Align2, RichText, Vec2};
use life_core::flora;
use life_core::genome::vegetarian;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, Shape, Space};

use crate::app::{LifeApp, Screen};
use crate::frame::PLANT_COLOR;
use crate::settings::{FIELDS, Field, PRESETS, SEED_MAX, Settings, Tab, UI_SCALES};
use crate::theme::{self, ACCENT, BG, DANGER, GOOD, MUTED, VEIL, spaced};

/// Оценка большого мира: сколько существ на старте и как быстро пойдёт тик.
/// Цена тика берётся из замера мира, который идёт сейчас (фон меню или
/// партия), пересчитанного на площадь: тик растёт с числом существ, а оно —
/// с площадью. Это замер на этой машине, а не выдуманная формула.
pub fn estimate(settings: &Settings, measured: Option<(f64, f64)>) -> (String, egui::Color32) {
    let cfg = settings.world_config(0);
    let start = format!("на старте {} травоядных", spaced(cfg.vegetarians_at_start() as u64));
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
                    ui.label(RichText::new("Tiny Life").size(34.0).strong());
                    ui.colored_label(MUTED, "эволюция растений и травоядных");
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
        // Кнопки — внизу, вне прокрутки: вкладки разной высоты, а «Начать»
        // должна быть видна всегда.
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
                    ui.horizontal(|ui| {
                        for (tab, name) in
                            [(Tab::World, "Мир"), (Tab::Food, "Еда"), (Tab::Lab, "Лаборатория")]
                        {
                            ui.selectable_value(&mut self.setup_tab, tab, RichText::new(name).size(16.0));
                        }
                    });
                    ui.separator();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        match self.setup_tab {
                            Tab::World => self.world_tab(ui, measured),
                            Tab::Food => {
                                ui.colored_label(
                                    MUTED,
                                    "Где растут растения. Это правила мира: их можно менять и посреди \
                                     партии — панелью «Лаборатория».",
                                );
                                ui.add_space(4.0);
                            }
                            Tab::Lab => {
                                ui.colored_label(
                                    MUTED,
                                    "Правила мира. Их можно менять и посреди партии — панелью «Лаборатория».",
                                );
                                ui.add_space(4.0);
                            }
                        }
                        if self.setup_tab == Tab::Food {
                            let space = Space::new(self.settings.scale, self.settings.shape);
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| fields(ui, &mut self.settings, Tab::Food));
                                ui.add_space(12.0);
                                food_preview(ui, &self.settings.rules(), space);
                            });
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
        egui::Window::new("Настройки экрана").open(&mut open).collapsible(false).resizable(false).show(
            ctx,
            |ui| {
                let s = &mut self.settings;
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
            },
        );
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
                    "Растения по умолчанию растут гуще у поверхности (вверху). Травоядные едят их, делятся и \
                     мутируют; крупные при каннибализме едят мелких сородичей. Отбор никто не задаёт: \
                     выживают те, чей геном окупается.",
                );
                ui.label(
                    "Светлое ядро травоядного — сколько у него энергии: у голодных оно маленькое. Глазок \
                     смотрит туда, куда существо идёт. Рамка на миникарте — то, что сейчас на экране.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Гены").strong());
                egui::Grid::new("гены").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for spec in vegetarian::GENES.iter().filter(|s| crate::charts::shown(s)) {
                        ui.label(spec.label);
                        ui.label(spec.about);
                        ui.end_row();
                        // у гена-выбора — что значит каждый вариант
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
                        ("Tab", "боковая панель"),
                        ("L", "лаборатория: правила на ходу"),
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
                     травоядные сами ищут, на какой глубине выгоднее. Профиль можно менять и посреди партии: \
                     выросшее остаётся, новое растёт по-новому.",
                );
            });
        });
        self.help_open = open;
    }
}

fn ui_scale_label(v: f64) -> String {
    if v == 0.0 { "как в системе".into() } else { format!("{:.0}%", v * 100.0) }
}

/// Поле из `FIELDS`: ползунок или, если у поля есть варианты, выпадающий
/// список. true — значение изменилось.
pub fn field_input(ui: &mut egui::Ui, f: &Field, value: &mut f64) -> bool {
    if f.toggle {
        let mut on = *value != 0.0;
        let changed = ui.checkbox(&mut on, "").on_hover_text(f.hint).changed();
        *value = on as u8 as f64;
        return changed;
    }
    if f.choices.is_empty() {
        let slider =
            egui::Slider::new(value, f.lo..=f.hi).step_by(f.step).custom_formatter(|v, _| (f.format)(v));
        return ui.add(slider).on_hover_text(f.hint).changed();
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

/// Предпросмотр еды: мир в своих пропорциях, закрашенный плотностью растений
/// по правилам — ярче там, где гуще. Верх — поверхность.
pub fn food_preview(ui: &mut egui::Ui, rules: &Rules, space: Space) {
    let aspect = (space.width / space.height) as f32;
    let (max_w, max_h) = (ui.available_width().clamp(120.0, 280.0), 170.0);
    // очень длинная полоса всё равно видна хотя бы полоской в 14 точек
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
        for j in 0..ny {
            for i in 0..nx {
                let d = flora::density(rules, (i as f64 + 0.5) / nx as f64, (j as f64 + 0.5) / ny as f64);
                let min = rect.min + Vec2::new(i as f32 * cell.x, j as f32 * cell.y);
                // клетки с запасом в полточки: иначе между ними видны швы
                let r = egui::Rect::from_min_size(min, cell + Vec2::splat(0.5)).intersect(rect);
                painter.rect_filled(r, 0.0, BG.lerp_to_gamma(plant, d.sqrt() as f32));
            }
        }
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, MUTED), egui::StrokeKind::Outside);
        ui.colored_label(MUTED, "вверху поверхность; ярче — гуще");
    });
}

/// Поля одной вкладки по таблице `FIELDS` — те, что сейчас видны.
fn fields(ui: &mut egui::Ui, s: &mut Settings, tab: Tab) {
    egui::Grid::new(("поля", tab as u8)).num_columns(3).spacing([12.0, 10.0]).show(ui, |ui| {
        for f in FIELDS.iter().filter(|f| f.tab == tab) {
            if !(f.shown)(s) {
                continue;
            }
            ui.label(f.label).on_hover_text(f.hint);
            let mut v = s.get(f.key);
            if field_input(ui, f, &mut v) {
                s.set(f.key, v);
            }
            if s.is_default(f.key) {
                ui.label("");
            } else if ui.small_button("↺").on_hover_text("По умолчанию").clicked() {
                s.set(f.key, Settings::default().get(f.key));
            }
            ui.end_row();
        }
    });
    if tab == Tab::World {
        ui.colored_label(
            MUTED,
            "Численность и рост растений — на участок 6000×4000: в большом мире всё в той же плотности.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn оценка_растёт_с_масштабом() {
        let small = Settings::default();
        let big = Settings { scale: 1000.0, ..Default::default() };
        let (a, ca) = estimate(&small, Some((0.5, 1.0)));
        let (b, cb) = estimate(&big, Some((0.5, 1.0)));
        assert!(a.contains("20 травоядных") && a.contains("плавно"), "{a}");
        assert!(b.contains("20\u{202F}000 травоядных") && b.contains("очень медленно"), "{b}");
        assert_ne!(ca, cb);
        let (c, _) = estimate(&small, None);
        assert!(c.contains("оценим"));
    }
}
