use crate::core::contract::AppCommand;
use crate::domain::SignalKey;

pub fn mark_read_command(key: SignalKey) -> AppCommand {
    AppCommand::MarkRead { key, read: true }
}

pub fn force_poll_command() -> AppCommand {
    AppCommand::ForcePoll
}

#[cfg(test)]
mod tests {
    use super::{force_poll_command, mark_read_command};
    use crate::core::contract::AppCommand;
    use crate::domain::SignalKey;

    #[test]
    fn mark_read_button_maps_to_mark_read_command() {
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        let cmd = mark_read_command(key.clone());
        assert!(matches!(cmd, AppCommand::MarkRead { key: actual, read: true } if actual == key));
    }

    #[test]
    fn force_poll_button_maps_to_force_poll_command() {
        assert!(matches!(force_poll_command(), AppCommand::ForcePoll));
    }
}
