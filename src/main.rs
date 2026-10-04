#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod persistence;
mod ui;

use eframe::egui;

use crate::core::Tracker;
use crate::persistence::SqliteStore;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resolve")
            .with_app_id("resolve")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([520.0, 480.0])
            .with_icon(ui::app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Resolve",
        options,
        Box::new(|cc| match open_tracker() {
            Ok(tracker) => Ok(Box::new(ui::ResolveApp::new(&cc.egui_ctx, tracker))),
            Err(message) => Ok(Box::new(ui::StartupError(message))),
        }),
    )
}

fn open_tracker() -> Result<Tracker<SqliteStore>, String> {
    let path = persistence::default_database_path()
        .ok_or("Could not determine a data directory for this user")?;
    let store = SqliteStore::open(&path)
        .map_err(|err| format!("Could not open {}: {err}", path.display()))?;
    Tracker::load(store).map_err(|err| format!("Could not load {}: {err}", path.display()))
}
