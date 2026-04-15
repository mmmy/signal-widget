use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use chrono::TimeZone;
use eframe::egui::{self, Color32};
use tracing::{info, warn};

use crate::alerts::AlertEngine;
use crate::api::{SignalPage, SignalState};
use crate::config::{AppConfig, GroupConfig};
use crate::core::contract::{AdapterId, AppEvent, UiAction};
use crate::core::queries::unread::{collect_new_unread_keys, effective_unread_keys};
use crate::core::runtime::{Runtime, RuntimeHandle};
use crate::domain::{compare_period_desc, period_to_millis, Side, SignalKey};
use crate::poller::{PollerEvent, PollerHandle};
use crate::unread_panel::{build_unread_items, HoverPanelState, HoverPanelTarget};

#[derive(Clone, Copy, Debug, Default)]
struct BarCell {
    side: Option<Side>,
    mixed: bool,
    unread: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PeriodLabelVisual {
    text: String,
    color: Option<Color32>,
    strong: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowModeState {
    always_on_top: bool,
    edge_mode: bool,
    edge_width_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportClosePlan {
    Show,
    Close,
    MinimizeToTray,
}

pub struct SignalDeskApp {
    config: AppConfig,
    config_path: PathBuf,
    poller: PollerHandle,
    signals: HashMap<SignalKey, SignalState>,
    pending_read: HashSet<SignalKey>,
    local_read_floor_t: HashMap<SignalKey, i64>,
    hover_panel: Option<HoverPanelState>,
    hover_anchor: Option<egui::Rect>,
    last_window_mode: Option<WindowModeState>,
    alerts: AlertEngine,
    has_seen_snapshot: bool,
    last_poll_ms: Option<i64>,
    last_poll_ok: Option<bool>,
    consecutive_poll_failures: u32,
    last_meta: Option<(u64, u32, u32)>,
    last_error: Option<String>,
    // Keep the runtime alive for the lifetime of the app.
    _runtime: Runtime,
    runtime_handle: RuntimeHandle,
    runtime_event_rx: std::sync::mpsc::Receiver<AppEvent>,
}

pub fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let Some((font_name, bytes)) = load_cjk_font_bytes() else {
        warn!("no CJK font file found, keep egui default fonts");
        return;
    };

    fonts
        .font_data
        .insert(font_name.clone(), egui::FontData::from_owned(bytes).into());

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, font_name.clone());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push(font_name.clone());

    ctx.set_fonts(fonts);
    info!("loaded CJK font: {}", font_name);
}

impl SignalDeskApp {
    pub fn new(
        config: AppConfig,
        config_path: PathBuf,
        poller: PollerHandle,
        runtime: Runtime,
        runtime_handle: RuntimeHandle,
        runtime_event_rx: std::sync::mpsc::Receiver<AppEvent>,
    ) -> Self {
        let _ = runtime_handle.send(crate::core::contract::AppCommand::ForcePoll);
        Self {
            config,
            config_path,
            poller,
            signals: HashMap::new(),
            pending_read: HashSet::new(),
            local_read_floor_t: HashMap::new(),
            hover_panel: None,
            hover_anchor: None,
            last_window_mode: None,
            alerts: AlertEngine::default(),
            has_seen_snapshot: false,
            last_poll_ms: None,
            last_poll_ok: None,
            consecutive_poll_failures: 0,
            last_meta: None,
            last_error: None,
            _runtime: runtime,
            runtime_handle,
            runtime_event_rx,
        }
    }

    fn drain_poller_events(&mut self) -> bool {
        let mut had_events = false;
        while let Ok(event) = self.poller.event_rx.try_recv() {
            had_events = true;
            match event {
                PollerEvent::Snapshot {
                    fetched_at_ms,
                    page,
                } => {
                    self.consume_snapshot(fetched_at_ms, page);
                    self.consecutive_poll_failures = 0;
                    self.last_poll_ok = Some(true);
                    self.last_error = None;
                }
                PollerEvent::PollFailed { error } => {
                    self.consecutive_poll_failures =
                        self.consecutive_poll_failures.saturating_add(1);
                    self.last_poll_ok = Some(self.consecutive_poll_failures < 2);
                    self.last_error = Some(error);
                }
                PollerEvent::SyncFailed { key, error } => {
                    let was_pending = self.pending_read.remove(&key);
                    self.local_read_floor_t.remove(&key);
                    if was_pending {
                        if let Some(state) = self.signals.get_mut(&key) {
                            state.read = false;
                        }
                    }
                    self.last_error = Some(format!(
                        "sync failed [{} {} {}]: {}",
                        key.symbol, key.period, key.signal_type, error
                    ));
                }
                PollerEvent::MarkReadSynced { key } => {
                    self.pending_read.remove(&key);
                }
            }
        }
        had_events
    }

