# Generative App Platform

> **中文名称：** 生成式应用平台  
> **完整技术定位：** Agent-native Generative App Platform  
> **英文介绍：** An agent-native platform for creating, using, and evolving apps.  
> **中文介绍：** 通过 AI 创建自己的应用，并在持续使用中塑造它们。  
> **仓库名称：** `generative-app-platform`

> 本文以 Generative App（生成式应用）为核心概念，描述从 Generative UI 到完整应用生成与持续演化的产品方向，以及 Pi、Agent UI Runtime 与相关技术在其中的架构位置。
> 当前确定的交付顺序：第一个 App 是基于 Pi 的 Coding 工作区，提供创建和演化软件的基础元能力；第二个 App 是通过该能力构建的自媒体工作区。
> **状态：** 产品与架构讨论稿；定义修订日期：2026-10-14（北京时间）。领域术语以 [GLOSSARY](../../GLOSSARY.md) 为准，工程职责与接入行为分别见 [架构方案](../design/generative-app-platform-architecture.md) 和 [Agent UI Runtime 契约](../design/agent-ui-runtime-contract.md)。AG-UI 是待原型验证的候选协议。

---

## 1. 产品愿景

目标不是做一个“AI 聊天应用”，也不是让 AI 每次临时生成一个网页，而是构建一个：

> **Generative App：能持久运行，并支持用户在持续使用中通过 AI 主动塑造的完整应用。用户通过应用或平台提供的塑造入口表达需求，改变应用的结构与行为，形成可重复使用的能力，同时延续应用身份、已有数据与工作上下文。**

应用可以由 AI 从零创建，也可以从人工开发的已有应用演进而来。用户主动塑造是核心能力，应用主动建议是增强能力；首次创建方式不决定它是否属于 Generative App。完整定义与最低成立条件见 [Generative App 定义](./generative-app.md)。

本平台应将塑造能力作为使用者可调用的产品能力提供，使用户可以针对已有 App 表达修改需求并获得可继续使用的结果。仅有开发团队在后台使用 AI 维护应用，不足以满足这一目标。

它从 Generative UI 演化而来：生成对象不再只是界面，而是包含界面、业务数据与逻辑、工作流、能力和生命周期的 App。

Generative App Platform（生成式应用平台）承载多个 Generative App，让用户能够持续创建、安装、选择和演化自己的 App，并通过 Agent-native 契约与 Agent 协作：

```text
Generative App Platform
├── Coding 工作区（基于 Pi，基础元能力）
├── 自媒体工作区（公众号等内容业务）
├── PR Review
├── 客户跟进
├── 数据分析
├── 周报生成
└── 用户未来提出的新 App
```

同一个系统，在不同用户长期使用之后，最终会“长成”完全不同的软件集合。

核心命题不是：

> AI 一次性生成一个 App。

而是：

> **把用户意图转化为可持续使用的应用，并根据明确需求和长期使用反馈不断结晶、重组和演化。**

“个人化”是应用适应用户后的结果；“Generative”才是核心机制，贯穿应用的创建、使用反馈与后续演化，而不只发生在首次生成时。

### 1.1 首个 App 与自举路径

第一个 App 是 **基于 Pi 的 Coding 工作区**，提供工作区与文件管理、代码编辑、Agent 会话、Diff、命令执行、测试、Preview 和版本产物。它为用户提供本项目创建和演化软件的基础元能力。

第二个 App 是 **自媒体工作区**。它通过 Coding 工作区内的 App Builder 流程生成、验证、预览并安装，用于验证同一平台能够交付和管理不同领域的 App。

```text
平台内核 + 首个 Coding 工作区
→ 创建/修改软件的基础元能力
→ App Builder：生成、验证、预览、安装
→ 第二个 App：自媒体工作区
→ 后续个人 App 与持续演化
```

平台内核提供所有 App 共享的注册、版本、授权、安装和运行契约。首个 Coding 工作区随平台引导版本交付，后续可通过版本化构建与安装流程演化；其首次出现不依赖一个尚未存在的 Builder。

---

## 2. 从 Generative UI 演化为 Generative App

### 2.1 从界面生成到应用生成

Generative UI 的核心问题是：

> **针对当前意图，应该呈现什么界面？**

Generative App 在此基础上进一步追问：

> **针对这个需求，应该存在什么应用，它如何持续运行并随需求演化？**

这里描述的是本产品的概念演进，而不是断言所有 Generative UI 都是临时界面、没有状态，或只能使用固定组件。Generative UI 可以是 Generative App 的交互层；区别在于生成和管理的边界是否扩展到了完整应用。

```text
Generative UI
生成适合当前任务的界面
        ↓ 扩展生成对象与管理边界
Generative App
生成包含 UI、业务数据与逻辑、工作流和能力的完整应用
        ↓ 接入运行环境并形成反馈闭环
持久运行、版本化管理、持续演化
```

| 维度 | Generative UI 的关注点 | Generative App 在此基础上的扩展 |
|---|---|---|
| 生成对象 | 界面、组件与交互呈现 | 完整 App：界面、业务数据与逻辑、工作流、能力 |
| 上下文 | 当前任务需要如何展示与交互 | 可跨会话延续的业务与工作上下文 |
| 交付结果 | 可渲染、可交互的界面 | 可验证、可安装、可再次打开的应用版本 |
| 管理边界 | UI 的渲染、状态与交互 | App 的身份、权限、版本与生命周期 |
| 演化方式 | 根据意图调整界面 | 根据需求和使用反馈修改应用，或提出新 App |

