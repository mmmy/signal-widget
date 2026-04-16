use eframe::egui;

use crate::config::GroupConfig;

pub fn render_toolbar(ui: &mut egui::Ui, always_on_top: &mut bool) -> bool {
    ui.checkbox(always_on_top, "窗口置顶").changed()
}

pub fn render_group_header(ui: &mut egui::Ui, group: &GroupConfig, unread: usize) {
    ui.heading(&group.symbol);
    if unread > 0 {
        ui.label(format!("{unread} unread"));
    }
}
