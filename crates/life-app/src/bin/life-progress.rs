//! A small always-on-top window with the progress of a `life-sweep` run: the bar, the time left,
//! what is being simulated right now (each variant with its description from the plan), the last
//! finished runs, and buttons to pause, resume or stop the sweep. The same progress shows on its
//! taskbar button (Windows). `life-sweep` opens it by itself; by hand:
//!
//!     target/release/life-progress target/sweeps/hunt/progress.json
//!
//! It reads `progress.json` twice a second and answers with `control.txt` next to it (`run`, `pause`,
//! `stop`); the sweep prints and logs every press, so whoever watches its output learns of it.
//! Closing the window does not stop the sweep.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, vec2};
use serde_json::Value;

// The window's own dark palette, whatever the system theme: text must read on its background.
const BG: Color32 = Color32::from_rgb(0x15, 0x17, 0x1c);
const CARD: Color32 = Color32::from_rgb(0x20, 0x24, 0x2c);
const EDGE: Color32 = Color32::from_rgb(0x2e, 0x34, 0x3e);
const TRACK: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x38);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xed);
const WEAK: Color32 = Color32::from_rgb(0xa4, 0xac, 0xb6);
const FAINT: Color32 = Color32::from_rgb(0x74, 0x7c, 0x88);
const GREEN: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
const BLUE: Color32 = Color32::from_rgb(0x5a, 0xa9, 0xff);
const AMBER: Color32 = Color32::from_rgb(0xf0, 0xb4, 0x4c);
const RED: Color32 = Color32::from_rgb(0xef, 0x6b, 0x5f);

/// Seconds as the window says them: «40 с», «12 мин», «1 ч 5 мин».
fn span(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    if s < 60 {
        format!("{s} с")
    } else if s < 3600 {
        format!("{} мин", (s + 30) / 60)
    } else {
        format!("{} ч {} мин", s / 3600, (s % 3600 + 30) / 60)
    }
}

/// The sweep's state as the window shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Running,
    Paused,
    Stopped,
    Finished,
}

impl State {
    fn of(key: &str, finished: bool) -> Self {
        match key {
            "paused" => State::Paused,
            "stopped" => State::Stopped,
            "finished" => State::Finished,
            _ if finished => State::Finished,
            _ => State::Running,
        }
    }

    fn label(self) -> &'static str {
        match self {
            State::Running => "идёт",
            State::Paused => "на паузе",
            State::Stopped => "остановлена",
            State::Finished => "готова",
        }
    }

    fn color(self) -> Color32 {
        match self {
            State::Running => BLUE,
            State::Paused => AMBER,
            State::Stopped => RED,
            State::Finished => GREEN,
        }
    }

    fn over(self) -> bool {
        matches!(self, State::Stopped | State::Finished)
    }
}

/// One running variant: its name, seeds and how long its oldest run goes, and its description.
#[derive(Debug, PartialEq)]
struct Running {
    variant: String,
    /// «сиды 1–8 · тик 12 400 из 20 000 · 1 мин из 5 мин»: where its runs are and their time limit.
    seeds: String,
    about: String,
    /// Share of their ticks its runs have done, once they said.
    share: Option<f32>,
    /// Each run apart, shown when the card is unfolded: «сид 3 · тик 11 193 из 20 000 · 26 с» and
    /// its share.
    runs: Vec<(String, Option<f32>)>,
}

/// «тик 12 400 из 20 000» and the share done, from a run's (or a variant's mean) tick fields.
fn tick_of(r: &Value) -> Option<(String, f32)> {
    let (t, of) = r["tick"].as_u64().zip(r["ticks"].as_u64()).filter(|t| t.1 > 0)?;
    Some((format!("тик {} из {}", thousands(t), thousands(of)), (t as f32 / of as f32).min(1.0)))
}

/// Seeds as short ranges: «1–8», «1, 3, 5–7».
fn ranges(seeds: &[u64]) -> String {
    let mut sorted = seeds.to_vec();
    sorted.sort_unstable();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && sorted[j + 1] == sorted[j] + 1 {
            j += 1;
        }
        parts.push(match j - i {
            0 => sorted[i].to_string(),
            1 => format!("{}, {}", sorted[i], sorted[j]),
            _ => format!("{}–{}", sorted[i], sorted[j]),
        });
        i = j + 1;
    }
    parts.join(", ")
}

