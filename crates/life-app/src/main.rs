//! lifegame, the game: a window on eframe (egui + wgpu), the simulation in its own thread.
//!
//!     cargo run -p life-app --release                        # the menu
//!     cargo run -p life-app --release -- --scale 10 --seed 3 # straight into a world
//!
//! The world's flags mean what they mean in `life-report`; `game::report_command` gives the whole
//! line that repeats a game without the window.

// A release build on Windows has no black console window: the game is launched by a double click.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod behaviour;
mod camera;
mod census;
mod charts;
mod diets;
mod frame;
mod game;
mod history;
mod motion;
mod render;
mod screens;
mod settings;
mod sim;
mod stats;
mod theme;
#[cfg(test)]
mod ui_tests;
mod view;

use clap::Parser;
use life_core::space::parse_scale;
use life_core::{Rules, Shape, WorldConfig};

#[derive(Parser)]
#[command(
    about = "lifegame: plants and creatures evolving. Without flags it opens the menu; any world flag \
             starts that world at once with the engine's default rules (cost_scale 1), not the game's \
             saved settings."
)]
struct Args {
    /// The world's seed; without it, a random one.
    #[arg(long)]
    seed: Option<u64>,
    /// The world's scale by area (1 is the base 6000x4000; from 1 to 10 000).
    #[arg(long, default_value_t = 1.0, value_parser = parse_scale)]
    scale: f64,
    /// The world's shape: 1:1, 3:2 (default), 2:1 or strip — a strip 4000 high.
    #[arg(long, value_parser = Shape::parse)]
    shape: Option<Shape>,
    /// Creatures at the start (by default by the world's area); the old name is
    /// `--vegetarians`.
    #[arg(long, alias = "vegetarians")]
    creatures: Option<usize>,
    /// A world rule: name=number (can be repeated), as in life-report. The food profile is also
    /// given by name: `--rule plant_width_profile=waves`.
    #[arg(long = "rule")]
    rules: Vec<String>,
}

/// Without a console of its own (a release build on Windows) the errors of the flags and `--help`
/// would vanish silently. If the game was started from a console, we write into it.
fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        unsafe extern "system" {
            fn AttachConsole(process: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: a WinAPI call without pointers; a failure (no console) is harmless.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// The window icon: three circles — a plant, a big and a small creature.
fn icon() -> eframe::egui::IconData {
    const N: usize = 64;
    let circles = [
        (18.0, 44.0, 10.0, frame::PLANT_COLOR),
        (40.0, 22.0, 16.0, frame::CREATURE_COLOR),
        (48.0, 48.0, 11.0, frame::lerp(frame::WORLD_BOTTOM, frame::CREATURE_COLOR, 0.65)),
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
    let rules = Rules::from_flags(&args.rules).unwrap_or_else(|e| {
        eprintln!("ошибка: {e}");
        std::process::exit(2);
    });
    // Any world flag goes straight into the game with that world; without flags, the menu.
    let direct = args.seed.is_some()
        || args.scale != 1.0
        || args.shape.is_some()
        || args.creatures.is_some()
        || !args.rules.is_empty();
    let start = direct.then(|| WorldConfig {
        seed: args.seed.unwrap_or_else(app::random_seed),
        scale: args.scale,
        shape: args.shape.unwrap_or(WorldConfig::default().shape),
        rules,
        n_creatures: args.creatures,
        ..Default::default()
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("lifegame")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            // maximised: the world is big, and a 1280×800 window at 125% scaling is taller than
            // many laptop screens
            .with_maximized(true)
            .with_icon(icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "lifegame",
        options,
        Box::new(move |cc| Ok(Box::new(app::LifeApp::new(cc, start, settings::default_path())))),
    )
}
