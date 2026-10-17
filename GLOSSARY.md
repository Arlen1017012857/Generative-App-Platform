# Generative App Platform

Generative App Platform（生成式应用平台）以 Generative App 为核心，支持用户通过 AI 创建应用，并在持续使用中塑造它们。以下术语用于区分应用的生成与演化机制、Agent 协作属性，以及多应用平台形态。

## Language

**[Generative App（生成式应用）](./docs/concepts/generative-app.md)**：
支持用户在持续使用中通过 AI 主动塑造的完整应用；用户通过应用或平台提供的产品入口表达需求，改变数据结构、业务规则、工作流与界面等应用结构与行为，形成可重复使用的能力，同时延续应用身份、相关数据与工作上下文。首次创建可由 AI 或人完成，用户主动塑造是核心能力，应用主动提出改进建议是增强能力。
_Avoid_：Generative AI App、AI-generated App、Personal Software Space、个人软件空间、Personal Software OS（作为本概念的同义词；前两者分别描述 AI 功能与生产来源）

**[Generative App Platform（生成式应用平台）](./docs/concepts/generative-app-platform.md)**：
承载多个 Generative App 的产品环境，使应用能够被创建、安装、发现、使用和持续管理。平台是应用集合的承载者，不等同于单个 Generative App。

**Agent-native**：
软件将 Agent 视为一等使用者，在声明的支持范围内提供理解上下文、发现能力、授权执行和验证结果的明确协作契约。它是应用的协作属性，不等同于由 AI 生成。

**App**：
具有独立身份、可跨会话延续的业务或工作上下文的应用，是生成、交付和演化的基本单元。

**AppVersion**：
属于同一 App 的不可变应用版本，包含该版本的应用定义与交付产物。版本生效改变 App 使用的定义，延续其身份与声明保留的业务上下文。

**AppManifest**：
某个 AppVersion 的接入声明，描述该版本的运行入口、工作界面、业务动作、能力与授权要求。

**AppChange**：
一次创建或塑造 App 的交付过程，关联需求、基线、候选版本、验证与生效结果。草稿和预览属于这一过程，同一 App 的已有版本可以继续使用。

**AppOrigin**：
App 首次建立时的来源记录，区分用户主动创建、使用反馈建议、复制已有 App、安装外部应用与平台内置。来源随 App 身份保留，后续塑造的依据由各次 AppChange 记录。

**Surface**：
App 内面向某项具体任务的工作界面；一个 App 可以包含多个 Surface，但界面数量不决定 App 的数量。
