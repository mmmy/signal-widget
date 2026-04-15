#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppSnapshot {
    pub unread_count: usize,
    pub last_poll_error: Option<String>,
}