    fn consume_snapshot(&mut self, fetched_at_ms: i64, page: SignalPage) {
        let previous_unread = effective_unread_keys(&self.signals, &self.pending_read);
        let mut next = HashMap::new();
        for row in &page.data {
            for (signal_type, state) in &row.signals {
                let key =
                    SignalKey::new(row.symbol.clone(), row.period.clone(), signal_type.clone());
                let mut next_state = state.clone();
                if let Some(&floor_t) = self.local_read_floor_t.get(&key) {
                    if next_state.t <= floor_t {
                        next_state.read = true;
                    } else {
                        self.local_read_floor_t.remove(&key);
                    }
                }
                next.insert(key, next_state);
            }
        }
        self.signals = next;
        self.last_poll_ms = Some(fetched_at_ms);
        self.last_meta = Some((page.total, page.page, page.page_size));

        let current_unread = effective_unread_keys(&self.signals, &self.pending_read);
        if self.has_seen_snapshot {
            let new_unread = collect_new_unread_keys(&previous_unread, &current_unread);
            self.alerts.on_new_unread(
                chrono::Utc::now().timestamp_millis(),
                &new_unread,
                self.config.ui.notifications,
                self.config.ui.sound,
            );
        } else {
            self.has_seen_snapshot = true;
        }
    }

    fn save_config(&mut self) {
        match self.config.save_to(&self.config_path) {
            Ok(()) => {
                info!("config saved to {}", self.config_path.display());
                self.last_error = None;
            }
            Err(err) => self.last_error = Some(format!("save config failed: {}", err)),
        }
    }

    fn apply_window_mode(&mut self, ctx: &egui::Context) {
        let state = WindowModeState {
            always_on_top: self.config.ui.always_on_top,
            edge_mode: self.config.ui.edge_mode,
            edge_width_bits: self.config.ui.edge_width.clamp(120.0, 600.0).to_bits(),
        };
        if self.last_window_mode == Some(state) {
            return;
        }

        let level = if self.config.ui.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));

        let size = if self.config.ui.edge_mode {
            [self.config.ui.edge_width.clamp(120.0, 600.0), 760.0]
        } else {
            [540.0, 760.0]
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
        self.last_window_mode = Some(state);
    }

    fn group_unread_count(&self, group: &GroupConfig) -> usize {
        group
            .periods
            .iter()
            .flat_map(|period| {
                group
                    .signal_types
                    .iter()
                    .map(move |signal_type| (period, signal_type))
            })
            .filter_map(|(period, signal_type)| {
                let key = SignalKey::new(group.symbol.clone(), period.clone(), signal_type.clone());
                self.signals.get(&key).map(|sig| (key, sig))
            })
            .filter(|(key, sig)| !sig.read && !self.pending_read.contains(key))
            .count()
    }

    fn total_unread_count(&self) -> usize {
        build_unread_items(
            &self.config.groups,
            &self.signals,
            &self.pending_read,
            &HoverPanelTarget::Global,
        )
        .len()
    }

    fn mark_group_read(&mut self, group: &GroupConfig) {
        for period in &group.periods {
            for signal_type in &group.signal_types {
                let key = SignalKey::new(group.symbol.clone(), period.clone(), signal_type.clone());
                let Some(is_unread) = self.signals.get(&key).map(|signal| !signal.read) else {
                    continue;
                };
                if !is_unread {
                    continue;
                }
                self.mark_one_read(key);
            }
        }
    }

