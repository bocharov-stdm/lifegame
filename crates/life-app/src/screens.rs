//! The menu, «Новый мир», the screen's settings and the help.

use eframe::egui::{self, Align2, RichText, Vec2};

use life_core::genome::creature;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Shape, Space, Terrain};

use crate::app::{LifeApp, Screen};
use crate::rules_ui::{body_formula, diet_table, fields, rules_preview, tab_name, water_note};
use crate::settings::{PRESETS, SEED_MAX, Settings, Tab, UI_SCALES, auto_threads, cpu_threads};
use crate::theme::{self, ACCENT, DANGER, GOOD, MUTED, VEIL, spaced};

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
    let speed = if tps >= 1000.0 {
        "больше 1000 тиков/с".into()
    } else {
        format!("до {tps:.0} тиков/с")
    };
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
                    ui.label(theme::heading("lifegame", 36.0).color(theme::ACCENT));
                    ui.colored_label(MUTED, "эволюция растений и существ");
                    ui.add_space(theme::GAP_WIDE);
                    let big = |t: &str| RichText::new(t).size(17.0);
                    if started && theme::primary_sized(ui, "Продолжить", 17.0).clicked() {
                        self.resume();
                    }
                    let new = if started {
                        ui.button(big("Новый мир"))
                    } else {
                        theme::primary_sized(ui, "Новый мир", 17.0)
                    };
                    if new.clicked() {
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
        // Esc closes an open window first, then goes back to the game
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !self.was_editing {
            if self.help_open {
                self.help_open = false;
            } else if self.prefs_open {
                self.prefs_open = false;
            } else if started {
                self.resume();
            }
        }
    }

    pub fn setup_screen(&mut self, ui: &mut egui::Ui) {
        let measured = self.view.frame.as_ref().map(|f| (f.tick_ms, f.scale));
        // The buttons are at the bottom, outside the scroll: the tabs differ in height, and «Начать»
        // («Start») must always be visible.
        egui::Panel::bottom("кнопки нового мира").frame(theme::panel_frame(ui.style())).show(ui, |ui| {
            theme::backdrop(ui, Some(Align2::CENTER_TOP));
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(760.0);
                ui.horizontal(|ui| {
                    if ui.button("Назад").clicked() {
                        self.save_settings();
                        self.screen = Screen::Menu;
                    }
                    if theme::primary_button(ui, "Начать").clicked() {
                        if self.game.is_some() {
                            self.confirm = Some(crate::app::Confirm::NewGame);
                        } else {
                            self.start_game();
                        }
                    }
                    // apart from «Начать»: a slip of the hand must not reset a tab
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Сбросить вкладку")
                            .on_hover_text("Вернуть значения этой вкладки к значениям по умолчанию")
                            .clicked()
                        {
                            self.confirm = Some(crate::app::Confirm::ResetTab(self.setup_tab));
                        }
                    });
                });
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().frame(egui::Frame::central_panel(ui.style()).fill(theme::BG)).show(ui, |ui| {
            // the deep water behind the settings: lighter at the top
            theme::gradient(ui.painter(), ui.clip_rect(), theme::PANEL_TOP, theme::BG);
            // the title and the tabs stay put; only the tab's contents scroll
            ui.vertical_centered(|ui| {
                ui.set_max_width(760.0);
                ui.label(theme::heading("Новый мир", theme::TITLE));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (tab, name) in std::iter::once((Tab::World, "Мир")).chain(Tab::RULES) {
                        let name = tab_name(ui.ctx(), tab, name);
                        ui.selectable_value(&mut self.setup_tab, tab, RichText::new(name).size(16.0));
                    }
                });
                ui.separator();
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(760.0);
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
                                // the formula is a researcher's detail
                                if self.settings.details {
                                    ui.add_space(12.0);
                                    body_formula(ui, &self.settings.rules());
                                }
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
                            ui.add_space(theme::GAP_WIDE);
                            ui.label(theme::strong("Как меняется питание у потомков"));
                            fields(ui, &mut self.settings, Tab::Diets);
                        } else {
                            if self.setup_tab == Tab::Water {
                                ui.label(water_note(ui.ctx()));
                            }
                            for (i, (tab, heading)) in self.setup_tab.pages().iter().enumerate() {
                                if let Some(heading) = heading.filter(|_| i > 0) {
                                    ui.add_space(theme::GAP_WIDE);
                                    ui.label(theme::strong(heading));
                                }
                                fields(ui, &mut self.settings, *tab);
                            }
                        }
                    });
                });
            });
        });
        // Esc that ended typing into a field only ends the typing
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !self.was_editing {
            self.save_settings();
            self.screen = Screen::Menu;
        }
    }

    fn world_tab(&mut self, ui: &mut egui::Ui, measured: Option<(f64, f64)>) {
        let s = &mut self.settings;
        ui.label(crate::theme::strong("Масштаб"));
        ui.horizontal_wrapped(|ui| {
            for (name, scale) in PRESETS {
                let label = format!("{name} ×{}", spaced(scale as u64));
                let hint = if scale == Settings::default().scale {
                    "Размер мира по умолчанию"
                } else {
                    ""
                };
                let button = ui.selectable_label(s.scale == scale, label);
                let button = if hint.is_empty() { button } else { button.on_hover_text(hint) };
                if button.clicked() {
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
            ui.label(crate::theme::strong("Форма"));
            for shape in Shape::ALL {
                let hint = match shape {
                    Shape::Strip => "Высота всегда 4000, большой мир растёт только вширь — длинной лентой.",
                    _ => "Большой мир растёт в обе стороны и держит пропорции.",
                };
                ui.selectable_value(&mut s.shape, shape, shape.label()).on_hover_text(hint);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::theme::strong("Мир"));
            for terrain in Terrain::ALL {
                let hint = match terrain {
                    Terrain::Ocean => {
                        "Море сбоку: вверху тёплая поверхность, внизу холодная глубина. Трупы тонут на дно."
                    }
                    Terrain::Land => {
                        "Суша сверху: вверху тёплый край, внизу холодный, слой — климатический пояс. Трупы \
                         лежат, где упали."
                    }
                };
                let label = match terrain {
                    Terrain::Ocean => "Океан",
                    Terrain::Land => "Суша",
                };
                ui.selectable_value(&mut s.terrain, terrain, label).on_hover_text(hint);
            }
        });
        let space = Space::new(s.scale, s.shape);
        ui.colored_label(
            MUTED,
            s.terrain
                .say(&format!(
                    "мир {} × {}; глубина и еда по ней — в процентах, так что баланс от формы не зависит",
                    spaced(space.width.round() as u64),
                    spaced(space.height.round() as u64)
                ))
                .into_owned(),
        );
        ui.add_space(8.0);

        ui.label(crate::theme::strong("Сид"));
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
        let screen = ctx.content_rect();
        egui::Window::new("Настройки")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .show(ctx, |ui| {
                // at a big interface scale the window is taller than the screen: it scrolls
                egui::ScrollArea::vertical().max_height((screen.height() - 120.0).max(200.0)).show(
                    ui,
                    |ui| {
                        let s = &mut self.settings;
                        // two columns: the screen and help at the left, the computation at the right,
                        // so the window fits 960×600 without scrolling
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_max_width(280.0);
                                ui.label(crate::theme::strong("Экран"));
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
                                ui.add_space(theme::GAP);
                                ui.label(crate::theme::strong("Подсказки"));
                                ui.checkbox(&mut s.details, "Подробности для исследователя").on_hover_text(
                                    "Формула цены тела, «Внутри видов» в статистике, команда повтора партии без окна",
                                );
                                if s.intro_seen && ui.button("Подсказка новичка снова").clicked() {
                                    s.intro_seen = false;
                                }
                            });
                            ui.separator();
                            ui.vertical(|ui| {
                                ui.set_max_width(340.0);
                                computation(ui, s, now);
                            });
                        });
                    },
                );
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
        let screen = ctx.content_rect();
        egui::Window::new("Справка")
            .open(&mut open)
            .collapsible(false)
            .default_width((screen.width() - 40.0).clamp(300.0, 560.0))
            .pivot(Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height((screen.height() - 160.0).max(200.0)).show(ui, |ui| {
                ui.label(crate::theme::strong("Управление"));
                keys_grid(ui);
                ui.add_space(6.0);
                ui.label(crate::theme::strong("Что происходит"));
                let say = |text: &str| crate::words::say(ctx, text);
                ui.label(say(
                    "Растения по умолчанию растут гуще у поверхности (вверху). Существа едят их, растут, \
                     делятся и мутируют. Тело задают гены, поведение — две программы из блоков, детская \
                     и взрослая; мутируют и они. Питание — тоже ген: травоядные едят растения, \
                     всеядные — всё, мясоеды охотятся на тех, кто заметно мельче, падальщики едят гниль \
                     и кости на дне. Умершие оставляют трупы. Отбор никто не задаёт: выживают те, чьи \
                     тело и поведение окупаются.",
                ));
                ui.label(
                    "Светлое ядро существа — сколько у него энергии: у голодных оно маленькое. Глазок \
                     смотрит туда, куда существо идёт. Рамка на миникарте — то, что сейчас на экране.",
                );
                ui.add_space(6.0);
                ui.label(crate::theme::strong("Гены"));
                egui::Grid::new("гены").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for spec in creature::GENES.iter().filter(|s| crate::charts::shown(s)) {
                        ui.label(spec.label);
                        ui.label(say(spec.about));
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
                ui.label(crate::theme::strong("Масштаб и форма"));
                ui.label(
                    "Масштаб — во сколько раз мир больше по площади; плотность жизни та же. Форма — его \
                     пропорции: квадрат, 3:2, 2:1 или полоса, которая растёт только вширь. Если тик не успевает за \
                     скоростью, вверху появляется «отстаёт»: мир идёт медленнее, но окно не тормозит.",
                );
                ui.add_space(6.0);
                ui.label(crate::theme::strong("Еда"));
                ui.label(say(
                    "Где растут растения, задаётся по глубине и по ширине отдельно: равномерно, линейно, \
                     экспонентой, логарифмом или волнами-полосами. Слой глубины задают программы поведения, \
                     а не мир: выгодную глубину существа находят сами. Профиль можно менять и посреди партии: \
                     выросшее остаётся, новое растёт по-новому.",
                ));
            });
        });
        self.help_open = open;
    }
}

