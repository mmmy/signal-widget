# Widget Native Style Upgrade Design

Date: 2026-04-16
Status: Draft for user review

## Summary

Upgrade the existing desktop widget window on Windows so it behaves like a real floating ball instead of a square transparent viewport with a circle drawn inside it.

The target effect is:

1. visually circular
2. frameless
3. transparent outside the circle
4. click-through outside the circle
5. draggable anywhere inside the circle

This design keeps the existing widget runtime and viewport architecture in place. It adds a Windows-native enhancement layer on top of the current widget viewport rather than rewriting the widget as a fully custom Win32 window.

## Goals

1. Eliminate the current square/white-background look.
2. Make the widget feel like a real floating desktop orb on Windows.
3. Preserve the current display-only interaction model.
4. Keep the implementation extensible for future click zones or richer widget interactions.
5. Avoid throwing away the current `egui` widget adapter architecture.

## Non-Goals

1. No full rewrite of the widget as a pure Win32 rendering pipeline.
2. No new widget interactions beyond drag inside the circle.
3. No cross-platform parity guarantee for native hit-testing behavior.
4. No introduction of `UpdateLayeredWindow` unless the simpler approach proves insufficient.
5. No redesign of runtime or tray architecture in this task.

## Confirmed Product Decisions

The user confirmed:

1. This enhancement should target the existing widget implementation rather than force a full shell rewrite.
2. Windows-specific native enhancement is acceptable.
3. The entire circle should be draggable.
4. The desired result is the recommended hybrid approach:
   - transparent `egui` viewport for visuals
   - Win32-native hit testing for pointer behavior

## Why The Current Widget Looks Wrong

The current implementation draws a dark circle inside a normal `egui` viewport. That means the viewport itself still behaves like a regular rectangular native window. Even if decorations are removed, the user still perceives a square host surface around the circle.

This produces two problems:

1. the window still reads visually as a rectangular widget host
2. pointer behavior still belongs to the rectangle, not the circle

A prettier circle in `view.rs` alone cannot solve this. The visual layer and the native hit-test layer both need to change.

## Recommended Approaches

### Option A: Hybrid `egui` + Win32 native enhancement (recommended)

- Keep the current widget viewport architecture.
- Make the viewport transparent and frameless.
- Draw only the circle visually in `egui`.
- Use Win32 native APIs on the widget `HWND` to:
  - enable layered behavior
  - disable non-client rendering artifacts
  - override `WM_NCHITTEST`

Pros:

1. Preserves the current code structure.
2. Delivers the target effect with the least architectural churn.
3. Leaves room for future hit-test zones.

Cons:

1. Requires Windows-specific shell code.
2. Depends on reliably accessing the widget native handle.

### Option B: Pure `egui` visual improvement only

- Transparent viewport
- Better painting
- No custom Win32 hit testing

Pros:

1. Lowest implementation risk.

Cons:

1. Does not fully solve click-through outside the circle.
2. Still feels like a rectangle for input.

### Option C: Full custom Win32 widget window

- Replace `egui` widget viewport with a native Win32 top-level window
- Manually manage transparency, shape, hit testing, and painting

Pros:

1. Maximum native control.

Cons:

1. Much more complexity than the current task needs.
2. Breaks the current adapter structure earlier than necessary.

Decision: Option A.

## Target Layering Model

The implementation should be split across three layers:

### 1. Widget View Layer (`adapters/floating_widget/view.rs`)

Responsible for:

1. painting the dark orb
2. painting unread count
3. painting connection status dot
4. avoiding any opaque panel/frame styling

This layer answers: what the widget looks like.

### 2. Widget Viewport Layer (`adapters/floating_widget/app.rs`)

Responsible for:

1. creating the transparent, undecorated widget viewport
2. defining widget size and position
3. wiring drag initiation from inside the circle
4. invoking native enhancement installation once the viewport exists

This layer answers: how the widget is hosted in `egui`.

### 3. Windows Native Enhancement Layer (`shell/windows/widget_window.rs` or equivalent)

Responsible for:

1. obtaining the widget `HWND`
2. applying layered-window style configuration
3. disabling DWM non-client artifacts
4. installing a custom `WM_NCHITTEST` handler
5. returning `HTTRANSPARENT` outside the circle
6. returning `HTCAPTION` inside the circle

This layer answers: how the widget behaves as a real floating desktop window on Windows.

## Concrete Technical Plan

### Transparent Widget Viewport

At the viewport layer, configure the widget host with:

1. `ViewportBuilder::with_transparent(true)`
2. `ViewportBuilder::with_decorations(false)`
3. `ViewportBuilder::with_resizable(false)`
4. `ViewportBuilder::with_always_on_top()`

The widget app should also ensure its clear color/background is fully transparent so the only visible pixels are the orb itself and its content.

This is the fix for the current white/square host issue.

### Layered Window Style

Once the widget `HWND` is available, add Windows-native support for transparency by applying layered-window style behavior.

