# Floating Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a floating mode that collapses the app into a draggable ball showing only poll health and total unread, with half-ball rendering when edge mode is enabled.

**Architecture:** Keep a single window and branch UI rendering by `ui.float_mode`. Reuse existing poll/unread state, add a small pure helper module for floating geometry/health mapping, and route toolbar + right-click menu through shared action handlers to keep behavior consistent.

**Tech Stack:** Rust, eframe/egui 0.29, serde config persistence, existing in-file unit tests (`#[cfg(test)]`).

---

## File Structure

- Create: `src/float_mode.rs`
  - Floating-only pure logic: window size calculation, health visual mapping, right-click menu action enum.
- Modify: `src/main.rs`
  - Register the new module.
- Modify: `src/config.rs`
  - Add persisted `ui.float_mode` flag with default.
- Modify: `src/app.rs`
  - Render branch for floating mode, ball UI drawing, drag-only primary input, right-click menu handling, shared menu action handler, toolbar entry.
- Test: `src/float_mode.rs` and `src/app.rs` unit tests.

### Task 1: Add Pure Floating Helpers (Geometry + Health + Actions)

**Files:**
- Create: `src/float_mode.rs`
- Modify: `src/main.rs`
- Test: `src/float_mode.rs`

- [ ] **Step 1: Write the failing tests in `src/float_mode.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_window_size_uses_ball_size_in_float_mode() {
        assert_eq!(compute_window_size(true, false, 240.0), [56.0, 56.0]);
    }

    #[test]
    fn compute_window_size_uses_half_ball_when_float_and_edge() {
        assert_eq!(compute_window_size(true, true, 240.0), [28.0, 56.0]);
    }

    #[test]
    fn health_visual_maps_states() {
        assert_eq!(health_visual(Some(true)).1, "轮询正常");
        assert_eq!(health_visual(Some(false)).1, "上次轮询失败");
        assert_eq!(health_visual(None).1, "尚未轮询");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test compute_window_size_uses_ball_size_in_float_mode -- --nocapture`
Expected: FAIL with unresolved functions/types in `src/float_mode.rs`.

- [ ] **Step 3: Add minimal helper implementation**

```rust
use eframe::egui::Color32;

pub const FLOAT_DIAMETER: f32 = 56.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatMenuAction {
    ToggleFloatMode,
    ToggleEdgeMode,
    ToggleAlwaysOnTop,
    ForcePoll,
}

pub fn compute_window_size(float_mode: bool, edge_mode: bool, edge_width: f32) -> [f32; 2] {
    if float_mode {
        if edge_mode {
            [FLOAT_DIAMETER / 2.0, FLOAT_DIAMETER]
        } else {
            [FLOAT_DIAMETER, FLOAT_DIAMETER]
        }
    } else if edge_mode {
        [edge_width.clamp(120.0, 600.0), 760.0]
    } else {
        [540.0, 760.0]
    }
}

pub fn health_visual(last_poll_ok: Option<bool>) -> (Color32, &'static str) {
    match last_poll_ok {
        Some(true) => (Color32::from_rgb(48, 181, 122), "轮询正常"),
        Some(false) => (Color32::from_rgb(214, 84, 105), "上次轮询失败"),
        None => (Color32::LIGHT_GRAY, "尚未轮询"),
    }
}
```

- [ ] **Step 4: Register module and verify tests pass**

Code change in `src/main.rs`:

```rust
mod float_mode;
```

Run: `cargo test health_visual_maps_states -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/float_mode.rs src/main.rs
git commit -m "feat: add floating mode helper module"
```

### Task 2: Persist `float_mode` in Config

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (new unit test)