/// «Скорость расчёта»: how many threads compute the world and whether it keeps to the fast cores.
/// Nothing here can spoil a game: the world goes the same on any threads, only faster or slower.
fn computation(ui: &mut egui::Ui, s: &mut Settings, now: Option<(usize, bool)>) {
    let (cpu, auto) = (cpu_threads(), auto_threads());
    ui.label(crate::theme::strong("Скорость расчёта"));
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

/// The game's keys and mouse, for the help and the keys' card.
pub const CONTROLS: [(&str, &str); 14] = [
    ("Пробел", "пауза"),
    ("→", "один тик на паузе"),
    ("+ / −", "быстрее / медленнее"),
    ("колесо", "приблизить к курсору"),
    ("перетаскивание, WASD", "двигать камеру"),
    ("Home", "весь мир"),
    ("клик", "выбрать существо (промах выбор не снимает)"),
    ("F", "следить за выбранным"),
    ("B", "поведение выбранного: блок-схема его программы"),
    ("Tab", "боковая панель"),
    ("L", "лаборатория: правила на ходу"),
    ("I", "статистика: сытость, где живут, область"),
    ("Esc", "закрыть верхнее окно, снять выбор; потом меню"),
    ("F1", "эта подсказка"),
];

/// The keys as a two-column grid.
pub fn keys_grid(ui: &mut egui::Ui) {
    egui::Grid::new("клавиши").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
        for (k, what) in CONTROLS {
            ui.label(theme::strong(k));
            ui.label(what);
            ui.end_row();
        }
    });
}

/// 0 is the system's scale (a file saved with 1 reads as 0: `Settings::from_json`).
fn ui_scale_label(v: f64) -> String {
    if v == 0.0 { "как в системе".into() } else { format!("{:.0}%", v * 100.0) }
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
