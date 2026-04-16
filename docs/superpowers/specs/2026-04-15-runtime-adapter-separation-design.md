# Runtime 与 Adapter 完全分离设计（主窗口 / 托盘 / 悬浮插件同级）

## 1. 背景与目标

当前代码中，`src/app.rs` 同时承担了：

- UI 渲染（egui 组件绘制、交互）
- 业务状态管理（signals、未读、已读同步中间态、轮询状态）
- 生命周期控制（关闭窗口转托盘、退出策略）

这会导致后续新增“悬浮插件（floating widget）”时，改动横跨 UI、状态和生命周期多个区域，风险高且回归难。

本设计目标是把系统拆为三层并确保职责单一：

1. `Core Runtime`：唯一业务状态与命令处理中心
2. `Adapter`：主窗口、托盘、悬浮插件三个同级前端适配层
3. `Contract`：统一命令/事件契约，三端仅通过契约与核心通信

## 2. 用户需求映射

用户明确要求：

1. 主窗口、系统托盘、悬浮插件架构上同级
2. 悬浮插件不仅只读，还要支持写操作：`标记已读`、`立即轮询`
3. 悬浮插件与主窗口/托盘一样能接收新通知
4. 同进程模式（不是独立进程 IPC）

这些需求决定了“单一真状态 + 多 Adapter 双向通信”模型。

## 3. 非目标

本次不做：

1. 插件动态加载系统（热插拔、外部 WASM/动态库）
2. 跨进程通信协议（命名管道/TCP）作为主通道
3. 全量 Actor 化重写（避免当前体量下过度工程）

## 4. 目标架构

```text
                +----------------------+
                |   Main Window Adapter|
                +----------+-----------+
                           |
                +----------v-----------+
                |      App Contract    |
                | AppCommand/AppEvent  |
                +----------+-----------+
                           |
      +--------------------v--------------------+
      |             Core Runtime                |
      | state + reducer + command handlers      |
      | poller service + alert service          |
      +-----------+----------------+------------+
                  |                |
      +-----------v----+    +------v-------------+
      | Tray Adapter   |    | Floating Adapter   |
      +----------------+    +--------------------+
```

原则：

1. Adapter 不直接改核心状态
2. 核心状态只在 Runtime 内更新
3. Adapter 通过发送 `AppCommand` 改变状态，通过订阅 `AppEvent` 刷新 UI

## 5. 契约设计

新增核心契约（`src/core/contract.rs`）：

```rust
pub enum AdapterId {
    MainWindow,
    Tray,
    FloatingWidget,
}

pub enum AppCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    MarkGroupRead { group_id: String },
    SaveUiConfig { patch: UiConfigPatch },

    RequestCloseMainWindow,
    RequestShowMainWindow,
    RequestExitApp,
}

pub enum UiAction {
    ShowMainWindow,
    HideMainWindowToTray,
    ExitProcess,
}

pub enum AppEvent {
    SnapshotUpdated(AppSnapshot),
    UnreadChanged { total: usize, by_group: Vec<(String, usize)> },
    PollStatusChanged { ok: bool, consecutive_failures: u32, error: Option<String> },
    NotificationRaised { title: String, body: String },
    AdapterAction { target: AdapterId, action: UiAction },
}
```

说明：

1. `Request*` 是 Adapter 发起的意图，不是直接执行结果
2. Runtime 根据策略产出 `AdapterAction`
3. 关闭窗口是否隐藏到托盘由 Runtime 决策，避免策略分散在 UI

## 6. Runtime API

`src/core/runtime.rs` 提供统一句柄：

```rust
pub struct RuntimeHandle {
    pub fn send(&self, cmd: AppCommand) -> anyhow::Result<()>;
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AppEvent>;
    pub fn snapshot(&self) -> AppSnapshot;
}
```

Runtime 职责：

1. 消费 `AppCommand`
2. 更新 `AppState`（reducer）
3. 驱动服务（poller / alerts / config persistence）
4. 广播 `AppEvent`

## 7. 状态归属与服务边界

`AppState`（Runtime 内唯一真状态）：

1. signals、pending_read、local_read_floor
2. poll 元信息：last_poll_ms、ok/failure_count、last_error
3. UI 相关全局设置：always_on_top、edge_mode、edge_width、notifications、sound
4. 统计快照：unread_total、group_unread_map

服务边界：

1. `poller_service`：仅轮询与已读同步，不持有 UI context
2. `alert_service`：仅根据事件与配置决定通知行为
3. `window_lifecycle_policy`：关闭/显示/退出的策略函数

## 8. Adapter 设计

### 8.1 Main Window Adapter

保留 egui 绘制与用户交互：

1. 渲染快照（signals、未读、状态栏）
2. 交互映射成 `AppCommand`
3. 接收 `AdapterAction(MainWindow)` 执行窗口命令

### 8.2 Tray Adapter

