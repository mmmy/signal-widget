use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Color32};
use tracing::{info, warn};

use crate::api::{SignalPage, SignalState};
use crate::config::{AppConfig, GroupConfig};
use crate::domain::{compare_period_desc, period_to_millis, Side, SignalKey};
use crate::poller::{PollerCommand, PollerEvent, PollerHandle};
use crate::unread_panel::HoverPanelState;

#[derive(Clone, Copy, Debug, Default)]
struct BarCell {
    side: Option<Side>,
    mixed: bool,
    unread: bool,
}

pub struct SignalDeskApp {
    config: AppConfig,
    config_path: PathBuf,
    poller: PollerHandle,
    signals: HashMap<SignalKey, SignalState>,
    pending_read: HashSet<SignalKey>,
    hover_panel: Option<HoverPanelState>,
    last_poll_ms: Option<i64>,
    last_meta: Option<(u64, u32, u32)>,
    last_error: Option<String>,
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
    pub fn new(config: AppConfig, config_path: PathBuf, poller: PollerHandle) -> Self {
        let _ = poller.command_tx.send(PollerCommand::ForcePoll);
        Self {
            config,
            config_path,
            poller,
            signals: HashMap::new(),
            pending_read: HashSet::new(),
            hover_panel: None,
            last_poll_ms: None,
            last_meta: None,
            last_error: None,
        }
    }

    fn drain_poller_events(&mut self) {
        while let Ok(event) = self.poller.event_rx.try_recv() {
            match event {
                PollerEvent::Snapshot { fetched_at_ms, page } => {
                    self.consume_snapshot(fetched_at_ms, page);
                    self.last_error = None;
                }
                PollerEvent::PollFailed { error } => {
                    self.last_error = Some(error);
                }
                PollerEvent::SyncFailed { key, error } => {
                    self.pending_read.remove(&key);
                    if let Some(state) = self.signals.get_mut(&key) {
                        state.read = false;
                    }
                    self.last_error = Some(format!("sync failed [{} {} {}]: {}", key.symbol, key.period, key.signal_type, error));
                }
                PollerEvent::MarkReadSynced { key } => {
                    self.pending_read.remove(&key);
                }
            }
        }
    }

    fn consume_snapshot(&mut self, fetched_at_ms: i64, page: SignalPage) {
        let mut next = HashMap::new();
        for row in &page.data {
            for (signal_type, state) in &row.signals {
                let key = SignalKey::new(row.symbol.clone(), row.period.clone(), signal_type.clone());
                next.insert(key, state.clone());
            }
        }
        self.signals = next;
        self.last_poll_ms = Some(fetched_at_ms);
        self.last_meta = Some((page.total, page.page, page.page_size));
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

    fn apply_window_mode(&self, ctx: &egui::Context) {
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
    }

    fn group_unread_count(&self, group: &GroupConfig) -> usize {
        group
            .periods
            .iter()
            .flat_map(|period| group.signal_types.iter().map(move |signal_type| (period, signal_type)))
            .filter_map(|(period, signal_type)| {
                let key = SignalKey::new(group.symbol.clone(), period.clone(), signal_type.clone());
                self.signals.get(&key)
            })
            .filter(|sig| !sig.read)
            .count()
    }

    fn mark_group_read(&mut self, group: &GroupConfig) {
        for period in &group.periods {
            for signal_type in &group.signal_types {
                let key = SignalKey::new(group.symbol.clone(), period.clone(), signal_type.clone());
                let Some(signal) = self.signals.get_mut(&key) else {
                    continue;
                };
                if signal.read {
                    continue;
                }
                signal.read = true;
                if let Err(err) = self
                    .poller
                    .command_tx
                    .send(PollerCommand::MarkRead { key: key.clone(), read: true })
                {
                    warn!("send mark-read command failed: {}", err);
                }
            }
        }
    }

    fn build_bars(&self, group: &GroupConfig, period: &str) -> [BarCell; 60] {
        let mut bars = [BarCell::default(); 60];
        let Some(period_ms) = period_to_millis(period) else {
            return bars;
        };
        let now_ms = chrono::Utc::now().timestamp_millis();

        for signal_type in &group.signal_types {
            let key = SignalKey::new(group.symbol.clone(), period.to_string(), signal_type.clone());
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
            let unread = !signal.read;
            merge_bar_cell(&mut bars[idx], next_side, unread);
        }
        bars
    }

    fn render_period_row(&mut self, ui: &mut egui::Ui, group: &GroupConfig, period: &str) {
        ui.horizontal(|ui| {
            ui.add_sized(
                [21.0, 14.0],
                egui::Label::new(egui::RichText::new(period).size(11.0).monospace()),
            );
            let bars = self.build_bars(group, period);
            paint_60_bar_line(ui, &bars);
        });
    }

    fn render_group_card(&mut self, ui: &mut egui::Ui, group: &GroupConfig) {
        let unread = self.group_unread_count(group);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&group.symbol);
                ui.label(egui::RichText::new(&group.name).small().color(Color32::LIGHT_GRAY));
                if unread > 0 {
                    ui.label(
                        egui::RichText::new(format!("{unread} unread"))
                            .color(Color32::BLACK)
                            .background_color(Color32::from_rgb(245, 173, 0)),
                    );
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
        self.drain_poller_events();
        self.apply_window_mode(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("立即轮询").clicked() {
                    let _ = self.poller.command_tx.send(PollerCommand::ForcePoll);
                }
                if ui.button("保存配置").clicked() {
                    self.save_config();
                }

                let edge_changed = ui.checkbox(&mut self.config.ui.edge_mode, "贴边模式").changed();
                let top_changed = ui.checkbox(&mut self.config.ui.always_on_top, "窗口置顶").changed();
                let width_changed = ui
                    .add(
                        egui::DragValue::new(&mut self.config.ui.edge_width)
                            .range(120.0..=600.0)
                            .speed(1.0)
                            .prefix("贴边宽度 "),
                    )
                    .changed();
                ui.checkbox(&mut self.config.ui.notifications, "通知");
                ui.checkbox(&mut self.config.ui.sound, "声音");

                if edge_changed || top_changed || width_changed {
                    self.save_config();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Signal Groups");
            ui.small("交易信号监控台（MVP 骨架）");
            ui.add_space(8.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..self.config.groups.len() {
                    let group = self.config.groups[i].clone();
                    if !group.enabled {
                        continue;
                    }
                    self.render_group_card(ui, &group);
                    ui.add_space(8.0);
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let poll_text = match self.last_poll_ms {
                Some(ts) => format!("last poll: {}", ts),
                None => "last poll: never".to_string(),
            };
            let meta = self
                .last_meta
                .map(|(total, page, page_size)| format!("total={total}, page={page}, pageSize={page_size}"))
                .unwrap_or_else(|| "total=0".to_string());

            ui.horizontal(|ui| {
                ui.small(poll_text);
                ui.separator();
                ui.small(meta);
                if let Some(err) = &self.last_error {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                }
            });
        });

        ctx.request_repaint_after(Duration::from_millis(200));
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
        let slot = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(bar_width, rect.height()));
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
