use anyhow::{Context as _, Result};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::shell::MainWindowController;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayAdapter {
    _tray_icon: TrayIcon,
    _event_pump: TrayEventPump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayUserAction {
    ShowMainWindow,
    ExitApp,
}

struct TrayEventPump {
    shutdown_tx: mpsc::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl TrayEventPump {
    fn spawn(show_id: MenuId, exit_id: MenuId, main_window: MainWindowController) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let join = thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    if let Some(action) = menu_event_to_action(&event, &show_id, &exit_id) {
                        apply_tray_action(action, &main_window);
                    }
                }

                while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    if let Some(action) = map_tray_click_to_action(&event) {
                        apply_tray_action(action, &main_window);
                    }
                }

                thread::sleep(Duration::from_millis(16));
            }
        });

        Self {
            shutdown_tx,
            join: Some(join),
        }
    }
}

impl Drop for TrayEventPump {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl TrayAdapter {
    pub fn new(main_window: MainWindowController) -> Result<Self> {
        let tray_menu = Menu::new();
        let show_item = MenuItem::new("显示主窗口", true, None);
        let exit_item = MenuItem::new("退出", true, None);
        tray_menu
            .append_items(&[&show_item, &PredefinedMenuItem::separator(), &exit_item])
            .context("failed to build tray menu")?;

        let event_pump = TrayEventPump::spawn(
            show_item.id().clone(),
            exit_item.id().clone(),
            main_window,
        );

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Signal Desk")
            .with_menu_on_left_click(false)
            .with_icon(default_tray_icon()?)
            .build()
            .context("failed to create system tray icon")?;

        Ok(Self {
            _tray_icon: tray_icon,
            _event_pump: event_pump,
        })
    }
}

fn apply_tray_action(action: TrayUserAction, main_window: &MainWindowController) {
    match action {
        TrayUserAction::ShowMainWindow => main_window.show(),
        TrayUserAction::ExitApp => main_window.request_exit(),
    }
}

fn menu_event_to_action(
    event: &MenuEvent,
    show_id: &MenuId,
    exit_id: &MenuId,
) -> Option<TrayUserAction> {
    if event.id == *show_id {
        Some(TrayUserAction::ShowMainWindow)
    } else if event.id == *exit_id {
        Some(TrayUserAction::ExitApp)
    } else {
        None
    }
}

fn map_tray_click_to_action(event: &TrayIconEvent) -> Option<TrayUserAction> {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => Some(TrayUserAction::ShowMainWindow),
        _ => None,
    }
}

fn default_tray_icon() -> Result<Icon> {
    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;
    let mut rgba = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH + x) * 4) as usize;
            let border = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
            let trend_line = (x > 6 && x < 26) && (y == 11 || y == 20);
            let accent = x >= 14 && x <= 18 && y >= 7 && y <= 25;

            let (r, g, b, a) = if border {
                (26, 30, 36, 255)
            } else if accent {
                (245, 173, 0, 255)
            } else if trend_line {
                (72, 196, 142, 255)
            } else {
                (33, 39, 46, 255)
            };

            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT).context("failed to create tray icon pixels")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tray_icon::menu::MenuId;
    use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIconEvent, TrayIconId};

    #[test]
    fn menu_event_for_show_id_maps_to_show_action() {
        let show_id = MenuId::new("show-main");
        let exit_id = MenuId::new("exit-app");
        let event = MenuEvent {
            id: MenuId::new("show-main"),
        };

        let action = menu_event_to_action(&event, &show_id, &exit_id);
        assert_eq!(action, Some(TrayUserAction::ShowMainWindow));
    }

    #[test]
    fn menu_event_for_exit_id_maps_to_exit_action() {
        let show_id = MenuId::new("show-main");
        let exit_id = MenuId::new("exit-app");
        let event = MenuEvent {
            id: MenuId::new("exit-app"),
        };

        let action = menu_event_to_action(&event, &show_id, &exit_id);
        assert_eq!(action, Some(TrayUserAction::ExitApp));
    }

    #[test]
    fn left_click_maps_to_request_show_main_window() {
        let event = TrayIconEvent::Click {
            id: TrayIconId::new("test-tray"),
            position: tray_icon::dpi::PhysicalPosition::new(0.0, 0.0),
            rect: Rect::default(),
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };
        let mapped = map_tray_click_to_action(&event);
        assert_eq!(mapped, Some(TrayUserAction::ShowMainWindow));
    }
}
