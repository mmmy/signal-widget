# Floating Mode Design

Date: 2026-04-15
Status: Draft for user review

## Summary
Add a new `float_mode` to Signal Desk. In this mode, the window collapses into a small floating ball that shows only:
- Connection health status (green/red/gray dot)
- Total unread count

If `edge_mode` is enabled at the same time, the floating ball becomes a half-ball visual to sit against the screen edge.

Interaction in floating mode:
- Left mouse: drag only (no click action)
- Right mouse: open context menu

Entry points:
- Toolbar toggle
- Right-click menu toggle

## Goals
1. Provide a minimal always-on-screen monitoring mode.
2. Keep cognitive load low: only health and unread are shown.
3. Preserve existing polling, unread, notification, and sound behavior.
4. Keep switching between normal mode and floating mode fast and stable.

## Non-Goals
1. No system-level custom window shape APIs in this phase.
2. No additional notification logic changes.
3. No new backend API calls.

## User Flows
### Flow A: Enter floating mode from toolbar
1. User enables `浮动模式` in toolbar.
2. Window switches to compact floating view immediately.
3. User drags ball with left mouse.
4. User right-clicks to open menu and can exit floating mode.

### Flow B: Enter/exit floating mode from right-click menu
1. User right-clicks floating ball.
2. Menu offers `进入/退出浮动模式`.
3. Mode toggles and is persisted to config.

### Flow C: Floating + edge mode
1. User turns on `贴边模式` while `浮动模式` is on (or vice versa).
2. Floating ball uses half-ball visual and compact edge width.

## Approaches Considered
### Approach 1: Single-window mode switch (recommended)
- Reuse current window and state tree.
- Render either full UI or floating UI based on `float_mode`.
- Resize window per mode.

Pros:
- Lowest implementation risk.
- Reuses existing polling/unread state and persistence.
- Minimal architecture change.

Cons:
- Half-ball is visual clipping in app content, not native shaped window.

### Approach 2: Dual-window architecture
- Separate full panel window and floating window.

Pros:
- Strong visual separation.

Cons:
- More complexity (focus, z-order, synchronization).
- Higher regression risk.

### Approach 3: Tray-first control model
- Put most controls in tray, keep floating UI ultra-minimal.

Pros:
- Minimal visual noise.

Cons:
- Worse discoverability for this project’s current workflow.

Decision: Approach 1.

## Detailed Design
### Config Model
Add to `UiConfig`:
- `float_mode: bool` (default `false`)
- Optional `float_size: f32` (if not added now, use a fixed constant)

Persist through existing `save_config()` flow.

### Rendering Model
In `SignalDeskApp::update()`:
- If `float_mode == false`: keep current top/center/bottom layout.
- If `float_mode == true`: render floating view only:
  - Health dot (`last_poll_ok` mapping)
  - Total unread number (`total_unread_count()`)

### Window Geometry
Update `apply_window_mode()` sizing logic:
- Normal mode:
  - Existing size behavior remains unchanged.
- Floating mode:
  - Default size e.g. `56 x 56`.
- Floating + edge mode:
  - Width reduced (e.g. half of float size) to create half-ball visual.

### Floating Visual
- Base shape: circle-like panel with strong contrast.
- Health dot color mapping:
  - Green: last poll success
  - Red: last poll failed (current threshold behavior preserved)
  - Gray: no poll result yet
- Unread count:
  - Centered and readable at small size.

### Input Behavior
- Left mouse drag only:
  - No click-to-expand action.
- Right mouse context menu entries:
  - `进入/退出浮动模式`
  - `贴边模式`
  - `窗口置顶`
  - `立即轮询`

Both toolbar and context menu must call shared state mutation helpers to avoid logic drift.

### Data Flow and State Ownership
- No new backend calls.
- No changes to poller thread protocol.
- Floating UI reads existing state only:
  - `last_poll_ok`
  - `total_unread_count()`
  - existing config flags

### Error Handling and Fallback
1. If floating view render path fails, fall back to normal mode in-memory and keep app responsive.
2. If context-menu action persistence fails, show existing `last_error` mechanism.
3. Maintain event-driven repaint strategy already in place.

## Testing Plan
### Unit Tests
1. Window size selection by mode:
- normal
- floating
- floating + edge

2. Mode toggles update config correctly from:
- toolbar action
- context-menu action

3. Health/unread mapping in floating view logic.

### Manual Verification
1. Toggle floating mode from toolbar and from right-click menu.
2. Left drag moves floating window; left click performs no action.
3. Right-click menu actions work and persist.
4. Half-ball visual appears when floating + edge mode.
5. Restart app and verify mode persistence.

## Rollout Sequence
1. Add config field(s) and persistence.
2. Add rendering branch and floating UI component.
3. Add context menu actions.
4. Add tests and run verification.
5. Manual QA checklist pass.

## Acceptance Criteria
1. Floating mode can be toggled from toolbar and right-click menu.
2. Floating mode shows only health + unread.
3. Left click has no action; left drag moves window.
4. Right-click opens context menu with required actions.
5. Floating + edge mode displays half-ball visual.
6. Settings persist across restart.
