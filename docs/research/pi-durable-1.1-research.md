# Pi Durable 1.1 调研

> 调研日期：2026-10-12（Asia/Shanghai）。  
> 文档修订日期：2026-10-14（Asia/Shanghai）。  
> 核验对象：`earendil-works/pi` 的 `v1.1.0`，固定 commit `abe508e1b89912adde45528136c3221eb69acdd7`。  
> 对照基线：上游 `v1.0.4` 的固定 commit `428a12bc775145afa342530a9eaa652efb3e4422`；版本差异与迁移边界在本文第 3–6 节说明。  
> 方法：阅读官方发布记录、固定源码、规范与官方测试；未运行上游测试、调用真实模型、部署 Cloudflare Workers，未完成 Tauri 或本仓库的集成验证。下文明确区分上游事实、源码推论与平台建议。

## 1. 判断

**1.1 主要改善共享执行底座的接入与运行效率：工具和 hook 可直接使用 Harness 的 Models，历史模型上下文可读取，任务可按新旧排序并带有耗时记录，长会话的上下文读取采用增量缓存，新增 Cloudflare Durable Object SQLite 适配器。它仍是实验性的 agent harness。** 新增能力使它更适合承担平台的 Coding 元能力，但没有把 Coding Agent CLI 的全部能力自动带入 Durable。[公开 API][harness-types]、[上下文实现][context]、[Cloudflare 适配器][cloudflare]、[README][readme]、[Coding 示例][coding-example]

对当前平台方案，建议将候选验证基线从 1.0.4 更新为 **固定 1.1.0 与 lockfile**，继续保留 Coding Agent SDK 对照。以下四项需要单独设计，1.1 没有消除其边界：业务 Operation 的幂等与结果确认、实际执行边界的权限检查、AppVersion 与扩展代码的恢复兼容、桌面宿主的进程与存储生命周期。这是平台建议，源码依据与责任划分见本文第 6 节的固定源码复核和第 7 节的平台接入约定。

## 2. 版本与发布时间

| 项目 | 核验结果 | 一手依据 |
|---|---|---|
| Durable 包 | `@earendil-works/pi-durable`，`1.1.0` | [package.json][package] |
| 上游发布 tag | `v1.1.0`，指向 `abe508e1b89912adde45528136c3221eb69acdd7` | [tag ref REST 记录][tag-ref]、[发布 commit][release-commit] |
| CHANGELOG 日期 | 2026-10-07 | [Durable CHANGELOG][changelog] |
| GitHub 正式发布时刻 | `2026-10-07T22:26:31Z`，即北京时间 **2026-10-08 06:26:31**；`draft=false`、`prerelease=false` | [release REST 记录][release-api]、[发布页][release] |
| 本轮看到的最新 GitHub release | 截至调研时，官方 releases 列表首条为 `v1.1.0` | [官方 releases REST 列表][releases] |
| 稳定性承诺 | README 仍标注 Experimental，API 可在版本间无预告变化 | [README 第 3 行][readme] |
| 运行与依赖 | ESM；Node `>=22.19.0`；依赖 `pi-ai ^1.1.0`、`chord ^1.1.0` | [package.json][package] |

GitHub 的非 prerelease 标记与 Durable 的实验性声明是两种信息，不能据前者推断 API 已稳定。本轮核验的是上述源码与发布元数据；**未独立核验 npm artifact、manifest 的 gitHead/integrity，也没有核验 tarball 字节内容是否与固定 commit 一致**。官方整体 Pi release 页面主要描述 Coding Agent CLI 的更新，Durable 的专属变化应以其包内 CHANGELOG、类型与测试为准。[发布页][release]、[Durable CHANGELOG][changelog]、[README][readme]

## 3. 相对 1.0.4 的新增与变化

### 3.1 工具、hook 获得共享 Models

`ToolExecutionApi.models` 与 `HookApi.models` 指向 `HarnessOptions.models`。这使工具或 hook 能直接解析模型与发起请求，沿用 Harness 的模型目录、凭据和请求配置入口。官方测试用对象身份比较，确认拿到的是 Harness 的同一份 Models。`TaskRuntime.models` 则在旧体系中已存在；1.1 的变化是将它正式暴露到工具与 hook 的公开 API。[ToolExecutionApi 与 HookApi][harness-types]、[models 身份测试][models-test]、[TaskRuntime][core-types]