仅托盘相关：

1. 左键点击 -> `RequestShowMainWindow`
2. 菜单“显示主窗口” -> `RequestShowMainWindow`
3. 菜单“退出” -> `RequestExitApp`
4. 订阅 `UnreadChanged` / `PollStatusChanged` 用于 tooltip 与后续图标状态

### 8.3 Floating Widget Adapter

新建同级适配层：

1. 订阅 `SnapshotUpdated` 展示关键信号/未读
2. 用户操作支持：
   - `MarkRead`
   - `ForcePoll`
3. 接收 `NotificationRaised` 触发悬浮态视觉提醒（后续实现细节）

## 9. 文件迁移计划

目标目录：

```text
src/
  core/
    contract.rs
    runtime.rs
    state.rs
    reducer.rs
    command_handlers.rs
    policy/window_lifecycle.rs
    queries/unread.rs
    services/poller_service.rs
    services/alert_service.rs
  adapters/
    main_window/{mod.rs, app.rs, view.rs, fonts.rs}
    tray/mod.rs
    floating_widget/{mod.rs, app.rs}
```

从 `src/app.rs` 的迁移映射：

1. `drain_poller_events` -> `core/runtime.rs`
2. `consume_snapshot` -> `core/reducer.rs`
3. `mark_one_read` / `mark_group_read` -> `core/command_handlers.rs`
4. `period_has_unread` / `effective_unread_keys` / `collect_new_unread_keys` -> `core/queries/unread.rs`
5. `close_action_for_request` -> `core/policy/window_lifecycle.rs`
6. 渲染相关函数保留在 `adapters/main_window/view.rs`

## 10. 分阶段实施（低风险）

### Phase 1：建立 Core 骨架（不改行为）

1. 新增 `core/contract`、`core/state`、`core/runtime` 空实现
2. `main.rs` 改为先创建 runtime，再注入到现有主窗口与托盘

验收：

1. 应用行为不变
2. 现有测试通过

### Phase 2：迁移纯业务函数

1. 把 unread 计算函数迁到 `core/queries`
2. 把关闭策略迁到 `core/policy`

验收：

1. 迁移函数单测通过
2. UI 交互行为不变

### Phase 3：迁移命令处理与状态更新

1. `mark read` / `force poll` 通过 runtime 命令处理
2. 主窗口仅发送命令与渲染快照

验收：

1. 标记已读与立即轮询行为一致
2. 回归测试通过

### Phase 4：托盘改为纯 Adapter

1. 托盘不再直接操作窗口内部状态，只发 `Request*`
2. Runtime 统一下发 `AdapterAction`

验收：

1. 关闭窗口隐藏到托盘仍生效
2. 托盘左键恢复、菜单退出仍生效

### Phase 5：接入悬浮插件

1. 新增 `floating_widget` adapter
2. 通过同一契约实现读写和通知接收

验收：

1. 悬浮插件可标记已读、立即轮询
2. 通知事件可在悬浮插件感知

## 11. 测试策略

1. `core` 单元测试：
   - reducer 状态迁移
   - 命令处理
   - 关闭策略
2. `adapter` 单元测试：
   - 托盘左键/菜单映射命令
   - 主窗口交互映射命令
3. 集成测试：
   - 命令 -> 状态变化 -> 事件广播闭环
   - 三端订阅同一快照的一致性

## 12. 风险与缓解

1. 风险：迁移中行为回归  
缓解：阶段化迁移 + 每阶段回归测试

2. 风险：事件风暴导致 UI 频繁重绘  
缓解：Runtime 做节流与合并（同帧合并快照事件）

3. 风险：退出时序混乱  
缓解：统一 `RequestExitApp -> AdapterAction::ExitProcess` 单出口

## 13. 验收标准（完全分离判定）

满足以下全部条件才算完成“完全分离”：

1. 主窗口/托盘/悬浮插件之间无直接调用
2. 三者仅通过 `AppCommand/AppEvent` 与 Runtime 交互
3. 核心状态只存在于 `core/state`，Adapter 不持有可写业务真状态
4. `src/app.rs`（或替代主窗口模块）不再直接处理 poller 事件与业务归并
5. 新增一个 Adapter 时无需修改 Runtime 业务核心（仅注册与契约实现）

## 14. 实施状态

截至 `codex/runtime-adapter-separation` 当前实现：

1. Core contract / state / runtime scaffolding 已建立
2. Tray 与 floating widget 的 adapter command shell 已建立
3. Poll request shaping 与 window lifecycle policy 已从旧 UI 逻辑中抽出
4. Binary 已改为通过 library entrypoint 启动，避免 lib/bin 双模块图漂移
5. Runtime 已成为 adapter 事件与窗口生命周期的唯一实时来源
6. `ForcePoll` / `MarkRead` / `PollerEvent` 已经通过 Runtime 闭环流转，主窗口改为渲染 runtime snapshot
