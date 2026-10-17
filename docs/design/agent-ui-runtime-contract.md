# Agent UI Runtime 最小接入契约

> 日期：2026-10-12（北京时间）  
> 定义修订日期：2026-10-14（北京时间）  
> 状态：职责与行为基线已确认，接口草案 v0.1  
> 基础：[平台讨论稿](../concepts/generative-app-platform.md)、[Agent-native 定义](../concepts/agent-native.md)

Agent UI Runtime 连接 App 工作界面、Agent 任务与业务模块：捕获用户的工作目标，接入明确的动作，将任务过程、持久提案与实际结果呈现在对应界面。首期以 React 和 TypeScript 接入，支持选区与完整筛选结果的语义上下文，并为纯文本和 Markdown 提供有明确范围的并发修改契约。

以下接口用于确定调用者必须知道的身份、顺序、失败和结果语义。接口名称、字段编码和具体实现仍可在原型中调整。

## 1 职责与接入范围

| 模块 | 职责 |
|---|---|
| Agent UI Runtime | Surface 注册与实例释放、上下文捕获、界面动作、任务订阅与审阅接入 |
| Coding Runtime | 理解目标、推进任务、调用工具、任务状态、输入请求与恢复 |
| App 业务模块 | 业务事实、持久提案、内容映射、执行授权与前置条件、原子提交、Operation 与结果查询 |
| App Runtime | 应用托管、版本交付、长期业务动作注册与运行资源管理 |

App 自己渲染界面，通过显式声明提供语义。声明的结构由 Runtime 验证；对象、选区和动作是否确实符合业务含义，由 App 的接入实现与场景验证保证。结构验证不单独证明 App 正确接入。

业务动作按 App 生命周期提供，不依赖某个组件存活。打开预览、滚动到段落等界面动作属于具体 Surface 实例。释放实例会移除其界面动作和订阅，不取消已创建的业务任务。

## 2 身份与上下文

`surfaceId` 标识 App 声明的工作界面，`instanceId` 标识一次挂载。同一 Surface 可以同时有多个实例；任务保存来源实例以便追溯，但业务目标绑定对象、内容版本和逻辑范围。

上下文采用公共结构与 App 业务 schema。公共结构表达来源、对象版本和读取路径；业务结构表达文本范围、选中记录或筛选结果。

```ts
type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Revision = string;
type SchemaRef = { id: string; version: string };
type ObjectRef = { type: string; id: string };
type ObjectVersion = { object: ObjectRef; revision: Revision };
type SurfaceRef = { surfaceId: string; instanceId: string };
type ActionRef = { id: string; contractVersion: string };

interface CapturedContext {
  objects: ObjectVersion[];
  schema: SchemaRef;
  data: Json;
  readActions: ActionRef[];
}

interface ContextSnapshot extends CapturedContext {
  contextId: string;
  appId: string;
  source: SurfaceRef;
  capturedAt: string;
}

interface SurfaceAction {
  id: string;
  label: string;
  inputSchema: SchemaRef;
  outputSchema: SchemaRef;
  run(input: Json): Promise<Json>;
}

interface SurfaceRegistration {
  surfaceId: string;
  role: string;
  label: string;
  contextSchema: SchemaRef;
  capture(): Promise<CapturedContext>;
  actions: SurfaceAction[];
}

interface SurfaceHandle extends SurfaceRef {
  release(): void;
}
```

修订是事实源提供的、不透明的版本标识。Runtime 不解析或自行递增修订。业务数据保留在 App 的事实存储中，上下文通过引用和读取动作按需取得内容。

### 请求时的捕获顺序

`capture` 的契约包含以下顺序，而不只是返回一个 Promise：

1. 进入捕获时，固定工作对象、编辑内容版本和选区。点击聊天框不会替换已关联的工作 Surface。
2. 将固定的编辑内容保存为可恢复的草稿版本，并保持其与选区一致。保存不能覆盖请求之后产生的用户修改。
3. 返回指向该版本的上下文。Runtime 验证结构、序列化为不可变快照并随任务持久绑定，完成后返回任务引用；不保留对 React 可变状态的引用。

