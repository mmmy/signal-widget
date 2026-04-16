pub mod adapters;
pub mod alerts;
pub mod api;
pub mod app;
pub mod config;
pub mod core;
pub mod domain;
pub mod poller;
pub mod shell;
pub mod unread_panel;

use std::cell::RefCell;
use std::rc::Rc;

use tracing::Level;
use winit::raw_window_handle::HasWindowHandle;

use crate::adapters::tray::TrayAdapter;
use crate::api::ApiClient;
use crate::app::{setup_chinese_fonts, SignalDeskApp};
use crate::config::AppConfig;
use crate::core::runtime::Runtime;
use crate::poller::PollerHandle;
use crate::shell::MainWindowController;

fn build_native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Signal Desk")
            .with_inner_size([540.0, 760.0])
            .with_min_inner_size([220.0, 360.0]),
        // Keep the event loop active while the main window is hidden so
        // tray/menu actions still get processed on Windows.
        run_and_return: false,
        ..Default::default()
    }
}

pub fn run() {
    let (config, config_path) = match AppConfig::load_or_create() {
        Ok(data) => data,
        Err(err) => {
            eprintln!("failed to load config: {err}");
            return;
        }
    };
    init_tracing();
    println!("using config: {}", config_path.display());

    let native_options = build_native_options();

    let runtime_holder = Rc::new(RefCell::new(None));
    let tray_holder = Rc::new(RefCell::new(None));
    let runtime_slot = Rc::clone(&runtime_holder);
    let tray_slot = Rc::clone(&tray_holder);

    let result = eframe::run_native(
        "Signal Desk",
        native_options,
        Box::new(move |cc| {
            setup_chinese_fonts(&cc.egui_ctx);
            let api_client = ApiClient::new(&config.api);
            let mut poller = PollerHandle::spawn(api_client, config.clone(), cc.egui_ctx.clone());
            let (runtime, runtime_handle, runtime_event_rx) =
                Runtime::spawn(
                    cc.egui_ctx.clone(),
                    poller.command_tx.clone(),
                    poller.take_event_rx(),
                );
            let main_window =
                MainWindowController::from_raw_window_handle(cc.window_handle()?.as_raw())?;
            let tray_adapter = match TrayAdapter::new(main_window.clone(), cc.egui_ctx.clone()) {
                Ok(adapter) => Some(adapter),
                Err(_err) => None,
            };
            let _ = runtime_handle.set_tray_available(tray_adapter.is_some());

            runtime_slot.borrow_mut().replace(runtime);
            tray_slot.borrow_mut().replace(tray_adapter);

            Ok(Box::new(SignalDeskApp::new(
                config,
                config_path,
                poller,
                main_window,
                runtime_slot
                    .borrow_mut()
                    .take()
                    .expect("runtime initialized once"),
                runtime_handle,
                runtime_event_rx,
            )))
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

#[cfg(test)]
mod integration_contract_tests {
    use crate::core::contract::AppCommand;

    #[test]
    fn command_contract_exposes_force_poll() {
        let cmd = AppCommand::ForcePoll;
        match cmd {
            AppCommand::ForcePoll => {}
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn native_options_keep_event_loop_running_for_tray_actions() {
        let options = super::build_native_options();
        assert!(!options.run_and_return);
    }
}
