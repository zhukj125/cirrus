# PR 提交与 CI 核查

## 提交前

1. 确认实际工作目录、分支与远端，执行 `git status --short`、`git remote -v`，用 `git diff` 检查本次要提交的全部改动。工作区有未提交改动时不要直接拉取覆盖，先 `git stash` 或提交。
2. 按改动范围选择检查（见下节）。**格式、lint、类型、测试、编译是彼此不同的检查，不能互相替代。**
3. 把改动 rebase 到最新基线后，确认上游改动是否影响你已经验证过的路径。
4. 文档类改动（本仓库的 `README.md` / `README.zh-CN.md` / `CONTRIBUTING*.md` / `docs/`）没有自动化检查，必须自己核对：路径、命令、模块名、版本号是否与仓库实际一致；中英文两份是否同步。

### 选择检查

按改动内容选择，不要在无关路径上跑全套发布关卡：

| 改动范围 | 至少执行 |
| --- | --- |
| Rust（`crates/`、`src-tauri/`） | `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` |
| 只做编译验证 | `make cargo-check`（= `cargo check --workspace --all-targets`） |
| 前端（`apps/desktop/`） | `make check`（= `pnpm check` = `node scripts/run-check.mjs`，并行跑 oxfmt --check / oxlint / vue-tsc / vitest） |
| 只改了前端测试或想快速回归 | `make test`（只有 vitest，**不包含**类型与格式检查） |
| 打包 / 配置（`tauri.conf.json`、`Makefile`、`package.json`） | 前端 `make check` + Rust 三项；改 CSP 或打包目标时另外跑一次 `make package` |

说明：

- `make cargo-check`、`make cargo-test`、`make cargo-clippy`、`make check` 是四类不同的检查，不能互相替代：`cargo check` 不会运行测试，`cargo test` 不做 lint，`make check` 完全不看 Rust。
- `cargo fmt --all -- --check` 没有对应的 Makefile 目标（`make fmt` 会**直接改写文件**），需要手敲。
- 提交时 husky 的 `pre-commit` 会跑 `lint-staged`（oxfmt）与 Rust 暂存文件的 `cargo fmt`，并把结果重新 `git add`。所以提交后请再看一眼 `git diff`，确认钩子没有把未预期的改动带进来。

## 推送与远端核查

```sh
git push -u origin <branch>
gh pr checks <number>
gh pr view <number> --json headRefOid,state,mergeable,statusCheckRollup
gh run view <run-id> --log-failed
```

- **push 成功只代表远端分支收到了提交。** PR 已创建、无合并冲突、CI 通过、已合并、已发布是五个不同的状态，不要用其中一个描述另一个。
- 创建或更新 PR 后，查看**对应提交 SHA** 的检查结果。排队或运行中要写明「待完成」，**不能把本地测试通过写成远端全绿**。
- 失败时保存任务名、run/job 链接、提交 SHA 与首个实际错误。先判断是代码、格式、权限、在线服务还是 runner 环境问题，不要只凭红叉推测。
- 修复本分支可解决的问题后，重新跑对应检查再推送。不要靠不断 rerun 掩盖确定性缺口，也不要为了绿灯跳过检查或改无关 workflow。
- 若仓库配置了会自行提交的机器人（版本 bump、格式化），它会改变 SHA；更新前先 `git fetch` 核实，避免覆盖机器人的改动。
- GitHub CLI 的授权/scope 问题与代码失败分开处理；只检查 scope，不要输出 token。

### 本仓库当前状态

- 本仓库目前没有 `.github/workflows/`，也没有配置远端，所以上面的 `gh` 命令在仓库托管起来之前不会有输出。**「没有任何检查项」不等于「检查通过」**，在 CI 就位前不要用远端状态当作绿灯。
- 在没有 CI 之前，`make check` 与 Rust 三项就是唯一关卡，PR 描述里请把这些命令的实际输出贴出来，而不是写「本地通过」四个字。仓库地址确定后，记得把本节更新成实际的仓库名。

## 常见的假绿灯

这几类「绿了但没验证到」的情况在本仓库特别容易发生：

| 做法 | 为什么不算通过 |
| --- | --- |
| 只跑 `cargo build` / `make cargo-check`，不跑 `cargo test` | 编译通过不证明行为正确。SRP 派生向量、wire 映射、CloudKit schema 构造都只有 `cargo test` 能覆盖，改了这些文件却只 build 等于没测 |
| 只跑 `vitest`（`make test`），跳过 `vue-tsc` | 类型错误完全不会被 vitest 捕获；`make test` 只是 `make check` 的子集 |
| 只跑 `oxlint`，跳过 `oxfmt --check` | 两者检查的是不同东西：lint 管可疑写法，格式管输出格式。`pnpm check`（即 `make check`）是两条都跑的 |
| 跑 `make check` 就宣称「全部通过」 | 它一行 Rust 都不编译。改了 `crates/` 或 `src-tauri/` 时必须另外跑 cargo 三项 |
| 改了文档却只在一种语言里改 | 本仓库没有多语言完整性 CI，中英不一致不会被任何检查拦住，只能靠 review |
| 改了界面就认为「检查全绿 = 界面可用」 | 前端自动化测试覆盖的是纯函数级行为（坐标纠偏等），地图打点、相册网格、大图查看、2FA 交互这些依赖真实网络与 DOM 的路径没有覆盖。四类检查全绿**不代表**界面能用，改完必须 `make` 起来在真实窗口里走一遍 |

## 报告要求

交付时同时给出：

- 实际跑过的命令与结果（通过/失败），以及失败时的首个错误
- 推送后的远端状态：最新提交 SHA、`gh pr view` 的 `state` / `mergeable` / `statusCheckRollup`
- 明确区分「本地通过」「远端检查通过」「已合并」三种状态

测试数量不能代替检查覆盖范围：`cargo test` 的用例数与 `make check` 的四类检查各自只证明自己那部分。
