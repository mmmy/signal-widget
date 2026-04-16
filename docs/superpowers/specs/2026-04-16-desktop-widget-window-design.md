# Desktop Widget Window Design

Date: 2026-04-16
Status: Draft for user review

## Summary

Add a new desktop floating widget as a standalone native window that is a peer to the main window rather than a mode inside it. The widget is a draggable circular button that only displays:

- current connection status
- current unread count

The system tray will gain a widget show/hide control. Hiding the widget must stop its UI refresh work so the widget itself does not consume CPU for repainting, while the shared app runtime continues polling and maintaining fresh state in the background. Showing the widget again should immediately render the latest snapshot without any extra fetch step.

This design explicitly keeps the first release display-only. It reserves extension points for future widget interactions such as click-to-open, context menus, unread previews, and mark-as-read actions without forcing another architecture rewrite.

## Goals

1. Add a widget window that is truly independent from the main window.
2. Reuse the existing runtime, polling, unread, and connection-state pipeline.
3. Keep the first release intentionally small: display-only, draggable, tray-controlled.
4. Ensure widget hide/show semantics are fast and stable.
5. Keep the architecture extensible for future widget interactions.

## Non-Goals

1. No single-window `float_mode` implementation.
2. No extra backend API calls for the widget.
3. No widget click action, right-click menu, unread preview, or mark-read behavior in v1.
4. No notification-window implementation in this task.
5. No separate widget process or IPC channel.
6. No shaped native hit-test region requirement in v1.

## Confirmed Product Decisions

The user confirmed the following:

1. The widget must be a fully independent window and may coexist with the main window.
2. The first version is display-only.
3. Tray control is `show/hide`, not create/destroy or enable/disable.
4. When hidden, the widget should not repaint or consume CPU for UI work.
5. The app runtime continues polling and maintaining fresh state while the widget is hidden.
6. The widget displays new messages as an unread number, not as animations or expanded detail.

## Why The Existing Single-Window Floating Mode Is Not Enough

The repository already contains a draft `float_mode` design that collapses the main window into a floating ball. That approach is lower risk for a single-window app, but it fails the core product requirement here: the widget must be a peer window, not a visual mode switch inside the main window.

Using the old approach would create immediate follow-up problems:

1. The main window and widget could not coexist.
2. Future widget-only interactions would remain coupled to main-window lifecycle code.
3. Tray control semantics would be ambiguous because it would be toggling a mode rather than controlling an actual peer window.

This task therefore adopts the multi-window direction already outlined by the repository's runtime-centric shell notes rather than the earlier `float_mode` draft.

## Recommended Architecture

### Option A: Independent widget window with shared runtime (recommended)

- Keep one shared runtime as the single source of truth.
- Add a second window adapter for the widget.
- Add a thin window manager/shell layer for main-window and widget-window show/hide control.
- Let the tray send intent commands rather than directly controlling native windows.

Pros:

1. Matches the product requirement exactly.
2. Keeps all business state shared and consistent.
3. Leaves clean extension points for future widget interactions.
4. Avoids later rework from a fake single-window shortcut.

Cons:

1. Requires a small amount of window infrastructure work first.
2. Requires runtime event delivery to support more than one UI consumer.

### Option B: Reuse the existing main window and add widget mode

- Keep only one native window.
- Switch between full UI and circular widget UI.

Pros:

1. Lowest implementation effort right now.

Cons:

1. Violates the peer-window requirement.
2. Creates future migration work when widget interactions expand.

### Option C: Separate widget process

- Run widget as a separate executable or subprocess.
- Sync state through IPC.

Pros:

1. Very strong isolation between surfaces.

Cons:

1. Far more complexity than this product needs.
2. Introduces avoidable lifecycle, config, and synchronization risk.

Decision: Option A.

## Target Architecture

```text
                +----------------------+
                |    Main Window       |
                |      Adapter         |
                +----------+-----------+
                           |
                +----------v-----------+
                |    Shared Runtime    |
                | poll + unread + cfg  |
                | connection summary   |
                +----+------------+----+
                     |            |
          snapshot/event fanout   | shell/window intents
                     |            v
                +----v------------+----+
                |   Window Manager      |
                | main + widget control |
                +----+------------+----+
                     |            |
                     v            v
                +---------+   +---------+
                | Tray    |   | Widget  |
                | Adapter |   | Adapter |
                +---------+   +---------+
```

