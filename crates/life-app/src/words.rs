//! The world's terrain the window shows, kept in egui's memory for the texts: the game's texts are
//! written for the sea and said for the land (`Terrain::say`) — a layer is a belt there, depth
//! latitude. The app sets it every frame (`LifeApp::ui`): the game's world, else the settings'.

use eframe::egui;
use life_core::Terrain;

fn id() -> egui::Id {
    egui::Id::new("terrain")
}

pub fn set(ctx: &egui::Context, terrain: Terrain) {
    ctx.data_mut(|d| d.insert_temp(id(), terrain));
}

pub fn terrain(ctx: &egui::Context) -> Terrain {
    ctx.data(|d| d.get_temp(id())).unwrap_or_default()
}

/// A text for the sea, said for the terrain the window shows.
pub fn say(ctx: &egui::Context, text: &str) -> String {
    terrain(ctx).say(text).into_owned()
}
