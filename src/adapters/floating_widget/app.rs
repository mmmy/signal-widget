use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use eframe::egui::{self, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};

use crate::config::WidgetConfig;
use crate::config_store::ConfigStore;
use crate::core::contract::AppSnapshot;

use super::state::build_view_model;
use super::view::render_widget;

pub fn widget_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("desktop-widget-window")
}

pub const WIDGET_VIEWPORT_TITLE: &str = "Signal Desk Widget";

pub fn widget_viewport_title() -> &'static str {
    WIDGET_VIEWPORT_TITLE
}

fn should_install_native(installed: &AtomicBool) -> bool {
    !installed.load(Ordering::SeqCst)
}

fn mark_native_install(installed: &AtomicBool) {
    installed.store(true, Ordering::SeqCst);
}

pub fn show_widget_viewport(
    ctx: &egui::Context,
    snapshot: &AppSnapshot,
    unread_count: usize,
    widget: &WidgetConfig,
    config_store: &ConfigStore,
) {
    let viewport_id = widget_viewport_id();
    let builder = ViewportBuilder::default()
        .with_title(widget_viewport_title())
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_resizable(false)
        .with_inner_size([widget.size, widget.size])
        .with_position([widget.x, widget.y])
        .with_always_on_top();
    let snapshot = snapshot.clone();
    let widget = widget.clone();
    let unread_count = unread_count;
    let config_store = config_store.clone();
    let native_installed = Arc::new(AtomicBool::new(false));

    ctx.show_viewport_deferred(viewport_id, builder, {
        let native_installed = Arc::clone(&native_installed);
        move |viewport_ctx, class| {
            if matches!(class, ViewportClass::Embedded) {
                return;
            }

            if should_install_native(&native_installed) {
                #[cfg(target_os = "windows")]
                unsafe {
                    if let Some(hwnd) = crate::shell::windows::widget_window::find_widget_hwnd(
                        widget_viewport_title(),
                    ) {
                        crate::shell::windows::widget_window::apply_widget_surface_style(hwnd);
                        crate::shell::windows::widget_window::install_widget_hit_test(
                            hwnd,
                            widget.size / 2.0,
                        );
                        mark_native_install(&native_installed);
                    }
                }
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                .show(viewport_ctx, |ui| {
                    let vm = build_view_model(&snapshot, unread_count);
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
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_install_guard_runs_only_once() {
        let installed = AtomicBool::new(false);
        assert!(should_install_native(&installed));
        mark_native_install(&installed);
        assert!(!should_install_native(&installed));
    }

    #[test]
    fn widget_viewport_title_is_stable() {
        assert_eq!(widget_viewport_title(), "Signal Desk Widget");
    }
}
