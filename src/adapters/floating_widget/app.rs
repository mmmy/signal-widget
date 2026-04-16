use eframe::egui::{self, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};

use crate::config::WidgetConfig;
use crate::config_store::ConfigStore;
use crate::core::contract::AppSnapshot;

use super::state::build_view_model;
use super::view::render_widget;

pub fn widget_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("desktop-widget-window")
}

pub fn show_widget_viewport(
    ctx: &egui::Context,
    snapshot: &AppSnapshot,
    widget: &WidgetConfig,
    config_store: &ConfigStore,
) {
    let viewport_id = widget_viewport_id();
    let builder = ViewportBuilder::default()
        .with_title("Signal Desk Widget")
        .with_decorations(false)
        .with_resizable(false)
        .with_inner_size([widget.size, widget.size])
        .with_position([widget.x, widget.y])
        .with_always_on_top();
    let snapshot = snapshot.clone();
    let widget = widget.clone();
    let config_store = config_store.clone();

    ctx.show_viewport_deferred(viewport_id, builder, move |viewport_ctx, class| {
        if matches!(class, ViewportClass::Embedded) {
            return;
        }

        egui::CentralPanel::default().show(viewport_ctx, |ui| {
            let vm = build_view_model(&snapshot);
            let response = render_widget(ui, widget.size, &vm);
            if response.drag_started() {
                viewport_ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        });

        if !viewport_ctx.input(|i| i.pointer.primary_down()) {
            if let Some(rect) = viewport_ctx.input(|i| i.viewport().outer_rect) {
                let pos = rect.min;
                if (pos.x - widget.x).abs() > 0.5 || (pos.y - widget.y).abs() > 0.5 {
                    let _ = config_store.update_ui(|ui| {
                        ui.widget.x = pos.x;
                        ui.widget.y = pos.y;
                    });
                }
            }
        }
    });
}