保存等待期间发生的焦点变化、选区变化或新输入，不得改变这次请求。接入实现如果不能将固定内容保存且保护后续修改，应明确返回捕获失败。失败请求不会提交一个依赖错误内容的任务。

已释放实例不能作为新请求的来源。已经固定目标的请求只能继续处理原目标；无法完成时明确失败。重复提交通过稳定请求 ID 关联原任务，并校验输入绑定一致。

### 筛选结果的捕获

“分析当前筛选结果”绑定捕获时全部匹配对象的 ID 集合，包含尚未滚动加载的对象。App 可以将集合持久保存，并在 `data` 中提供集合快照引用、数量和分页读取方式。正文按需读取，并记录实际读取的版本；集合固定不表示所有正文也被冻结。

新增或移出的对象不自动改变该任务集合。超出支持规模时返回明确限制，不悄悄改为屏幕可见记录。持续监测变化采用另行声明的任务契约。

## 3 业务动作声明

业务动作由 App 接入声明关联实际实现，App Runtime 在应用运行期间提供注册。Coding Runtime 与业务界面使用同一业务契约；业务动作实现不能只是需要活动编辑器的 React 回调。

本文的文本编辑场景使用 `document.proposeEdit` 保存持久 TextEditProposal，使用 `document.applyEdit` 处理获准提案的正文提交。提案保存声明为 `sideEffect: "write"`，其授权范围是提案记录；正文提交另行检查允许效果与授权。`reviewProposal` 的接受决定调用正文提交实现，拒绝只改变审阅状态，要求修改记录反馈。展示 Diff、保存提案或传递接受参数本身均不表示正文已提交。

```ts
interface BusinessActionDescriptor {
  ref: ActionRef;
  purpose: string;
  inputSchema: SchemaRef;
  outputSchema: SchemaRef;
  sideEffect: "read" | "write" | "external";
  permissionRefs: string[];
  preconditionRefs: string[];
  resultSemantics: string;
  retry: "safe_read" | "idempotent" | "reconcile_before_retry";
}

interface ActionAvailability {
  ref: ActionRef;
  exists: boolean;
  authorized: boolean;
  executable: boolean;
  reason?: { code: string; message: string };
}
```

动作描述说明契约，能力发现返回当前可用性。存在、获准和此刻可执行分别判断；发现结果不代替提交时的执行检查。

App 身份、授权主体、执行主体与任务范围来自可信调用上下文。目标 ID、自然语言目标、`accept` 参数或生成组件自报的确认标记，均不能自行扩大权限或证明审批成立。权限与访问范围由业务执行模块和宿主约束，具体可信调用路径需要在桌面接入中验证。

## 4 持久修改提案

提案由 App 业务模块保存，任务通过引用关联。提案内容版本不可变；接受、拒绝和冲突状态单独保存。修改提案内容会产生新版本，不能替换用户已经审阅的版本。

```ts
type SnapshotRef = { id: string; target: ObjectVersion };
type TextRange = { unit: "utf16"; from: number; to: number };
type ProposalRef = { proposalId: string; proposalVersion: number };

interface TextEditUnit {
  unitId: string;
  groupId: string;
  baseRange: TextRange;
  before: string;
  replacement: string;
  logicalRangeRef?: string;
}

interface TextEditProposal {
  ref: ProposalRef;
  taskId: string;
  base: SnapshotRef;
  units: TextEditUnit[];
}

interface ProposalReviewState {
  ref: ProposalRef;
  groups: {
    groupId: string;
    state: "pending" | "conflict" | "accepted" | "rejected" | "superseded";
    operationId?: string;
    reason?: { code: string; message: string };
  }[];
}
```

