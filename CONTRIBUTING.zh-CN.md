# 参与 Cirrus 贡献

感谢你愿意花时间了解 Cirrus。不管是改一个错别字、补一段文档，还是修认证流程里的某个边界问题，都很有价值。

## 从哪里开始

1. 先读 [README.zh-CN.md](README.zh-CN.md) 与 [docs/testing/pr-preflight.md](docs/testing/pr-preflight.md)，把工程结构和检查方式过一遍。
2. 从 Issue 里挑一个范围清晰、你能自己验证的问题。不要只看标题：读完正文、评论和复现步骤，确认没有人在做。
3. Fork 仓库，新建分支开发，然后向 `main` 提 PR。
4. 拿不准改哪里时，优先选择**可以离线验证**的部分：SRP/密码学派生、wire DTO 映射、CloudKit schema 构造、坐标纠偏这类纯逻辑都有真实单测，改动后跑 `make cargo-test` / `make test` 就能确认。涉及真实账号的链路（认证、2FA、相册）没有自动化测试，请谨慎评估风险。

## 开发环境

### 环境要求

- Node.js >= 22.13.0（`.nvmrc` 固定为 `22.13.0`，直接 `nvm use`）
- pnpm 10.10.0（`package.json` 的 `packageManager` 字段固定，建议用 Corepack 或已安装的对应版本）
- Rust >= 1.88.0
- `make`

**各平台依赖：**

- Windows：Visual Studio C++ 生成工具、Windows SDK 和 WebView2。
- macOS：Xcode Command Line Tools。
- Linux：`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf`、`libsecret-1-dev` 和 `libdbus-1-dev`。

### 本地运行

```bash
git clone <your-fork-url>
cd cirrus
nvm use
pnpm install      # 或 make install
make
```

`make` 会在需要时安装依赖，并启动 Tauri 桌面开发环境：Vite 固定监听 1420 端口，Tauri 的 `devUrl` 指向 `http://localhost:1420`。端口被占用时 Makefile 会直接报错退出，不会连到别的进程上。

常用快捷命令：

```bash
make dev-web       # 只启动前端开发服务器（非 Tauri 下用 5173）
make build         # 类型检查 + 构建前端产物
make check         # 格式 + lint + 类型 + 前端测试
make cargo-clippy  # clippy，警告视为错误
```

### 只调后端

`crates/cirrus-core` 不依赖 tauri，可以单独编译与测试：

```bash
cargo test -p cirrus-core
cargo check -p cirrus-core
```

## 项目结构

| 路径 | 说明 |
| --- | --- |
| `apps/desktop/src/` | Vue 3 前端；`lib/api.ts` 是唯一的 `invoke` 入口 |
| `apps/desktop/src/styles/` | Tailwind 入口（`globals.css`）与 `tokens.css` 里的设计 token |
| `crates/cirrus-core/` | 共享核心逻辑：SRP 密码学、Apple 协议、会话管理、对外 DTO；**不依赖 tauri** |
| `crates/cirrus-core/src/apple/` | 端点常量、请求头常量、wire DTO |
| `crates/cirrus-core/src/services/` | 业务流程：`auth` / `devices` / `photos` / `media` |
| `src-tauri/` | Tauri 薄壳：入口、插件、`generate_handler`、应用状态与 command 层 |
| `src-tauri/src/commands/` | `session` / `devices` / `photos` 三组 command |
| `scripts/run-check.mjs` | `pnpm check` 的入口：格式 / lint / 类型 / 测试 四类并行 |
| `docs/testing/` | 提交与 CI 核查文档 |
| `dist/` | 前端构建产物（由 `apps/desktop/vite.config.ts` 的 `outDir` 生成） |

## 开发约定

### 分支命名

分支名尽量简短明确，例如：

- `docs/pr-preflight-checklist`
- `fix/session-cleanup-race`
- `feat/photos-live-photo-download`

### 控制改动范围

一个 PR 只做一类事。文档 PR 不要夹带无关代码；修 Bug 时也不要顺手大重构，除非重构是修复所必需的。