Recommended direction:

1. read existing extended window styles
2. add `WS_EX_LAYERED`
3. preserve unrelated existing flags

This enables the system to treat the widget window as a layered transparent window rather than a conventional opaque host.

This design intentionally does **not** use `WS_EX_TRANSPARENT` as the main strategy. That flag is better for whole-window pass-through overlays and would work against the requirement that the orb itself remain draggable.

### DWM Non-Client Rendering Control

To avoid leftover frame or caption artifacts, the native layer should apply DWM configuration for the widget window, specifically disabling non-client rendering where appropriate.

This is a polish layer, but it matters because a supposedly frameless floating orb still feels wrong if DWM applies stray frame/shadow behavior around it.

### `WM_NCHITTEST`

This is the most important behavioral piece.

The widget window should subclass or otherwise intercept its native message handling and implement custom `WM_NCHITTEST` logic:

1. compute the widget circle center from the window rectangle
2. compute the pointer distance from that center
3. if pointer distance is greater than radius:
   - return `HTTRANSPARENT`
4. if pointer distance is less than or equal to radius:
   - return `HTCAPTION`

Because the whole circle is draggable in v1, no further sub-zones are needed.

This gives exactly the desired interaction model:

1. outside the orb, pointer events pass through to underlying windows
2. inside the orb, the entire widget drags naturally

## Why `WS_EX_TRANSPARENT` Is Not The Main Answer

It is tempting to enable both `WS_EX_LAYERED` and `WS_EX_TRANSPARENT`, but that would make the whole widget window behave more like a passive overlay. That is the wrong trade-off for this product because the user still needs the orb itself to receive drag interaction.

The safer pattern is:

1. `WS_EX_LAYERED` for transparency support
2. `WM_NCHITTEST` for selective pass-through outside the circle

This keeps the input model precise and extensible.

## File Structure Direction

Recommended file additions/changes:

```text
src/
  adapters/
    floating_widget/
      app.rs
      view.rs
  shell/
    mod.rs
    window_controller.rs
    windows/
      mod.rs
      widget_window.rs
```

If a `shell/windows/` subtree does not yet exist, this task is a good reason to introduce one because the widget native enhancement is highly Windows-specific and should not be mixed into cross-platform controller code.

## Integration With Existing Branch

This design assumes the current `codex/desktop-widget-window` branch structure:

1. keep the existing widget viewport entrypoint
2. keep the existing runtime snapshot plumbing
3. keep the existing tray visibility behavior
4. change only the visual hosting and Windows-native behavior

This means the work can stay focused on style and pointer semantics rather than reopening the already-working runtime/tray foundation.

## Error Handling

1. If native widget enhancement fails, the widget should fall back to the plain transparent viewport version rather than crash the app.
2. If `HWND` acquisition fails, log the failure and keep the current widget visible.
3. If DWM configuration fails, continue with layered + hit-test behavior where possible.
4. If custom hit testing fails or is unavailable, degrade to the current full-rectangle interaction behavior rather than breaking the widget entirely.

## Testing Strategy

### Pure tests

1. circle radius helper correctness
2. hit-test helper math for inside/outside circle classification
3. widget display model still maps snapshot to unread/health correctly

### Shell tests

1. Windows helper computes `HTTRANSPARENT` for points outside the orb
2. Windows helper computes `HTCAPTION` for points inside the orb
3. style mutation preserves unrelated window style bits

### Manual verification

1. Launch widget and confirm the square/white host is gone.
2. Confirm only the circular orb is visually present.
3. Move the cursor just outside the orb and click; underlying app/window should receive the click.
4. Drag from anywhere inside the orb; widget should move.
5. Hide and show widget from tray; native enhancement should still apply after re-show.

## Risks And Mitigations

### Risk: viewport transparency is not sufficient on its own

Mitigation:

Treat transparent viewport as the visual baseline only. Use native layered + hit-test behavior for the full effect.

### Risk: widget `HWND` is difficult to access in deferred viewport flow

Mitigation:

Constrain the native enhancement API around “install once a handle is available.” If that path proves unstable, this becomes the clear escalation point for moving widget window control deeper into shell.

### Risk: Windows-specific code leaks into generic widget rendering

Mitigation:

Keep all Win32 APIs in a dedicated Windows-native helper module and keep `view.rs` purely visual.

### Risk: future click actions require redoing hit-testing

Mitigation:

Use a helper that computes hit-test zones from geometry now, even if v1 only has two outcomes: outside transparent, inside caption.

## Acceptance Criteria

1. On Windows, the widget no longer appears as a square/white host window.
2. The widget is visually a clean circular floating orb.
3. The widget is undecorated and does not show normal frame/title UI.
4. Pointer input outside the circle passes through to underlying windows.
5. Pointer input inside the circle drags the widget.
6. The implementation preserves the existing runtime/tray/widget architecture and leaves room for future widget interactions.
