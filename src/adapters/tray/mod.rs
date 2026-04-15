use crate::core::contract::AppCommand;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

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
        assert_eq!(mapped, Some(AppCommand::RequestShowMainWindow));
    }
}