接入意义（平台推论）：分类、验证或路由工具不必再通过外部闭包注入另一份模型服务。TypeSafe/Jev 可以通过 pi-ai 的 classifier 接口接入；**Jev/classifier 基础并非 1.1 首次引入**，应将新增点理解为 Durable 工具与 hook 的统一访问入口。同一版本的 pi-ai 还新增 OpenAI Decisions classifier 与 classifier images，属于模型层更新。[1.0.4 pi-ai Classification][old-ai-readme]、[pi-ai 1.1 CHANGELOG][ai-changelog]、[Models 接口与 dispatch][models]

但直接调用 `api.models.classify()` 是普通模型请求：Models 实现负责校验、凭据与 provider dispatch，没有创建 Durable task 或提交 checkpoint。因此它不会自动成为独立的可恢复分类步骤；如果决定必须稳定重用，应由工具 replay 策略、自定义 task 或 memo 保存结果。这个结论来自调用链分析，尚未故障注入实测。[Models.classify 实现][classify-implementation]、[ToolTask 执行与恢复][tool]

工具请求产生的 usage 也不会因为共享 Models 自动计入 Harness：`ToolExecutionResult.usage` 是可选字段，追加工具结果时只有显式携带 usage 才调用 `recordUsage`；工具费用按工具名汇总，没有模型身份。hook 内直接模型请求同样没有此自动记录路径。接入时需要检查 classifier 的错误结果，并显式记录分类判断与费用；hook 的费用需要自定义记录约定。[ToolExecutionResult][harness-types]、[工具结果入账][tool-usage]、[UsageState][usage]、[Models.classify][classify-implementation]

### 3.2 历史模型上下文读取

新增 `Conversation.context(context, { at })`，读取截止某个可见 Entry 的模型上下文；不传 `at` 则保持当前上下文。`TaskRuntime.context(conversationId, context, { at })` 采用相同选项形状。[Conversation.context][harness-types]、[TaskRuntime.context][core-types]

官方测试确认：历史 context 等于在同一 Entry 上建立 fork 后的初始 context；截止点之后的 edit/reset 不生效，截断在工具调用、尚未出现结果的位置会合成缺失结果，当前 Conversation 不可见的 Entry 会拒绝。它是模型历史视图，不能据此推断项目文件、App 数据库或 AppVersion 已恢复到那个时间点。[历史 context 测试][history-context-test]、[捕获范围与派生实现][context]

接入意义（平台建议）：可在 Coding 工作区的审阅、诊断、分支预览中查看“模型当时能看到什么”，避免仅为读历史创建额外 Conversation。业务对象的 as-of 视图仍应由其修订契约提供。

### 3.3 双向分页与 opaque cursor

新增 `ScanOrder = "ascending" | "descending"`，覆盖 Conversation、Entry、Task、Submission 查询，包括 `Tx.scanTasks()` 和 `Conversation.entries()`。排序按 ID；默认仍是 Entry 最新在前，其余最旧在前。[查询类型与 Storage 契约][core-types]、[Conversation.entries 实现][harness]

cursor 携带原扫描方向：后续请求可省略 `order`，但不能拿同一个 cursor 改用反向扫描。内置实现对旧的无 `order` cursor 采用该 scan 的默认方向。官方 conformance 增加双方向分页与方向不匹配断言，并覆盖 fork ancestry 下的 Entry 排序。[scanStart][scan]、[Storage conformance][storage-conformance]

接入意义（平台建议）：任务列表可以直接请求最新记录，无需先取全部记录再倒序；API/IPC 应将 cursor 当成 opaque continuation 转交。ID 顺序并不是任意业务字段排序，不能把它当成按 `endedAt`、优先级或状态排名的通用查询器。

### 3.4 任务生命周期与执行耗时