文本范围使用存储文本的 UTF-16 偏移，起点包含、终点不包含。`before` 必须对应基准快照中的该范围；DOM 位置、视觉列号或经过额外规范化的文本位置不能直接作为存储偏移。

修改单元在提案版本内具有稳定身份。App 声明哪些单元可以独立提交，哪些属于不可拆分的组。组内修改一起审阅、校验和提交；重叠或相互依赖的修改不能声明为独立组。

“拒绝”结束当前修改组，不自动生成替代版本。“要求修改”记录反馈并交给 Coding Runtime 生成新版本。其他独立组仍可审阅。部分接受或全部拒绝后的任务状态由完成条件与真实结果确定，不由 Diff 面板自行标记成功。

## 5 内容映射与提交

App 保存基准内容 B 与 Agent 提案 P，提交时读取当前业务内容 C。SDK 可提供公共文本映射能力，实际映射的适用范围与业务提交由 App 负责。

```ts
type GroupMapping =
  | {
      kind: "mapped";
      groupId: string;
      currentRevision: Revision;
      edits: {
        unitId: string;
        currentRange: TextRange;
        before: string;
        replacement: string;
      }[];
    }
  | {
      kind: "conflict";
      groupId: string;
      reason: "overlap" | "target_deleted" | "ambiguous_target" | "unsupported_mapping";
    };
```

明确不重叠的编辑可以映射到 C，并保留用户其他位置的修改。双方修改同一范围、原目标被删除、映射有歧义或超出已声明支持范围时，返回对应组的冲突，保留当前内容与提案。

`mapped` 是映射结果，不是写入许可。接受时，业务模块仍须验证：

- 提案版本和修改组确实对应所审阅的内容，逻辑目标、原文与修改效果保持一致。
- 当前授权、所需用户决定及业务前置条件仍然成立。
- 组内修改完整且一致，目标范围未重叠，写入与当前修订检查原子完成。

仅位置变化且上述条件成立时，无需重复审阅。目标或效果变化时产生新提案版本；旧决定不默认授权新的修改。映射完成到提交之间如果内容再次变化，重新校验或返回明确冲突，不能无条件应用旧映射。

接受标记、正文效果与 Operation 记录需要通过业务事务或可恢复的等效流程保持一致。多组批量接受可以分别完成，但必须返回每组实际结果；不可拆分的组不能部分写入。

在线编辑变换改善实时定位。重启后使用持久的 B、P、C 及可用的逻辑目标信息恢复；无法唯一恢复时返回冲突。首期不要求永久完整编辑日志，也不将启发式文本匹配当作目标身份的证明。

## 6 Runtime 的最小调用接口

App 使用绑定于自身的 Runtime 客户端。业务动作发现、工具执行、提案读取与 Operation 查询复用平台和 App 的业务契约，不在 Surface 客户端中另建一套业务事实存储。

