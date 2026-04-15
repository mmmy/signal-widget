pub mod api;
pub mod config;
pub mod core;
pub mod domain;

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
}
