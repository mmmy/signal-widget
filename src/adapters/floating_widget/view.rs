use eframe::egui::{self, Align2, Color32, FontId};

use super::state::{connection_color, WidgetViewModel};

pub fn circle_radius(size: f32) -> f32 {
    size / 2.0
}

pub fn orb_fill_color() -> Color32 {
    Color32::from_rgb(28, 31, 38)
}

pub fn unread_text_color() -> Color32 {
    Color32::WHITE
}

pub fn render_widget(ui: &mut egui::Ui, size: f32, vm: &WidgetViewModel) -> egui::Response {
    let desired = egui::vec2(size, size);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let radius = circle_radius(size);
    let center = rect.center();

    painter.circle_filled(center, radius, orb_fill_color());
    painter.circle_filled(
        egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
        4.0,
        connection_color(vm.connection_state),
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        &vm.unread_text,
        FontId::proportional(18.0),
        unread_text_color(),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_radius_is_half_of_configured_size() {
        assert_eq!(circle_radius(56.0), 28.0);
    }

    #[test]
    fn orb_fill_color_uses_dark_surface() {
        assert_eq!(orb_fill_color(), Color32::from_rgb(28, 31, 38));
    }

    #[test]
    fn unread_text_color_is_white() {
        assert_eq!(unread_text_color(), Color32::WHITE);
    }
}