因此，Generative App 不只是“生成更多页面”，也不只是一次性代码生成。它需要把生成结果变成可运行、可管理、可持续使用的软件。

例如，用户说“帮我做一个公众号写作工具”时，Generative UI 可以生成文章列表、编辑器和预览界面；Generative App 则还要让文章可保存、工作流可继续、能力与权限可发现，并让用户下次能打开同一个 App，在保留业务数据的前提下继续修改它。

### 2.2 Generative App、Agent-native 与平台的关系

这三个概念分别回答不同的问题：

- **Generative App**：应用如何产生和演化。它是产品的核心概念，单个 App 是生成与演化的基本单元。
- **Agent-native**：Agent 如何理解应用、发现能力、在授权边界内执行动作并验证结果。AI 生成的 App 不会自动具备这些契约。
- **Generative App Platform**：多个 App 如何被构建、安装、发现、路由和管理。它是本产品承载 Generative App 的平台形态，不是单个 App 的定义。

本产品把 Generative App 与 Agent-native 结合：既让 AI 创建应用，也让 Agent 能在应用交付后持续与用户协作。具体协作契约见 [Agent-native 定义](./agent-native.md)。

### 2.3 实现自由，运行契约统一

这个方向不依赖 A2UI，也不要求 AI 只能从固定 UI 组件目录中选组件。

AI 可以自由编写 React、使用 shadcn、assistant-ui、CodeMirror、Tiptap、Canvas、WebGL，未来也可以使用更先进的 UI 技术。

真正需要被约束和标准化的是：

> **AI 生成的任何 UI，都必须运行在一个 Agent-native UI Runtime 中，并提供统一的感知、状态、动作和能力接口。**

因此核心不是：

```text
AI
↓
A2UI
↓
Renderer
```

而是：

```text
AI-generated React App
        │
        ▼
Agent UI Runtime
        │
        ├── Perception
        ├── Semantic State
        ├── Actions
        ├── Capabilities
        └── Lifecycle
        │
        ▼
平台任务与动作契约
        │
        ▼
      Agent
```

### 2.4 为什么不直接用 Codex、Claude Code、Cursor 编写应用？

首先承认：**用户完全可以直接用 AI 编程工具构建完整应用。** 这些工具也可以编写部署配置、持久化逻辑、权限系统、Agent 接口和迁移脚本；不能把它们描述成“只能生成代码或一次性网页”。

如果用户只需要一个独立应用，愿意选择技术栈并承担运行和维护责任，直接使用 AI 编程工具可能更自由、更合适。Generative App Platform 的价值不能建立在“我们比它们更会写代码”上。

核心区别是：

> **AI 编程工具帮助用户构建软件；Generative App Platform 的目标是把构建、日常使用和持续演化连接起来，并把共性的运行与管理责任变成平台服务。**

以下是本产品需要兑现的设计目标，不是当前已交付的能力声明，也不是对其他工具能力上限的判断。

#### 2.4.1 不是做不到，而是不必为每个 App 重复搭建

| 用户需要解决的问题 | 直接使用 AI 编程工具的路径 | 平台需要提供的增量价值 |
|---|---|---|
| 交付后如何再次使用 | 为项目配置运行、访问入口和持久化 | 将 App 安装为稳定的使用入口，恢复对应业务上下文 |
| 用户与 Agent 如何协作 | 为应用实现上下文、能力、授权和执行契约 | 共享 Agent-native 契约，让 Agent 理解当前对象、选择范围和可执行动作 |
| 如何安全修改正在用的 App | 为项目建立测试、发布、版本与迁移流程 | 统一草稿、验证、预览、安装与版本切换流程，明确数据兼容边界 |
| 多个 App 如何共存与复用 | 逐项集成公共服务、连接和应用入口 | 共享可授权的能力与业务服务，通过 Registry 和 Router 管理应用；共享能力不等于共享全部数据和权限 |
| 后续修改从哪里获得依据 | 用户整理问题、需求和运行证据，交给编程工具 | 在用户授权范围内，将使用上下文、失败记录与反馈转为可审核的修改建议 |

AI 编程工具能够搭建这些基础设施。平台提供的应是**现成、统一、可反复复用的服务**，而不是声称这些基础设施无法被 AI 编写。用户需要比较的是完整使用周期中的成本，而不是第一版代码的生成速度。

#### 2.4.2 从“开发一个应用”到“在使用中改进应用”

以公众号写作 App 为例：

1. 用户提出需求，生成并预览工作台，确认后安装。
2. 几天后重新打开同一个 App，继续处理已有文章，而不需要重新进入开发项目来启动它。
3. 在编辑器选中一段文字，说“把这一段改得更简洁”。这是业务操作：Agent 使用请求时捕获的选区和文档版本提出修改，而不是改写 App 源码。
4. 用户说“以后增加发布前检查”。这是应用演化：系统进入草稿构建与验证流程，展示新版本，确认后再切换已安装版本。
5. 升级需要改变数据结构时，必须先处理迁移、备份和兼容性；回退代码版本不等于回退业务数据，不能承诺无条件回滚。

