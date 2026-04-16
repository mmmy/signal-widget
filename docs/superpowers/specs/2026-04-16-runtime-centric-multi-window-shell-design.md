# Runtime-Centric Multi-Window Shell 设计

## 1. 背景

当前应用已经把轮询、部分状态归并和托盘命令转发搬进了 `runtime`，但主窗口仍然承担了两个不该继续留在 UI 内的职责：

1. 消费来自 `runtime` 的窗口动作事件
2. 真正执行 `ViewportCommand::Visible/Minimized/Close/Focus/CancelClose`

这导致一个结构性问题：

- 托盘点击“显示主窗口/退出程序”后，命令虽然进入 `runtime`
- `runtime` 也能产出 `AdapterAction`
- 但真正执行窗口动作的代码还挂在主窗口 `update` 循环里
- 一旦主窗口被隐藏，这条消费链就可能停摆，托盘请求因此失效

这不是单个 bug，而是“窗口自身负责自己的生命周期控制”带来的架构耦合。

## 2. 核心理念

本次重构的首要原则是：

1. `runtime` 是程序真正运行的核心
2. 窗口只是渲染与输入适配层
3. 任何关键功能都不能以“主窗口仍在刷新”为前提

具体含义：

1. 轮询、通知、未读状态、已读同步、配置驱动、生命周期意图都属于 `runtime`
2. 主窗口、托盘、未来通知窗口都只是同级 adapter
3. 原生窗口的显示/隐藏/聚焦/关闭属于 shell 层，不属于某个具体窗口 UI

## 3. 目标

本次设计要实现：

1. 主窗口关闭后，应用继续在后台运行，轮询与通知不中断
2. 托盘可以可靠地恢复主窗口、退出程序
3. 主窗口不再承载关键生命周期逻辑
4. 为未来“通知使用独立窗口显示”预留稳定接口
5. `src/app.rs` 中的窗口控制逻辑整体迁出，主窗口退化为纯 adapter

## 4. 非目标

本次不做：

1. 多进程架构
2. 动态插件系统
3. 完整的窗口编排框架
4. 立即实现通知窗口完整 UI

但会把通知窗口需要的契约、事件流和 shell 接口提前设计好。

## 5. 推荐架构

```text
           +------------------------------------+
           |              Runtime               |
           | poller + unread + alerts + config  |
           | lifecycle intent + app state       |
           +----------------+-------------------+
                            |
          +-----------------+-----------------+
          |                                   |
          v                                   v
 +----------------------+         +----------------------+
 | Runtime Snapshots    |         | Shell Commands       |
 | / Domain Events      |         | Show/Hide/Focus/...  |
 +----------+-----------+         +----------+-----------+
            |                                |
            |                                v
            |                     +----------------------+
            |                     |    Window Manager    |
            |                     | native window shell  |
            |                     +-----+----------+-----+
            |                           |          |
            v                           v          v
 +----------------------+      +----------------+  +-------------------+
 | Main Window Adapter  |      | Tray Adapter   |  | Notification      |
 | render + input map   |      | menu + clicks  |  | Window Adapter    |
 +----------------------+      +----------------+  +-------------------+
```

这个模型里有三条清晰边界：

1. `runtime` 决定“应用应该发生什么”
2. `window manager` 负责“原生窗口应该怎么动”
3. adapter 负责“如何显示”和“如何把用户操作翻译成命令”

## 6. 分层职责

### 6.1 Runtime

`runtime` 是唯一真实业务状态与后台任务宿主，负责：

1. 轮询任务生命周期
2. 新信号归并与未读计算
3. 已读同步与失败恢复
4. 通知决策与节流
5. 应用级配置状态
6. 生命周期意图决策

这里的“生命周期意图”指的是：

- 用户请求关闭主窗口
- 用户请求显示主窗口
- 用户请求退出程序
- 新通知到达，是否需要打开通知窗口

`runtime` 负责决定这些意图应该转化成什么抽象动作，但不直接碰原生窗口句柄。

### 6.2 Window Manager

`window manager` 是一个很薄的 shell 层，负责执行原生窗口动作：

1. 注册和持有各窗口控制器
2. 执行 `show/hide/focus/close` 等动作
3. 在根窗口收到 close 请求的当帧同步执行 `CancelClose`
4. 在需要时创建或销毁附属窗口

