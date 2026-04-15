#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod config;
mod domain;
mod poller;
mod unread_panel;

use tracing::Level;

use crate::api::ApiClient;
use crate::app::setup_chinese_fonts;
use crate::app::SignalDeskApp;
use crate::config::AppConfig;
use crate::poller::PollerHandle;

fn main() {
    init_tracing();

    let (config, config_path) = match AppConfig::load_or_create() {
        Ok(data) => data,
        Err(err) => {
            eprintln!("failed to load config: {err}");
            return;
        }
    };
    println!("using config: {}", config_path.display());
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Signal Desk")
            .with_inner_size([540.0, 760.0])
            .with_min_inner_size([220.0, 360.0]),
        ..Default::default()
    };

    let result = eframe::run_native(
        "Signal Desk",
        native_options,
        Box::new(move |cc| {
            setup_chinese_fonts(&cc.egui_ctx);
            let api_client = ApiClient::new(&config.api);
            let poller = PollerHandle::spawn(api_client, config.clone(), cc.egui_ctx.clone());
            Ok(Box::new(SignalDeskApp::new(config, config_path, poller)))
        }),
    );
    if let Err(err) = result {
        eprintln!("failed to start app window: {err}");
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .try_init();
}