The runtime remains the only owner of real app state. The widget adapter remains a thin view layer. Native show/hide concerns move out of the main window into a shell/window-management layer so future windows can be added without repeating lifecycle logic.

## Responsibilities By Layer

### Runtime

The runtime remains responsible for:

1. polling lifecycle
2. unread calculation
3. optimistic read state and failure recovery
4. connection health summary
5. UI configuration persistence
6. application-level window intent decisions

The runtime must not become widget-specific. The widget should consume the same shared snapshot the main window uses, reduced into a smaller widget-facing display model.

### Window Manager / Shell

This layer becomes responsible for:

1. registering window controllers for `Main` and `Widget`
2. showing and hiding either window independently
3. remembering whether the widget is currently visible
4. restoring widget visibility and position on startup
5. isolating native-window commands from any specific adapter update loop

This is the key extensibility boundary. Future widget interactions should not need to own native lifecycle logic themselves.

### Main Window Adapter

The main window continues to:

1. render the full application view
2. translate main-window UI input into app commands

It should no longer be the unique consumer of runtime UI events, and it should not remain the only place where window actions are executed.

### Widget Adapter

The widget adapter in v1 only:

1. renders the circular widget UI
2. receives current snapshot data
3. maps snapshot data into `connection state + unread text`
4. handles drag-to-move behavior
5. persists drag position through configuration or shell state

It does not perform business logic, polling, or unread derivation on its own.

### Tray Adapter

The tray adapter becomes responsible for:

1. show main window
2. exit application
3. show widget
4. hide widget

The tray must no longer directly own all native window behavior. It should emit intent commands that are routed through the same architecture as every other surface.

## Contract Changes

The existing contracts already include `AdapterId::FloatingWidget`, which is the right direction. This task extends the contract shape so widget visibility is a first-class concept.

Recommended additions:

```rust
pub enum WindowId {
    Main,
    Widget,
}

pub enum AppCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    RequestShowMainWindow,
    RequestCloseMainWindow,
    RequestExitApp,
    RequestShowWidget,
    RequestHideWidget,
    SaveWidgetVisibility { visible: bool },
    SaveWidgetPosition { x: f32, y: f32 },
}

pub enum ShellCommand {
    ShowWindow(WindowId),
    HideWindow(WindowId),
    FocusWindow(WindowId),
    ExitProcess,
}
```

This design does not require every command to be implemented immediately. The important decision is that widget visibility and movement are modeled as app-level intents rather than as ad hoc direct native calls.

## Runtime Event Delivery

The current code path uses a single event receiver consumed by the main window. That is not sufficient for a real peer widget window.

The runtime event path must evolve into a shape that allows more than one consumer:

1. broadcast/subscribe fanout, or
2. shared latest snapshot plus per-adapter subscription for delta events

For this task, the minimal acceptable outcome is:

1. main window can receive updates
2. widget can receive the same updates
3. hiding the widget does not require stopping the runtime

This design deliberately leaves the exact mechanism open as an implementation choice, but the interface must support multiple adapters as a permanent capability.

## Widget UI Design

### Visual Structure

The widget is visually a circular floating button with:

1. unread count centered in the circle
2. a small connection-status dot near the edge

It intentionally omits labels, timestamps, and secondary details to keep the widget legible at small size.

### State Mapping

The widget reads from shared runtime snapshot state and maps it to:

- green dot: last poll healthy
- red dot: last poll failed
- gray dot: no poll result yet
- centered number: total unread count

Display `0` when there are no unread messages. A stable numeric center is preferred over a disappearing badge because it reduces jitter and visual ambiguity.

### Input Model

Version 1 behavior:

1. left-button drag moves the widget window
2. no click action
3. no right-click action
4. no hover expansion
5. no edge snapping

This keeps the first release focused while preserving compatibility with future click and context-menu extensions.

### Window Shape Constraint

The first version targets a circular visual, not a guaranteed shaped native hit-test region. A frameless window with a circular drawn surface is acceptable even if the native window still uses a rectangular bounding box internally. This removes a large amount of Windows-specific complexity from v1 while preserving the same user-facing design language.

## Tray Behavior

Tray behavior in the target design:

1. `显示主窗口`
2. `显示小组件` or `隐藏小组件` depending on current widget visibility
3. `退出`

Tray actions should become intent commands, not direct `show()` or `exit(0)` side effects. This keeps tray behavior aligned with the same control flow as future widget-related commands.