平台需要把“使用 App 完成任务”和“修改 App 本身”区分开，再连接为一个连续体验。这里的价值是减少用户在开发、运行、业务上下文之间的反复切换，而不只是提供聊天生成代码的入口。

#### 2.4.3 与 AI 编程工具互补，而不是互斥

平台不需要拥有一个优于所有外部工具的 Coding Agent。当前设计以 Pi 作为首个构建引擎；Codex、Claude Code 等也可以作为未来的构建引擎候选，开发者可以使用 Cursor 等外部工具开发符合平台契约的 App。

这种互补需要明确的 AppManifest、运行契约、验证和安装流程，并不意味着现在已支持这些工具的集成。平台应围绕应用产物和运行契约建立边界，而不是让核心价值绑定在某一个模型或编程工具上。

> **编程能力越强，创建和修改 App 的成本越低；如果运行、复用与管理仍有重复成本，平台的价值就在于消除这些重复成本。**

反过来，如果编程工具与现有运行服务已经提供同等顺畅的体验，平台就必须证明额外收益，不能仅凭“Generative App”这个名称获得存在理由。

#### 2.4.4 什么情况下不应该使用平台？

- 只需要一次性脚本、原型或少量独立应用，已有运行与交付方式足够好。
- 需要特殊基础设施、独立部署或平台契约之外的架构，适配成本超过复用收益。
- 已有成熟的研发、Agent 集成和运维体系，不希望再引入一套运行环境。
- 平台没有实质降低使用与维护负担，却增加了费用、隐私风险或迁移成本。

平台最值得验证的用户，是需要持续使用和修改专用软件、但不希望为每个 App 单独承担工程管理负担的人；多个 App 之间的共性复用会进一步放大收益。用户不必完全不会编程，也不应该因为选择平台而失去对应用和数据的控制。源码与数据导出、退出路径及其限制应成为明确的产品要求，而不是未验证的承诺。

#### 2.4.5 如何证明平台值得使用？

不能只比较“谁更快生成第一个页面”。应使用同样的业务需求，对比“AI 编程工具 + 合适的现有部署和数据服务”与平台的完整路径，至少覆盖：

- 首次构建、验证、安装并完成真实任务。
- 隔天重新进入应用，保留业务数据并继续任务。
- 提出修改、处理数据兼容、发布新版本，以及应对失败。
- 再创建一个相关 App，检查公共能力的复用收益和授权隔离。

记录用户介入次数、完成时间、成功率、维护负担、总成本，以及平台带来的限制。第二个自媒体工作区应承担这一验证：它不仅证明平台能生成不同领域的 App，也要证明安装后的使用与演化，比直接维护独立项目更省心。

如果最终只做到“聊天 + 代码生成 + Preview”，那么用户直接使用 AI 编程工具是合理选择。**平台成立的条件，是交付一个可持续使用和演化应用的环境，而不是又一个代码生成入口。**

---

## 3. “App”必须成为一级领域对象

随着产品进入“多 App”模型，不能再把所有功能理解为一个 App 里的不同 Surface。

建议区分三个层次：

```text
App
└── Surface
    └── Component
```

例如：

```text
微信公众号工作台 App
├── Draft List Surface
├── Article Workbench Surface
│   ├── Markdown Editor
│   ├── Preview
│   └── Assistant Panel
├── Asset Library Surface
└── Publish Checklist Surface
```

其中：

- **App**：长期存在的业务/工作上下文。
- **Surface**：App 内某个具体工作界面。
- **Component**：React 组件或 UI 交互单元。

---

## 4. GenerativeApp / AppManifest

每个 App 以稳定身份注册到统一的 `AppRegistry` 中。App 的管理状态、不可变 AppVersion 与创建或塑造过程 AppChange 分别记录；草稿或预览不会覆盖 App 的当前有效版本。

AppManifest 属于具体 AppVersion，描述该版本的入口、Surface、业务动作、能力和所需权限。声明权限不等于已经获得授权，实际授予与执行检查由平台和业务模块负责。

示意类型：

```ts
interface GenerativeApp {
  id: string;
  name: string;
  description: string;

  origin: AppOrigin;
  managementState: "active" | "dormant" | "archived";
  currentVersionId?: string;
}

interface AppVersion {
  id: string;
  appId: string;
  manifest: AppManifest;
}

interface AppManifest {
  capabilities: string[];
  permissionRefs: string[];

  entry: {
    frontend: string;
    backend: string;
  };

  surfaces: SurfaceDefinition[];
  businessActions: ActionRef[];

  routing: {
    description: string;
    aliases?: string[];
    examples?: string[];
  };
}
```

`AppOrigin` 统一采用第 25 节的对象联合类型；`ActionRef` 沿用接入契约，`SurfaceDefinition` 表达 App 声明的工作界面。以上是领域边界示意，最终 AppManifest 格式随托管原型确定。`currentVersionId` 只能指向属于该 App 且已完成交付校验的版本；首次版本生效前可以缺省，此时尚不能作为已安装应用使用。`managementState` 表达管理状态，不代表进程正在运行。

例如，已安装的公众号写作 App：

