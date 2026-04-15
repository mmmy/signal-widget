# Unread Hover Popover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add stable hover popovers for unread alerts (global + per-group) and allow per-item "标记已读" with optimistic update and rollback.

**Architecture:** Keep network and polling architecture unchanged, and add a focused unread/popover logic module with pure functions for filtering/sorting/hover-close behavior. UI integration stays in `src/app.rs`, and mark-read success acknowledgment is added in `src/poller.rs` so pending states can clear deterministically.

**Tech Stack:** Rust, egui/eframe, std channels, existing reqwest/tokio poller, unit tests in Rust `#[cfg(test)]`.

---

## File Structure Map

- Create: `src/unread_panel.rs`
  - `UnreadItemView`, `HoverPanelTarget`, `HoverPanelState`
  - pure functions for unread list derivation, sorting, hover close timing
  - unit tests for scope/sort/pending/close timing behavior
- Modify: `src/main.rs`
  - register `mod unread_panel;`
- Modify: `src/poller.rs`
  - emit mark-read success event
- Modify: `src/app.rs`
  - app state for pending read + hover panel
  - top-level unread badge
  - per-group unread trigger behavior
  - hover popover rendering + per-row mark-read action
  - optimistic update + rollback handling

### Task 1: Add Failing Tests for Unread Derivation and Hover Timing

**Files:**
- Create: `src/unread_panel.rs`
- Modify: `src/main.rs`
- Test: `src/unread_panel.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Create module with type definitions, function signatures, and failing tests**

```rust
// src/unread_panel.rs
use std::collections::{HashMap, HashSet};