- [ ] **Step 1: Write failing config persistence test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_default_has_float_mode_disabled() {
        let cfg = UiConfig::default();
        assert!(!cfg.float_mode);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui_default_has_float_mode_disabled -- --nocapture`
Expected: FAIL because `float_mode` does not exist yet.

- [ ] **Step 3: Implement minimal config field**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub edge_mode: bool,
    pub edge_width: f32,
    pub always_on_top: bool,
    pub notifications: bool,
    pub sound: bool,
    pub float_mode: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            edge_mode: false,
            edge_width: 240.0,
            always_on_top: true,
            notifications: true,
            sound: false,
            float_mode: false,
        }
    }
}
```

- [ ] **Step 4: Verify test passes**

Run: `cargo test ui_default_has_float_mode_disabled -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: persist float mode in ui config"
```

### Task 3: Add Shared Floating Menu Reducer + Geometry Wiring

**Files:**
- Modify: `src/float_mode.rs`
- Modify: `src/app.rs`
- Test: `src/float_mode.rs`

- [ ] **Step 1: Write failing reducer tests in `src/float_mode.rs`**

```rust
#[test]
fn apply_menu_action_toggles_ui_flags() {
    let mut ui = crate::config::UiConfig::default();

    let side_effect = apply_menu_action(&mut ui, FloatMenuAction::ToggleFloatMode);
    assert!(side_effect.is_none());
    assert!(ui.float_mode);

    let _ = apply_menu_action(&mut ui, FloatMenuAction::ToggleEdgeMode);
    assert!(ui.edge_mode);

    let _ = apply_menu_action(&mut ui, FloatMenuAction::ToggleAlwaysOnTop);
    assert!(!ui.always_on_top);
}

#[test]
fn apply_menu_action_returns_force_poll_side_effect() {
    let mut ui = crate::config::UiConfig::default();
    let side_effect = apply_menu_action(&mut ui, FloatMenuAction::ForcePoll);
    assert_eq!(side_effect, Some(FloatSideEffect::ForcePoll));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test apply_menu_action_returns_force_poll_side_effect -- --nocapture`
Expected: FAIL because reducer/side-effect types are not implemented yet.

- [ ] **Step 3: Implement reducer + side effect + app wiring**

In `src/float_mode.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSideEffect {
    ForcePoll,
}

pub fn apply_menu_action(
    ui: &mut crate::config::UiConfig,
    action: FloatMenuAction,
) -> Option<FloatSideEffect> {
    match action {
        FloatMenuAction::ToggleFloatMode => {
            ui.float_mode = !ui.float_mode;
            None
        }
        FloatMenuAction::ToggleEdgeMode => {
            ui.edge_mode = !ui.edge_mode;
            None
        }
        FloatMenuAction::ToggleAlwaysOnTop => {
            ui.always_on_top = !ui.always_on_top;
            None
        }
        FloatMenuAction::ForcePoll => Some(FloatSideEffect::ForcePoll),
    }
}
```

In `src/app.rs`, update shared action handler to call reducer:

```rust
fn apply_float_menu_action(&mut self, action: crate::float_mode::FloatMenuAction) {
    let side_effect = crate::float_mode::apply_menu_action(&mut self.config.ui, action);
    self.save_config();
    if matches!(side_effect, Some(crate::float_mode::FloatSideEffect::ForcePoll)) {
        let _ = self.poller.command_tx.send(PollerCommand::ForcePoll);
    }
}
```

Also wire geometry through helper:

```rust
let size = crate::float_mode::compute_window_size(
    self.config.ui.float_mode,
    self.config.ui.edge_mode,
    self.config.ui.edge_width,
);
ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
```

- [ ] **Step 4: Verify tests pass**

Run: `cargo test apply_menu_action_toggles_ui_flags -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/float_mode.rs src/app.rs
git commit -m "refactor: add floating menu reducer and side effects"
```

### Task 4: Implement Floating View Rendering (Drag + Right-Click Menu)

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs`

- [ ] **Step 1: Write failing tests for floating-only render branch helpers**

```rust
#[test]
fn floating_poll_indicator_uses_health_visual_mapping() {
    let (_, text_ok) = crate::float_mode::health_visual(Some(true));
    let (_, text_err) = crate::float_mode::health_visual(Some(false));
    assert_eq!(text_ok, "轮询正常");
    assert_eq!(text_err, "上次轮询失败");
}
```

- [ ] **Step 2: Run test to verify it fails/passes before branch wiring (RED check)**

Run: `cargo test floating_poll_indicator_uses_health_visual_mapping -- --nocapture`
Expected: If mapping helper not wired/imported yet in app tests, FAIL; after import, PASS.

- [ ] **Step 3: Add floating render path and right-click menu**

In `update()` early branch:

```rust
if self.config.ui.float_mode {
    self.hover_panel = None;
    self.hover_anchor = None;
    self.render_floating_mode(ctx, now_ms);
    if had_events {
        ctx.request_repaint();
    }
    return;
}
```

Add renderer method:

```rust
fn render_floating_mode(&mut self, ctx: &egui::Context, _now_ms: i64) {
    let unread = self.total_unread_count();
    let (health_color, health_text) = crate::float_mode::health_visual(self.last_poll_ok);

    egui::CentralPanel::default().show(ctx, |ui| {
        let desired = ui.available_size_before_wrap();
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());

        if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let painter = ui.painter_at(rect);
        let radius = rect.height() / 2.0;
        let center_x = if self.config.ui.edge_mode { rect.left() } else { rect.center().x };
        let center = egui::pos2(center_x, rect.center().y);

        painter.circle_filled(center, radius, Color32::from_rgb(38, 43, 54));
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            unread.to_string(),
            egui::FontId::proportional(18.0),
            Color32::WHITE,
        );

        let dot_center = egui::pos2(center.x, center.y - radius + 10.0);
        painter.circle_filled(dot_center, 4.0, health_color);

        response.clone().on_hover_text(health_text);

        let mut picked = None;
        response.context_menu(|ui| {
            if ui.button("退出浮动模式").clicked() {
                picked = Some(crate::float_mode::FloatMenuAction::ToggleFloatMode);
                ui.close_menu();
            }
            if ui.button("贴边模式").clicked() {
                picked = Some(crate::float_mode::FloatMenuAction::ToggleEdgeMode);
                ui.close_menu();
            }
            if ui.button("窗口置顶").clicked() {
                picked = Some(crate::float_mode::FloatMenuAction::ToggleAlwaysOnTop);
                ui.close_menu();
            }
            if ui.button("立即轮询").clicked() {
                picked = Some(crate::float_mode::FloatMenuAction::ForcePoll);
                ui.close_menu();
            }
        });

        if let Some(action) = picked {
            self.apply_float_menu_action(action);
        }
    });
}
```

- [ ] **Step 4: Verify behavior tests pass**

Run: `cargo test floating_poll_indicator_uses_health_visual_mapping -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add floating ball render branch with drag and context menu"
```

### Task 5: Add Toolbar Entry and Save Path Consistency

**Files:**
- Modify: `src/app.rs`
- Test: `src/float_mode.rs`

- [ ] **Step 1: Write failing test for reducer idempotence symmetry**

```rust
#[test]
fn toggle_float_mode_twice_returns_to_original_value() {
    let mut ui = crate::config::UiConfig::default();
    let before = ui.float_mode;
    let _ = apply_menu_action(&mut ui, FloatMenuAction::ToggleFloatMode);
    let _ = apply_menu_action(&mut ui, FloatMenuAction::ToggleFloatMode);
    assert_eq!(ui.float_mode, before);
}
```

- [ ] **Step 2: Run test to verify red/green transition**

Run: `cargo test toggle_float_mode_twice_returns_to_original_value -- --nocapture`
Expected: FAIL before test is added or reducer is complete, PASS after implementation.

- [ ] **Step 3: Add toolbar `浮动模式` checkbox using same config path**

In the top toolbar section:

```rust
let float_changed = ui
    .checkbox(&mut self.config.ui.float_mode, "浮动模式")
    .changed();