    fn mark_one_read(&mut self, key: SignalKey) {
        if self.pending_read.contains(&key) {
            return;
        }

        if let Some(signal_t) = self.signals.get(&key).map(|signal| signal.t) {
            self.local_read_floor_t.insert(key.clone(), signal_t);
        }
        if let Some(signal) = self.signals.get_mut(&key) {
            signal.read = true;
        }
        self.pending_read.insert(key.clone());

        if let Err(err) = self.runtime_handle.send(crate::core::contract::AppCommand::MarkRead {
            key: key.clone(),
            read: true,
        }) {
            self.pending_read.remove(&key);
            self.local_read_floor_t.remove(&key);
            if let Some(signal) = self.signals.get_mut(&key) {
                signal.read = false;
            }

            let message = format!(
                "send mark-read failed [{} {} {}]: {}",
                key.symbol, key.period, key.signal_type, err
            );
            warn!("{}", message);
            self.last_error = Some(message);
        }
    }

    fn build_bars(&self, group: &GroupConfig, period: &str) -> [BarCell; 60] {
        let mut bars = [BarCell::default(); 60];
        let Some(period_ms) = period_to_millis(period) else {
            return bars;
        };
        let now_ms = chrono::Utc::now().timestamp_millis();

        for signal_type in &group.signal_types {
            let key = SignalKey::new(
                group.symbol.clone(),
                period.to_string(),
                signal_type.clone(),
            );
            let Some(signal) = self.signals.get(&key) else {
                continue;
            };
            let elapsed = now_ms.saturating_sub(signal.t);
            let slot = elapsed / period_ms;
            if !(0..60).contains(&slot) {
                continue;
            }

            let idx = 59 - slot as usize;
            let next_side = Side::from_code(signal.sd);
            let unread = !signal.read && !self.pending_read.contains(&key);
            merge_bar_cell(&mut bars[idx], next_side, unread);
        }
        bars
    }

    fn render_period_row(&mut self, ui: &mut egui::Ui, group: &GroupConfig, period: &str) {
        let bars = self.build_bars(group, period);
        let has_unread = period_has_unread(&self.signals, &self.pending_read, group, period);
        let visual = period_label_visual(period, has_unread);
        let mut level_text = egui::RichText::new(visual.text).size(11.0).monospace();
        if let Some(color) = visual.color {
            level_text = level_text.color(color);
        }
        if visual.strong {
            level_text = level_text.strong();
        }

        ui.horizontal(|ui| {
            ui.add_sized([28.0, 14.0], egui::Label::new(level_text));
            paint_60_bar_line(ui, &bars);
        });
    }

