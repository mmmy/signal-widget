#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAction {
    MinimizeToTray,
    CloseApp,
}

pub fn close_action_for_request(
    close_requested: bool,
    allow_close: bool,
    tray_available: bool,
) -> Option<CloseAction> {
    if !close_requested {
        return None;
    }

    if allow_close || !tray_available {
        Some(CloseAction::CloseApp)
    } else {
        Some(CloseAction::MinimizeToTray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_requests_hide_when_tray_exists() {
        let action = close_action_for_request(true, false, true);
        assert_eq!(action, Some(CloseAction::MinimizeToTray));
    }
}
