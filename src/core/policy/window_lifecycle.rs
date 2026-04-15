#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAction {
    MinimizeToTray,
    CloseApp,
}

pub fn default_allow_close() -> bool {
    true
}

pub fn default_tray_available() -> bool {
    cfg!(target_os = "windows")
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
    fn close_requests_return_none_when_not_requested() {
        let action = close_action_for_request(false, false, true);
        assert_eq!(action, None);
    }

    #[test]
    fn close_requests_close_app_when_allowed() {
        let action = close_action_for_request(true, true, true);
        assert_eq!(action, Some(CloseAction::CloseApp));
    }

    #[test]
    fn close_requests_close_app_when_tray_is_missing() {
        let action = close_action_for_request(true, false, false);
        assert_eq!(action, Some(CloseAction::CloseApp));
    }

    #[test]
    fn close_requests_hide_when_tray_exists() {
        let action = close_action_for_request(true, false, true);
        assert_eq!(action, Some(CloseAction::MinimizeToTray));
    }
}