| 字段 | 含义 | 边界与证据 |
|---|---|---|
| `TaskRecord.startedAt` | Session 在第一次切换到 `running` 时记录的墙钟时间 | 等待、重开后保留；开始前与旧版本记录可缺失。[记录类型][core-types]、[时间戳实现][task-timestamps]、[恢复测试][task-time-recovery] |
| `TaskRecord.endedAt` | 切换到 `terminal` 时的墙钟时间 | `endedAt - startedAt` 包括等待和重启间隔；不是 CPU 时间或单次工具执行时间。[规范][spec]、[时间戳测试][task-time-test] |
| ToolResult `durationMs` | 本次 `execute()` 实际执行的单调时钟耗时 | 排除 beforeTool、环境构建等步骤；执行抛错也有耗时，blocked/未执行、interrupted/aborted 无该字段。[执行实现][tool-duration]、[耗时测试][tool-duration-test] |
| AssistantMessage `durationMs` | pi-ai 从请求开始到最终消息测量的单次响应耗时 | 之后 fetch 的 deferred result 不计时；属于 pi-ai 的共同消息字段。[pi-ai CHANGELOG][ai-changelog] |

`HarnessOptions.now` 用于 Harness 生命周期时间；新增 `createSession(storage, { now })` 支持直接使用 Session kernel 时提供同类时钟。记录来自旧版本时没有时间戳，不应在 UI 或指标中填成 0。[createSession][session]、[任务时间戳实现][task-timestamps]

接入意义（平台建议）：平台 Task 时间、Durable 内部 Task 时间、工具本次 attempt 时间分别显示与统计；平台的目标完成仍由结果证据确认。耗时字段能改善观察，不能替代业务完成条件。

### 3.5 长会话的增量 context 缓存

TaskScheduler 按 Conversation 保存最近的 context 范围与派生视图；同 head marker 的后续读取只扫描已保存 tail 之后的 Entry。新增 edit 等会重新派生范围，新 head 则重新扫描。缓存是内存派生状态，close 时清空，不是另一份持久事实源。[context 增量实现][context]、[scheduler 缓存][scheduler]

`settings.contextRetentionMs` 默认 600,000 ms。忙碌 Conversation 保留缓存，空闲后保留指定时长，`0` 表示空闲即丢弃。Node 可用 unref timer 过期；不能 unref timer 的环境（规范举 Cloudflare Workers）随任务变化检查过期，避免计时器妨碍对象驱逐。[settings 类型][harness-types]、[scheduler 过期实现][scheduler]、[跨任务与保留期测试][context-cache-test]

需要注意范围：**缓存用于 `TaskRuntime.context()`，包括内置 generation；公开 `Conversation.context()` 仍直接调用 `readContext()`。** 不应宣称任意 UI 历史读取都自动增量缓存。官方测试验证跨 task 复用、过期后全读，以及增量视图等价全量视图；本轮没有测量实际长会话延迟、内存或吞吐。[两条读取路径][harness]、[scheduler runtime.context][scheduler]、[context 测试][context-tests]

另修复了首条 system message（及 compaction/reset 后的 baseline）在模型 context 中排在启动输入之后的问题。现在它置于前面，后续 positional system message 保持位置。官方 live e2e 针对 Anthropic 工具变化测试缓存命中；无凭据时测试跳过，因此这里只能确认测试覆盖与实现，不能报告我们的真实命中率。[leadWithSystem][context]、[官方 Anthropic e2e][system-order-e2e]

### 3.6 Cloudflare Durable Object SQLite

新增 `@earendil-works/pi-durable/storage/sqlite/cloudflare`，提供 `openDurableObjectSqliteStorage(ctx.storage)` 与 `DurableObjectSqliteDatabase`。使用 SQLite-backed Durable Object 的 SQL 与 transaction，适配器串行化操作，将 BLOB 在 ArrayBuffer/Uint8Array 之间转换，拒绝超出安全整数范围的 bigint binding。[导出定义][package]、[Cloudflare 适配器][cloudflare]

