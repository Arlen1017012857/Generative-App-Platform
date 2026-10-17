# Ghostex 可选择复用审计

核验日期：2026-10-17（北京时间）。范围：只提取对自建 `generative-app-platform` 有用的部分；不 fork 整个产品，也不把 Ghostex 安装程序或 daemon 变成运行依赖。

当前桌面选型：用户已确定采用 [GPUI Kit](https://gpui-kit.com/) 自建原生宿主，平台核心使用 Rust；初版完整保留 Ghostex 界面，右侧仅文件预览与终端，设置最小化，只建设桌面端。最新的界面与代码取用决策以 [桌面规格](../design/desktop-mvp-ui-and-code-scope.md) 为准；具体分层和生成 App 的承载候选见 [实施方案](../design/ghostex-selective-reuse.md#4-gpui-kit-宿主与-app-承载)。本文对 Ghostex 源码的事实审计仍以以下固定版本为准。

- Ghostex 固定版本：[`05c27ecc77d738ad853f2db0d574bffa02a23a77`](https://github.com/maddada/Ghostex/commit/05c27ecc77d738ad853f2db0d574bffa02a23a77)。研究副本位于 `/private/tmp/ghostex-research-20261017`，未初始化其子模块。
- Pi 官方资料基线：[`698a2d23b86a5045dd54554354a5e0cb7e466795`](https://github.com/earendil-works/pi/commit/698a2d23b86a5045dd54554354a5e0cb7e466795)。资料来自官方源码与文档，不以第三方介绍为依据。
- 本文是源码与依赖静态审计；未编译 Ghostex、未向现有会话发请求，也未验证各 crate 独立移植后的构建。

## 结论

Ghostex 值得借鉴的是**多 Agent 工作台的工程模块与状态同步经验**。它的项目、会话、终端、聊天和预览可以辅助构建应用；本项目所需的 App 身份、业务数据、能力授权、版本、安装、迁移与演化流程仍需要自建。Ghostex 自身定位也是多 Agent 开发工作区。源码体现为 Rust/GPUI 桌面端、Rust gxserver、CEF 浏览器和一部分 React/TypeScript 工具界面，不能把整个仓库理解成一个可直接嵌入的 Rust 应用生成 SDK。[定位](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/README.md)、[桌面依赖](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/apps/desktop/Cargo.toml)、[服务依赖](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/Cargo.toml)、[前端依赖](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/package.json)。

建议首期采用：**GPUI Kit 原生宿主 + 自建 Rust 本地运行模块 + 官方 Pi RPC 子进程 + 自有会话/能力/应用契约**。依用户最新界面要求，原生侧栏、聊天、标题/标签栏、文件预览和终端是优先提取适配对象；状态同步算法选择性取用。纯 Rust crate 保留为可选候选，不因为它们能够独立编译就整体搬入。

## 哪些部分可以提取

以下“可直接提取”指依赖边界相对独立，仍需保留许可证、调整路径、核验构建与接口；不表示已经移植完成。

| 源码位置 | 实际职责与依赖 | 对本项目的建议 |
|---|---|---|
| [`packages/gx-protocol`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-protocol) | 纯类型与解析，只依赖 `serde`、`serde_json`；无传输代码。定义的是 **gxserver 协议**，不是 Pi RPC。 | 可独立提取；首期更适合借鉴开放枚举、缺失/null/值三态和兼容解析规则。除非决定兼容 Ghostex 消息模型，否则不要把全部项目、侧栏、agentbox 协议作为平台公共 API。 |
| [`packages/gx-visual`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-visual) | 只依赖 `serde`、`serde_json`；将声明式聊天图表/表格转换为 scene/SVG，Vega-Lite 只支持明确子集。 | 最容易独立复用的完整模块；仅在确实需要聊天内图表时引入，与 GPUI Kit 原生控件和 App 托管职责分开。 |
| [`packages/gx-chat-core`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-chat-core) | 平台无关的聊天状态机；依赖上面两个 crate、`markdown`、`serde`、`serde_json`。`Event → ChatCore → Document + Effect`，宿主执行 IO。 | 可连同依赖一起提取，但适配成本中等偏高：`Event`、RPC 效果、composer 启动/草稿/问题状态都遵循 Ghostex 契约。不是 Agent 推理引擎，也没有独立管理 Pi 进程。先只借鉴状态机边界。 |
| [`gx-chat-core/src/session/stream.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-chat-core/src/session/stream.rs) | 64 行纯状态判断：重复帧丢弃、顺序帧接受、缺口重同步；绑定一个 `FramePosition` 类型。 | 优先级最高的小规模提取候选。改为平台自己的游标类型，并保留 server/epoch/seq 的作用域含义。它适用于平台向 UI 的事件流，不应误认为 Pi 原生 RPC 自带这套序号。 |
| [`packages/gx-chat-client`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-chat-client)、[`packages/gx-client`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-client) | 前者拥有 gxserver 聊天 WebSocket；后者拥有本机 presentation WebSocket。都按 gxserver 的订阅与 RPC 规则工作。 | 不直接采用。平台不运行 gxserver 时，它们不能直接连接 Pi；借鉴重连、订阅与 UI 流分离方式即可。 |
| [`packages/gx-core`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-core) | 纯状态核心，但绑定 Ghostex projects/sessions/tabs/sidebar/workspaces/settings 等产品模型；还依赖 `settings-catalog`。 | 借鉴单一状态所有者、乐观覆盖、未加载与空值区分；不整体搬入。 |
| [`packages/client-storage-native`](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/client-storage-native) | SQLite 客户端存储，并包含从 Chromium IndexedDB/localStorage 导入的依赖及大量 Ghostex 命名空间。 | 不整体搬入。新平台不需要继承 Ghostex 的浏览器历史迁移；借鉴持久化确认与草稿恢复原则。 |
| [`server/src/domain/repository/session.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/domain/repository/session.rs)、[`storage/schema_migrations.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/storage/schema_migrations.rs) | 会话存储在 gxserver 内部，耦合 project、zmx provider、agentbox、terminal/runtime settings 与完整迁移链；没有独立 session-store crate。 | 自建最小 `sessions/runs/events` schema，参考事务和迁移实践。 |

最小依赖图由各 crate 的 `Cargo.toml` 可复查：

```text
gx-chat-core ── gx-protocol ── serde / serde_json
             ├─ gx-visual ─── serde / serde_json
             └─ markdown / serde / serde_json

gx-core ─────── gx-protocol + settings-catalog + serde / serde_json
gx-chat-client ─ gx-protocol + WebSocket transport
```

### Pi 适配部分：提取规则，不搬整个运行时

Ghostex 没有一个可以直接依赖的独立 Pi adapter crate。其聊天投影主要读取 Agent 自身 JSONL transcript，同时依赖 hook、终端与 server 状态模块；因此它适合兼容既有终端会话，不是本项目新建 Pi 集成时的最短路径。[聊天入口与模块重导出](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/session_chat.rs)、[Pi hook 生成器](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/agent_hooks/plugin_sources.rs#L504)。

可以有选择地移植：

- [`session_chat_decode_pi.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/session_chat_decode_pi.rs)：254 行，依赖 `crate::session_chat::*`。值得保留的规则包括 thinking 与正文分开、工具调用/结果通过 call ID 关联、compaction/branch summary、错误回复与图像占位处理。复制时必须一并处理公用消息类型和解析 helper，不能只复制单文件就声称完成集成。
- [`session_chat_pi_status.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/session_chat_pi_status.rs)：256 行，解析模型、thinking、token/cost/context。可用于历史导入，实时状态优先使用 Pi 官方 RPC，减少维护 transcript 扫描和缓存。
- [`session_chat_tests/real_transcripts.rs`](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/server/src/session_chat_tests/real_transcripts.rs)：用于了解历史边界与回归场景；采用具体 fixture 前应核实内容来源及是否适合随平台分发。

Ghostex 的整套 GPUI/CEF 宿主、zmx/wmx、完整 daemon、账户轮换、移动端、远程 AgentBox、扩展商店不进入首期复用范围。桌面呈现按新规格优先提取；文件预览只取原生只读部分，终端优先验证 `terminal_model.rs`、`terminal_element.rs` 和 `ghostty_vt/` 包装及必要 `portable-pty`/libghostty-vt 依赖。保留终端不要求继承 CEF 或完整远程恢复链；具体移植与原生打包尚未验证。[桌面依赖](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/apps/desktop/Cargo.toml)、[终端模型](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/apps/desktop/src/terminal_model.rs)、[VT 包装](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77/apps/desktop/src/ghostty_vt)、[子模块](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/.gitmodules)。

## Pi 官方 RPC/SDK 核验

Pi 官方明确支持 `pi --mode rpc` 长驻子进程，stdin/stdout 交换 JSONL，适合 Rust 等其他语言。Rust 需要持续读 stdout、独立收集 stderr、按 LF 分帧、用请求 ID 关联 response，并处理背压、退出、取消与超时。`prompt` 成功仅表示接受/排队/处理；自动重试或后续工作可能继续，应按 `agent_settled` 判断不再自动续跑，而不是按一次 `agent_end` 判断整个任务完成。[官方 RPC 文档](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/rpc.md)。

首期适配器只需选定并验证 `prompt/abort/get_state/get_messages/get_entries/get_session_stats` 等必要命令。持久会话应使用明确的 session 路径或 session directory；官方示例中的 `--no-session` 是临时模式，不适合照搬为产品持久化默认。平台保存自己的 App/Task/Run 身份与 Pi session 映射，Pi 保存模型上下文与自己的会话树。[官方 RPC 命令](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/rpc-commands.md)、[官方 SDK 会话说明](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/sdk.md)。

**RPC 是传输/控制边界，不提供平台的细粒度授权或 OS 沙箱。** Pi 默认以启动用户的权限读写和执行文件；项目 `cwd`、project trust、观察 transcript 都不是隔离边界，也不能把默认 `bash` 当作已授权业务动作的执行后门。[官方安全模型](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/security.md)。

业务能力应由平台的 capability broker 校验主体、App、资源范围、版本与动作，再执行。Pi 可加载受控 TypeScript extension，注册调用 broker 的工具，或在 `tool_call` hook 阻断未授权动作；需要用户决定时，extension 的 `ctx.ui.confirm/select` 可经 RPC UI 子协议传回宿主。这些 hook 不替代 OS 隔离，禁用内置工具也不限制同权限 extension 自身的访问能力。[官方扩展接口](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/extensions.md)、[RPC Extension UI](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/rpc-extension-ui.md)。

SDK 的 `customTools/resourceLoader/tools` 等程序化定制运行于 Node.js/Bun；若选 SDK，必须有对应宿主或 sidecar。选择 Rust + Pi RPC 时，受控 extension 可直接加载在已有 Pi 子进程中，**不必额外再启动一个 Node sidecar**；但仍需编写 TypeScript 适配代码，不能把 Rust RPC 连接误写为“全 Rust 推理引擎”。[官方 SDK](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/sdk.md)、[官方扩展加载](https://github.com/earendil-works/pi/blob/698a2d23b86a5045dd54554354a5e0cb7e466795/packages/coding-agent/docs/extensions.md)。

## 自建的最小闭环

以下为根据产品文档与上述源码审计提出的工程建议，不是 Ghostex 已有功能清单：

先按 [D0 桌面规格](../design/desktop-mvp-ui-and-code-scope.md) 交付真实 Pi 会话、Files/Terminal 和本地持久化；以下步骤中涉及 App 安装与更新的部分属于后续 D1，不扩展初版右侧工具种类。

1. 自建稳定的 `App/Workspace/Task/Session/Run` 类型和 SQLite 元数据；平台 Session 显式关联 Pi 会话上下文，同一 Session 可承载多次 Run，App 身份不依赖会话是否存活。
2. 实现一个 Pi RPC adapter，将 Pi 事件归一化为平台的 message/tool/run 事件。恢复时先查询 Pi 权威状态，再恢复 UI；不要复制 Ghostex 完整终端探测链。
3. 保持 `UI → intent → core → effects → runtime IO` 的职责分离；只在需要重放/断线恢复时引入自有 epoch/sequence 规则。
4. 用 Coding 工作区生成一个带 AppManifest 和独立业务数据目录的最小 App；经过 validate → preview → install，再关闭重开，确认业务数据仍在。
5. 对同一 App 生成 v2，验证 App 身份和数据延续。代码版本回退与数据迁移分开设计；此闭环通过后才评估聊天图表、多 Agent、移动端等附加模块。

## 许可证与落地方式

Ghostex 根许可证为 MIT，允许使用、修改和分发，复制实质部分时需保留原版权与许可文本。选定的三个纯 Rust crate 源码未发现覆盖根许可的独立声明；它们的 Cargo manifest 均 `publish = false`，不能将其视为已发布且稳定维护的 crates.io SDK。实际采用时应 vendor 明确目录、保留根 LICENSE，并记录上游 commit、原路径与本地修改。[Ghostex LICENSE](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/LICENSE)、[protocol manifest](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-protocol/Cargo.toml)、[chat-core manifest](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-chat-core/Cargo.toml)、[visual manifest](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-visual/Cargo.toml)。

根许可不代表所有依赖与资源没有额外要求。例如 `gx-chat-core/visual/reference-visual.json` 内含 Tabler 图标的独立 MIT notice；如复制这些资源也需保留。界面还原所需图标、字体和终端资源按实际使用逐项记录来源；Ghostex 品牌替换为本项目内容，不整包引入 `.dependencies`。[资源中的 notice](https://github.com/maddada/Ghostex/blob/05c27ecc77d738ad853f2db0d574bffa02a23a77/packages/gx-chat-core/visual/reference-visual.json)。

实际提取遵循先记录来源、按能力适配、再独立构建/运行回归的顺序。静态调研之后已按用户批准实现 D0 桌面，提取 VT 包装并适配 PTY/GPUI 原生视图，界面使用 GPUI Kit 重建，Pi 承担执行引擎职责；实际来源见 [THIRD_PARTY.md](../../THIRD_PARTY.md)，运行结果及验收缺口见 [D0 验收记录](../verification/d0/README.md)。本调研中的动态 App 建议仍不是已实现能力。
