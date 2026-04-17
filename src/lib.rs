pub mod adapters;
pub mod alerts;
pub mod api;
pub mod app;
pub mod config;
pub mod config_store;
pub mod core;
pub mod domain;
pub mod poller;
pub mod shell;
pub mod unread_panel;

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;

use tokio::sync::broadcast;
use tracing::{warn, Level};
use winit::raw_window_handle::HasWindowHandle;

use crate::adapters::tray::TrayAdapter;
use crate::api::ApiClient;
use crate::app::{setup_chinese_fonts, SignalDeskApp};
use crate::config_store::ConfigStore;
use crate::core::contract::{AppEvent, ShellCommand, WindowId};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainWindowShellEffect {
    Show,
    Hide,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellEffect {
    MainWindow(MainWindowShellEffect),
    WidgetVisibility(bool),
}

fn shell_effect(command: &ShellCommand) -> Option<ShellEffect> {
    match command {
        ShellCommand::ShowWindow(WindowId::Main) | ShellCommand::FocusWindow(WindowId::Main) => {
            Some(ShellEffect::MainWindow(MainWindowShellEffect::Show))
        }
        ShellCommand::HideWindow(WindowId::Main) => {
            Some(ShellEffect::MainWindow(MainWindowShellEffect::Hide))
        }
        ShellCommand::ShowWindow(WindowId::Widget) => Some(ShellEffect::WidgetVisibility(true)),
        ShellCommand::HideWindow(WindowId::Widget) => Some(ShellEffect::WidgetVisibility(false)),
        ShellCommand::ExitProcess => Some(ShellEffect::MainWindow(MainWindowShellEffect::Exit)),
        _ => None,
    }
}

fn apply_main_window_shell_effect(
    ctx: &eframe::egui::Context,
    main_window: &MainWindowController,
    effect: MainWindowShellEffect,
) {
    match effect {
        MainWindowShellEffect::Show => {
            main_window.show();
            ctx.request_repaint();
        }
        MainWindowShellEffect::Hide => {
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::CancelClose);
            main_window.hide_to_tray();
        }
        MainWindowShellEffect::Exit => {
            main_window.request_exit();
        }
    }
}

fn apply_shell_effect(
    ctx: &eframe::egui::Context,
    main_window: &MainWindowController,
    config_store: &ConfigStore,
    effect: ShellEffect,
) {
    match effect {
        ShellEffect::MainWindow(effect) => apply_main_window_shell_effect(ctx, main_window, effect),
        ShellEffect::WidgetVisibility(visible) => {
            if let Err(err) = config_store.update_ui(|ui| ui.widget.visible = visible) {
                warn!("failed to persist widget visibility: {}", err);
            }
            ctx.request_repaint();
        }
    }
}

fn spawn_shell_pump(
    ctx: eframe::egui::Context,
    main_window: MainWindowController,
    config_store: ConfigStore,
    mut event_rx: broadcast::Receiver<AppEvent>,
) {
    thread::spawn(move || loop {
        match event_rx.blocking_recv() {
            Ok(AppEvent::ShellCommand(command)) => {
                if let Some(effect) = shell_effect(&command) {
                    apply_shell_effect(&ctx, &main_window, &config_store, effect);
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
    });
}

pub fn run() {
    let config_store = match ConfigStore::load() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("failed to load config: {err}");
            return;
        }
    };
    let config = config_store.snapshot();
    let config_path = config_store.path();
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
            let (runtime, runtime_handle) = Runtime::spawn(
                cc.egui_ctx.clone(),
                poller.command_tx.clone(),
                poller.take_event_rx(),
            );
            let runtime_event_rx = runtime_handle.subscribe_events();
            let runtime_snapshot_rx = runtime_handle.subscribe_snapshot();
            let main_window =
                MainWindowController::from_raw_window_handle(cc.window_handle()?.as_raw())?;
            spawn_shell_pump(
                cc.egui_ctx.clone(),
                main_window.clone(),
                config_store.clone(),
                runtime_event_rx,
            );
            let tray_adapter =
                match TrayAdapter::new(runtime_handle.clone(), config.ui.widget.visible) {
                    Ok(adapter) => Some(adapter),
                    Err(_err) => None,
                };
            let _ = runtime_handle.set_tray_available(tray_adapter.is_some());

            runtime_slot.borrow_mut().replace(runtime);
            tray_slot.borrow_mut().replace(tray_adapter);

            Ok(Box::new(SignalDeskApp::new(
                config_store.clone(),
                config,
                poller,
                main_window,
                runtime_slot
                    .borrow_mut()
                    .take()
                    .expect("runtime initialized once"),
                runtime_handle,
                runtime_snapshot_rx,
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::AppConfig;
    use crate::config_store::ConfigStore;
    use crate::core::contract::{AppCommand, ShellCommand, WindowId};
    use crate::shell::MainWindowController;
    use winit::raw_window_handle::{RawWindowHandle, Win32WindowHandle};

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "signal-desk-shell-effect-{name}-{unique}-{}.yaml",
            std::process::id()
        ))
    }

    fn test_main_window_controller() -> MainWindowController {
        use core::num::NonZeroIsize;

        MainWindowController::from_raw_window_handle(RawWindowHandle::Win32(
            Win32WindowHandle::new(NonZeroIsize::new(7).expect("non zero")),
        ))
        .expect("controller")
    }

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

    #[test]
    fn main_window_shell_effect_maps_main_window_commands() {
        assert_eq!(
            super::shell_effect(&ShellCommand::ShowWindow(WindowId::Main)),
            Some(super::ShellEffect::MainWindow(
                super::MainWindowShellEffect::Show
            ))
        );
        assert_eq!(
            super::shell_effect(&ShellCommand::FocusWindow(WindowId::Main)),
            Some(super::ShellEffect::MainWindow(
                super::MainWindowShellEffect::Show
            ))
        );
        assert_eq!(
            super::shell_effect(&ShellCommand::HideWindow(WindowId::Main)),
            Some(super::ShellEffect::MainWindow(
                super::MainWindowShellEffect::Hide
            ))
        );
        assert_eq!(
            super::shell_effect(&ShellCommand::ExitProcess),
            Some(super::ShellEffect::MainWindow(
                super::MainWindowShellEffect::Exit
            ))
        );
    }

    #[test]
    fn shell_effect_maps_widget_visibility_commands() {
        assert_eq!(
            super::shell_effect(&ShellCommand::ShowWindow(WindowId::Widget)),
            Some(super::ShellEffect::WidgetVisibility(true))
        );
        assert_eq!(
            super::shell_effect(&ShellCommand::HideWindow(WindowId::Widget)),
            Some(super::ShellEffect::WidgetVisibility(false))
        );
    }

    #[test]
    fn shell_effect_ignores_non_handled_widget_commands() {
        assert_eq!(
            super::shell_effect(&ShellCommand::FocusWindow(WindowId::Widget)),
            None
        );
    }

    #[test]
    fn apply_shell_effect_persists_widget_visibility() {
        let path = temp_path("widget-visible");
        let store = ConfigStore::new_for_test(AppConfig::default(), path.clone());

        super::apply_shell_effect(
            &eframe::egui::Context::default(),
            &test_main_window_controller(),
            &store,
            super::ShellEffect::WidgetVisibility(false),
        );

        let updated = store.snapshot();
        assert!(!updated.ui.widget.visible);

        std::fs::remove_file(path).expect("remove temp config");
    }
}
