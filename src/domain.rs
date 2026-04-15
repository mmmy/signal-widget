use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bull,
    Bear,
    Unknown,
}

impl Side {
    pub fn from_code(code: i32) -> Self {
        match code {
            1 => Self::Bull,
            -1 => Self::Bear,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalKey {
    pub symbol: String,
    pub period: String,
    pub signal_type: String,
}

impl SignalKey {
    pub fn new(
        symbol: impl Into<String>,
        period: impl Into<String>,
        signal_type: impl Into<String>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            period: period.into(),
            signal_type: signal_type.into(),
        }
    }
}

pub fn period_to_millis(period: &str) -> Option<i64> {
    let normalized = period.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return None;
    }

    if normalized.ends_with('W') {
        let num = normalized.trim_end_matches('W');
        let weeks = if num.is_empty() {
            1
        } else {
            num.parse::<i64>().ok()?
        };
        return Some(weeks * 7 * 24 * 60 * 60 * 1000);
    }

    if normalized.ends_with('D') {
        let num = normalized.trim_end_matches('D');
        let days = if num.is_empty() {
            1
        } else {
            num.parse::<i64>().ok()?
        };
        return Some(days * 24 * 60 * 60 * 1000);
    }

    let minutes = normalized.parse::<i64>().ok()?;
    Some(minutes * 60 * 1000)
}

pub fn compare_period_desc(a: &str, b: &str) -> Ordering {
    let ma = period_to_millis(a).unwrap_or_default();
    let mb = period_to_millis(b).unwrap_or_default();
    mb.cmp(&ma).then_with(|| a.cmp(b))
}