    fn render_group_card(&mut self, ui: &mut egui::Ui, group: &GroupConfig) -> bool {
        let unread = self.group_unread_count(group);
        let mut unread_trigger_hovered = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&group.symbol);
                ui.label(
                    egui::RichText::new(&group.name)
                        .small()
                        .color(Color32::LIGHT_GRAY),
                );
                if unread > 0 {
                    let unread_badge = ui.label(
                        egui::RichText::new(format!("{unread} unread"))
                            .color(Color32::BLACK)
                            .background_color(Color32::from_rgb(245, 173, 0)),
                    );
                    if unread_badge.hovered() {
                        unread_trigger_hovered = true;
                        self.hover_panel = Some(HoverPanelState {
                            target: HoverPanelTarget::Group(group.id.clone()),
                            close_deadline_ms: None,
                        });
                        self.hover_anchor = Some(unread_badge.rect);
                    }
                }
                if ui.small_button("全部已读").clicked() {
                    self.mark_group_read(group);
                }
            });

            ui.small(format!(
                "signals: {} | periods: {}",
                group.signal_types.join(","),
                group.periods.join(",")
            ));

            let mut periods = group.periods.clone();
            periods.sort_by(|a, b| compare_period_desc(a, b));

            ui.add_space(4.0);
            for period in periods {
                self.render_period_row(ui, group, &period);
            }
        });
        unread_trigger_hovered
    }

    fn render_unread_popover(&mut self, ctx: &egui::Context) -> bool {
        let Some(panel) = self.hover_panel.clone() else {
            return false;
        };
        let Some(anchor) = self.hover_anchor else {
            return false;
        };
        let viewport = ctx.input(|i| i.screen_rect());
        let margin = 8.0_f32;
        let max_width = (viewport.width() - margin * 2.0).max(1.0);
        let desired_width = if self.config.ui.edge_mode {
            (self.config.ui.edge_width - 16.0).max(1.0)
        } else {
            420.0
        };
        let popover_width = desired_width.clamp(1.0, max_width);
        // Switch to compact layout when wide row columns + button can no longer fit safely.
        let wide_columns_width = 96.0 + 44.0 + 88.0 + 32.0 + 112.0;
        let wide_button_width = 92.0;
        let wide_spacing_width = 48.0;
        let wide_layout_min_width = wide_columns_width + wide_button_width + wide_spacing_width;
        let compact_layout = popover_width < wide_layout_min_width;
        let estimated_popover_height = 380.0_f32;
        let preferred_pos = anchor.left_bottom() + egui::vec2(0.0, 6.0);
        let min_x = viewport.left() + margin;
        let max_x = (viewport.right() - margin - popover_width).max(min_x);
        let min_y = viewport.top() + margin;
        let max_y = (viewport.bottom() - margin - estimated_popover_height).max(min_y);
        let popover_pos = egui::pos2(
            preferred_pos.x.clamp(min_x, max_x),
            preferred_pos.y.clamp(min_y, max_y),
        );

        let rows = build_unread_items(
            &self.config.groups,
            &self.signals,
            &self.pending_read,
            &panel.target,
        );
        let title = match &panel.target {
            HoverPanelTarget::Global => "全部未读警报",
            HoverPanelTarget::Group(_) => "分组未读警报",
        };

        let mut clicked_key = None;
        let popover = egui::Area::new(egui::Id::new("unread_hover_popover"))
            .order(egui::Order::Foreground)
            .fixed_pos(popover_pos)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| {
                        ui.set_min_width(popover_width);
                        ui.set_max_width(popover_width);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(title).strong());
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .max_height(320.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if rows.is_empty() {
                                        ui.label("暂无未读信号");
                                        return;
                                    }

                                    for row in &rows {
                                        let time_text =
                                            format_trigger_time_local(row.trigger_time_ms);
                                        if compact_layout {
                                            ui.vertical(|ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&row.symbol)
                                                            .monospace(),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&row.period)
                                                            .monospace(),
                                                    );
                                                    ui.label(&row.signal_type);
                                                    ui.label(side_rich_text(row.side));
                                                });
                                                ui.label(
                                                    egui::RichText::new(time_text.as_str())
                                                        .monospace(),
                                                );
                                                if ui.button("标记已读").clicked() {
                                                    clicked_key = Some(row.key.clone());
                                                }
                                            });
                                        } else {
                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [96.0, 18.0],
                                                    egui::Label::new(
                                                        egui::RichText::new(&row.symbol)
                                                            .monospace(),
                                                    ),
                                                );
                                                ui.add_sized(
                                                    [44.0, 18.0],
                                                    egui::Label::new(
                                                        egui::RichText::new(&row.period)
                                                            .monospace(),
                                                    ),
                                                );
                                                ui.add_sized(
                                                    [88.0, 18.0],
                                                    egui::Label::new(&row.signal_type),
                                                );
                                                ui.add_sized(
                                                    [32.0, 18.0],
                                                    egui::Label::new(side_rich_text(row.side)),
                                                );
                                                ui.add_sized(
                                                    [112.0, 18.0],
                                                    egui::Label::new(
                                                        egui::RichText::new(time_text.as_str())
                                                            .monospace(),
                                                    ),
                                                );
                                                if ui.button("标记已读").clicked() {
                                                    clicked_key = Some(row.key.clone());
                                                }
                                            });
                                        }
                                        ui.separator();
                                    }
                                });
                        });
                    })
                    .response
                    .hovered()
            });

        if let Some(key) = clicked_key {
            self.mark_one_read(key);
        }

        popover.inner || popover.response.hovered()
    }

    fn drain_runtime_events(&mut self, ctx: &egui::Context) -> bool {
        let mut had_events = false;
        while let Ok(event) = self.runtime_event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::AdapterAction {
                    target: AdapterId::MainWindow,
                    action,
                } => apply_viewport_close_plan(ctx, action_to_viewport_plan(action)),
                _ => {}
            }
        }
        had_events
    }
}