官方测试对 `node:sqlite` 实现的 Durable Object storage stand-in 运行 Storage conformance，并检查 BLOB、回滚、事务期间串行化与关闭行为。它不是 Cloudflare 线上部署测试。本轮不据此宣称 Workers 的 eviction、alarm、并发事件或远程 ExecutionEnv 已验收。[Cloudflare 测试][cloudflare-tests]

接入意义（平台推论）：以后增加云端宿主时可复用 harness 与业务扩展的一个存储接入点；当前本地优先的桌面平台不必因此迁移到 Cloudflare，也不必新增远程宿主目录。

## 4. 破坏性变化与迁移

### 4.1 从 Durable 1.0.4 升级

1. **修改自定义 Storage。** 对四种 Query 正确实现 `order`，让 cursor 延续原方向，方向冲突时拒绝。仅接受新增字段但忽略它会产生错误分页；升级后运行官方 conformance 的双向场景。使用上游 Memory/SQLite/JSONL 的宿主无需自行补实现，但仍需验证自己封装的分页 API。[Storage 类型][core-types]、[conformance][storage-conformance]
2. **修改历史 context 调用。** `runtime.context(conversationId, context, at)` 改为 `runtime.context(conversationId, context, { at })`。第三参数缺省的调用不需要新增选项。[TaskRuntime 定义][core-types]、[CHANGELOG][changelog]
3. **适配新增可选字段。** TaskRecord 的时间戳与消息 `durationMs` 允许缺失；避免硬编码序列化字段清单丢掉它们，也避免用字段缺失判定记录损坏。[TaskRecord][core-types]、[pi-ai 消息变化][ai-changelog]
4. **配置 context 缓存预算。** 首期可使用十分钟默认值；多 Conversation、大 context 或受限宿主需测量内存后设置 retention。缓存过期不影响已提交事实与恢复，仅影响后续重读成本。[scheduler][scheduler]、[context 等价测试][context-tests]
5. **同步检查 pi-ai 自定义 stream。** Durable 依赖 pi-ai 1.1；pi-ai 的破坏性变化要求 stream 返回 `AssistantMessageEventStream`，可用 `createAssistantMessageEventStream()`，手写通用 `EventStream<AssistantMessageEvent, AssistantMessage>` 子类不再类型兼容。如果只用内置 provider，该项由上游实现承接。[pi-ai CHANGELOG][ai-changelog]、[ProviderStreams][ai-types]、[依赖版本][package]

上游将旧 Task 时间戳设计为可缺失，内置扫描兼容旧无方向 cursor；这有助于向前升级。**本轮未打开真实 1.0.4 存储进行 1.1 恢复，也未验证由 1.1 写入后回退到 1.0.4 的行为**，不能将代码层兼容线索等同于无风险升级/回滚。[时间戳实现][task-timestamps]、[scanStart][scan]

### 4.2 从 Coding Agent SDK 改用 Durable

两者是不同 harness 接入方式。SDK 用 `createAgentSession()`、`AgentSession`、`SessionManager` 和 `ResourceLoader`；Durable 用 `Harness`、`Conversation`、`Submission`、`Registry`、durable Task 与 Documents。官方定位也明确 Durable 是构建 agent 应用的框架，并不取代 Pi Coding Agent。[Coding Agent SDK][sdk]、[Durable README][readme]、[官方发布文章][launch]

迁移应先做平台适配：平台 Session 映射 Conversation，平台 Task 关联 Submission 与内部 Task，Surface 订阅同一执行状态。随后逐项补齐项目上下文、skills、模板、开发工具和授权策略；不能仅替换构造函数就声称沿用全部 Coding Agent 扩展。这个步骤是平台建议，由两套公开接入接口及 Durable Coding 示例的装配范围得出。[SDK 配置接口][sdk]、[Durable 扩展接口][harness-types]、[Coding 示例][coding-example]

## 5. Coding Agent CLI、SDK 与 Durable 的能力差距