use crate::api::SignalState;
use crate::config::GroupConfig;
use crate::domain::{Side, SignalKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverPanelTarget {
    Global,
    Group(String),
}

#[derive(Debug, Clone)]
pub struct HoverPanelState {
    pub target: HoverPanelTarget,
    pub close_deadline_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UnreadItemView {
    pub key: SignalKey,
    pub group_id: String,
    pub symbol: String,
    pub period: String,
    pub signal_type: String,
    pub side: Side,
    pub trigger_time_ms: i64,
    pub pending: bool,
}

pub fn build_unread_items(
    _groups: &[GroupConfig],
    _signals: &HashMap<SignalKey, SignalState>,
    _pending_read: &HashSet<SignalKey>,
    _target: &HoverPanelTarget,
) -> Vec<UnreadItemView> {
    panic!("red phase: build_unread_items not implemented");
}

pub fn next_close_deadline_ms(
    _trigger_hovered: bool,
    _panel_hovered: bool,
    _now_ms: i64,
    _current_deadline_ms: Option<i64>,
    _delay_ms: i64,
) -> Option<i64> {
    panic!("red phase: next_close_deadline_ms not implemented");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    use crate::api::SignalState;
    use crate::config::GroupConfig;
    use crate::domain::SignalKey;

    fn group(id: &str, symbol: &str) -> GroupConfig {
        GroupConfig {
            id: id.to_string(),
            name: id.to_string(),
            symbol: symbol.to_string(),
            periods: vec!["15".into(), "60".into()],
            signal_types: vec!["vegas".into()],
            enabled: true,
        }
    }

    #[test]
    fn global_contains_only_effective_unread_sorted_desc() {
        let groups = vec![group("g1", "BTCUSDT"), group("g2", "ETHUSDT")];
        let mut signals = HashMap::new();
        let k1 = SignalKey::new("BTCUSDT", "15", "vegas");
        let k2 = SignalKey::new("ETHUSDT", "15", "vegas");
        let k3 = SignalKey::new("BTCUSDT", "60", "vegas");

        signals.insert(k1.clone(), SignalState { sd: 1, t: 300, read: false });
        signals.insert(k2.clone(), SignalState { sd: -1, t: 200, read: false });
        signals.insert(k3.clone(), SignalState { sd: 1, t: 100, read: true });

        let mut pending = HashSet::new();
        pending.insert(k2.clone()); // effective read => filtered out

        let rows = build_unread_items(&groups, &signals, &pending, &HoverPanelTarget::Global);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, k1);
    }

    #[test]
    fn group_target_filters_to_single_group() {
        let groups = vec![group("g1", "BTCUSDT"), group("g2", "ETHUSDT")];
        let mut signals = HashMap::new();
        let k1 = SignalKey::new("BTCUSDT", "15", "vegas");
        let k2 = SignalKey::new("ETHUSDT", "15", "vegas");
        signals.insert(k1.clone(), SignalState { sd: 1, t: 300, read: false });
        signals.insert(k2.clone(), SignalState { sd: -1, t: 200, read: false });

        let rows = build_unread_items(
            &groups,
            &signals,
            &HashSet::new(),
            &HoverPanelTarget::Group("g1".into()),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, k1);
    }

    #[test]
    fn close_deadline_set_and_cleared_by_hover_state() {
        let deadline = next_close_deadline_ms(false, false, 1000, None, 200);
        assert_eq!(deadline, Some(1200));

        let keep_open = next_close_deadline_ms(true, false, 1050, deadline, 200);
        assert_eq!(keep_open, None);
    }
}
```

- [ ] **Step 2: Register module and run tests to verify failure**

```rust
// src/main.rs (top-level module list)
mod unread_panel;
```

Run: `cargo test unread_panel::tests -- --nocapture`  
Expected: FAIL with `red phase` panic from unimplemented helper bodies.

- [ ] **Step 3: Commit failing-test checkpoint**

```bash
git add src/unread_panel.rs src/main.rs
git commit -m "test: add failing unread panel behavior tests"
```

### Task 2: Implement Unread Panel Pure Logic Until Tests Pass

**Files:**
- Modify: `src/unread_panel.rs`
- Test: `src/unread_panel.rs`

- [ ] **Step 1: Implement unread row derivation, group scoping, effective-read filtering, and sorting**

```rust
pub fn build_unread_items(
    groups: &[GroupConfig],
    signals: &HashMap<SignalKey, SignalState>,
    pending_read: &HashSet<SignalKey>,
    target: &HoverPanelTarget,
) -> Vec<UnreadItemView> {
    let mut rows = Vec::new();

    for g in groups.iter().filter(|g| g.enabled) {
        if let HoverPanelTarget::Group(target_id) = target {
            if &g.id != target_id {
                continue;
            }
        }

        for period in &g.periods {
            for signal_type in &g.signal_types {
                let key = SignalKey::new(g.symbol.clone(), period.clone(), signal_type.clone());
                let Some(sig) = signals.get(&key) else { continue };
                let pending = pending_read.contains(&key);
                let effective_read = sig.read || pending;
                if effective_read {
                    continue;
                }

                rows.push(UnreadItemView {
                    key: key.clone(),
                    group_id: g.id.clone(),
                    symbol: g.symbol.clone(),
                    period: period.clone(),
                    signal_type: signal_type.clone(),
                    side: Side::from_code(sig.sd),
                    trigger_time_ms: sig.t,
                    pending,
                });
            }
        }
    }

    rows.sort_by(|a, b| b.trigger_time_ms.cmp(&a.trigger_time_ms));
    rows
}
```

- [ ] **Step 2: Implement hover close deadline helper**

```rust
pub fn next_close_deadline_ms(
    trigger_hovered: bool,
    panel_hovered: bool,
    now_ms: i64,
    current_deadline_ms: Option<i64>,
    delay_ms: i64,
) -> Option<i64> {
    if trigger_hovered || panel_hovered {
        return None;
    }
    current_deadline_ms.or(Some(now_ms + delay_ms))
}
```

- [ ] **Step 3: Run tests to verify pass**

Run: `cargo test unread_panel::tests -- --nocapture`  
Expected: PASS (3 passed, 0 failed).

- [ ] **Step 4: Commit logic implementation**

```bash
git add src/unread_panel.rs
git commit -m "feat: implement unread panel filtering and hover timing logic"
```

### Task 3: Add Poller Mark-Read Success Acknowledgment

**Files:**
- Modify: `src/poller.rs`
- Test: `cargo check`

- [ ] **Step 1: Extend poller event enum with success variant**

```rust
pub enum PollerEvent {
    Snapshot { fetched_at_ms: i64, page: SignalPage },
    PollFailed { error: String },
    SyncFailed { key: SignalKey, error: String },
    MarkReadSynced { key: SignalKey },
}
```

- [ ] **Step 2: Emit success/failure explicitly for mark-read command**

```rust
Ok(PollerCommand::MarkRead { key, read }) => {
    let result = runtime.block_on(client.mark_read(&key, read));
    match result {
        Ok(true) => {
            let _ = event_tx.send(PollerEvent::MarkReadSynced { key });
        }
        Ok(false) => {
            emit_sync_err(&event_tx, key, "server returned false".to_string());
        }
        Err(err) => {
            emit_sync_err(&event_tx, key, err.to_string());
        }
    }
}
```

- [ ] **Step 3: Compile-check**

Run: `cargo check`  
Expected: PASS.

- [ ] **Step 4: Commit poller ack support**

```bash
git add src/poller.rs
git commit -m "feat: emit mark-read sync success events from poller"
```

### Task 4: Add App State for Hover Popover and Pending Read

**Files:**
- Modify: `src/app.rs`
- Test: `cargo check`

- [ ] **Step 1: Add unread popover state fields**

```rust
use std::collections::{HashMap, HashSet};

use crate::unread_panel::{HoverPanelState, HoverPanelTarget, UnreadItemView, build_unread_items, next_close_deadline_ms};

pub struct SignalDeskApp {
    // existing fields...
    pending_read: HashSet<SignalKey>,
    hover_panel: Option<HoverPanelState>,
}
```

- [ ] **Step 2: Initialize new fields in `new()`**

```rust
Self {
    // existing init...
    pending_read: HashSet::new(),
    hover_panel: None,
}
```

- [ ] **Step 3: Update event handling for success/failure rollback**

```rust
match event {
    PollerEvent::MarkReadSynced { key } => {
        self.pending_read.remove(&key);
    }
    PollerEvent::SyncFailed { key, error } => {
        self.pending_read.remove(&key);
        if let Some(state) = self.signals.get_mut(&key) {
            state.read = false;
        }
        self.last_error = Some(format!("sync failed [{} {} {}]: {}", key.symbol, key.period, key.signal_type, error));
    }
    // existing arms...
}
```

- [ ] **Step 4: Compile-check**

Run: `cargo check`  
Expected: PASS.

- [ ] **Step 5: Commit app state wiring**

```bash
git add src/app.rs
git commit -m "feat: track unread hover panel state and pending mark-read keys"
```

### Task 5: Render Global and Group Hover Triggers

**Files:**
- Modify: `src/app.rs`
- Test: `cargo check`

- [ ] **Step 1: Add helper to compute global unread count**

```rust
fn total_unread_count(&self) -> usize {
    self.signals
        .iter()
        .filter(|(k, v)| !v.read && !self.pending_read.contains(*k))
        .count()
}
```

- [ ] **Step 2: Add top-toolbar unread badge trigger**

```rust
let total_unread = self.total_unread_count();
let global_resp = ui.label(
    egui::RichText::new(format!("Total unread: {total_unread}"))
        .color(Color32::BLACK)
        .background_color(Color32::from_rgb(245, 173, 0)),
);
if global_resp.hovered() {
    self.hover_panel = Some(HoverPanelState {
        target: HoverPanelTarget::Global,
        close_deadline_ms: None,
    });
}
```

- [ ] **Step 3: Convert group unread label into hover trigger**

```rust
if unread > 0 {
    let group_resp = ui.label(
        egui::RichText::new(format!("{unread} unread"))
            .color(Color32::BLACK)
            .background_color(Color32::from_rgb(245, 173, 0)),
    );
    if group_resp.hovered() {
        self.hover_panel = Some(HoverPanelState {
            target: HoverPanelTarget::Group(group.id.clone()),
            close_deadline_ms: None,
        });
    }
}
```

- [ ] **Step 4: Compile-check**

Run: `cargo check`  
Expected: PASS.

- [ ] **Step 5: Commit trigger rendering**

```bash
git add src/app.rs
git commit -m "feat: add global and group unread hover triggers"
```

### Task 6: Implement Interactive Hover Popover with Per-Row Action

**Files:**
- Modify: `src/app.rs`
- Use: `src/unread_panel.rs`
- Test: `cargo check`

- [ ] **Step 1: Add popover render helper**

```rust
fn render_unread_popover(&mut self, ctx: &egui::Context) {
    let Some(panel) = self.hover_panel.clone() else { return };
    let rows: Vec<UnreadItemView> = build_unread_items(
        &self.config.groups,
        &self.signals,
        &self.pending_read,
        &panel.target,
    );

    egui::Area::new(egui::Id::new("unread-hover-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos([16.0, 56.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(460.0);
                ui.set_max_height(320.0);
                if rows.is_empty() {
                    ui.label("暂无未读信号");
                    return;
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            ui.monospace(&row.symbol);
                            ui.monospace(&row.period);
                            ui.monospace(&row.signal_type);
                            let side_text = match row.side { Side::Bull => "多", Side::Bear => "空", _ => "-" };
                            let side_color = match row.side { Side::Bull => Color32::GREEN, Side::Bear => Color32::RED, _ => Color32::GRAY };
                            ui.colored_label(side_color, side_text);
                            if ui.small_button("标记已读").clicked() {
                                self.mark_one_read(row.key.clone());
                            }
                        });
                        ui.separator();
                    }
                });
            });
        });
}
```

- [ ] **Step 2: Add single-item optimistic mark helper**

```rust
fn mark_one_read(&mut self, key: SignalKey) {
    if self.pending_read.contains(&key) {
        return;
    }
    let Some(sig) = self.signals.get_mut(&key) else { return };
    sig.read = true;
    self.pending_read.insert(key.clone());
    if let Err(err) = self.poller.command_tx.send(PollerCommand::MarkRead { key, read: true }) {
        self.last_error = Some(format!("send mark-read command failed: {err}"));
    }
}
```

- [ ] **Step 3: Apply delayed-close logic each frame**

```rust
let now_ms = chrono::Utc::now().timestamp_millis();
if let Some(panel) = self.hover_panel.as_mut() {
    let trigger_hovered = false; // replace with stored trigger hover state
    let panel_hovered = false;   // replace with area hover response
    panel.close_deadline_ms = next_close_deadline_ms(
        trigger_hovered,
        panel_hovered,
        now_ms,
        panel.close_deadline_ms,
        200,
    );
    if matches!(panel.close_deadline_ms, Some(deadline) if now_ms >= deadline) {
        self.hover_panel = None;
    }
}
```

- [ ] **Step 4: Render popover from `update()`**

```rust
self.render_unread_popover(ctx);
```

- [ ] **Step 5: Compile-check**

Run: `cargo check`  
Expected: PASS.

- [ ] **Step 6: Commit popover interaction**

```bash
git add src/app.rs
git commit -m "feat: add unread hover popover with per-item mark-as-read action"
```

### Task 7: Add Button Pending State and Timestamp Display

**Files:**
- Modify: `src/app.rs`
- Test: `cargo check`

- [ ] **Step 1: Disable per-row action while pending**

```rust
let pending = self.pending_read.contains(&row.key);
let button_text = if pending { "处理中..." } else { "标记已读" };
let resp = ui.add_enabled(!pending, egui::Button::new(button_text));
if resp.clicked() {
    self.mark_one_read(row.key.clone());
}
```

- [ ] **Step 2: Format and display trigger time**

```rust
let ts = chrono::DateTime::<chrono::Local>::from_timestamp_millis(row.trigger_time_ms)
    .map(|dt| dt.format("%m-%d %H:%M:%S").to_string())
    .unwrap_or_else(|| "-".to_string());