fn load_cjk_font_bytes() -> Option<(String, Vec<u8>)> {
    let candidates = [
        r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
    ];

    for path in candidates {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let name = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cjk-font")
            .to_string();
        return Some((name, bytes));
    }

    None
}

impl eframe::App for SignalDeskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let had_events = self.drain_poller_events();
        self.apply_window_mode(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            let _ = self
                .runtime_handle
                .send(crate::core::contract::AppCommand::RequestCloseMainWindow);
        }
        let had_runtime_events = self.drain_runtime_events(ctx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut trigger_hovered = false;

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let top_changed = ui
                    .checkbox(&mut self.config.ui.always_on_top, "窗口置顶")
                    .changed();

                if ui.button("立即轮询").clicked() {
                    let _ = self
                        .runtime_handle
                        .send(crate::core::contract::AppCommand::ForcePoll);
                }
                if ui.button("保存配置").clicked() {
                    self.save_config();
                }

                let edge_changed = ui
                    .checkbox(&mut self.config.ui.edge_mode, "贴边模式")
                    .changed();
                let width_changed = ui
                    .add(
                        egui::DragValue::new(&mut self.config.ui.edge_width)
                            .range(120.0..=600.0)
                            .speed(1.0)
                            .prefix("贴边宽度 "),
                    )
                    .changed();
                let notifications_changed = ui
                    .checkbox(&mut self.config.ui.notifications, "通知")
                    .changed();
                let sound_changed = ui.checkbox(&mut self.config.ui.sound, "声音").changed();

                if edge_changed
                    || top_changed
                    || width_changed
                    || notifications_changed
                    || sound_changed
                {
                    self.save_config();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let total_unread = self.total_unread_count();
            let mut total_text = egui::RichText::new(format!("Total unread: {total_unread}"))
                .color(Color32::BLACK);
            if total_unread > 0 {
                total_text = total_text.background_color(Color32::from_rgb(245, 173, 0));
            }
            let total_badge = ui.label(total_text);
            if total_badge.hovered() {
                trigger_hovered = true;
                self.hover_panel = Some(HoverPanelState {
                    target: HoverPanelTarget::Global,
                    close_deadline_ms: None,
                });
                self.hover_anchor = Some(total_badge.rect);
            }
            ui.add_space(8.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..self.config.groups.len() {
                    let group = self.config.groups[i].clone();
                    if !group.enabled {
                        continue;
                    }
                    trigger_hovered |= self.render_group_card(ui, &group);
                    ui.add_space(8.0);
                }
            });
        });

        let panel_hovered = self.render_unread_popover(ctx);
        if let Some(panel) = self.hover_panel.as_mut() {
            let next_deadline = crate::unread_panel::next_close_deadline_ms(
                trigger_hovered,
                panel_hovered,
                now_ms,
                panel.close_deadline_ms,
                200,
            );

            if let Some(deadline) = next_deadline {
                if now_ms >= deadline {
                    self.hover_panel = None;
                    self.hover_anchor = None;
                } else {
                    panel.close_deadline_ms = Some(deadline);
                }
            } else {
                panel.close_deadline_ms = None;
            }
        } else {
            self.hover_anchor = None;
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let poll_text = match self.last_poll_ms {
                Some(ts) => format!(
                    "last poll: {} ({})",
                    format_trigger_time_local(ts),
                    format_elapsed_since_local(ts, now_ms)
                ),
                None => "last poll: never".to_string(),
            };
            let meta = self
                .last_meta
                .map(|(total, page, page_size)| {
                    format!("total={total}, page={page}, pageSize={page_size}")
                })
                .unwrap_or_else(|| "total=0".to_string());
            let (poll_state_color, poll_state_text) = match self.last_poll_ok {
                Some(true) => (Color32::from_rgb(48, 181, 122), "轮询正常"),
                Some(false) => (Color32::from_rgb(214, 84, 105), "上次轮询失败"),
                None => (Color32::LIGHT_GRAY, "尚未轮询"),
            };

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(poll_state_color))
                    .on_hover_text(poll_state_text);
                ui.small(poll_text);
                ui.separator();
                ui.small(meta);
                if let Some(err) = &self.last_error {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                }
            });
        });

        if self.hover_panel.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if had_events || had_runtime_events {
            ctx.request_repaint();
        }
    }
}

