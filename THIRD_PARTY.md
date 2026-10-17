# 第三方来源与本地修改

记录日期：2026-10-17。产品源码位于 `apps/desktop`；以下来源是固定版本，未追踪上游 HEAD。

## Ghostex

上游：[maddada/Ghostex](https://github.com/maddada/Ghostex/tree/05c27ecc77d738ad853f2db0d574bffa02a23a77)，commit `05c27ecc77d738ad853f2db0d574bffa02a23a77`。许可：[MIT](third-party/Ghostex-LICENSE)，保留原始版权和许可全文，随 `.app` 放入 Resources/third-party。

| 上游路径 | 本地路径 | 取用与修改 |
| --- | --- | --- |
| `apps/desktop/src/ghostty_vt/{mod,ffi,terminal,render_state,input_encoders,host_callbacks}.rs` | `apps/desktop/src/terminal/ghostty_vt/` | 提取 VT C ABI 包装；rustfmt；提取回调类型别名以通过 Clippy；保留完整 ABI 定义 |
| `apps/desktop/src/terminal_model.rs`、`terminal_element.rs` | `apps/desktop/src/terminal/{model,view}.rs` | 参考并适配 PTY→VT→GPUI 原生渲染路径、键盘编码和原生输入；重建精简进程所有权、快照、选择和视图；删除 zmx/wmx、远程和跨应用恢复 |
| `apps/desktop/scripts/build-libghostty-vt.sh` | `apps/desktop/scripts/build-vt.sh` | 适配项目内 Zig、vendor 源码、target 缓存及静态库输出；沿用 macOS SDK 检测 |
| `.dependencies/ghostty/` | `vendor/ghostty/` | 固定其跟踪的完整依赖源码树；只构建 VT 静态库，不构建完整 Ghostty 桌面程序 |
| `app/native_sidebar/`、`app/native_chat/`、`app/render/workarea_header/`、`app/render/view_tab_strip.rs`、`app/native_docs/`、`app/native_modal/` | `shell/`、`workspace/`、`files/`、`settings/` | 参考界面结构、信息密度、配色和交互后，用同一 GPUI Kit 谱系重建；不整包搬入 Ghostex app/store/server |

用户提供的 [参考截图](docs/design/assets/ghostex-desktop-reference.png) 作为视觉基准；不从截图中的第三方浮动图标推导功能。图标使用 GPUI Kit Assets，不提取 Ghostex 品牌、账户数据或配置。

本地后续修改：新增 `workspace/session_actions.rs`，由本项目的 SQLite/Pi 生命周期实现归档、恢复和确认删除；左右面板开关移到窗口固定位置，保持同一元素身份。沿用上述许可与视觉来源，不引入额外上游服务。

项目/输入修订：`workspace/project_actions.rs` 增加项目层级、事务删除和资源释放；`workspace/composer.rs` 用固定 Kit 的 List/Popover 组合 `/` 建议和 `@` 文件引用，读取 Pi `get_commands` 并复用现有 RPC/附件通道。以上为本地实现，不复制账户或服务端能力。

## Ghostty / libghostty-vt

源代码来自上述 Ghostex commit 的 `.dependencies/ghostty`（跟踪目录树 `7bf6084891de0f537681ad64dc6dc2889e10b4de`，不是本项目另行猜测的 Ghostty 发布版本）。保留 [Ghostty MIT 许可](third-party/Ghostty-LICENSE) 和 vendor 内各依赖原有 notice。构建依赖包一并固定在 `vendor/ghostty/zig-pkg/`；未修改 VT 算法实现。完整上游目录含其他系统和示例代码，但产品构建入口只生成 `libghostty-vt.a`。

首次 Git 导入显式添加上游忽略的 `zig-pkg/` 固定依赖，并在该目录根增加 `.gitattributes` 禁止换行转换，保留依赖包原始字节。未修改依赖包源码。

## GPUI Kit

上游：[longbridge/gpui-kit](https://github.com/longbridge/gpui-kit/tree/eec1208dd57d8bc8abc769fd833d181d297790f9)，commit `eec1208dd57d8bc8abc769fd833d181d297790f9`，crate 0.7.3。作为 Cargo 依赖启用默认组件、Assets、`syntect`，测试启用 `test-support`；无本地补丁。许可：[Apache-2.0](third-party/GPUI-Kit-LICENSE)。应用统一使用 umbrella re-export，正常构建激活 GPUI pre 0.3.9，不叠加 Ghostex 的旧 GPUI。

## 构建与其他依赖

Zig 0.16.0 使用官方 `zig-aarch64-macos-0.16.0.tar.xz`，SHA-256 `b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489`，下载和校验脚本为 `scripts/bootstrap-native.sh`。Zig 是构建工具，不随 `.app` 分发。

SQLite（rusqlite bundled）、portable-pty、notify、image、serde 等 Rust 依赖以根目录 Cargo.lock 为准确版本记录，保留其各自许可。Pi 使用本机已有安装，通过 JSONL RPC 调用，不打包 Pi、provider 凭据或账户系统。
