# Generative App Platform · Desktop D0

Apple Silicon macOS 原生桌面初版：GPUI Kit、Pi RPC、只读 Files、本地 PTY/VT Terminal，以及四项设置。界面以固定版本 Ghostex 和用户参考图为基准。动态 App、gpui-shell、安装与更新属于 D1。

## 启动

要求 Apple Silicon macOS 14+、Xcode Command Line Tools（含 macOS SDK）、Rust 1.95.0，以及已安装并配置 provider 的 [Pi](https://github.com/earendil-works/pi)。已在当前机器上的 Pi 0.85.1 验证。应用使用 Pi 自己的凭据和 provider 配置。

```sh
./scripts/bootstrap-native.sh
cargo run -p generative-app-desktop --locked
```

首次构建需要联网获取 Cargo 依赖；bootstrap 将校验过的 Zig 0.16.0 安装到项目 `.tools`，不依赖系统 Zig。libghostty-vt 从仓库内固定的源代码构建并静态链接。GPUI Kit 固定到 `eec1208dd57d8bc8abc769fd833d181d297790f9`，Cargo.lock 纳入源码。

已生成的本地应用在 `dist/Generative App Platform.app`。重新打包：

```sh
./scripts/package-macos.sh
open "dist/Generative App Platform.app"
```

这是供本机验证的未签名开发构建，不含公证、分发或自动更新。运行时不启动 Ghostex、gxserver、CEF 或 WebView。

## 使用

用 **Open Project… / ⌘O** 选择目录；新建、切换或重命名项目下的会话。输入任务后 Send；执行中使用 Stop。模型及推理级别由当前 Pi 进程提供。文件附件以路径引用交给 Pi 的文件工具。工具结果与思考内容可展开，工具中的文件引用可进入 Files。

切换会话时工具继续执行，草稿与展开状态分别保留。工具结果显示工具名称；失败结果持续显示 **Error**，详情可展开查看，重开后仍可恢复。

会话行和标题栏的 **…** 菜单提供 **Archive Session** 和 **Delete Session…**。归档记录放在项目下的 **Archived** 分组，可查看历史并用 **Restore Session** 恢复；归档不终止正在运行的 Pi。归档会话的输入区为只读。删除需要确认，随后终止该会话的 Pi 进程和工具，清理消息、草稿及应用管理的本地 Pi 历史，保留项目文件；存储失败时保留会话并显示错误。

侧栏开关固定在 macOS 窗口按钮右侧，右侧工作区开关固定在窗口右上角。收起、展开或自动隐藏面板时，开关保持原位；支持 **⌘B / ⌘J**。

项目标题使用文件夹图标、加粗名称和目录路径；会话使用缩进的聊天行。项目的 **… → Delete Project…** 会确认清理范围，删除应用内的项目记录、全部普通/归档会话和本地 Pi 历史，并停止该项目的任务及终端；**磁盘上的项目目录与文件保留**，其他项目不受影响。数据库删除以事务提交，失败时保留项目和会话。

输入 **/** 显示命令建议：桌面提供 `/new`、`/rename`、`/model`、`/settings`、`/stop`、`/help`；Pi 的扩展、提示模板和技能通过 `get_commands` 读取并交给 RPC 执行。`/rename 新名称` 可直接重命名。输入 **@** 搜索当前项目文件，选择后插入引用并添加可移除的文件附件；空格路径自动加引号。用 **↑↓** 选择、**Enter / Tab** 插入、**Esc** 关闭，再次 Enter 才执行命令或发送；**Shift+Enter** 换行。

文件建议在后台限量搜索，最多遍历 20,000 个条目、显示 80 个结果，跳过符号链接及 `.git`、`.tools`、`node_modules`、`target`、`dist`、`vendor`；当前项目和光标变化会使过期结果失效。未由桌面或 Pi 提供的 `/` 命令会保留输入并报错，不会当作普通提示发给模型；Pi TUI 的全部内置命令不在本轮范围。接口依据 [Pi RPC 命令说明](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc-commands.md) 与本机 Pi 0.85.1 随包文档核对。

Files 左侧预览、右侧文件树；路径框按 Enter 定位或搜索。支持只读文本/代码、Markdown、图片，HTML/JS 显示源码。预览限制 8 MiB，图片另限 40 MP；二进制或不支持的格式使用系统打开。项目外的符号链接不自动读取。展开的目录与当前预览接受文件监听。

右侧 `+ → Terminal` 或 **⌘⇧T** 创建本地登录 shell。支持多终端、原生文本输入、选择、⌘C/⌘V、滚动和 resize。隐藏工作区不停止 Pi 或终端；关闭活动终端、退出应用会确认终止。重开应用会创建新终端，原 PTY 不跨应用退出存活。

**Settings / ⌘,** 仅主题、100%/125%/150% UI 缩放、Pi 路径和 Shell 路径。主题/缩放即时保存；路径通过 Save Paths 保存，影响新进程。空路径表示自动发现 Pi 或使用默认登录 shell。About 只读。

| 快捷键 | 操作 |
| --- | --- |
| ⌘N | New Session |
| ⌘⇧F | 搜索本地项目/会话 |
| ⌘B | 收起/显示侧栏 |
| ⌘J | 收起/显示右侧工作区 |
| ⌘Q | 保存状态并退出 |

## 数据与恢复

应用数据独立保存在 `~/Library/Application Support/Generative App Platform/`，不修改 Ghostex 数据。`desktop.sqlite` 保存项目、会话映射、消息投影、草稿、设置和布局；`sessions/*.jsonl` 是 Pi 的权威引擎记录。重连查询引擎状态和历史，不自动重发发送结果未知的请求。

开发时可隔离数据：

```sh
GAP_DATA_DIR=/tmp/generative-app-desktop-data cargo run -p generative-app-desktop --locked
```

## 验证与源码

```sh
cargo fmt --all -- --check
cargo clippy -p generative-app-desktop --all-targets --locked -- -D warnings
cargo test -p generative-app-desktop --locked
```

功能代码按 `shell / workspace / files / terminal / settings / storage` 组织在 `apps/desktop/src`；每个能力拥有自己的实体、后台任务和订阅，shell 负责组合及命令路由。单窗口使用一个 Kit Root。

[验收记录与原生截图](docs/verification/d0/README.md) 明确区分已验证行为和待验证项。[来源与修改记录](THIRD_PARTY.md) 记录固定版本、实际取用路径及许可。[桌面规格](docs/design/desktop-mvp-ui-and-code-scope.md) 和 [D1 架构](docs/design/generative-app-platform-architecture.md) 保留后续边界。