```json
{
  "id": "wechat-writing",
  "name": "公众号写作",
  "description": "用于选题、撰写、修改、预览和发布公众号文章",
  "origin": {
    "type": "explicit",
    "request": "给我做一个公众号工作台"
  },
  "managementState": "active",
  "currentVersionId": "wechat-writing-v1"
}
```

其入口、能力与路由描述从 `wechat-writing-v1` 的 AppManifest 读取。新的修改产生独立 AppChange 与候选版本，只有成功生效后才切换 `currentVersionId`。

---

## 5. 多 App 的顶层产品形态

Generative App 平台需要一个长期存在的 App Shell，承载多个 App 的发现、切换与创建入口：

```text
┌────────────────────────────────────────────────┐
│ Generative App Platform                        │
├──────────────┬─────────────────────────────────┤
│ Apps         │                                 │
│              │                                 │
│ ✍️ 写作      │        Active App               │
│ 🔀 PR Review │                                 │
│ 📊 数据      │                                 │
│ 👥 客户      │                                 │
│              │                                 │
│ + New App    │                                 │
│              │                                 │
├──────────────┴─────────────────────────────────┤
│ Ask anything...                               │
└────────────────────────────────────────────────┘
```

用户既可以：

- 点击 App；
- 直接说 App 名；
- 用自然语言描述一个任务；
- 请求创建一个全新的 App；
- 根据上下文自动停留在当前 App。

---

## 6. App Router：TypeSafe/Jev 的核心位置

TypeSafe/Jev 适合做快速、结构化、低延迟的判断层，而不是完整 Agent。

它最适合承担：

> **在当前用户请求和上下文下，应该进入哪个 App。**

示意：

```text
User Input
    │
    ▼
App Router
    │
    ├── deterministic rules
    │
    └── TypeSafe / Jev
          │
          ▼
      RouteDecision
```

候选结果可以是：

```text
wechat-writing
pr-review
customer-crm
data-analysis
global-agent
new-app
```

但最终路由应该由程序代码执行，而不是 Jev 直接控制界面。

---

## 7. App Routing 的优先级

不是所有导航都需要 AI。

建议：

```text
① 用户点击某个 App
   → 直接打开

② 用户明确说 App 名
   → alias/name 确定性匹配

③ 用户正在当前 App 中进行明确操作
   → 保持当前 App

④ 自然语言存在歧义
   → TypeSafe/Jev App Router

⑤ 没有合适 App
   → Global Agent

⑥ 明确是在构建新软件
   → App Builder
```

因此：

> **AI Routing 不是所有导航的唯一入口。**

---

## 8. App Router 与 Agent Router 分开

未来最好存在两层 Routing：

```text
User
 ↓
App Router
 ↓
当前应该进入哪个软件环境？
 ↓
Agent Router
 ↓
这个任务应该由谁处理？
```

例如：

### “帮我改一下今天文章的开头”

```text
App Router
→ wechat-writing

Agent Router
→ Pi
```

### “打开昨天的文章”

```text
App Router
→ wechat-writing

Agent Router
→ deterministic command
```

不一定需要 LLM。

---

## 9. 用户主动创建 App 是第一等路径

产品不能只依赖 Agent 被动观察用户，然后建议生长界面。

应该存在两条并行 Growth Path。

### 主动构建

```text
User
↓
“帮我做一个公众号工作台”
↓
App Builder
```

### 被动生长

```text
Repeated Usage
↓
Growth Engine
↓
“你经常做这件事，要不要做成一个 App？”
```

两条最终汇合：

```text
                    App Builder
                   ▲           ▲
                  /             \
                 /               \
      Explicit Request       Growth Candidate
```

---

## 10. App Builder

App Builder 是统一的软件构建入口，首期作为 Coding 工作区内的构建能力提供。Global Agent 或 Growth Engine 可以创建 BuildRequest 并转交该流程，不需要另建一个与 Coding 工作区重复的构建 App。

用户可以说：

> 我想要一个公众号文章工作台，左边管理文章，中间写 Markdown，右边让 AI 帮我修改，还要能预览公众号最终效果。

系统进入：

```text
Coding 工作区
↓
App Builder
↓
Pi
↓
检查已有代码与能力
↓
生成/修改 React App
↓
接入 Agent UI Runtime
↓
运行
↓
测试
↓
Preview
↓
Install
↓
AppRegistry
```

用户看到的不是一次性代码生成，而是：

```text
Preview
↓
继续修改 / 安装 App
```

安装之后，这个 Generative App 成为平台中可再次打开、继续使用和演化的应用，而不是随本次会话结束而丢弃的生成结果。

构建发生在草稿工作区；安装将经验证的不可变版本关联到用户的 App 安装实例。正在使用的 App 版本不会随草稿文件编辑而被直接改写。

---

## 11. AI 不需要被限制只能组合固定组件

我们最终明确：

> **AI 可以自由编写组件和 App。**

未来模型能力越强，越不应该把它限制在固定的组件 Catalog。

但自由编写不等于失去协议能力。

需要保证：

```text
任何 AI 生成的 UI
↓
必须接入 Agent UI Runtime
```

也就是：

```tsx
<AgentAppProvider appId="wechat-writing">
  <GeneratedApp />
</AgentAppProvider>
```

需要语义感知的区域则注册为：

