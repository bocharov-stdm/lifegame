//! Tiny Life — игра. Окно на eframe (egui + wgpu), симуляция в своём потоке.
//!
//!     cargo run -p life-app --release                       # меню
//!     cargo run -p life-app --release -- --scale 10 --seed 3 # сразу в игру
//!
//! Флаги мира — как у `life-report`, так что партию из игры можно повторить
//! без окна тем же сидом, масштабом и правилами.

// Релиз на Windows — без чёрного окна консоли: игру запускают двойным щелчком.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod camera;
mod charts;
mod frame;
mod game;
mod history;
mod motion;
mod render;
mod screens;
mod settings;
mod sim;
mod theme;
#[cfg(test)]
mod ui_tests;
mod view;

use clap::Parser;
use life_core::space::{MAX_SCALE, MIN_SCALE};
use life_core::{Rules, WorldConfig};

#[derive(Parser)]
#[command(about = "Tiny Life — эволюция растений, травоядных и хищников")]
struct Args {
    /// Сид мира; без него — случайный.
    #[arg(long)]
    seed: Option<u64>,
    /// Масштаб мира по площади (1 — базовый 6000x4000; от 1 до 10 000).
    #[arg(long, default_value_t = 1.0, value_parser = parse_scale)]
    scale: f64,
    /// Травоядных на старте (по умолчанию — по площади мира).
    #[arg(long)]
    vegetarians: Option<usize>,
    /// Хищников на старте (по умолчанию — по площади мира).
    #[arg(long)]
    predators: Option<usize>,
    /// Правило мира: имя=число (можно несколько раз), как в life-report.
    #[arg(long = "rule")]
    rules: Vec<String>,
}

fn parse_scale(s: &str) -> Result<f64, String> {
    let scale: f64 = s.trim().parse().map_err(|_| format!("«{s}» — не число"))?;
    if (MIN_SCALE..=MAX_SCALE).contains(&scale) {
        Ok(scale)
    } else {
        Err(format!("масштаб должен быть от {MIN_SCALE} до {MAX_SCALE}"))
    }
}

fn parse_rules(pairs: &[String]) -> Result<Rules, String> {
    let mut rules = Rules::default();
    for pair in pairs {
        let (key, value) = pair.split_once('=').ok_or(format!("правило «{pair}»: нужно имя=число"))?;
        let value: f64 = value.trim().parse().map_err(|_| format!("правило {key}: «{value}» — не число"))?;
        rules = rules.with(key.trim(), value)?;
    }
    Ok(rules)
}

/// Без своей консоли (релиз на Windows) ошибки флагов и `--help` пропали бы
/// молча. Если игру запустили из консоли — пишем в неё.
fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        unsafe extern "system" {
            fn AttachConsole(process: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: вызов WinAPI без указателей; неудача (нет консоли) безвредна.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Значок окна: три кружка — растение, травоядное, хищник (как в Python-версии).
fn icon() -> eframe::egui::IconData {
    const N: usize = 64;
    let circles = [
        (18.0, 44.0, 10.0, frame::PLANT_COLOR),
        (40.0, 22.0, 16.0, frame::VEGETARIAN_COLOR),
        (48.0, 48.0, 12.0, frame::PREDATOR_COLOR),
    ];
    let mut rgba = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            for (cx, cy, r, c) in circles {
                let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                let a = (r - d + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    let px = &mut rgba[(y * N + x) * 4..][..4];
                    for ch in 0..3 {
                        px[ch] = (px[ch] as f64 * (1.0 - a) + c[ch] as f64 * a) as u8;
                    }
                    px[3] = px[3].max((a * 255.0) as u8);
                }
            }
        }
    }
    eframe::egui::IconData { rgba, width: N as u32, height: N as u32 }
}

fn main() -> eframe::Result {
    if std::env::args().len() > 1 {
        attach_parent_console();
    }
    let args = Args::parse();
    let rules = parse_rules(&args.rules).unwrap_or_else(|e| {
        eprintln!("ошибка: {e}");
        std::process::exit(2);
    });
    // Любой флаг мира — сразу в игру с этим миром; без флагов — меню.
    let direct = args.seed.is_some()
        || args.scale != 1.0
        || args.vegetarians.is_some()
        || args.predators.is_some()
        || !args.rules.is_empty();
    let start = direct.then(|| WorldConfig {
        seed: args.seed.unwrap_or_else(app::random_seed),
        scale: args.scale,
        rules,
        n_vegetarians: args.vegetarians,
        n_predators: args.predators,
        ..Default::default()
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Tiny Life")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            // развёрнуто: мир большой, а окно 1280×800 при масштабе 125% выше
            // многих экранов ноутбуков
            .with_maximized(true)
            .with_icon(icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Tiny Life",
        options,
        Box::new(move |cc| Ok(Box::new(app::LifeApp::new(cc, start, settings::default_path())))),
    )
}