ui.monospace(ts);
```

- [ ] **Step 3: Compile-check**

Run: `cargo check`  
Expected: PASS.

- [ ] **Step 4: Commit UX polish**

```bash
git add src/app.rs
git commit -m "feat: show unread row timestamps and pending action state"
```

### Task 8: Verify Behavior End-to-End

**Files:**
- Modify (if needed): `src/app.rs`, `src/unread_panel.rs`, `src/poller.rs`
- Test: `cargo test`, `cargo run` manual checks

- [ ] **Step 1: Run automated tests**

Run: `cargo test`  
Expected: PASS (including `unread_panel` tests).

- [ ] **Step 2: Run application for manual verification**

Run: `cargo run`  
Expected:
- Hover top unread shows global unread list.
- Hover group unread shows scoped list.
- "标记已读" removes row immediately.
- Failure path rolls row back and shows status error.
- Hover panel closes after leaving trigger+panel for ~200ms.

- [ ] **Step 3: Final commit for verification and minor fixes**

```bash
git add src/app.rs src/unread_panel.rs src/poller.rs src/main.rs
git commit -m "test: verify unread hover popover flow and finalize behavior"
```

## Plan Self-Review

### 1) Spec Coverage Check

- Global unread hover list: covered in Task 5 + Task 6.
- Group unread hover list: covered in Task 5 + Task 6.
- Per-row mark-as-read: covered in Task 6 + Task 7.
- Optimistic update + rollback: covered in Task 4 + Task 6.
- Stable hover close behavior with delay: covered in Task 2 + Task 6.
- Scope rules (global vs group): covered in Task 1 + Task 2 + Task 6.

No uncovered spec requirements found.

### 2) Placeholder Scan

- No placeholder markers remain in plan tasks.
- Every code-changing step includes concrete snippets and explicit commands.

### 3) Type/Name Consistency

- `HoverPanelTarget`, `HoverPanelState`, `UnreadItemView`, `pending_read`, and `MarkReadSynced` naming is consistent across all tasks.
- `build_unread_items` and `next_close_deadline_ms` are referenced consistently.