```tsx
<AgentSurface
  id="article-editor"
  role="document-editor"
>
  ...
</AgentSurface>
```

---

## 12. Agent UI Runtime

这是整个产品最关键的前端基础设施之一。

它应该提供：

```text
Perception
State
Affordance
Action
Lifecycle
```

### Perception

Agent 能知道：

```text
当前 App
当前 Surface
当前 focus
用户选择了什么
当前正在操作什么对象
```

### Semantic State

Agent 获取的是业务语义状态，而不是 React 内部状态。

例如：

```ts
{
  app: "wechat-writing",

  surface: "article-editor",

  document: {
    id: "article_123",
    revision: 17,
    title: "AI 产品正在发生什么变化"
  },

  selection: {
    from: 123,
    to: 181,
    text: "过去两年，AI 产品..."
  }
}
```

### Affordance

告诉 Agent：

> 当前可以做什么。

例如：

```text
document.read
document.proposeEdit
document.applyEdit
openPreview
document.publish
```

### Action

Agent 可以通过正式 Action/Tool 调用操作 UI 或业务状态。

### Lifecycle

Runtime 知道：

```text
App mounted/unmounted
Surface active/inactive
focus/visibility
capability registration/disposal
```

---

## 13. Agent Surface：类似 “ARIA for Agents”

这个抽象可以类比 Web Accessibility。

普通网页中，人能看出一个元素是按钮，但 accessibility 工具未必能理解，因此需要：

```html
<button />
```

或：

```html
role="button"
aria-label="..."
```

在 Agent-native UI 中同样需要：

> **Agent Semantic Layer**

例如：

```tsx
<AgentSurface
  id="article-editor"
  role="document-editor"
  label="公众号文章编辑器"
/>
```

或者：

```ts
useAgentSurface({
  id: "article-editor",
  role: "document-editor"
});
```

核心思想：

```text
视觉结构
≠
Agent Semantic Structure
```

---

## 14. 公众号文章工作台示例

用户在一个 Markdown 工作台中：

```text
┌────────────────────────────────────────────────────┐
│ 公众号文章                              [预览][发布] │
├───────────────┬───────────────────────┬────────────┤
│ Outline       │ Markdown Editor       │ AI         │
│               │                       │            │
│ 1. Intro      │ # AI 产品...          │ assistant  │
│ 2. Changes    │                       │            │
│ 3. Future     │ 过去两年... █████     │            │
│               │                       │            │
├───────────────┴───────────────────────┴────────────┤
│ 字数 2,351 · 阅读约 8 分钟 · 已保存                 │
└────────────────────────────────────────────────────┘
```

用户选中一句话，然后说：

> 这一段太书面了，改得自然一点。

用户提交请求时，Agent UI Runtime 固定来源实例、文档、编辑内容和选区，保存一致的草稿版本，并把不可变 ContextSnapshot 绑定到任务。后续焦点或选区变化不改变任务目标：

```text
appId = wechat-writing
source = article-editor / editor_1
documentId = doc_123
revision = "42"
selection = 保存版本中的 UTF-16 范围 [123, 144)
```

界面使用绑定于 App 的客户端提交任务：

```ts
const submission = await agentUI.requestTask({
  requestId: "request_edit_doc_123_1",
  source: { surfaceId: "article-editor", instanceId: "editor_1" },
  taskContractId: "article.propose-edit",
  goal: "这一段太书面了，改得自然一点。"
});
```

`requestTask` 完成目标捕获与任务接纳后返回稳定任务引用；未接纳或结果未确认时按接入契约处理，不能假定任务已经开始。Coding Runtime 读取捕获的版本并生成修改，App 业务模块通过 `document.proposeEdit` 保存不可变提案版本，例如：

```ts
const proposal: TextEditProposal = {
  ref: { proposalId: "proposal_1", proposalVersion: 1 },
  taskId: "task_123",
  base: {
    id: "snapshot_doc_123_42",
    target: { object: { type: "article", id: "doc_123" }, revision: "42" }
  },
  units: [{
    unitId: "edit_1",
    groupId: "group_1",
    baseRange: { unit: "utf16", from: 123, to: 144 },
    before: "过去两年，AI 产品主要经历了巨大的变化。",
    replacement: "短短两年，AI 产品已经从聊天框跨进了软件系统的核心。"
  }]
};
```

`task_123` 是接纳结果中的任务 ID；这里的类型与范围语义沿用 [接入契约](../design/agent-ui-runtime-contract.md)。保存提案会产生持久记录，但此时不修改正式正文。前端根据该提案展示 Diff：

```diff
- 过去两年，AI 产品主要经历了巨大的变化。
+ 短短两年，AI 产品已经从聊天框跨进了软件系统的核心。
```

用户可以：

```text
[接受] [拒绝]
```

接受时绑定所审阅的提案版本与修改组：

```ts
const result = await agentUI.reviewProposal({
  requestId: "review_proposal_1_group_1",
  proposal: proposal.ref,
  decision: { kind: "accept", groupIds: ["group_1"] }
});
```

App 业务模块将基准内容、提案与当前内容映射，检查目标与效果、有效授权及审阅决定，并在当前修订检查与正文写入的原子提交中保存修改组状态和 Operation。无关编辑可以保留；目标重叠、被删除或映射不唯一时返回局部冲突。拒绝只结束当前组，不修改正文；要求修改会形成新提案版本。

