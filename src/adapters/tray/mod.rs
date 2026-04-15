use anyhow::{Context as _, Result};

use crate::core::contract::AppCommand;
use crate::core::runtime::RuntimeHandle;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

pub struct TrayAdapter {
    _tray_icon: TrayIcon,
}

impl TrayAdapter {
    pub fn new(runtime_handle: RuntimeHandle) -> Result<Self> {
        let tray_menu = Menu::new();
        let show_item = MenuItem::new("显示主窗口", true, None);
        let exit_item = MenuItem::new("退出", true, None);
        tray_menu
            .append_items(&[&show_item, &PredefinedMenuItem::separator(), &exit_item])
            .context("failed to build tray menu")?;

        let show_id = show_item.id().clone();
        let exit_id = exit_item.id().clone();
        let menu_runtime_handle = runtime_handle.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                let _ = menu_runtime_handle.send(AppCommand::RequestShowMainWindow);
            }

            if event.id == exit_id {
                let _ = menu_runtime_handle.send(AppCommand::RequestExitApp);
            }
        }));

        let click_runtime_handle = runtime_handle;
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let Some(command) = map_tray_click_to_command(&event) {
                let _ = click_runtime_handle.send(command);
            }
        }));

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Signal Desk")
            .with_menu_on_left_click(false)
            .with_icon(default_tray_icon()?)
            .build()
            .context("failed to create system tray icon")?;

        Ok(Self {
            _tray_icon: tray_icon,
        })
    }
}

pub fn map_tray_click_to_command(event: &TrayIconEvent) -> Option<AppCommand> {
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
    use crate::core::contract::AppCommand;
    use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIconEvent, TrayIconId};

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