/// 12400 → «12 400».
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// A recently finished run in the «Последние» list.
#[derive(Debug, PartialEq)]
struct Finished {
    line: String,
    cut: bool,
    /// Its tick rate fell (the sweep's `SLOWDOWN_WARN`).
    slow: bool,
}

/// What the window shows, read from one `progress.json`.
#[derive(Debug, Default, PartialEq)]
struct View {
    title: String,
    about: String,
    done: u64,
    total: u64,
    cut: u64,
    /// Share of the whole sweep done, the running runs' ticks included.
    progress: f32,
    state: State,
    /// «осталось ≈ …», «готово за …» and the like.
    status: String,
    /// Silent for this many seconds while not over: the sweep may have been killed.
    silent: Option<u64>,
    running: Vec<Running>,
    /// A recently finished run's line, newest first, and whether it was cut or its tick rate fell.
    last: Vec<Finished>,
    summary: Option<String>,
    out: Option<String>,
}

impl View {
    fn of(p: &Value, now_unix: u64) -> Self {
        let n = |k: &str| p[k].as_u64().unwrap_or(0);
        let s = |v: &Value| v.as_str().unwrap_or("").to_string();
        let state = State::of(p["state"].as_str().unwrap_or(""), p["finished"].as_bool().unwrap_or(false));
        let elapsed = span(p["elapsed_s"].as_f64().unwrap_or(0.0));
        let status = match state {
            State::Finished => format!("готово за {elapsed}"),
            State::Stopped => format!("остановлена через {elapsed}"),
            State::Paused => format!("на паузе · идёт {elapsed}"),
            State::Running => {
                let left = p["left_s"].as_f64().map_or_else(
                    || "оценка после первого прогона".to_string(),
                    |l| format!("осталось ≈ {}", span(l)),
                );
                format!("{left} · идёт {elapsed} · по {} сразу", n("jobs"))
            }
        };
        let silent = now_unix.saturating_sub(n("updated_unix"));
        let running = p["running"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| {
                        let seeds: Vec<u64> = r["seeds"]
                            .as_array()
                            .map(|s| s.iter().filter_map(Value::as_u64).collect())
                            .unwrap_or_default();
                        let word = if seeds.len() == 1 { "сид" } else { "сиды" };
                        let longest = span(r["longest_s"].as_f64().unwrap_or(0.0));
                        let limit = n("limit_s");
                        let time = if limit > 0 {
                            format!("{longest} из {}", span(limit as f64))
                        } else {
                            longest
                        };
                        let ticks = tick_of(r);
                        let at = ticks.as_ref().map_or(String::new(), |(t, _)| format!(" · {t}"));
                        let runs = r["runs"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .map(|run| {
                                        let told = tick_of(run);
                                        let seed = run["seed"].as_u64().unwrap_or(0);
                                        let time = span(run["s"].as_f64().unwrap_or(0.0));
                                        // the tick rate of its latest 500 ticks, as the report measured it
                                        let pace = run["ms"]
                                            .as_f64()
                                            .map_or(String::new(), |ms| format!(" · {ms:.1} мс/тик"));
                                        let line = match &told {
                                            Some((t, _)) => format!("сид {seed} · {t} · {time}{pace}"),
                                            None => format!("сид {seed} · начинается · {time}"),
                                        };
                                        (line, told.map(|t| t.1))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Running {
                            variant: s(&r["variant"]),
                            seeds: format!("{word} {}{at} · {time}", ranges(&seeds)),
                            about: s(&r["about"]),
                            share: ticks.map(|t| t.1),
                            runs,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let last = p["last"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| {
                        let seed = r["seed"].as_u64().unwrap_or(0);
                        Finished {
                            line: format!("{} · сид {seed} · {}", s(&r["variant"]), s(&r["what"])),
                            cut: r["cut"].as_bool() == Some(true),
                            slow: r["slow"].as_bool() == Some(true),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        View {
            title: s(&p["title"]),
            about: s(&p["about"]),
            done: n("done"),
            total: n("total"),
            cut: n("cut"),
            progress: p["progress"].as_f64().map_or_else(
                || if n("total") == 0 { 0.0 } else { n("done") as f32 / n("total") as f32 },
                |x| x as f32,
            ),
            state,
            status,
            silent: (!state.over() && silent > 15).then_some(silent),
            running,
            last,
            summary: p["summary"].as_str().map(str::to_string),
            out: p["out"].as_str().map(str::to_string),
        }
    }

    fn share(&self) -> f32 {
        if self.state == State::Finished { 1.0 } else { self.progress.clamp(0.0, 1.0) }
    }

    /// The window's title: the share, the state and the plan, so the taskbar tells it too.
    fn window_title(&self) -> String {
        let pct = format!("{:.0}%", 100.0 * self.share());
        match self.state {
            State::Running => format!("{pct} · {} — серия", self.title),
            State::Paused => format!("пауза {pct} · {} — серия", self.title),
            State::Stopped => format!("остановлена {pct} · {} — серия", self.title),
            State::Finished => format!("готово · {} — серия", self.title),
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What a press of the window asks the sweep for, written to `control.txt` next to `progress.json`.
fn send(progress: &Path, what: &str) {
    if let Some(dir) = progress.parent() {
        let _ = std::fs::write(dir.join("control.txt"), what);
    }
}

/// The window's palette and sizes, set once.
fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = TRACK;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2c, 0x32, 0x3c);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, EDGE);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x38, 0x40, 0x4c);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x44, 0x4d, 0x5b);
    // «≈», «⏸», «▶», «⏹» are missing from egui's main font; the built-in Hack falls back for them
    let mut fonts = egui::FontDefinitions::default();
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.push("Hack".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(12.0, 5.0);
        for (text, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 12.0),
            (egui::TextStyle::Heading, 19.0),
        ] {
            if let Some(f) = s.text_styles.get_mut(&text) {
                f.size = size;
            }
        }
    });
}

/// The bar: a painted track with the done share in the state's colour and the count on it.
fn bar(ui: &mut egui::Ui, view: &View) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
    let p = ui.painter();
    let r = CornerRadius::same(7);
    p.rect_filled(rect, r, TRACK);
    let mut fill = rect;
    fill.set_width((rect.width() * view.share()).max(if view.done > 0 { 14.0 } else { 0.0 }));
    if view.done > 0 {
        p.rect_filled(fill, r, view.state.color().gamma_multiply(0.85));
    }
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} из {} прогонов", view.done, view.total),
        egui::FontId::proportional(13.5),
        TEXT,
    );
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, EDGE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// What a press asked for: shown until the sweep's state answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Pause,
    Resume,
    Stop,
}

/// A thin line of how far a run or a variant is.
fn thin_bar(ui: &mut egui::Ui, share: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(1), TRACK);
    let mut done = rect;
    done.set_width(rect.width() * share);
    ui.painter().rect_filled(done, CornerRadius::same(1), color);
}

/// What the window keeps between frames: the stop confirmation and which variants' cards are
/// unfolded to their seeds.
#[derive(Default)]
struct Local {
    confirm_stop: bool,
    unfolded: BTreeSet<String>,
}

/// The window's content; returns what a button asked for. A separate function, so a test can
/// render it without a native window.
fn draw(ui: &mut egui::Ui, view: &View, local: &mut Local, asked: Option<Asked>) -> Option<Asked> {
    let confirm_stop = &mut local.confirm_stop;
    let mut pressed = None;
    ui.horizontal(|ui| {
        ui.heading(RichText::new(format!("Серия «{}»", view.title)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{:.0}%", 100.0 * view.share()))
                    .size(22.0)
                    .strong()
                    .color(view.state.color()),
            );
            ui.label(RichText::new(view.state.label()).color(view.state.color()));
        });
    });
    if !view.about.is_empty() {
        ui.label(RichText::new(&view.about).color(WEAK));
    }
    ui.add_space(2.0);
    bar(ui, view);
    ui.label(&view.status);
    if view.cut > 0 {
        ui.label(RichText::new(format!("оборвано или с ошибкой: {}", view.cut)).color(RED));
    }
    if let Some(s) = view.silent {
        ui.label(
            RichText::new(format!("серия молчит {} — возможно, её закрыли", span(s as f64))).color(AMBER),
        );
    }
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        if view.state.over() {
            if let Some(summary) = &view.summary
                && ui.button("Открыть сводку").clicked()
            {
                open(summary);
            }
            if let Some(out) = &view.out
                && ui.button("Папка с результатами").clicked()
            {
                open(out);
            }
        } else if *confirm_stop {
            ui.label(RichText::new("Остановить? Готовое сохранится, идущее пропадёт.").color(AMBER));
            if ui.button(RichText::new("Да, остановить").color(RED)).clicked() {
                pressed = Some(Asked::Stop);
                *confirm_stop = false;
            }
            if ui.button("Нет").clicked() {
                *confirm_stop = false;
            }
        } else {
            let (label, ask) = if view.state == State::Paused {
                ("▶  Продолжить", Asked::Resume)
            } else {
                ("⏸  Пауза", Asked::Pause)
            };
            if ui
                .button(label)
                .on_hover_text(
                    "На паузе идущие прогоны снимаются и потом считаются заново: мир зависит только от сида.",
                )
                .clicked()
            {
                pressed = Some(ask);
            }
            if ui.button(RichText::new("⏹  Остановить").color(RED)).clicked() {
                *confirm_stop = true;
            }
            if let Some(a) = asked {
                let text = match a {
                    Asked::Pause => "ставлю на паузу…",
                    Asked::Resume => "продолжаю…",
                    Asked::Stop => "останавливаю…",
                };
                ui.label(RichText::new(text).color(FAINT));
            }
        }
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if !view.running.is_empty() {
            ui.label(RichText::new("Сейчас считается").strong().color(WEAK));
            for r in &view.running {
                let unfolded = local.unfolded.contains(&r.variant);
                card(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        // the corner toggle: every seed apart, when there are several; first, so the
                        // line of ticks never runs under it
                        if r.runs.len() > 1 {
                            let (sign, hint) = if unfolded {
                                ("−", "Свернуть сиды")
                            } else {
                                ("+", "Показать каждый сид")
                            };
                            if ui.small_button(sign).on_hover_text(hint).clicked() {
                                if unfolded {
                                    local.unfolded.remove(&r.variant);
                                } else {
                                    local.unfolded.insert(r.variant.clone());
                                }
                            }
                        }
                        ui.label(RichText::new(&r.variant).strong().color(BLUE));
                        ui.label(RichText::new(&r.seeds).color(FAINT));
                    });
                    if let Some(share) = r.share {
                        thin_bar(ui, share, BLUE.gamma_multiply(0.8));
                    }
                    if unfolded {
                        for (line, share) in &r.runs {
                            ui.label(RichText::new(line).small().color(WEAK));
                            thin_bar(ui, share.unwrap_or(0.0), BLUE.gamma_multiply(0.5));
                        }
                    }
                    if !r.about.is_empty() {
                        ui.label(&r.about);
                    }
                });
            }
            ui.add_space(4.0);
        }
        if !view.last.is_empty() {
            ui.label(RichText::new("Последние").strong().color(WEAK));
            for f in &view.last {
                // red: cut; amber: its tick rate fell; green: fine
                let (dot, text) = if f.cut {
                    (RED, RED)
                } else if f.slow {
                    (AMBER, AMBER)
                } else {
                    (GREEN, WEAK)
                };
                ui.horizontal(|ui| {
                    let (at, _) = ui.allocate_exact_size(vec2(8.0, 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(at.center(), 3.5, dot);
                    ui.label(RichText::new(&f.line).small().color(text));
                });
            }
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("Закрытие окна не останавливает серию. Пауза и остановка видны Claude.")
                .small()
                .color(FAINT),
        );
    });
    pressed
}