界面与任务根据持久 Operation 和实际完成条件确认结果。响应丢失时查询或安全重试原请求，不重复应用修改。完整顺序是“捕获目标 → 保存提案 → 审阅 → 原子提交 → 确认结果”，业务执行不依赖编辑器继续打开。

---

## 15. AG-UI 的角色

AG-UI 不是 UI Schema，也不是 App Builder。

它是 Agent UI Runtime 与 Coding Runtime 之间的**候选协议适配层**，是否接入通过原型确定；平台任务、上下文、提案和业务 Operation 契约保持独立。若采用，它承担：

> **Agent ↔ UI 的交互协议层。**

主要承载：

```text
message
tool call
state
activity
run lifecycle
frontend action
shared state
```

候选接入路径：

```text
Agent UI Runtime
      ↓
AG-UI 适配（候选）
      ↓
Coding Runtime / Pi
```

接入原型需要验证平台上下文、任务观察、输入请求与业务动作如何映射到协议事件。协议事件不能替代提案持久化、执行授权或业务结果证据。

---

## 16. AG-UI Shared State 的正确边界

以下边界用于评估采用 AG-UI 的候选实现，不表示平台已经选定该协议。

Shared State 很适合传：

```text
activeApp
activeSurface
documentId
revision
selection
currentEntity
currentSection
dirty
```

但不适合把所有大数据都塞进去。

例如一篇 20,000 字 Markdown，不建议每次都放在 Shared State。

推荐分成三层：

### App / Business State

真正业务数据：

```text
文章正文
订单
数据库记录
任务
客户
```

由业务 Service 管理。

### Agent Context State

Agent 需要感知的 UI 语义：

```text
activeApp
activeSurface
selection
focus
currentDocument
currentSection
```

若采用 AG-UI，可通过 `RunAgentInput.state` 等机制传递；任务仍绑定请求时捕获的不可变上下文。

### Live Presence State

非常瞬时的状态：

```text
mouse
hover
scroll
drag
正在输入
selection 实时变化
```

默认不要进入 Agent。

只有需要实时协作时再单独设计 Live Context Channel。

---

## 17. AG-UI Shared State 不是完整实时感知通道

重要边界：

前端更新 Shared State 后，很多场景下是在下一次 Agent Run 时进入 `RunAgentInput.state`。

因此：

```text
用户选中文字
↓
用户提交：“帮我改自然一点”
↓
新一轮 RunAgentInput
↓
Pi 知道 selection
```

这是最自然的模式。

如果需要：

```text
Agent 正在执行
↓
用户此时改变 selection
↓
Agent 当前运行过程立刻感知
```

则需要单独增加：

```text
Live Context Bus
```

例如基于 WebSocket：

```json
{
  "type": "ui.context.patch",
  "surfaceId": "article-editor",
  "patch": [
    {
      "op": "replace",
      "path": "/selection",
      "value": {
        "from": 123,
        "to": 181
      }
    }
  ]
}
```

这个机制不需要强塞进 AG-UI 的普通 Shared State 模型。

---

## 18. assistant-ui 与 shadcn 的位置

### shadcn

负责：

```text
Design System
UI primitives
高级 UI 组件实现
```

AI 可以自由基于它进行开发。

### assistant-ui

负责：

```text
Agent Interaction UI
Thread
Message
Composer
Tool UI
Approval
Attachments
Reasoning
Agent status
```

若原型采用 AG-UI，可以评估将它作为前端交互实现基础，具体事件与平台契约的映射仍需验证。

但你的长期核心领域模型不应直接绑定 assistant-ui 当前的具体 API。

---

## 19. Cordis 的角色

Cordis 最适合成为：

> **Capability Runtime / App Runtime 的生命周期管理底座。**

尤其在多 App 场景中：

```text
Root Context
│
├── Global services
│   ├── agent
│   ├── router
│   ├── appRegistry
│   └── storage
│
├── App: wechat-writing
│   ├── document service
│   ├── asset service
│   ├── agent surfaces
│   └── app-specific tools
│
├── App: pr-review
│   ├── github
│   ├── diff
│   ├── tests
│   └── tools
│
└── App: customer-crm
    ├── contacts
    ├── email
    └── followups
```

一个 App 可以近似理解为一个 Cordis scope / subtree。

当 App 激活时，只注册：

```text
global tools
+
app-specific tools
+
surface-specific actions
```

这样不会出现：

```text
几十个 App × 每个十几个 Tool
= 巨大的 Tool 上下文
```

---

## 20. TypeSafe/Jev 的其他价值

除了 App Router，它还适合承担：

```text
这个需求是不是在构建新 App？
这个行为是不是一个稳定 workflow？
当前需求是不是应该继续留在当前 App？
这个 App 是否应该被建议归档？
某个 Growth Candidate 是否值得提出？
```

但是：

> **Jev 提供判断信号，程序代码做最终决策和副作用。**

---

## 21. Pi 的角色

Pi 主要负责：

```text
复杂推理
Coding
Tool use
构建/修改 App
分析复杂业务任务
执行多步骤工作流
```

例如：

