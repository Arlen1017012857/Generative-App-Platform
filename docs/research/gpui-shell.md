# gpui-shell 动态 App 承载审计

核验日期：2026-10-17（北京时间）。目标：判断 `gpui-shell` 能否支撑本平台动态生成、安装、运行与更新 App，尤其是插件生命周期、权限、数据及故障边界。

交付顺序补充：先完成 [D0 Ghostex 界面的桌面基线](../design/desktop-mvp-ui-and-code-scope.md)，右侧仅 Files/Terminal；本文讨论 D1 动态 App 的采用条件，不要求桌面初版预先引入 Shell 或新增工具入口。分阶段实施见 [架构方案](../design/generative-app-platform-architecture.md#10-实施次序与本次证据范围)。

- 官方仓库固定版本：[`eec1208dd57d8bc8abc769fd833d181d297790f9`](https://github.com/longbridge/gpui-kit/commit/eec1208dd57d8bc8abc769fd833d181d297790f9)，crate 版本为 `0.7.3`，`publish = false`；研究副本 `/private/tmp/gap-gpui-kit-audit`。
- 本文依据固定版本源码、源码内测试及官方 README。官网页面、设计目标与此 commit 的实现有差别时，以源码为准；版本号相同不表示官网示例与 Git HEAD 完全一致。
- 验证方式：静态审计，未编译 GUI、未运行上游测试，未验证发布构建的 JIT、原生窗口或卸载后的资源回收。下文“已有测试”仅指源码中存在该测试。

## 结论与采用边界

**适合有条件地作为动态原生 UI 承载层，不适合作为现成的 App 安装、升级和安全运行平台。** JavaScript 模块可在不重新编译宿主的情况下组成 GPUI 界面；原生控件和已注册的 Rust 能力由宿主提供。源码具备进入文章工作台承载原型的基础，实际适用性仍需验证；App 身份、交付记录、离线依赖、授权、业务数据、迁移及回滚由 Rust 核心拥有。[README][readme]

上游仍明确写作 **M0 feasibility baseline、非稳定接口**，且完整贡献注册与使用 `PluginManager` 的 CLI 尚不齐全。采用应固定 Git SHA、封装私有适配层，不能直接把 Shell API 或安装目录当作平台长期契约。[README][readme]

建议首期：每个运行中的 `AppInstance` 使用独立 VM、独立 `Policy` 和独立 HostModule；候选版本校验使用隔离数据和独立校验进程。独立 VM 能缩小全局状态、模块缓存与 job queue 故障的影响，**仍与原生宿主共享 OS 进程、UI 线程及系统资源**，不能承诺恶意代码的进程隔离。首期仅运行平台生成并校验的第一方 App；若要运行任意不可信第三方代码，需另行设计 OS 隔离及跨进程 UI 边界。[VM 构造][engine-construct]、[共享上下文与资源限制][sandbox-budget]

## 1. 当前实际能力

| 方面 | 固定版本已实现 | 平台含义 |
|---|---|---|
| 动态界面 | ES module 默认导出 `View`；脚本描述界面，Rust 将 snapshot 变成原生 GPUI 元素 | 可动态交付脚本与资源；不是运行任意 HTML/CSS/React 的容器 |
| 样式组件 | 默认 `components` feature 内置 `gpui-component` JS module，可扩展冻结后的组件目录 | 生成器应依赖平台白名单和声明文件，而非自行装原生组件 |
| 插件发现 | `discover` 只读取 manifest，逐项返回错误；重复 ID 按目录顺序取第一个并报冲突 | 可借鉴发现逻辑，但平台应拒绝版本目录意外遮蔽 |
| 加载授权 | `PluginManager::load` 先调用布尔 `authorize(manifest)`，拒绝时不执行脚本 | 不提供持久化授权记录、权限差异比较或部分授权 |
| 生命周期 | `unload` 退休 application generation、取消 policy 任务，再释放持有的 view | 有清理基础，但不是卸载安装包、业务事务回滚或数据迁移 |
| 数据命名空间 | PluginManager 按 manifest ID 放置 data directory 与 `store.json` | ID 可跨代码路径延续；不能允许其他发布者冒用 ID |
| 热重载 | 同 VM 重新求值、构造替换对象；构造失败保留旧 view | 适用于开发预览；与事务化生产升级不同 |

表内证据：[界面与组件][readme]、[组件目录][components-doc]、[插件发现/加载/卸载][manager]、[Policy 和数据路径][plugin-policy]、[热重载][reload]。

`gpui-component-shell` 在此 commit 已不是独立 crate：其目录已并入 `gpui_shell::components`，由 `components` feature 开启，脚本 import 名称为 `gpui-component`。上游设计文档 §14.7 明确记录了这次合并，不能继续把旧名称当作待引入依赖。[合并说明][components-doc]

## 2. 会改变平台架构的加载入口限制

`PluginManager::load` 没有显式 `Policy`、policy factory 或 HostModule 注入参数。内部 `load_plugin` 从 `Policy::default()` 新建空模块表，再加入 manifest 的全部 capabilities、固定 ID 和存储路径。`authorize` 只有接受/拒绝两个结果，无法返回缩减后的权限。因此它不能直接承载“平台批准部分能力，并为该 App 注入 PlatformAPI”的需求。[加载实现][manager]、[内部 Policy 构造][plugin-policy]

虽然 `Policy::with_host_module` 是公开构建方法，但 `instantiate_view_with_policy` 与 `load_view_with_policy` 为 crate 内部方法。公开 `load_application`/`mount_application` 也未提供显式 Policy 参数：后者取默认 Policy，而前者本身会开始模块求值。不能只在创建界面之后补权限；模块顶层执行、constructor、init、render、事件及异步恢复都必须属于同一显式 Policy。[Policy API][policy]、[公开加载/挂载][public-mount]、[私有实例化][engine-load]

单 App 的 `try_load` 能使用宿主安装的默认 Policy，且不会把 manifest 请求自动变成批准；这是原型入口。多 App 平台不应以 `set_default`、`set_capabilities` 或 `export_module` 的轮换模拟主体上下文：`update_default` 的副本保留旧 capabilities，但共享 modules/store/session 句柄，修改默认模块与存储可能影响已持有这些句柄的 view。源码的每调用 Policy 设计是正确方向，平台需要把它变成公开且不可遗漏的加载契约。[单 App 加载][single-load]、[默认 Policy 共享语义][policy-default]

**落地条件**：在固定上游版本上补窄适配 seam：显式传入 Policy，在求值前绑定主体，明确失败清理和卸载所有者，并补充资源限额配置；同时避免运行时隐式联网解析依赖。这些是平台对 Interface 的要求，并非上游现有 API。首期可将依赖预先纳入包内本地引用，拒绝 Manifest 中的运行期 Git dependencies，减少加载器改动。应用交付与安装细节见 [架构方案](../design/generative-app-platform-architecture.md)。

## 3. 权限模型有效，但有明确出口

已有有价值的边界：Policy 随调用帧、view 与异步恢复携带，不依赖最后一次全局授权；文件 API 通过 `cap-std` 目录句柄执行，网络可按 host 或 HTTP scheme/port/method/path 授权；HostModule 经当前 Policy 查表，每次调用重新解析转发，可撤销。LLRT 兼容层只直接引入 `buffer/crypto/path/url/zlib`；`fs/net/os/console/fetch` 使用 Shell 自己的适配，未直接开放原始特权模块。[Policy][policy]、[文件与网络检查][capability]、[模块转发][host-dispatch]、[Standard Runtime][stdlib]

但是注册 HostModule 就是在授予该函数，Shell 不会替宿主再次检查平台 Action、Workspace、数据集或业务角色。同步 Rust closure 可能执行任意宿主 IO，VM 的中断不能抢占正在 Rust/内核中阻塞的代码。平台应只暴露短同步查询和异步 Action 入口；主体由宿主闭包绑定，不能接受脚本传来的 app_id 作为授权依据；具体权限在 Rust broker 每次调用时检查。[HostModule 边界][host-module]、[dispatch][host-dispatch]、[异步后台工作][scheduler-io]

三个需要首期直接收紧的出口：

1. **子进程不是 Shell 沙箱的延伸**：`may_run` 只匹配命令；`process.run/spawn` 清空环境、限制输出/超时并管理进程树，但子进程仍是普通 OS 进程，没有继承 `cap-std` 文件根或网络 grant。允许 shell、解释器或通用网络命令即可越过这些 JS API 约束。Pi 应由可信 Rust 核心启动，生成 App 不获 `fs.execute`。[命令授权][capability]、[进程实现][process]
2. **默认 WebView 不受同样的 network grant 检查**：`WebViewState` 的 URL 与导航只检查 `http/https/about` scheme；其 Wry 创建、`load_url`、导航处理路径没有查询 Policy 的网络权限。网页及其子资源网络不经过 Shell 的 fetch adapter。首期构建 `default-features = false`，明确开启 `quickjs, components`，关闭 `webview`；以后如开放，应独立定义 WebView 权限与网络隔离。[WebView 实现][webview]、[feature 配置][cargo]
3. **默认可读环境包含代理变量**：`default-env` 默认放行 `HTTP_PROXY/HTTPS_PROXY/ALL_PROXY`，其注释也承认 URL 可能包含用户名密码；空 manifest 请求不意味着看不到任何敏感值。平台需在该层净化这些值或缩小默认表，且不能通过修改宿主全局环境来轮换 App 权限。[环境默认表][default-env]、[环境访问检查][capability]

`Policy::new()` 默认拒绝 capabilities，但 `PluginManifest` 未写 `capabilities.storage` 时默认允许本 App 的 localStorage；`sessionStorage` 总是存在。官网“默认不授予能力”的概述不能替代该 manifest 实际默认值。[manifest 默认值及测试][manifest-storage]、[Policy session][policy]

## 4. 存储、更新与失败语义

PluginManager 的数据位置为 `<data_home>/gpui-shell/plugins/<id>/store.json`，与代码根目录分离，确实适合跨版本延续。但这只是命名空间，不包含 App 所有权、签名、schemaVersion、迁移日志或安装事务；`version` 可省略为 `unknown`，`shell-version` 仅是最低版本数值比较，也不等于平台 ABI 兼容保证。[数据目录与生命周期][manager]、[manifest 验证][manifest-version]

localStorage 是按 Policy 缓存的 JSON store，最大 8 MiB、4096 个 key、单值 1 MiB；后台写入按 revision 排队，以临时文件替换并同步磁盘，`flush` 是写入屏障。这适合 UI 设置；主要业务数据应通过 Rust 数据服务使用事务和 schema 迁移，而非把 store 当成 App 数据库。[存储实现][storage]、[持久化调度][store-drive]

热重载重用现有 Policy，重新执行模块和 constructor/init，成功后才替换 view；**没有 serialize/deserialize 状态恢复**。watch 仅观察 `.js/.mjs`，不是 manifest、权限、资源和依赖变更的完整更新检测器。新代码的 module/init 即使最终失败，也可能已调用 HostModule、写存储或发起 IO；“旧界面仍显示”不意味着这些副作用被回滚。[watch 与 reload][reload]

ES module 无法逐项从 QuickJS 模块缓存卸载：每次 load 以 generation 标记模块名，旧模块仍在 VM 缓存直到销毁。正常卸载虽能取消拥有者任务、退休组件与订阅，也不应解释为清空 VM 内全部旧代码或撤销已完成业务动作。[模块缓存说明][module-cache]、[卸载][manager]、[任务取消][cancel]

任务取消会阻止后续 JS callback 恢复，并对提供取消 hook 的操作执行取消；普通后台闭包/HostModule future 已开始的业务 IO 不因此自动获得事务回滚。例如 localStorage 持久化队列本身在后台运行。平台 Action 必须有 request_id、执行记录、幂等/去重与可查询结果，升级不得靠卸载“抹掉”进行中的写入。[后台执行][scheduler-io]、[存储调度][store-drive]

因此生产更新建议：准备不可变版本 → 校验与隔离预览 → 权限差异检查 → 停止接收新 Action 并等待/记录进行中 Action → 数据快照和受控迁移 → 新 VM 挂载候选版本 → 切换活动版本记录 → 销毁旧 VM。失败时明确区分代码切回与数据恢复；不要将 `watch` 当作升级引擎。预览和候选加载都不能写生产数据。

## 5. 资源与故障范围

固定版本确实在 `JitRuntime` 初始化时接入最大栈 **1 MiB** 和 interrupt handler；当前 watchdog 为 render/layout **2 s**、event/task **5 s**、无 scope 求值 **10 s**。在 PluginManager 的 Task scope 内加载走 Task 预算，不能一概把所有模块加载都写成 10 s。这是防无限循环的截止时间，不是流畅 UI 的帧预算，更不是每 App 累计 CPU 配额。[初始化接线][engine-construct]、[预算实现][sandbox-budget]

**当前默认没有 JS heap 上限**，源码测试还明确断言“无上限”；公开 ShellRuntime 构造入口也未提供可配置 heap/budget 参数。同 VM 内的多个插件共享 heap、globalThis、ESM 缓存和 job queue；部分原型被冻结也不能建立不同 JS realm。job queue 超过事务限制时会隔离整个 runtime 并取消其任务。每 AppInstance 独立 VM 能缩小这些逻辑故障范围，但仍不能保证主 UI 线程不被一个慢 render 阻塞，也不能隔离原生崩溃或整进程 OOM。[heap 与预算][sandbox-budget]、[VM/上下文构造][engine-construct]、[runtime quarantine][engine-load]

上游有 store、桥接值、队列等局部上限；不能将这些称作跨 App 的总资源治理。平台仍需补 VM heap 配置、Action 并发/响应大小/超时、订阅/计时器预算、版本重启与闲置回收。独立 VM 销毁是更新时释放旧 ESM 缓存的最明确边界。

发布构建需单独验证：`shell_jit_config` 在 debug 用极高阈值避免 native tier，注释记录 Linux/Windows 多短生命周期 VM 的 native 编译崩溃，而 release 保留 JIT。源码已接中断不表示已验证热函数 JIT 后的截止时间、崩溃隔离或频繁 VM 关闭行为。[JIT 配置][jit-config]

## 6. 实现前必须通过的最小验证

以下均为待执行验证，当前没有通过结论：

1. **公开适配 seam**：显式 Policy 覆盖 module 顶层、constructor/init、render、事件、timer、Promise 与异步 HostModule；两个 App 的同名模块与存储不可串用，越权 Action 被 Rust broker 拒绝。
2. **生命周期与持久化**：v1 安装、关闭重开、v2 独立 VM 切换保留业务数据；升级失败/迁移失败/磁盘写失败有可观察结果；旧 timer、订阅、child process、组件 effect 不再执行。
3. **预览隔离**：模块求值或 init 先写数据再抛错，生产库、生产授权和真实连接器均无写入；运行时不联网获取未锁定依赖。
4. **发布构建资源测试**：无限循环、深递归、持续分配、Promise 链、超大 bridge 参数、慢 HostModule 与多 VM 反复开关；覆盖 JIT 预热后路径，并观测宿主是否仍能响应。
5. **受控组件闭包**：关闭 WebView 后验证脚本无法通过未审计 native component、原生 URL handler 或执行命令绕过平台权限；默认环境不得暴露代理凭据。

可参考而尚未运行的上游测试：`plugin.rs` 的异步 init、卸载实体/订阅、manager Drop、ID 存储；`scheduler.rs` 的按 Policy/VM/generation 取消与跨实例保留；`sandbox.rs` 的动态代码、原型、栈/中断/OOM 边界。它们验证的范围不等于平台安装事务、数据迁移及 release JIT 验收。[插件测试][plugin-tests]、[调度测试][scheduler-tests]、[沙箱测试][sandbox-budget]

[readme]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/README.md
[cargo]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/Cargo.toml
[components-doc]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/docs/gpui-shell.md#L2135-L2158
[manager]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L1584-L1783
[plugin-policy]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L1299-L1419
[single-load]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L1454-L1490
[public-mount]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/mod.rs#L1532-L1572
[engine-load]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/mod.rs#L2158-L2358
[engine-construct]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/mod.rs#L1576-L1744
[policy]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/policy.rs#L60-L252
[policy-default]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/policy.rs#L254-L312
[host-module]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/host_modules.rs#L1-L106
[host-dispatch]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/host_modules.rs#L906-L990
[capability]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/capability.rs#L299-L440
[stdlib]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/standard/mod.rs
[process]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/process.rs#L75-L167
[webview]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/components/shell/media/webview.rs#L108-L245
[default-env]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/default-env
[manifest-storage]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L750-L779
[manifest-version]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L596-L625
[reload]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/watch.rs#L250-L565
[module-cache]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/mod.rs#L5384-L5401
[storage]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/storage.rs
[store-drive]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/host.rs#L695-L798
[cancel]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/scheduler.rs#L373-L424
[scheduler-io]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/scheduler.rs#L1303-L1448
[sandbox-budget]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/sandbox.rs#L668-L1341
[jit-config]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/mod.rs#L5316-L5329
[plugin-tests]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/plugin.rs#L2133-L2374
[scheduler-tests]: https://github.com/longbridge/gpui-kit/blob/eec1208dd57d8bc8abc769fd833d181d297790f9/crates/shell/src/engine/quickjs/scheduler.rs#L1917-L2050