/// Opens a file or folder the way Explorer would.
fn open(path: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path.replace('/', "\\")).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

struct Viewer {
    path: PathBuf,
    view: Option<View>,
    read_at: Option<Instant>,
    title: String,
    placed: bool,
    local: Local,
    asked: Option<Asked>,
    /// The taskbar button's bar: None until tried, then kept, a failure too (no retry every frame).
    #[cfg(windows)]
    taskbar: Option<Option<taskbar::Taskbar>>,
}

impl Viewer {
    fn poll(&mut self) {
        if self.read_at.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            return;
        }
        self.read_at = Some(Instant::now());
        if let Some(p) = std::fs::read_to_string(&self.path).ok().and_then(|t| serde_json::from_str(&t).ok())
        {
            let view = View::of(&p, unix_now());
            // a press is shown until the sweep's state answers it
            let answered = match self.asked {
                Some(Asked::Pause) => view.state != State::Running,
                Some(Asked::Resume) => view.state != State::Paused,
                Some(Asked::Stop) => view.state.over(),
                None => true,
            };
            if answered {
                self.asked = None;
            }
            self.view = Some(view);
        }
    }
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(500));
        if !self.placed
            && let Some(monitor) = ctx.input(|i| i.viewport().monitor_size)
        {
            // bottom right, above the taskbar
            let size = ctx.input(|i| i.viewport().outer_rect.map_or(vec2(500.0, 380.0), |r| r.size()));
            let at = monitor - size - vec2(16.0, 64.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                at.x.max(0.0),
                at.y.max(0.0),
            )));
            self.placed = true;
        }
        #[cfg(not(windows))]
        let _ = frame;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| {
                let Some(view) = &self.view else {
                    ui.label(format!("Жду файл прогресса:\n{}", self.path.display()));
                    return;
                };
                let title = view.window_title();
                if title != self.title {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
                    self.title = title;
                }
                #[cfg(windows)]
                {
                    let bar = self.taskbar.get_or_insert_with(|| taskbar::Taskbar::of(frame));
                    if let Some(t) = bar {
                        t.show(view.share(), view.state, view.cut > 0 || view.silent.is_some());
                    }
                }
                if let Some(ask) = draw(ui, view, &mut self.local, self.asked) {
                    send(
                        &self.path,
                        match ask {
                            Asked::Pause => "pause",
                            Asked::Resume => "run",
                            Asked::Stop => "stop",
                        },
                    );
                    self.asked = Some(ask);
                }
            });
    }
}