```text
User:
“帮我做一个公众号工作台”
↓
第一个 App：Coding 工作区 / App Builder
↓
Pi
↓
理解需求
生成 React
接入 Agent UI Runtime
运行
测试
预览
↓
安装第二个 App：自媒体工作区
```

Pi 不是 App Router，也不应该处理所有简单导航。

Coding 工作区的构建任务与自媒体工作区的业务任务可以共用 Pi 底座，但使用不同的工具集、工作区和授权范围。构建软件的权限不能自动成为修改已安装 App 或对外发布内容的权限。

---

## 22. DeepSeek Harness 与本产品的差异

DeepSeek Harness 已经具备：

```text
everything-is-a-plugin
Cordis
Agent loop
Session log
Tool registry
Web UI
Plugin Manager
Creator Mode
持久安装 UI / Capability Plugin
```

所以差异不能简单描述为：

> “Harness 是固定 UI，我们是动态 UI。”

真正区别是：

### DeepSeek Harness

更关注：

> **How should an agent be composed?**

即：

```text
Agent
Plugin
Session
Capability
Runtime
```

### 本产品

更关注：

> **What software should exist for this person?**

即：

```text
User
Intent
App
Workflow
Surface
Usage
Growth
```

特别是：

```text
Observe
↓
Understand
↓
Generalize
↓
Build / Crystallize
↓
Evaluate
↓
Adapt / Decay
```

这一整套“Generative App 持续演化闭环”才是产品核心：从生成界面扩展到创建完整应用，再用需求与使用反馈驱动后续演化。

---

## 23. 主动创建与被动成长并存

最终 Growth 模型：

```text
                    User App Graph
                           ▲
             ┌─────────────┴─────────────┐
             │                           │
     Explicit Creation             Implicit Growth
             │                           │
     “帮我做一个...”            repeated behavior
             │                           │
             └──────────┬────────────────┘
                        ▼
                    App Builder
                        │
               Preview / Validate
                        │
                        ▼
                   App Registry
                        │
                        ▼
                    Usage Events
                        │
                ┌───────┴───────┐
                ▼               ▼
              Adapt           Decay
```

---

## 24. App 生命周期

生命周期分为 App 管理与 AppChange 交付两个维度。AppVersion 是不可变的交付版本；被某个 App 使用、被候选预览或被未完成任务引用，不改变版本内容。

App 的 `managementState`：

```text
active ↔ dormant
active / dormant → archived
archived → active（用户恢复）
```

`active` 表示正常管理，`dormant` 表示暂时休眠，`archived` 表示已归档；它们不表示进程状态或版本交付进度。首次安装和后续更新都通过 AppChange 交付：

```text
记录需求与基线 → 草稿工程 → 候选 AppVersion
                         → 预览 / 验证
                         → 授权、兼容与迁移检查
                         → 生效并切换 App.currentVersionId
```

这些是交付步骤，不是 App 的互斥状态枚举。例如，App 当前使用 v1，新的 AppChange 可以同时预览 v2；预览绑定独立数据与能力范围。只有交付成功才切换到 v2，失败时保留或明确恢复当前有效版本与数据状态。App 的身份、来源与声明保留的业务上下文持续延续，未完成任务引用的版本按架构约定保留。

用户始终保有控制权。

例如：

```text
My Apps

✍️ 公众号写作
[打开] [编辑 App] [...]

🔀 PR Review
[打开] [编辑 App] [...]

📊 Weekly Metrics
[打开] [编辑 App] [...]

+ Build an App
```

支持：

```text
Rename
Customize
Duplicate
Archive
Delete
Inspect capabilities
Inspect permissions
```

系统可以建议归档，但不应该神秘自动删除。

---

## 25. App 的来源需要记录

`GenerativeApp.origin` 统一使用 `AppOrigin`。来源记录 App 首次建立的依据，后续更新的需求与基线由各次 AppChange 记录，不改写来源。

```ts
type AppOrigin =
  | { type: "explicit"; request: string }
  | { type: "observed"; workflowId: string }
  | { type: "fork"; sourceAppId: string }
  | { type: "installed"; package: string }
  | { type: "system" };
```

`explicit` 表示用户主动创建，`observed` 表示依据使用反馈提出并获授权创建，`fork` 表示复制已有 App，`installed` 表示安装外部来源的应用，`system` 表示平台内置。安装用户创建或复制的 App 不会把其来源改为 `installed`；安装事实由当前有效版本与交付记录表达。复制形成新的 App 身份，并以 `sourceAppId` 保留来源关系。

这能用于：

```text
Growth Engine
产品评估
训练数据
用户行为研究
版本追踪
```

---

## 26. App Archetype / App DNA

App Builder 不应该每次从零开发。

可以逐渐沉淀：

```text
ContentWorkbench
CRMWorkbench
AnalysisWorkbench
CodingWorkbench
OperationsWorkbench
```

例如用户要构建：

```text
小红书运营工作台
```

可以从：

```text
ContentWorkbench
+
document editor
+
asset manager
+
AI rewrite
+
platform-specific preview
```

进行派生。

这不是限制 AI，而是让 AI 有更好的可复用软件资产。

---

## 27. 自动验收比限制生成更重要

不要只依赖 Prompt：

> “请记得支持平台接入契约。”