## Visibility And Performance Model

The widget visibility model is intentionally simple:

1. Hidden means the widget native window is not shown.
2. Hidden does not stop polling.
3. Hidden does not trigger widget UI refresh work.
4. Showing the widget again renders from the latest shared state immediately.

This satisfies the product need for low CPU overhead without inventing a second state machine for background behavior.

In practical terms, the performance contract for v1 is:

1. the runtime continues running as the app core
2. the widget adapter should not request repaints when invisible
3. widget-specific event work should be dormant while hidden

The design does not require suspending the entire runtime or poller, because that would conflict with the requirement that the app remain current in the background.

## Configuration Additions

Add widget-specific configuration under `UiConfig` or a nested widget section. Recommended fields:

```rust
pub struct WidgetConfig {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}
```

Version 1 only strictly needs:

1. `visible`
2. `position`
3. optional `size`

Even if the current product only needs show/hide, storing position now avoids losing drag state and prevents later migration churn when interactions expand.

## File Structure Direction

Recommended structure after this design:

```text
src/
  core/
    contract.rs
    runtime.rs
    state.rs
  shell/
    mod.rs
    window_manager.rs
    window_controller.rs
  adapters/
    tray/mod.rs
    main_window/...
    floating_widget/
      mod.rs
      app.rs
      view.rs
      state.rs
```

The widget adapter can stay small in v1, but separating `app.rs` and `view.rs` is worth doing because future widget interactions will quickly outgrow a single flat file.

## Migration Plan Shape

### Phase 1: Multi-window shell foundation

1. Introduce `WindowId` and generic window control boundaries.
2. Stop depending on the main window as the only executor of window actions.
3. Add widget visibility intent handling.

### Phase 2: Runtime fanout for multiple adapters

1. Allow both main window and widget to observe runtime state.
2. Ensure hidden widget does not request repaint work.

### Phase 3: Widget adapter implementation

1. Add the independent widget window.
2. Render circular display-only UI.
3. Support drag-to-move and position persistence.

### Phase 4: Tray integration

1. Add widget show/hide item.
2. Reflect current widget visibility in menu labeling.

## Error Handling

1. If widget window creation fails, the main app and tray continue functioning.
2. If widget position persistence fails, keep the widget usable and fall back to the last in-memory position.
3. If runtime delivery to widget fails temporarily, the widget may display stale data but must not interfere with polling or the main window.
4. If the tray cannot reflect widget visibility immediately, the runtime-visible state remains the source of truth.

## Testing Strategy

### Core tests

1. widget visibility commands map to the correct shell actions
2. runtime state snapshots remain valid with more than one adapter consumer
3. widget hide does not affect poller behavior

### Shell tests

1. `ShowWindow(Widget)` shows only the widget window
2. `HideWindow(Widget)` hides only the widget window
3. main-window close behavior remains independent from widget visibility

### Adapter tests

1. widget display model maps poll state to the correct color state
2. widget display model maps unread counts to the correct centered text
3. drag events update widget position persistence commands correctly
4. hidden widget does not request repaint loops

### Manual verification

1. Launch app with widget visible and confirm main window + widget coexist.
2. Drag widget and restart app; confirm position persists.
3. Hide widget from tray; confirm widget disappears while background polling continues.
4. Show widget from tray; confirm latest unread count and connection state appear immediately.
5. Confirm widget itself does not visibly animate or consume refresh work while hidden.

## Risks And Mitigations

### Risk: window lifecycle remains coupled to main window update

Mitigation:

Move native window execution into shell/window-manager boundaries before deepening widget UI work.

### Risk: widget becomes another place that owns business logic

Mitigation:

Keep the widget adapter restricted to display-model mapping only. No unread derivation or polling logic belongs there.

### Risk: tray behavior diverges from runtime truth

Mitigation:

Treat tray actions as intent commands and derive menu state from shared widget visibility state.

### Risk: future widget interactions require a rewrite

Mitigation:

Reserve command and adapter boundaries now, even if v1 only uses a subset of them.

## Acceptance Criteria

1. A widget window exists independently from the main window.
2. The widget can be shown and hidden from the system tray.
3. The widget displays only connection status and unread count.
4. The widget can be dragged freely on the desktop.
5. Hiding the widget stops widget UI refresh work while polling continues in the background.
6. Showing the widget again reflects the latest runtime state without extra fetches.
7. The architecture leaves clean extension points for future widget interactions.