| 能力 | Coding Agent CLI / SDK 1.1 | Durable 1.1 | 平台接入含义 |
|---|---|---|---|
| 基础开发工具 | SDK 默认工具与项目资源发现由工厂装配 | `CodingTools` 提供 read/write/edit/bash，需显式 install 与 ExecutionEnv；PowerShell 另创建 | 共同验证真实软件修改、命令执行及结果呈现。[SDK][sdk]、[CodingTools][coding-tools] |
| skills、prompt templates、context files、themes | SDK 的 ResourceLoader 是公开可替换边界 | Durable 的公开扩展以 tools/sections/hooks/wraps/tasks 组成；示例只有工具、cwd section、settings、env | Durable 资源加载需要宿主/平台扩展完成，当前示例不能代表完整 Coding 工作区。[SDK][sdk]、[Coding 示例][coding-example] |
| codemode、tool_search、MCP | CLI 自动装配；**SDK 默认不加载**，官方示例要求提供对应 extension factories、激活工具，并 bindExtensions 启动 MCP | Durable 包公开依赖与 CodingTools 未自动装配这些 Coding Agent 扩展 | SDK 也需要显式能力清单；采用 Durable 时另评估对应接入。[SDK codemode/MCP][sdk]、[SDK 示例][sdk-mcp]、[Durable 包定义][package] |
| 模型与凭据 | SDK 默认创建 ModelRuntime，也允许显式提供 | Harness 显式接收 Models；1.1 工具和 hook 可直接访问 | 平台应集中提供一份模型服务，并定义调用身份与费用记录。[SDK][sdk]、[models 测试][models-test] |
| UI 状态、运行中指导 | SDK 流式事件、steer/followUp 与 session lifecycle | Conversation 的提交、committed view/watch、queued submission 与恢复执行 | Surface 适配仍需平台对象与当前状态；不能直接假定事件语义相同。[SDK][sdk]、[Durable README][readme] |
| 进程退出后的执行 | 官方定位由人查看现场并指导 Coding Agent 继续 | durable Task checkpoints 与恢复策略为核心 | 对照重点是自动恢复与外部效果确认，持续记录会话不等同于执行步骤可恢复。[官方定位][launch]、[ToolTask 恢复][tool] |
| 图片读取 | 应按所选 SDK 工具配置验收 | README 明确基础 read 尚不支持图片 | 业务 App 的图片输入/理解需要专门工具或模型输入接入。[Durable Tools][readme] |

同一 Pi 1.1 release 中的 OSC 7501、CLI `+name/-name` 工具选择、`agent_settled.aborted` 与 MCP OAuth 修复，属于 Coding Agent/相关包的变化。它们不会因为 Durable 也升到 1.1 自动出现在 Durable Harness 的公开 API 中，应按实际采用入口对照。[Pi 1.1 release][release]、[SDK][sdk]、[Durable 公开类型][harness-types]

## 6. 1.1 中仍成立的恢复、权限与版本边界

以下是对固定 1.1.0 源码的复核，不只沿用旧笔记。

- **请求去重只到 Conversation Submission。** `requestId` 查到已有记录时只检查 submission type，不比较 content；相同 key、不同 payload 不会自动报冲突。平台应持久绑定 key 与逻辑请求，外部 Operation 另用幂等身份。[admitSubmission][submissions]
- **安全重放仍要同时满足已存 policy 与当前工具的 `safe`。** 恢复 execute phase 使用已提交参数，不再走 beforeTool；否则记录 interrupted result。read/write/edit/bash 仍未声明 `replay: "safe"`，默认 unsafe。权限必须在真实执行边界检查，不能只放首次 hook。[ToolTask][tool]、[read][read-tool]、[write][write-tool]、[edit][edit-tool]、[bash][bash-tool]
- **默认扩展选择仍是全部已安装扩展。** 没有显式设置时，Conversation 默认获得 Registry 的安装列表；工具白名单、cwd 与 ownership 的目的不同于安全隔离。平台应显式绑定 App 可用扩展与工具，使用受约束环境与业务服务授权。[selectExtensions][agent]、[执行环境规范][spec]
- **Node SQLite 仍是 WAL / synchronous=NORMAL。** 可抵抗普通进程 crash，主机/断电故障可能丢最新提交；公开 Node options 仍只有 checkpoint 与 busy timeout。本轮未验收断电恢复。[Node SQLite adapter][node-sqlite]、[Storage 文档][readme]
- **Registry 仍按名字替换，不提供 AppVersion 绑定。** 同名 install 替换当前代码，已启动调用保留旧实现，后续 phase/request 使用新实现；重启时 Registry 需要重新安装。平台的扩展版本固定、迁移与旧资源保留需自行约定。[Registry 实现][registry]、[Reload 文档][readme]、[任务恢复与 handover 测试][task-recovery-tests]
- **close 与 abort 仍需区分。** Harness close 中断当前 invocation 而保留可续任务，重开将 surviving running records 回到 pending；用户取消有自己的 durable abort 语义。UI 卸载只应释放观察订阅。这是将源码行为映射到平台的建议。[scheduler open/seal][scheduler]、[Persist and Resume][readme]、[关闭恢复测试][task-recovery-tests]
- **持久等待需要运行中的宿主推进。** Scheduler 使用 JavaScript timer 与活跃进程推进，Cloudflare storage adapter 未自动装配 alarm 或外部调度。整个 Runtime 停止后的后台执行、桌面唤醒、外部进程复活仍需独立机制。这是 scheduler/adapter 范围的源码推论，尚未环境实测。[scheduler sleep][scheduler]、[Cloudflare adapter][cloudflare]