```ts
type TaskRef = { appId: string; taskId: string };

type TaskSubmission =
  | { kind: "accepted"; requestId: string; task: TaskRef; contextId: string }
  | { kind: "not_submitted"; requestId: string; stage: "capture" | "admission"; reason: string }
  | { kind: "unconfirmed"; requestId: string };

interface TaskView {
  ref: TaskRef;
  contextId: string;
  status: "pending" | "running" | "needs_input" | "completed" | "failed" | "cancelled";
  proposalRefs: ProposalRef[];
  operationIds: string[];
  resultRefs: ObjectRef[];
}

interface TaskObservation {
  connection: "connected" | "disconnected";
  observedAt: string | null;
  snapshot: TaskView | null;
}

type ReviewDecision =
  | { kind: "accept" | "reject"; groupIds: string[] }
  | { kind: "request_changes"; groupIds: string[]; feedback: string };

interface OperationView {
  operationId: string;
  status: "accepted" | "in_progress" | "succeeded" | "conflict" | "rejected"
    | "failed" | "cancelled" | "partially_succeeded";
  outcomeCertainty: "confirmed" | "partial" | "unknown";
  effects: Json[];
  detail?: { code: string; message: string };
}

type OperationResult =
  | { kind: "observed"; requestId: string; observedAt: string; operation: OperationView }
  | { kind: "unavailable"; requestId: string;
      lastKnown?: { observedAt: string; operation: OperationView } };

type SurfaceActionResult =
  | { kind: "succeeded"; value: Json }
  | { kind: "unavailable"; reason: string }
  | { kind: "failed"; reason: string };

interface AgentUIClient {
  attachSurface(registration: SurfaceRegistration): SurfaceHandle;

  requestTask(input: {
    requestId: string;
    source: SurfaceRef;
    taskContractId: string;
    goal: string;
  }): Promise<TaskSubmission>;

  watchTask(task: TaskRef, listener: (view: TaskObservation) => void): { release(): void };

  reviewProposal(input: {
    requestId: string;
    proposal: ProposalRef;
    decision: ReviewDecision;
  }): Promise<OperationResult>;

  invokeSurfaceAction(input: {
    source: SurfaceRef;
    actionId: string;
    input: Json;
  }): Promise<SurfaceActionResult>;
}
```

`taskContractId` 指向 App 已声明的任务约定，包括允许使用的能力、工作规则和完成条件。业务任务不因获得这个客户端就获得开发工具或其他 App 数据。接口中的引用仍需按可信 App 和任务范围校验。

`requestTask` 在目标捕获和平台任务接纳成功后返回 `accepted` 与稳定引用。确定未接纳时返回 `not_submitted` 和失败阶段；该结果不表示已保存的草稿被撤销。响应丢失、无法确定接纳结果时返回 `unconfirmed`，通过平台任务接口按原请求 ID 查询，不能换一个请求 ID 盲目重复提交。

`watchTask` 首次提供权威快照，随后提供有序视图。`observedAt` 表示最近一次获得权威快照的时间；尚未取得快照时为 null。断开时保留最后已知状态和该时间，明确连接状态；连接恢复重新取得当前快照。释放订阅不取消任务，连接中断也不将任务改为失败。补充指导、取消任务和一般输入请求复用平台任务接口。

`reviewProposal` 根据审阅动作调用 App 的业务实现。接受与拒绝返回持久处理结果；要求修改关联持久反馈与任务输入。`request_changes` 被接纳不表示新提案已经生成。

同一个请求 ID 必须绑定相同提案版本、决定和修改组。响应丢失后查询原操作或安全重试，返回原效果，不再次应用同一修改。去重有效期及超期查询方式由业务动作声明。

