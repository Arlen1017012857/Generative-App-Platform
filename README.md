# Generative App Platform

> An agent-native platform for creating, using, and evolving apps.

**生成式应用平台：通过 AI 创建自己的应用，并在持续使用中塑造它们。**

Generative App Platform 承载多个具有稳定身份、持久数据和长期工作上下文的应用。用户可以创建、安装和使用应用，也可以在使用中提出新需求，通过 AI 改变应用的数据结构、业务规则、工作流和界面，并延续已有数据与工作上下文。

平台以 Agent-native 协作契约支持 Agent 理解应用上下文、发现能力、在授权范围内执行动作并验证结果。

## 命名约定

| 用途 | 名称 |
|---|---|
| 项目名称与对外主标题 | Generative App Platform |
| 中文名称 | 生成式应用平台 |
| 完整技术定位 | Agent-native Generative App Platform |
| 仓库名称 | `generative-app-platform` |

单个应用称为 **Generative App**，应用集合称为 **Generative Apps**。**Agent-native** 描述协作属性，在副标题和技术介绍中使用。术语定义见 [GLOSSARY.md](./GLOSSARY.md)。

## 项目进展

当前仓库以概念定义和架构方案为主。交付顺序是先建立 Coding 工作区，提供创建和修改软件的基础能力，再通过同一平台构建自媒体工作区，验证应用创建、日常使用与持续塑造的完整流程。

## 文档

- [术语表](./GLOSSARY.md)
- [产品与架构讨论稿](./docs/concepts/generative-app-platform.md)
- [Generative App 定义](./docs/concepts/generative-app.md)
- [Agent-native 定义](./docs/concepts/agent-native.md)
- [Monorepo 架构方案](./docs/design/generative-app-platform-architecture.md)
- [Agent UI Runtime 接入契约](./docs/design/agent-ui-runtime-contract.md)
- [Pi Durable 1.1 调研](./docs/research/pi-durable-1.1-research.md)