它不持有业务状态，不做轮询，不做通知策略，不做未读计算。

### 6.3 Main Window Adapter

主窗口 adapter 只做两件事：

1. 渲染 runtime snapshot
2. 把 UI 交互转译成 `AppCommand`

它不再负责：

1. 直接消费托盘恢复请求
2. 决定是否退出程序
3. 决定是否隐藏到托盘
4. 直接执行窗口显示/关闭命令

### 6.4 Tray Adapter

托盘 adapter 负责：

1. 左键或菜单触发 `RequestShowMainWindow`
2. 菜单触发 `RequestExitApp`
3. 可选展示 unread / poll health 的摘要状态

它不直接控制窗口，不操作业务状态。

### 6.5 Notification Window Adapter

未来通知窗口也是同级 adapter，负责：

1. 渲染通知 payload
2. 接收最小交互，例如“标记已读”“忽略”“打开主窗口”

通知是否出现、何时出现、出现哪种内容，不由通知窗口自身决定，而由 `runtime` 决策并通过 shell 命令驱动。

## 7. 契约设计

建议把现有“主窗口动作事件”再明确拆成两类输出：

1. 给 adapter 的渲染/领域事件
2. 给 shell 的窗口动作

### 7.1 输入命令

```rust
pub enum AppCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    RequestCloseMainWindow,
    RequestShowMainWindow,
    RequestExitApp,
}
```

这些命令都由 adapter 发给 `runtime`。

### 7.2 Runtime 输出

```rust
pub enum RuntimeEvent {
    SnapshotUpdated(AppSnapshot),
    PollFailed { error: String },
    SyncFailed { key: SignalKey, error: String },
    NotificationRaised(NotificationPayload),
}
```

这类事件给各 adapter 订阅和渲染使用。

### 7.3 Shell 输出

```rust
pub enum ShellCommand {
    ShowWindow(WindowId),
    HideWindow(WindowId),
    FocusWindow(WindowId),
    CloseWindow(WindowId),
    ExitProcess,
    OpenNotificationWindow(NotificationPayload),
}
```

这类命令只给 `window manager` 使用。

关键点：

1. `runtime` 决策 shell 命令
2. `window manager` 执行 shell 命令
3. 主窗口 adapter 不再承担 shell 命令消费职责

## 8. 窗口生命周期策略

### 8.1 关闭主窗口

正确流程应为：

1. 根窗口收到原生 close 请求
2. `window manager` 在同一帧同步发出 `CancelClose`
3. `window manager` 把 `RequestCloseMainWindow` 送给 `runtime`
4. `runtime` 根据当前策略决定：
   - 有托盘且不允许整进程退出：`HideWindow(Main)`
   - 无托盘或显式允许退出：`ExitProcess`
5. `window manager` 执行对应 shell 命令

这保证“是否隐藏还是退出”的判断由核心统一决策，但“来不及拦 close”的时序风险由 shell 层承担。

### 8.2 从托盘恢复主窗口

正确流程应为：

1. 托盘 adapter 发送 `RequestShowMainWindow`
2. `runtime` 产出：
   - `ShowWindow(Main)`
   - `FocusWindow(Main)`
3. `window manager` 直接执行窗口动作

这条链不再依赖主窗口 update 循环继续运行。

### 8.3 退出程序

正确流程应为：

1. 托盘或主窗口发送 `RequestExitApp`
2. `runtime` 决策 `ExitProcess`
3. `window manager` 为根窗口切换到允许原生关闭状态
4. `window manager` 执行真正的 close / process exit

“允许原生关闭”的状态只应由 `window manager` 持有，不能散落在 UI adapter 中。

## 9. 文件结构建议

目标结构建议调整为：

```text
src/
  core/
    contract.rs
    runtime.rs
    state.rs
    policy/window_lifecycle.rs
    services/poller_service.rs
    services/alert_service.rs
  shell/
    mod.rs
    window_manager.rs
    window_controller.rs
  adapters/
    main_window/
      mod.rs
      app.rs
      view.rs
      fonts.rs
    tray/
      mod.rs
    notification_window/
      mod.rs
```

职责建议如下：

1. `src/core/runtime.rs`
   只管命令、状态、事件、shell 意图输出
