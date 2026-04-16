use eframe::egui::Color32;

use crate::core::contract::AppSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetConnectionState {
    Unknown,
    Healthy,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetViewModel {
    pub unread_text: String,
    pub connection_state: WidgetConnectionState,
}

pub fn build_view_model(snapshot: &AppSnapshot) -> WidgetViewModel {
    WidgetViewModel {
        unread_text: snapshot.unread_count.to_string(),
        connection_state: match snapshot.last_poll_ok {
            Some(true) => WidgetConnectionState::Healthy,
            Some(false) => WidgetConnectionState::Failed,
            None => WidgetConnectionState::Unknown,
        },
    }
}

pub fn connection_color(state: WidgetConnectionState) -> Color32 {
    match state {
        WidgetConnectionState::Healthy => Color32::from_rgb(48, 181, 122),
        WidgetConnectionState::Failed => Color32::from_rgb(214, 84, 105),
        WidgetConnectionState::Unknown => Color32::LIGHT_GRAY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_view_model_maps_snapshot_into_circle_text_and_health() {
        let snapshot = crate::core::contract::AppSnapshot {
            unread_count: 12,
            last_poll_ok: Some(false),
            ..Default::default()
        };

        let vm = build_view_model(&snapshot);
        assert_eq!(vm.unread_text, "12");
        assert_eq!(vm.connection_state, WidgetConnectionState::Failed);
    }

    #[test]
    fn connection_color_uses_expected_palette() {
        assert_eq!(
            connection_color(WidgetConnectionState::Healthy),
            eframe::egui::Color32::from_rgb(48, 181, 122)
        );
        assert_eq!(
            connection_color(WidgetConnectionState::Unknown),
            eframe::egui::Color32::LIGHT_GRAY
        );
    }
}
