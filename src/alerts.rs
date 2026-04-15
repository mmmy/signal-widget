use crate::domain::SignalKey;

const ALERT_THROTTLE_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AlertContent {
    title: String,
    body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastSoundMode {
    Default,
    Silent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlertDispatchPlan {
    show_toast: bool,
    toast_sound: Option<ToastSoundMode>,
    play_beep: bool,
}

#[derive(Debug, Default)]
pub struct AlertEngine {
    last_alert_ms: Option<i64>,
}

impl AlertEngine {
    pub fn on_new_unread(
        &mut self,
        now_ms: i64,
        new_unread: &[SignalKey],
        notifications_enabled: bool,
        sound_enabled: bool,
    ) -> bool {
        if new_unread.is_empty() || (!notifications_enabled && !sound_enabled) {
            return false;
        }
        if let Some(last_ms) = self.last_alert_ms {
            if now_ms.saturating_sub(last_ms) < ALERT_THROTTLE_MS {
                return false;
            }
        }

        let content = build_alert_content(new_unread);
        dispatch_alert(
            notifications_enabled,
            sound_enabled,
            &content.title,
            &content.body,
        );
        self.last_alert_ms = Some(now_ms);
        true
    }
}

fn build_alert_content(new_unread: &[SignalKey]) -> AlertContent {
    let first = &new_unread[0];
    let title = "Signal Desk 新信号".to_string();
    let body = if new_unread.len() == 1 {
        format!(
            "{} {} {} 出现新信号",
            first.symbol, first.period, first.signal_type
        )
    } else {
        format!(
            "{} {} {} +{} 条新信号",
            first.symbol,
            first.period,
            first.signal_type,
            new_unread.len() - 1
        )
    };
    AlertContent { title, body }
}

fn build_dispatch_plan(show_toast: bool, play_sound: bool) -> AlertDispatchPlan {
    if show_toast {
        let toast_sound = if play_sound {
            Some(ToastSoundMode::Default)
        } else {
            Some(ToastSoundMode::Silent)
        };
        return AlertDispatchPlan {
            show_toast: true,
            toast_sound,
            play_beep: false,
        };
    }

    AlertDispatchPlan {
        show_toast: false,
        toast_sound: None,
        play_beep: play_sound,
    }
}

#[cfg(all(target_os = "windows", not(test)))]
fn dispatch_alert(show_toast: bool, play_sound: bool, title: &str, body: &str) {
    let plan = build_dispatch_plan(show_toast, play_sound);

    if plan.show_toast {
        let toast = winrt_notification::Toast::new(winrt_notification::Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(body)
            .duration(winrt_notification::Duration::Short);
        let toast = match plan.toast_sound {
            Some(ToastSoundMode::Default) => toast.sound(Some(winrt_notification::Sound::Default)),
            Some(ToastSoundMode::Silent) => toast.sound(None),
            None => toast,
        };
        let _ = toast.show();
    }

    if plan.play_beep {
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(
                windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONASTERISK,
            );
        }
    }
}

#[cfg(any(not(target_os = "windows"), test))]
fn dispatch_alert(_show_toast: bool, _play_sound: bool, _title: &str, _body: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(symbol: &str, period: &str, signal_type: &str) -> SignalKey {
        SignalKey::new(symbol, period, signal_type)
    }

    #[test]
    fn builds_single_alert_content() {
        let content = build_alert_content(&[key("BTCUSDT", "15", "vegas")]);
        assert_eq!(content.title, "Signal Desk 新信号");
        assert_eq!(content.body, "BTCUSDT 15 vegas 出现新信号");
    }

    #[test]
    fn builds_aggregated_alert_content() {
        let content = build_alert_content(&[
            key("BTCUSDT", "15", "vegas"),
            key("ETHUSDT", "60", "trend"),
            key("SOLUSDT", "15", "divMacd"),
        ]);
        assert_eq!(content.body, "BTCUSDT 15 vegas +2 条新信号");
    }

    #[test]
    fn throttles_alerts_within_30_seconds() {
        let mut engine = AlertEngine::default();
        let keys = vec![key("BTCUSDT", "15", "vegas")];

        assert!(engine.on_new_unread(100_000, &keys, true, true));
        assert!(!engine.on_new_unread(120_000, &keys, true, true));
        assert!(engine.on_new_unread(130_000, &keys, true, true));
    }

    #[test]
    fn dispatch_plan_uses_silent_toast_when_sound_disabled() {
        let plan = build_dispatch_plan(true, false);
        assert_eq!(
            plan,
            AlertDispatchPlan {
                show_toast: true,
                toast_sound: Some(ToastSoundMode::Silent),
                play_beep: false,
            }
        );
    }

    #[test]
    fn dispatch_plan_avoids_double_sound_when_toast_has_sound() {
        let plan = build_dispatch_plan(true, true);
        assert_eq!(
            plan,
            AlertDispatchPlan {
                show_toast: true,
                toast_sound: Some(ToastSoundMode::Default),
                play_beep: false,
            }
        );
    }

    #[test]
    fn dispatch_plan_uses_beep_when_sound_enabled_without_toast() {
        let plan = build_dispatch_plan(false, true);
        assert_eq!(
            plan,
            AlertDispatchPlan {
                show_toast: false,
                toast_sound: None,
                play_beep: true,
            }
        );
    }
}