几条这个仓库特有的边界：

- **不要把 Apple 的字符串搬出常量模块**：新增端点/请求头/字段名时，加到 `apple/api.rs`、`apple/http.rs` 或 `services/photos.rs` 的 `cloud_kit_schema` 里，业务代码只引用常量。
- **前端不要绕过 `lib/api.ts`**：页面组件不直接 `invoke()`。
- **不要让 `cirrus-core` 依赖 tauri**：它是可以被单独编译和测试的纯逻辑 crate。
- 改动 `src-tauri/tauri.conf.json` 里的 CSP 时，请在 PR 描述里说明为什么需要放宽。

### 提交说明

提交信息用自然语言写清楚即可，约定式提交前缀：

- `docs: add pr preflight checklist`
- `fix(session): expire idle sessions before eviction`
- `feat(photos): support live photo download`

## 测试要求

按改动范围跑对应检查：

```bash
make cargo-check
make cargo-test
make cargo-clippy
make check      # 前端四类检查
```

四类检查是**不同的检查，不能互相替代**：格式过了不代表类型过了，测试过了不代表 lint 过了。改到 Rust 至少跑 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`。

测试质量比测试数量更重要：

- 调用生产函数，验证可观察的结果、状态变化与错误分支。Mock 外部边界（HTTP、文件系统、时间），不要 Mock 正在验证的行为。
- 不要在测试里复制实现，也不要用源码字符串匹配锁定函数名、字段名或模板片段。这类检查会阻碍无害重构，却不能证明运行行为。
- 回归用例优先补进已有行为测试，不另加一份「接线快照」。只有输入和预期不同的场景优先用参数化用例。
- 涉及真实 Apple 账号的链路（SRP 认证、2FA 下发与校验、CloudKit 分页）无法自动化，不要在测试里假装能跑；把可离线验证的部分（派生向量、DTO 映射、schema 构造、分页游标拼接、坐标纠偏）抽成纯函数再测。
- 不要为了让测试变绿而放宽断言范围，也不要写只断言 `status == "OK"` 的用例 —— 断言要能说明「行为对了」。

## 文档要求

- 仓库内文档：`README.md`（英文）、`README.zh-CN.md`（中文，主文档）、`CONTRIBUTING.md` / `CONTRIBUTING.zh-CN.md`、`docs/testing/pr-preflight.md`。
- **中英双语必须同步改**：只改一种语言时，在 PR 描述里说明另一种为什么不需要改。
- 文档里出现的路径、命令、模块名、版本号必须是自己核对过的；写不准的宁可省略。

## 提 PR

1. 把分支推到你自己的 Fork。
2. 向本仓库的 `main` 提 PR。
3. 在 PR 描述里写清楚：改了什么、为什么、怎么验证的（贴出实际跑过的命令与结果）。
4. 涉及界面的改动附上截图；涉及认证流程的改动请说明是否用真实账号验证过。
5. 推送后按 [docs/testing/pr-preflight.md](docs/testing/pr-preflight.md) 核对远端状态，不要用本地结果代替远端结果。

改动越小，越容易 review 和合并。

> [!NOTE]
> 本仓库使用 husky：`pre-commit` 会跑 `lint-staged`（对暂存的 `apps/desktop/src/**/*.{ts,vue}` 执行 `oxfmt`）以及对 `src-tauri`、`crates/cirrus-core` 下暂存的 Rust 文件执行 `cargo fmt`，并把格式化结果重新 `git add`。所以提交前先跑一次 `make check`，避免钩子把文件改得和你预期不一致。

## 欢迎的贡献类型

- 文档改进与中英同步
- 界面交互与视觉细节的改进（地图打点与浮层、相册网格与大图查看、2FA 交互流程）
- 可离线复现的纯逻辑修复与补测（SRP 派生、wire 映射、CloudKit schema、坐标纠偏）
- 错误提示与日志脱敏改进
- 会话生命周期、并发与清理逻辑的边界用例
