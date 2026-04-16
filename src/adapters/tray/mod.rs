use anyhow::{Context as _, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::sync::broadcast;

use crate::core::contract::{AppCommand, AppEvent};
use crate::core::runtime::RuntimeHandle;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayAdapter {
    _tray_icon: TrayIcon,
    _event_pump: TrayEventPump,
}

struct TrayMenuIds {
    show_main: MenuId,
    widget: MenuId,
    exit: MenuId,
}

impl TrayMenuIds {
    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            show_main: MenuId::new("show-main"),
            widget: MenuId::new("toggle-widget"),
            exit: MenuId::new("exit"),
        }
    }
}

struct TrayEventPump {
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl TrayEventPump {
    fn spawn(ids: TrayMenuIds, runtime: RuntimeHandle, initial_widget_visible: bool) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let join = thread::spawn(move || {
            let widget_visible = Arc::new(AtomicBool::new(initial_widget_visible));
            let mut event_rx = runtime.subscribe_events();

            loop {
                if thread_shutdown.load(Ordering::SeqCst) {
                    break;
                }

                drain_runtime_events(&widget_visible, &mut event_rx);

                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    if let Some(command) =
                        menu_event_to_command(&event, &ids, widget_visible.load(Ordering::SeqCst))
                    {
                        let _ = runtime.send(command);
                    }
                }

                while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    if let Some(command) = map_tray_click_to_command(&event) {
                        let _ = runtime.send(command);
                    }
                }

                thread::sleep(Duration::from_millis(16));
            }
        });

        Self {
            shutdown,
            join: Some(join),
        }
    }
}

impl Drop for TrayEventPump {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl TrayAdapter {
    pub fn new(runtime: RuntimeHandle, initial_widget_visible: bool) -> Result<Self> {
        let tray_menu = Menu::new();
        let show_main = MenuItem::new("显示主窗口", true, None);
        let widget = MenuItem::new("切换小组件", true, None);
        let exit = MenuItem::new("退出", true, None);
        let ids = TrayMenuIds {
            show_main: show_main.id().clone(),
            widget: widget.id().clone(),
            exit: exit.id().clone(),
        };
        tray_menu
            .append_items(&[
                &show_main,
                &widget,
                &PredefinedMenuItem::separator(),
                &exit,
            ])
            .context("failed to build tray menu")?;

        let event_pump = TrayEventPump::spawn(ids, runtime, initial_widget_visible);

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

fn drain_runtime_events(
    widget_visible: &AtomicBool,
    event_rx: &mut broadcast::Receiver<AppEvent>,
) {
    loop {
        match event_rx.try_recv() {
            Ok(AppEvent::WidgetVisibilityChanged { visible }) => {
                widget_visible.store(visible, Ordering::SeqCst);
            }
            Ok(_) => {}
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(broadcast::error::TryRecvError::Closed) => break,
        }
    }
}

fn menu_event_to_command(
    event: &MenuEvent,
    ids: &TrayMenuIds,
    widget_visible: bool,
) -> Option<AppCommand> {
    if event.id == ids.show_main {
        Some(AppCommand::RequestShowMainWindow)
    } else if event.id == ids.widget {
        Some(if widget_visible {
            AppCommand::RequestHideWidget
        } else {
            AppCommand::RequestShowWidget
        })
    } else if event.id == ids.exit {
        Some(AppCommand::RequestExitApp)
    } else {
        None
    }
}

fn map_tray_click_to_command(event: &TrayIconEvent) -> Option<AppCommand> {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => Some(AppCommand::RequestShowMainWindow),
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
    use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIconEvent, TrayIconId};

    #[test]
    fn widget_menu_maps_to_show_when_hidden() {
        let ids = TrayMenuIds::for_test();
        let event = MenuEvent {
            id: ids.widget.clone(),
        };

        let command = menu_event_to_command(&event, &ids, false);
        assert!(matches!(command, Some(AppCommand::RequestShowWidget)));
    }

    #[test]
    fn widget_menu_maps_to_hide_when_visible() {
        let ids = TrayMenuIds::for_test();
        let event = MenuEvent {
            id: ids.widget.clone(),
        };

        let command = menu_event_to_command(&event, &ids, true);
        assert!(matches!(command, Some(AppCommand::RequestHideWidget)));
    }

    #[test]
    fn show_main_menu_maps_to_show_command() {
        let ids = TrayMenuIds::for_test();
        let event = MenuEvent {
            id: ids.show_main.clone(),
        };

        let command = menu_event_to_command(&event, &ids, true);
        assert!(matches!(command, Some(AppCommand::RequestShowMainWindow)));
    }

    #[test]
    fn exit_menu_maps_to_exit_command() {
        let ids = TrayMenuIds::for_test();
        let event = MenuEvent {
            id: ids.exit.clone(),
        };

        let command = menu_event_to_command(&event, &ids, true);
        assert!(matches!(command, Some(AppCommand::RequestExitApp)));
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
        let mapped = map_tray_click_to_command(&event);
        assert!(matches!(mapped, Some(AppCommand::RequestShowMainWindow)));
    }
}