fn merge_bar_cell(cell: &mut BarCell, side: Side, unread: bool) {
    if unread {
        cell.unread = true;
    }

    match cell.side {
        None => {
            cell.side = Some(side);
        }
        Some(existing) if existing == side => {}
        Some(_) => {
            cell.mixed = true;
        }
    }
}

fn paint_60_bar_line(ui: &mut egui::Ui, cells: &[BarCell; 60]) {
    let desired = egui::vec2(ui.available_width(), 14.0);
    let (rect, _response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let gap = 1.0;
    let bar_width = ((rect.width() - gap * 59.0) / 60.0).max(1.0);

    for (idx, cell) in cells.iter().enumerate() {
        let x = rect.left() + idx as f32 * (bar_width + gap);
        let slot = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            egui::vec2(bar_width, rect.height()),
        );
        painter.rect_filled(slot, 1.0, Color32::from_rgb(236, 239, 242));

        if cell.side.is_some() || cell.mixed {
            let active = slot.shrink2(egui::vec2(0.6, 2.0));
            painter.rect_filled(active, 1.0, bar_color(*cell));
            if cell.unread {
                painter.rect_stroke(
                    active.expand(0.35),
                    1.0,
                    egui::Stroke::new(0.8, Color32::WHITE),
                );
            }
        }
    }
}

fn bar_color(cell: BarCell) -> Color32 {
    if cell.mixed {
        return Color32::from_rgb(236, 170, 72);
    }
    match cell.side {
        Some(Side::Bull) => {
            if cell.unread {
                Color32::from_rgb(66, 220, 148)
            } else {
                Color32::from_rgb(48, 181, 122)
            }
        }
        Some(Side::Bear) => {
            if cell.unread {
                Color32::from_rgb(255, 119, 131)
            } else {
                Color32::from_rgb(214, 84, 105)
            }
        }
        _ => Color32::from_rgb(60, 84, 95),
    }
}

fn side_rich_text(side: Side) -> egui::RichText {
    match side {
        Side::Bull => egui::RichText::new("多").color(Color32::from_rgb(48, 181, 122)),
        Side::Bear => egui::RichText::new("空").color(Color32::from_rgb(214, 84, 105)),
        Side::Unknown => egui::RichText::new("未知").color(Color32::LIGHT_GRAY),
    }
}

fn format_trigger_time_local(trigger_time_ms: i64) -> String {
    match chrono::Local.timestamp_millis_opt(trigger_time_ms).single() {
        Some(dt) => dt.format("%m-%d %H:%M:%S").to_string(),
        None => "-".to_string(),
    }
}

fn format_elapsed_since_local(past_ms: i64, now_ms: i64) -> String {
    let delta_ms = now_ms.saturating_sub(past_ms);
    let minutes = delta_ms / 60_000;
    if minutes <= 0 {
        "刚刚".to_string()
    } else {
        format!("{minutes} 分钟前")
    }
}

fn period_label_visual(period: &str, has_unread: bool) -> PeriodLabelVisual {
    if has_unread {
        PeriodLabelVisual {
            text: format!("•{period}"),
            color: Some(Color32::from_rgb(245, 173, 0)),
            strong: true,
        }
    } else {
        PeriodLabelVisual {
            text: period.to_string(),
            color: None,
            strong: false,
        }
    }
}

fn period_has_unread(
    signals: &HashMap<SignalKey, SignalState>,
    pending_read: &HashSet<SignalKey>,
    group: &GroupConfig,
    period: &str,
) -> bool {
    group.signal_types.iter().any(|signal_type| {
        let key = SignalKey::new(
            group.symbol.clone(),
            period.to_string(),
            signal_type.clone(),
        );
        signals
            .get(&key)
        .is_some_and(|signal| !signal.read && !pending_read.contains(&key))
    })
}