2. `src/shell/window_manager.rs`
   只管窗口注册、shell 命令执行、close 拦截
3. `src/shell/window_controller.rs`
   抽象单窗口的 show/hide/focus/close 能力
4. `src/adapters/main_window/app.rs`
   egui `App` 实现，仅负责 UI 输入输出
5. `src/adapters/main_window/view.rs`
   渲染函数与 UI helper
6. `src/adapters/main_window/fonts.rs`
   字体加载
7. `src/adapters/notification_window/mod.rs`
   先建立适配层壳与接口，不必一次实现完整 UI

`src/app.rs` 不再作为长期承载文件，最终应删除或退化为过渡 shim。

## 10. 迁移策略

建议分两段进行。

### Phase 1：稳定主窗口/托盘生命周期链路

目标：

1. 把窗口控制逻辑从 `src/app.rs` 迁出
2. 新建 `shell/window_manager.rs`
3. 让托盘恢复与退出不再依赖主窗口 update

阶段完成标志：

1. 关闭主窗口后后台轮询和通知继续
2. 托盘“显示主窗口”稳定恢复
3. 托盘“退出程序”稳定生效

### Phase 2：完成 main window adapter 拆分并铺好通知窗口接口

目标：

1. 把现有 `src/app.rs` 拆到 `adapters/main_window/app.rs` 与 `view.rs`
2. `runtime` 输出从“混合事件”整理为“render event + shell command”
3. 建立 `notification_window` adapter 壳和 `OpenNotificationWindow` 命令通路

阶段完成标志：

1. 主窗口只是渲染器和命令输入器
2. shell 层可以独立控制任何窗口
3. 新增通知窗口不需要再修改主窗口生命周期代码

## 11. 测试策略

### 11.1 Core 测试

验证：

1. `RequestCloseMainWindow` 在不同托盘/退出条件下产出正确 shell 命令
2. `RequestShowMainWindow` 产出显示与聚焦命令
3. `RequestExitApp` 产出退出命令
4. 通知事件能转化为通知窗口 shell 命令

### 11.2 Shell 测试

验证：

1. 根窗口 close 请求会先同步 `CancelClose`
2. `ShowWindow(Main)` 会执行 `Visible(true)` + `Minimized(false)` + `Focus`
3. `HideWindow(Main)` 会执行隐藏而不是销毁
4. `ExitProcess` 不会被 close 拦截错误吞掉

### 11.3 Adapter 测试

验证：

1. 托盘菜单正确映射到 `AppCommand`
2. 主窗口按钮正确映射到 `AppCommand`
3. 通知窗口交互正确映射到 `AppCommand`

## 12. 风险与约束

### 12.1 风险：runtime 与 shell 边界再次混乱

缓解：

1. `runtime` 只发抽象 shell 命令，不接触 viewport/native API
2. shell 不持有业务状态

### 12.2 风险：重构时 `src/app.rs` 过渡期逻辑重复

缓解：

1. 先把 window control 提前迁走
2. 再拆 view 和 adapter
3. 每一步都用回归测试卡住

### 12.3 风险：通知窗口提前设计导致过度工程

缓解：

1. 本次只定接口，不做完整功能
2. 只保留最小必要 payload 和 shell 通路

## 13. 验收标准

满足以下条件才算这次架构重构完成：

1. 关闭主窗口后进程继续存活，轮询/通知继续工作
2. 托盘“显示主窗口”在主窗口隐藏后仍稳定有效
3. 托盘“退出程序”稳定有效
4. 主窗口 adapter 中不再直接执行关键窗口生命周期决策
5. `src/app.rs` 的窗口控制逻辑已迁出并完成拆分
6. `runtime` 成为唯一核心业务状态与生命周期意图来源
7. 为通知窗口新增 adapter 时，无需再把功能塞回主窗口

## 14. 推荐结论

推荐采用：

1. `runtime` 持有所有核心业务能力与生命周期意图
2. `window manager` 持有原生窗口控制权
3. 主窗口、托盘、未来通知窗口都作为同级 adapter

这条路径最符合当前产品方向：

1. 后台轮询和通知不依赖任何单个窗口
2. 主窗口可以真正退化成渲染器
3. 未来引入通知窗口时不会再出现“核心功能藏在主窗口里”的返工