if edge_changed
    || top_changed
    || width_changed
    || notifications_changed
    || sound_changed
    || float_changed
{
    self.save_config();
}
```

- [ ] **Step 4: Verify compile and focused tests**

Run: `cargo check`
Expected: build success.

Run: `cargo test toggle_float_mode_twice_returns_to_original_value -- --nocapture`
Expected: PASS for new action tests.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add toolbar entry for floating mode"
```

### Task 6: Full Verification and Manual QA

**Files:**
- Modify: none (verification-only task)
- Test: existing unit tests and manual behavior checklist

- [ ] **Step 1: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: PASS, no failing tests.

- [ ] **Step 2: Run compile check for run-path parity**

Run: `cargo check`
Expected: PASS (prevents test-only branch regressions).

- [ ] **Step 3: Manual QA checklist**

1. Start app in normal mode, toggle `浮动模式` on.
2. Confirm only floating ball is visible.
3. Hold left mouse and drag: window moves, no click action is triggered.
4. Right-click ball and verify menu items appear.
5. Toggle `贴边模式` while floating: ball becomes half-ball visual.
6. Toggle `窗口置顶` from menu and verify behavior.
7. Trigger polling success/failure and verify health dot color.
8. Restart app and confirm `float_mode` persistence.

Expected: all checks pass.

- [ ] **Step 4: Final commit**

```bash
git add src/app.rs src/config.rs src/float_mode.rs src/main.rs
git commit -m "feat: add floating monitoring mode with half-ball edge behavior"
```

## Self-Review

### Spec Coverage
- Floating mode added and persisted: covered by Tasks 2, 5.
- Floating-only content (health + unread): covered by Task 4.
- Left drag only, right-click menu: covered by Task 4.
- Half-ball in edge mode: covered by Tasks 1 and 4.
- Dual entry points (toolbar + menu): covered by Tasks 4 and 5.

### Placeholder Scan
- No TBD/TODO placeholders.
- All code-changing steps include concrete code snippets.
- Every run step includes explicit command and expected outcome.

### Type/API Consistency
- Shared action enum: `crate::float_mode::FloatMenuAction` used consistently.
- Window size helper: `compute_window_size(float_mode, edge_mode, edge_width)` used consistently.
- Health mapping helper: `health_visual(last_poll_ok)` used consistently.
