# Unread Hover Popover Design

Date: 2026-04-15  
Project: `signal-desk-v2`  
Scope: Unread area hover list + single-item mark-as-read

## 1. Goal

Improve unread handling UX by adding interactive hover popovers for:

- Global unread entry in top toolbar (shows all unread alerts)
- Per-group unread badge in each card (shows only unread alerts in that group)

Users can mark alerts as read directly inside the hover list using a per-row action button.

## 2. Non-Goals

- No new backend API endpoints
- No bulk mark-as-read in popover
- No delete signal actions in unread popover
- No Windows Toast implementation in this change

## 3. User Experience

## 3.1 Trigger Points

- Top toolbar unread label: `Total unread: N`
- Group card unread label: `N unread`

Both labels are hover triggers.

## 3.2 Popover Behavior

- `mouseenter` on trigger opens popover.
- Pointer can move from trigger to popover without closing.
- Popover closes only after pointer leaves the combined trigger+popover region and 200ms delay elapses.
- At most one popover open at any time.
- Moving between triggers switches anchor and content source.

## 3.3 List Contents

Each row shows:

- `symbol`
- `period`
- `signalType`
- `side` (`多` or `空`, colored)
- trigger timestamp (`t`, formatted local time for readability)
- action button `标记已读`

Empty state:

- `暂无未读信号`

## 3.4 Scope Rules

- Group popover: only unread items in the current group.
- Global popover: all unread items across enabled groups.

## 4. Architecture & Components

All changes stay in desktop UI state layer with existing poller command channel.

## 4.1 New View Models

- `UnreadItemView`
  - `key: SignalKey`
  - `group_id: String`
  - `symbol: String`
  - `period: String`
  - `signal_type: String`
  - `side: Side`
  - `trigger_time_ms: i64`

- `HoverPanelTarget`
  - `Global`
  - `Group(String)` (group id)

- `HoverPanelState`
  - `target: HoverPanelTarget`
  - anchor geometry / id
  - `close_deadline_ms: Option<i64>`

## 4.2 New Runtime State

In `SignalDeskApp`:

- `pending_read: HashSet<SignalKey>` for in-flight optimistic writebacks
- `hover_panel: Option<HoverPanelState>`
- Derived unread views from `signals` + group definitions

## 4.3 Existing Channel Reuse

Continue using:

- `PollerCommand::MarkRead { key, read: true }`
- `PollerEvent::SyncFailed` for rollback signal

No changes to API contract.

## 5. Data Flow

## 5.1 Render Path

1. Build unread rows from `signals` using effective read state:
   - `effective_read = signal.read || pending_read.contains(key)`
   - only rows with `effective_read == false` are shown.
2. Filter rows by `HoverPanelTarget`.
3. Sort by `trigger_time_ms` descending.
4. Render scrollable list in popover.

## 5.2 Single-Item Mark Read

1. User clicks `标记已读`.
2. If key already in `pending_read`, ignore.
3. Optimistic UI update:
   - set local `read = true`
   - insert key into `pending_read`
4. Send `PollerCommand::MarkRead`.
5. On success: remove key from `pending_read`.
6. On `SyncFailed`:
   - revert `read = false`
   - remove key from `pending_read`
   - surface error in status area (`last_error`)

## 5.3 Poll Snapshot Merge

- Incoming snapshots can race with optimistic updates.
- While key is in `pending_read`, UI forces `effective_read=true` to avoid flicker.
- Once sync settles, source of truth returns to snapshot data + local mutation.

## 6. Error Handling

- Network/API writeback failure:
  - row returns to unread state
  - status bar shows concise error with key context
- Missing record during mark-read:
  - treat as failure and rollback local read state
- Popover render failure (none expected):
  - fail closed (popover hidden), no panic

## 7. UI Details

- Popover max height: ~320px, vertical scroll enabled.
- Row button state:
  - default: `标记已读`
  - pending: `处理中...` and disabled
- Side color:
  - `多`: green
  - `空`: red
- Keep existing `全部已读` button unchanged.

## 8. Testing Plan

## 8.1 Manual

1. Hover group unread badge opens group-scoped list.
2. Hover top unread badge opens global unread list.
3. Clicking row action removes row immediately (optimistic).
4. Simulated sync failure restores unread row and error message.
5. High-frequency polling does not flicker pending rows.
6. Leaving trigger+popover region closes after delay.

## 8.2 Unit/Logic Tests

- unread filtering by scope (`Global` vs `Group`)
- pending override behavior during snapshot merge
- sorting by trigger time descending
- duplicate click suppression while pending

## 9. Implementation Boundaries

- Primary touchpoints:
  - `src/app.rs` (UI + state management)
  - optional helper module for unread filtering/ordering if file grows too large
- No backend change required.
- Keep code path compatible with current polling loop and optimistic group-level mark-read.

## 10. Acceptance Criteria

- User can hover unread entry (global and group) and see correct unread list.
- User can mark one item read directly from list.
- UI updates instantly with rollback on failure.
- No regression to existing polling and `全部已读` behavior.
- Popover interaction is stable (no accidental close while moving into panel).