AI 生成 App 后需要自动做 Agent UI Compliance Check。

例如：

```text
✓ registered App
✓ registered Surface
✓ stable IDs
✓ semantic state serializable
✓ action schema valid
✓ no DOM object in shared state
✓ state size within threshold
✓ document mutations use revision check
✓ permission declaration complete
✓ actions discoverable by Agent
```

甚至自动测试：

```ts
test("article workbench exposes current selection", () => {
  selectText("hello");

  expect(agentContext.selection.text)
    .toBe("hello");
});
```

最终原则：

```text
AI 自由写代码
+
Runtime Contract
+
Compiler/Linter
+
Tests
```

而不是：

```text
AI 只能从固定组件目录里拼页面
```

---

## 28. 最终核心架构

```text
┌─────────────────────────────────────────────┐
│           GENERATIVE APP PLATFORM           │
├─────────────────────────────────────────────┤
│                                             │
│  ① App Registry                            │
│     管理当前有哪些 App                      │
│                                             │
│  ② App Router                              │
│     决定当前应该进入哪个 App                │
│     TypeSafe/Jev + deterministic rules      │
│                                             │
│  ③ App Builder                             │
│     Pi 负责理解需求、开发、修改 App          │
│                                             │
│  ④ Agent UI Runtime                        │
│     App / Surface / Context / Action        │
│     Perception / Capability / Lifecycle     │
│                                             │
│  ⑤ Capability Runtime                      │
│     Cordis                                  │
│                                             │
│  ⑥ Growth Engine                           │
│     从使用模式提出新 App / 改造已有 App     │
│                                             │
└─────────────────────────────────────────────┘
```

交互主路径：

```text
                 User
                  │
           ┌──────┴───────┐
           │              │
        direct         language
         click            │
           │           App Router
           │              │
           └──────┬───────┘
                  ▼
             Active App
                  │
            Agent UI Runtime
                  │
         平台任务与动作契约
                  │
            Agent / Actions
```

AG-UI 可作为这一路径的协议适配候选，是否采用不改变平台契约的职责与结果语义。

构建路径：

```text
User
 ↓
“帮我做一个 App”
 ↓
App Builder
 ↓
Pi
 ↓
Generated App
 ↓
Agent UI compliance
 ↓
Preview
 ↓
Install
 ↓
App Registry
```

---

## 29. 技术角色总结

| 技术 / 模块 | 主要职责 |
|---|---|
| **Pi Coding Agent** | 深度推理、Coding、工具调用、构建/修改 App |
| **AG-UI（候选）** | Agent ↔ UI 的协议适配；是否接入由原型确定 |
| **Agent UI Runtime** | UI 的 Agent 感知、语义状态、动作、能力与生命周期 |
| **assistant-ui** | Agent 聊天与 Tool UI 基础能力 |
| **shadcn** | 设计系统和 UI 实现基础 |
| **Cordis** | Capability / Plugin / App 生命周期与作用域 |
| **TypeSafe / Jev** | App Routing、Intent Routing、Growth 判断 |
| **App Registry** | 多 App 的安装、发现、生命周期管理 |
| **App Builder** | 将用户需求转成可运行 App |
| **Growth Engine** | 从重复行为中发现新 App / App 演化机会 |
| **Document / Business Services** | 业务数据的 canonical source of truth |
| **Event Store** | 使用行为、App 演化、Workflow 轨迹 |

---

## 30. 当前最值得优先设计的核心类型

建议下一阶段先稳定以下领域模型：

```text
GenerativeApp
AppManifest
AppRuntimeContext
Surface
AgentContext
AgentAction
Capability
RouteDecision
BuildRequest
GrowthCandidate
```

其中最核心的几个：

```ts
GenerativeApp
AppRuntimeContext
AgentSurface
RouteDecision
BuildRequest
```

先稳定这些领域边界，再验证 Pi、Jev、Cordis、assistant-ui 等接入实现；AG-UI 是否采用及如何映射平台契约由原型确定。

---

## 31. 最终产品定义

核心概念是：

> **Generative App（生成式应用）：从 Generative UI 演化而来，将 AI 的生成边界从界面扩展到完整应用，并支持应用交付后的持续使用与演化。**

本产品的定位是：

> **Generative App Platform（生成式应用平台）：通过 AI 创建自己的应用，并在持续使用中塑造它们。**

完整技术定位为 **Agent-native Generative App Platform**，英文介绍为：

> An agent-native platform for creating, using, and evolving apps.

三者的关系可以概括为：

```text
Generative UI   → 生成界面：应用的交互层
Generative App  → 生成与演化完整应用：产品的核心概念
App Platform   → 承载和管理多个应用：本产品的平台形态
```

AI App Builder 是创建和修改应用的入口，但不能独自概括应用交付后的运行、协作、管理和演化。软件不再由产品团队一次性预先决定全部形态，而是在稳定的 Agent Runtime 和 App Runtime 之上，随着用户需求持续成长。

最终核心原则可以概括为：

```text
AI 可以自由创造 UI
但 UI 必须 Agent-native

App 可以不断生长
但必须可安装、可管理、可路由、可归档

Agent 可以理解用户
但重要决策要经过结构化判断和程序控制

用户可以被动获得建议
也必须能主动提出要构建什么软件
```