#[cfg(windows)]
mod taskbar {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::{
        ITaskbarList3, TBPF_ERROR, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED, TaskbarList,
    };

    use super::State;

    /// The window's taskbar button as a progress bar: green while running, yellow on pause, red
    /// when stopped, a run was cut or the sweep went silent; cleared when it is done.
    pub struct Taskbar {
        list: ITaskbarList3,
        hwnd: HWND,
    }

    impl Taskbar {
        pub fn of(frame: &eframe::Frame) -> Option<Self> {
            let RawWindowHandle::Win32(h) = frame.window_handle().ok()?.as_raw() else { return None };
            let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
            // SAFETY: plain COM calls on the UI thread; a failure only means no bar on the button
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
                list.HrInit().ok()?;
                Some(Taskbar { list, hwnd })
            }
        }

        /// `share` is the window's own bar: finished runs plus the running ones' ticks.
        pub fn show(&self, share: f32, state: State, trouble: bool) {
            let flag = match state {
                State::Finished => TBPF_NOPROGRESS,
                State::Stopped => TBPF_ERROR,
                State::Paused => TBPF_PAUSED,
                State::Running if trouble => TBPF_ERROR,
                State::Running => TBPF_NORMAL,
            };
            // SAFETY: the window handle is alive while the app runs
            unsafe {
                let _ = self.list.SetProgressState(self.hwnd, flag);
                if state != State::Finished {
                    let _ = self.list.SetProgressValue(self.hwnd, (share * 1000.0).round() as u64, 1000);
                }
            }
        }
    }
}

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map_or_else(|| PathBuf::from("progress.json"), PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Серия прогонов")
            .with_inner_size([500.0, 380.0])
            .with_min_inner_size([400.0, 260.0])
            .with_always_on_top(),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "life-progress",
        options,
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            Ok(Box::new(Viewer {
                path,
                view: None,
                read_at: None,
                title: String::new(),
                placed: false,
                local: Local::default(),
                asked: None,
                #[cfg(windows)]
                taskbar: None,
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_read_like_speech() {
        assert_eq!(span(40.0), "40 с");
        assert_eq!(span(719.0), "12 мин");
        assert_eq!(span(3900.0), "1 ч 5 мин");
    }

    fn running_sweep() -> Value {
        serde_json::json!({
            "title": "hunt1", "about": "Тела мясоедов на мире игрока: x20, 2:1, цены x3, без стай.",
            "total": 144, "done": 88, "cut": 1, "jobs": 16, "elapsed_s": 912.0, "left_s": 620.0,
            "progress": 0.63, "limit_s": 300,
            "updated_unix": 1000, "state": "running", "finished": false,
            "running": [
                { "variant": "young07_smell15", "about": "детство на траве и нюх в 1,5 раза острее", "seeds": [3, 4, 5], "longest_s": 95.0, "tick": 12400, "ticks": 20000,
                  "runs": [
                    { "seed": 3, "s": 95.0, "tick": 14000, "ticks": 20000, "ms": 4.25 },
                    { "seed": 4, "s": 90.0, "tick": 10800, "ticks": 20000 },
                    { "seed": 5, "s": 2.0 },
                  ] },
                { "variant": "smell3_paid", "about": "платный нюх: содержание как у зрения на его радиусе", "seeds": [8], "longest_s": 12.0 },
            ],
            "last": [
                { "variant": "smell3_paid", "seed": 7, "what": "165 s, done, tick rate 3.6x slower by tick 15000", "cut": false, "slow": true },
                { "variant": "tank15", "seed": 1, "what": "KILLED after 360 s", "cut": true },
            ],
        })
    }

    #[test]
    fn the_view_tells_what_runs_and_how_long_is_left() {
        let p = running_sweep();
        let v = View::of(&p, 1005);
        assert_eq!(v.state, State::Running);
        assert_eq!(v.window_title(), "63% · hunt1 — серия");
        assert_eq!(v.status, "осталось ≈ 10 мин · идёт 15 мин · по 16 сразу");
        assert_eq!(v.running[0].variant, "young07_smell15");
        assert_eq!(v.running[0].seeds, "сиды 3–5 · тик 12 400 из 20 000 · 2 мин из 5 мин");
        assert_eq!(
            v.running[0].runs[0],
            ("сид 3 · тик 14 000 из 20 000 · 2 мин · 4.2 мс/тик".to_string(), Some(0.7))
        );
        assert_eq!(v.running[0].runs[2], ("сид 5 · начинается · 2 с".to_string(), None));
        assert!(v.running[1].runs.is_empty(), "an old progress file without runs still reads");
        assert_eq!(v.running[0].share, Some(0.62));
        assert_eq!((v.running[1].seeds.as_str(), v.running[1].share), ("сид 8 · 12 с из 5 мин", None));
        assert_eq!(
            (ranges(&[8, 1, 2, 3, 5, 6]), ranges(&[4])),
            ("1–3, 5, 6, 8".to_string(), "4".to_string())
        );
        let killed =
            Finished { line: "tank15 · сид 1 · KILLED after 360 s".to_string(), cut: true, slow: false };
        assert_eq!(v.last[1], killed);
        assert!(v.last[0].slow && !v.last[0].cut, "a run whose tick rate fell is marked, not cut");
        assert_eq!(v.silent, None);
        assert_eq!(View::of(&p, 1100).silent, Some(100), "a sweep silent for long is flagged");
        let mut paused = p.clone();
        paused["state"] = "paused".into();
        let v = View::of(&paused, 1005);
        assert_eq!((v.state, v.status.as_str()), (State::Paused, "на паузе · идёт 15 мин"));
        let done = serde_json::json!({ "title": "hunt1", "total": 2, "done": 2, "state": "finished", "finished": true,
            "elapsed_s": 1500.0, "updated_unix": 0, "summary": "out/summary.md" });
        let v = View::of(&done, 99_999);
        assert_eq!(v.status, "готово за 25 мин");
        assert_eq!(v.silent, None, "a finished sweep is not silent");
        assert_eq!(v.window_title(), "готово · hunt1 — серия");
    }

    /// A press writes `control.txt` next to `progress.json`, which is what `life-sweep` reads.
    #[test]
    fn a_press_reaches_the_sweep() {
        let dir = std::env::temp_dir().join(format!("life-progress-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for what in ["pause", "run", "stop"] {
            send(&dir.join("progress.json"), what);
            assert_eq!(std::fs::read_to_string(dir.join("control.txt")).unwrap(), what);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The window at its default size, running, paused, asking to stop and finished, each with its
    /// own buttons and no others; a press on «Пауза» asks for a pause, and the «+» in a card's corner
    /// unfolds its seeds. With LIFEGAME_SHOTS=dir also as PNGs.
    #[test]
    fn the_window_draws_a_sweep() {
        let mut paused = running_sweep();
        paused["state"] = "paused".into();
        let finished = serde_json::json!({
            "title": "hunt1", "total": 144, "done": 144, "state": "finished", "finished": true, "elapsed_s": 1500.0,
            "updated_unix": 0, "summary": "sweep1/summary.md", "out": "sweep1",
            "last": [{ "variant": "young07_smell15_scav3", "seed": 8, "what": "170 s, done", "cut": false }],
        });
        use egui_kittest::kittest::Queryable;
        const PAUSE: &str = "⏸  Пауза";
        const BUTTONS: [&str; 7] = [
            PAUSE,
            "▶  Продолжить",
            "⏹  Остановить",
            "Да, остановить",
            "Нет",
            "Открыть сводку",
            "Папка с результатами",
        ];
        for (name, p, confirm, buttons) in [
            ("progress-running", running_sweep(), false, &[PAUSE, "⏹  Остановить"][..]),
            ("progress-paused", paused, false, &["▶  Продолжить", "⏹  Остановить"]),
            ("progress-confirm-stop", running_sweep(), true, &["Да, остановить", "Нет"]),
            ("progress-finished", finished, false, &["Открыть сводку", "Папка с результатами"]),
        ] {
            let view = View::of(&p, 1005);
            let mut local = Local { confirm_stop: confirm, ..Local::default() };
            let pressed = std::rc::Rc::new(std::cell::Cell::new(None));
            let got = pressed.clone();
            let mut h =
                egui_kittest::Harness::builder().with_size(vec2(500.0, 380.0)).wgpu().build_ui(move |ui| {
                    style(ui.ctx());
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(14)))
                        .show(ui, |ui| {
                            if let Some(ask) = draw(ui, &view, &mut local, None) {
                                got.set(Some(ask));
                            }
                        });
                });
            h.run_steps(4);
            if let Ok(dir) = std::env::var("LIFEGAME_SHOTS") {
                std::fs::create_dir_all(&dir).expect("a folder for the pictures");
                h.render().expect("a picture").save(format!("{dir}/{name}.png")).expect("saved");
            }
            for label in BUTTONS {
                let shown = h.query_by_label(label).is_some();
                assert_eq!(shown, buttons.contains(&label), "{name}: «{label}»");
            }
            if buttons.contains(&PAUSE) {
                h.get_by_label(PAUSE).click();
                h.run_steps(2);
                assert_eq!(pressed.get(), Some(Asked::Pause), "{name}: the press reaches the sweep");
                // the corner of the card with three seeds unfolds them, and folds them back
                const SEED: &str = "сид 4 · тик 10 800 из 20 000 · 2 мин";
                assert!(h.query_by_label(SEED).is_none(), "folded at first");
                h.get_by_label("+").click();
                h.run_steps(2);
                assert!(h.query_by_label(SEED).is_some(), "unfolded");
                if let Ok(dir) = std::env::var("LIFEGAME_SHOTS") {
                    h.render().expect("a picture").save(format!("{dir}/progress-seeds.png")).expect("saved");
                }
                h.get_by_label("−").click();
                h.run_steps(2);
                assert!(h.query_by_label(SEED).is_none(), "folded again");
            }
        }
    }
}