fn action_to_viewport_plan(action: UiAction) -> ViewportClosePlan {
    match action {
        UiAction::ShowMainWindow => ViewportClosePlan::Show,
        UiAction::HideMainWindowToTray => ViewportClosePlan::MinimizeToTray,
        UiAction::ExitProcess => ViewportClosePlan::Close,
    }
}

fn apply_viewport_close_plan(ctx: &egui::Context, plan: ViewportClosePlan) {
    match plan {
        ViewportClosePlan::Show => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        ViewportClosePlan::Close => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ViewportClosePlan::MinimizeToTray => {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GroupConfig;
    use crate::core::queries::unread::collect_new_unread_keys;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn action_to_viewport_plan_maps_close_app_to_close() {
        let plan = action_to_viewport_plan(UiAction::ExitProcess);
        assert_eq!(plan, ViewportClosePlan::Close);
    }

    #[test]
    fn action_to_viewport_plan_maps_minimize_to_tray_to_minimize_plan() {
        let plan = action_to_viewport_plan(UiAction::HideMainWindowToTray);
        assert_eq!(plan, ViewportClosePlan::MinimizeToTray);
    }

    #[test]
    fn action_to_viewport_plan_maps_show_to_show_plan() {
        let plan = action_to_viewport_plan(UiAction::ShowMainWindow);
        assert_eq!(plan, ViewportClosePlan::Show);
    }

    #[test]
    fn period_label_visual_marks_unread_level() {
        let visual = period_label_visual("15", true);
        assert_eq!(visual.text, "•15");
        assert_eq!(visual.color, Some(Color32::from_rgb(245, 173, 0)));
        assert!(visual.strong);
    }

    #[test]
    fn period_label_visual_keeps_default_for_read_level() {
        let visual = period_label_visual("15", false);
        assert_eq!(visual.text, "15");
        assert_eq!(visual.color, None);
        assert!(!visual.strong);
    }

    #[test]
    fn period_has_unread_uses_signal_state_not_bar_window() {
        let group = GroupConfig {
            id: "g1".to_string(),
            name: "BTC".to_string(),
            symbol: "BTCUSDT".to_string(),
            periods: vec!["15".to_string()],
            signal_types: vec!["vegas".to_string()],
            enabled: true,
        };
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        let mut signals = HashMap::new();
        signals.insert(
            key.clone(),
            SignalState {
                sd: 1,
                t: 1,
                read: false,
            },
        );

        let has_unread = period_has_unread(&signals, &HashSet::new(), &group, "15");
        assert!(has_unread);
    }

    #[test]
    fn period_has_unread_ignores_pending_read_signals() {
        let group = GroupConfig {
            id: "g1".to_string(),
            name: "BTC".to_string(),
            symbol: "BTCUSDT".to_string(),
            periods: vec!["15".to_string()],
            signal_types: vec!["vegas".to_string()],
            enabled: true,
        };
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        let mut signals = HashMap::new();
        signals.insert(
            key.clone(),
            SignalState {
                sd: -1,
                t: 1,
                read: false,
            },
        );
        let mut pending = HashSet::new();
        pending.insert(key);

        let has_unread = period_has_unread(&signals, &pending, &group, "15");
        assert!(!has_unread);
    }

    #[test]
    fn collect_new_unread_keys_returns_only_new_entries() {
        let old_key = SignalKey::new("BTCUSDT", "15", "vegas");
        let new_key = SignalKey::new("ETHUSDT", "60", "trend");
        let previous = HashSet::from([old_key.clone()]);
        let current = HashSet::from([old_key, new_key.clone()]);

        let keys = collect_new_unread_keys(&previous, &current);
        assert_eq!(keys, vec![new_key]);
    }

    #[test]
    fn format_elapsed_since_local_shows_just_now_for_sub_minute() {
        let text = format_elapsed_since_local(100_000, 159_999);
        assert_eq!(text, "刚刚");
    }

    #[test]
    fn format_elapsed_since_local_shows_minutes() {
        let text = format_elapsed_since_local(100_000, 280_000);
        assert_eq!(text, "3 分钟前");
    }
}