## 7. 对 Generative App Platform 与 monorepo 的影响

这一节是平台设计建议，不是 Pi 上游的目录规范。

具体工程布局、模块职责与依赖规则见 [Generative App Platform monorepo 架构方案](../design/generative-app-platform-architecture.md)。

1. **保持共享 Coding 元能力与 Coding 工作区 App 分离。** `packages/coding-runtime/engines/pi-durable` 的 Pi 适配可以集中装配 Models、Registry、Storage、ExecutionEnv，并转换平台 Task/Session；`apps/coding` 与业务 App 通过平台契约调用它。分类/路由实现可放在其内部 decision 模块或 extensions 中；1.1 不要求它们拆为独立包。
2. **能力装配归 Runtime，进程入口归 host。** 本地 `hosts/runtime` 启动与管理单 owner harness；models、权限、项目资源加载及工具选择作为 runtime 内部模块。Cloudflare adapter 可以在未来远程 host 中使用，当前无需先拆出每个 adapter 的独立 package。
3. **平台 contracts 保持自己的结果语义。** 新增 timing/order/context 通过 adapter 暴露为需要的观察与诊断字段，不能把 Durable Task terminal 直接当成 AppChange 已生效或业务 Task 已完成。
4. **App SDK 提供受控能力入口。** 业务 App 声明允许调用的 Coding 元能力与业务动作，Surface 提交上下文并订阅当前状态。App-facing SDK/contracts 不公开完整 Harness、Conversation、ToolExecutionApi 或全局 Models。工具取得完整 Models 后可以自己选择 provider/model；Conversation 的 `agent.model` 不是对此的自动权限限制。可信 routing 扩展内部可使用它，生成业务代码通过受约束的模型能力接口。AppVersion、Operation、业务修订与真实授权继续在平台管理，Registry 名字不是发布版本身份。[完整 Models API][models]、[工具 models 入口][harness-types]

建议的首个验证切片是：同一宿主与平台契约中，分别接 Coding Agent SDK 与 Durable 1.1，完成一个修改项目并运行验证的 Task，再用一个业务分类/文章动作验证共享元能力。加上重复提交、中断工具、关闭重开与过期修订的验收。历史 context、最新任务分页、耗时和 context 缓存放在同一切片中检查；后续再评估云端宿主。

## 8. 后续验收应回答的问题