`OperationResult.unavailable` 表示调用方暂时无法确认结果，不是操作已失败。其完整执行、结果认知与控制语义沿用 [Operation 模型](../concepts/agent-native.md#26-推荐的统一-operation-模型)。任务成功还需满足完成条件，不能只根据一个操作返回 `succeeded` 判断。

`invokeSurfaceAction` 只调用仍可用的对应实例动作。实例不存在或动作无法执行时明确返回不可用，不改为调用当前其他实例。界面动作的完成不能代替业务保存或发布证据。

## 7 文本编辑场景推演

文章 A 在编辑器实例 E1 中打开。用户选中第三段，点击与 E1 关联的聊天入口，要求精简这一段。

1. 捕获入口先固定 A、第三段范围与编辑内容；保存草稿版本 B，创建上下文 X。任务 T 持久关联 X。
2. 用户点击另一段或打开文章 D，T 仍指向 A 的原逻辑范围。Coding 读取 B，App 保存提案 P1 与修改组 G1。
3. 用户在 A 的第一段前插入新内容，形成 C。G1 的位置发生变化，原目标与替换效果保持一致；映射得到 C 中的新范围。
4. 用户接受 P1 的 G1。业务模块读取当前内容，重算并校验映射、授权和效果，以该当前修订进行原子提交，保存 G1 的接受状态与 Operation O。用户在其他位置的编辑保留。
5. 页面呈现 O 确认的文章新修订与修改事实。T 是否完成，由任务约定与实际结果判断。

如果用户修改的是第三段本身，则第 3 步产生局部冲突。用户可以保留原文、要求修改或手动解决；新的效果需要新的提案版本。其他独立修改组仍可继续。

## 8 接入验证场景

以下是契约的验证条件，原型应通过实际行为与业务记录检查它们。

| 场景 | 必须观察到的结果 |
|---|---|
| 提交后改变焦点、选区或打开另一文章 | Task 的 contextId 与原对象、原范围保持不变 |
| 保存等待期间继续输入 | 捕获版本与选区一致，后续用户修改未被旧内容覆盖 |
| 保存失败 | 不接纳依赖该内容的任务，反馈捕获失败阶段 |
| 同一 Surface 两个实例各有选区 | 来源实例可区分，不能从另一实例读取选区 |
| 筛选结果分页，提交后集合变化 | 使用捕获时全部匹配的 ID 集合，后续变化不增减目标 |
| 用户在目标之前插入内容 | 唯一、非重叠的映射可以接受，保留用户插入内容 |
| 用户修改目标、删除目标或出现歧义 | 对应组产生冲突，保留双方内容，不猜测目标 |
| 相同文本重复出现在不同位置 | 单独的字符串匹配不足以认定逻辑目标 |
| 接受后提交前发生无关修改 | 再验证；相同目标与效果可以提交，不要求重复确认 |
| 映射后提交前再次改变目标内容 | 原子校验阻止过期映射写入 |
| 部分组接受、部分组拒绝或冲突 | 保存每组实际结果，不把整份提案或 Task 自动记为成功 |
| 不可拆组中一项校验失败 | 整组不写入，不产生部分接受状态 |
| 已提交但响应丢失，重试同一请求 | 查询或返回原 Operation，不产生重复效果 |
| 相同请求 ID 配上不同输入 | 明确拒绝输入绑定不一致 |
| 关闭并重新打开页面或程序 | 恢复任务、提案版本和审阅状态；界面动作使用新实例 |
| 拒绝与要求修改 | 前者结束当前组，后者记录反馈并产生新版本 |
| 观察连接断开 | 结果认知标明未知或最后已知，不伪造失败或成功 |

## 9 原型需要回答的问题

首个接入原型应使用文本编辑 Surface、业务草稿存储、持久提案和最小任务适配，执行上述完整流程。优先验证请求捕获与业务原子提交之间的实际顺序，而不是先扩展支持的组件数量。

需要通过实现确定的部分包括：编辑器的范围变换能力；关闭后仅凭快照与可用锚点能恢复哪些映射；提案、正文和 Operation 的事务或恢复方式；可信用户决定与宿主调用归因；任务接纳、订阅和反馈如何接入既有 Coding Runtime。

首期不要求富文本和任意业务对象采用同一合并算法。不同内容类型必须分别声明可保证的范围。AG-UI 是候选协议适配层，平台任务、上下文、提案与 Operation 契约保持独立；具体 IPC、编辑器选择与是否接入 AG-UI，通过平台接入需要与原型结果确定。

## 10 AI Coding IDE 参考

VS Code 的公开编辑跟踪实现维护基线和当前模型，对非重叠用户编辑使用位置变换并调整待审阅差异；其重叠处理不构成本平台所有用户输入均可独立保留的保证。[编辑跟踪实现](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/chat/browser/chatEditing/chatEditingTextModelChangeService.ts#L322)

VS Code 官方文档区分不同会话的修改审阅方式，并说明 extension-host 修改可逐处 Keep 或 Undo。这里借鉴逐处审阅与位置跟踪，具体并发保证采用本文的映射、局部冲突与业务提交契约。[官方审阅说明](https://code.visualstudio.com/docs/agents/run/review-code-edits#_review-extensionhost-changes)
