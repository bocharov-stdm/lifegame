//! Меню, «Новый мир», настройки экрана и справка.

use eframe::egui::{self, Align2, RichText, Vec2};
use life_core::genome::{predator, vegetarian};
use life_core::space::{MAX_SCALE, MIN_SCALE};

use crate::app::{LifeApp, Screen};
use crate::settings::{FIELDS, PRESETS, SEED_MAX, Settings, Tab, UI_SCALES};
use crate::theme::{self, ACCENT, DANGER, GOOD, MUTED, VEIL, spaced};

/// Оценка большого мира: сколько существ на старте и как быстро пойдёт тик.
/// Цена тика берётся из замера мира, который идёт сейчас (фон меню или
/// партия), пересчитанного на площадь: тик растёт с числом существ, а оно —
/// с площадью. Это замер на этой машине, а не выдуманная формула.
pub fn estimate(settings: &Settings, measured: Option<(f64, f64)>) -> (String, egui::Color32) {
    let cfg = settings.world_config(0);
    let start = format!(
        "на старте {} травоядных и {} хищников",
        spaced(cfg.vegetarians_at_start() as u64),
        spaced(cfg.predators_at_start() as u64)
    );
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
                    ui.colored_label(MUTED, "эволюция растений, травоядных и хищников");
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
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(760.0);
                    ui.label(RichText::new("Новый мир").size(26.0).strong());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.setup_tab, Tab::World, RichText::new("Мир").size(16.0));
                        ui.selectable_value(
                            &mut self.setup_tab,
                            Tab::Lab,
                            RichText::new("Лаборатория").size(16.0),
                        );
                    });
                    ui.separator();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        if self.setup_tab == Tab::World {
                            self.world_tab(ui, measured);
                        } else {
                            ui.colored_label(
                                MUTED,
                                "Правила мира. Их можно менять и посреди партии — панелью «Лаборатория».",
                            );
                            ui.add_space(4.0);
                        }
                        fields(ui, &mut self.settings, self.setup_tab);
                    });
                    ui.add_space(12.0);
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
            .on_hover_text("Во сколько раз мир больше базового 6000×4000. Растёт вширь: глубина та же.");
            s.scale = s.scale.round().clamp(MIN_SCALE, MAX_SCALE);
        });
        let (text, color) = estimate(s, measured);
        ui.colored_label(color, text);
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
                    "Растения растут гуще у поверхности (вверху). Травоядные едят их, делятся и мутируют; хищники \
                     охотятся на травоядных и тоже мутируют. Отбор никто не задаёт: выживают те, чей геном \
                     окупается.",
                );
                ui.label(
                    "Светлое ядро травоядного — сколько у него энергии: у голодных оно маленькое. Глазок и нос \
                     хищника смотрят туда, куда существо идёт. Рамка на миникарте — то, что сейчас на экране.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Гены").strong());
                egui::Grid::new("гены").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (who, genes) in [("травоядные", &vegetarian::GENES[..]), ("хищники", &predator::GENES[..])] {
                        ui.colored_label(MUTED, who);
                        ui.label("");
                        ui.end_row();
                        for spec in genes.iter().filter(|s| crate::charts::shown(s)) {
                            ui.label(spec.label);
                            ui.label(spec.about);
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
                ui.label(RichText::new("Масштаб").strong());
                ui.label(
                    "Большой мир растёт вширь, глубина та же, а плотность жизни — прежняя. Если тик не успевает за \
                     скоростью, вверху появляется «отстаёт»: мир идёт медленнее, но окно не тормозит.",
                );
            });
        });
        self.help_open = open;
    }
}

fn ui_scale_label(v: f64) -> String {
    if v == 0.0 { "как в системе".into() } else { format!("{:.0}%", v * 100.0) }
}

/// Ползунки одной вкладки по таблице `FIELDS`.
fn fields(ui: &mut egui::Ui, s: &mut Settings, tab: Tab) {
    egui::Grid::new(("поля", tab == Tab::World)).num_columns(3).spacing([12.0, 10.0]).show(ui, |ui| {
        for f in FIELDS.iter().filter(|f| f.tab == tab) {
            ui.label(f.label).on_hover_text(f.hint);
            let mut v = s.get(f.key);
            let slider =
                egui::Slider::new(&mut v, f.lo..=f.hi).step_by(f.step).custom_formatter(|v, _| (f.format)(v));
            if ui.add(slider).on_hover_text(f.hint).changed() {
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
            "Численности и рост растений — на участок 6000×4000: в большом мире всё в той же плотности.",
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