| 验证 | 需要得到的证据 |
|---|---|
| 真实项目 Coding Task | 功能修改与验证结果可查；比较 SDK 与 Durable 的任务完成率、资源加载工作量与模型调用成本 |
| 1.0.4 数据升级 | 打开真实旧存储、恢复未完成 Task；时间戳缺失可处理；自定义 Storage 双向 conformance 通过 |
| 工具与 classifier 中断 | 请求已发出但结果未提交时，明确重试、费用与已执行副作用；相同判断的 memo/task 恢复规则可验证 |
| 长会话 | 测量 context 扫描、内存与耗时；确认 head/edit 变化、retention=0 与过期后视图一致 |
| 扩展更新 | task/document migration 的成功、阻塞与失败可诊断；AppVersion 实际生效、回退与待恢复任务匹配 |
| 桌面打包与恢复 | Node SQLite、执行工具在 Tauri sidecar 中可用；进程 owner、close/abort、强制杀进程与重连接正确 |
| 云端候选（按需求追加） | 真正 Workers/DO 环境的事务、驱逐/重开、外部调度与远程工具边界；不能只用 stand-in 测试代替 |

现有证据足以将 1.1.0 作为新的候选验证基线；尚不足以宣称平台已经完成 Durable 集成、获得外部副作用 exactly-once，或能够可靠升级/回退 App。

---

## 一手来源

除发布元数据、官方文章及明确标记为 1.0.4 的对照来源外，以下源码/规范/测试链接均固定到 `abe508e1b89912adde45528136c3221eb69acdd7`。

[release]: https://github.com/earendil-works/pi/releases/tag/v1.1.0
[release-api]: https://api.github.com/repos/earendil-works/pi/releases/tags/v1.1.0
[releases]: https://api.github.com/repos/earendil-works/pi/releases?per_page=10
[tag-ref]: https://api.github.com/repos/earendil-works/pi/git/ref/tags/v1.1.0
[release-commit]: https://github.com/earendil-works/pi/commit/abe508e1b89912adde45528136c3221eb69acdd7
[launch]: https://earendil.com/posts/pi-durable/
[package]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/package.json
[changelog]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/CHANGELOG.md
[readme]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/README.md
[spec]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/docs/spec.md
[harness-types]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/types.ts
[core-types]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/types.ts
[harness]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/harness.ts#L123-L140
[models-test]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-tools.test.ts#L115-L135
[context]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/context.ts
[history-context-test]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-context.test.ts#L223-L252
[context-cache-test]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-context.test.ts#L322-L393
[context-tests]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-context.test.ts
[system-order-e2e]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/system-order-cache-e2e.test.ts#L129-L144
[scheduler]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/scheduler.ts
[scan]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/storage/scan.ts
[storage-conformance]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/testing/storage-conformance.ts#L529-L591
[session]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/session/session.ts#L48-L50
[task-timestamps]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/session/transaction.ts#L948-L962
[task-time-test]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-tasks.test.ts#L310-L372
[task-time-recovery]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-tasks-recovery.test.ts#L148-L174
[tool]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/tool.ts
[tool-duration]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/tool.ts#L251-L281
[tool-duration-test]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-tools.test.ts#L431-L467
[tool-usage]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/tool.ts#L457-L480
[usage]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/usage.ts#L7-L12
[cloudflare]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/storage/sqlite/cloudflare.ts
[cloudflare-tests]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/sqlite-cloudflare.test.ts
[coding-example]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/examples/26-coding-agent.ts
[coding-tools]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/tools/index.ts
[read-tool]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/tools/read.ts
[write-tool]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/tools/write.ts
[edit-tool]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/tools/edit.ts
[bash-tool]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/tools/bash.ts
[agent]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/agent.ts#L227-L251
[submissions]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/submissions.ts#L148-L162
[node-sqlite]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/storage/sqlite/node.ts#L180-L209
[registry]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/src/harness/registry.ts#L73-L85
[task-recovery-tests]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/durable/test/harness-tasks-recovery.test.ts
[sdk]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/coding-agent/docs/sdk.md
[sdk-mcp]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/coding-agent/examples/sdk/14-codemode-mcp.ts
[ai-changelog]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/ai/CHANGELOG.md#L3-L33
[models]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/ai/src/models.ts
[classify-implementation]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/ai/src/models.ts#L972-L990
[ai-types]: https://github.com/earendil-works/pi/blob/abe508e1b89912adde45528136c3221eb69acdd7/packages/ai/src/types.ts#L292-L305
[old-ai-readme]: https://github.com/earendil-works/pi/blob/428a12bc775145afa342530a9eaa652efb3e4422/packages/ai/README.md#classification
