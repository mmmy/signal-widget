# Runtime/Adapter Separation Validation

- [x] Binary consumes the library entrypoint instead of compiling a separate local module graph
  Evidence: [main.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/main.rs) now only calls `signal_desk_egui::run()`, and [lib.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/lib.rs) owns the bootstrap sequence.

- [x] Core contract/runtime/state modules exist under a shared library surface
  Evidence: [lib.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/lib.rs), [core](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/mod.rs), [runtime.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/runtime.rs), [state.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/state.rs).

- [x] Tray adapter mapping exists as a peer adapter module
  Evidence: [adapters/mod.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/adapters/mod.rs) and [adapters/tray/mod.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/adapters/tray/mod.rs).

- [x] Floating widget adapter command shell exists as a peer adapter module
  Evidence: [adapters/floating_widget/mod.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/adapters/floating_widget/mod.rs).

- [x] Poll request shaping moved out of `poller.rs` into a core service
  Evidence: [core/services/poller_service.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/services/poller_service.rs) and [poller.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/poller.rs).

- [x] Close policy moved into `core/policy` and app consumes it
  Evidence: [window_lifecycle.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/policy/window_lifecycle.rs) and [app.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/app.rs).

- [x] Main window no longer references a legacy `crate::tray` module directly
  Evidence command: `rg -n "crate::tray::|mod tray;" src`
  Result: no matches in the worktree.

- [x] Full regression suite passes after separation work so far
  Evidence command: `cargo test -- --nocapture`
  Result: `37 passed; 0 failed`.

- [x] Runtime is the live source of truth for adapter events and lifecycle
  Evidence: [runtime.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/core/runtime.rs) now handles lifecycle commands and emits `AdapterAction` events. [app.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/app.rs) consumes those runtime events and applies viewport actions.

- [x] Legacy tray implementation has been fully retired from the worktree code path
  Evidence: [adapters/tray/mod.rs](/F:/test/signal-desk-v2/.worktrees/runtime-adapter-separation/src/adapters/tray/mod.rs) owns tray icon/menu/click handling and sends only `AppCommand`s to runtime. `src/tray.rs` is absent from this worktree.
