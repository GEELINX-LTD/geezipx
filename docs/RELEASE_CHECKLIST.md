# 下一版发布验收清单

目标版本建议为 **v0.7.5**，以最终发布决定为准。当前正式版是 [v0.7.4](https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.7.4)，已发布三平台 CLI 与 GUI 产物；本清单用于验收后续未发布修复。

## 1. 本轮范围

- 启动交接：主窗口创建后、前端订阅前到达的归档或右键动作保留至确认；连续启动不覆盖，重复通知不重复执行。
- Windows：可配置右键菜单、COM 多选压缩、独立快捷解压窗口、窗口关闭权限和卸载清理。
- GUI：并发任务隔离、密码提示、原生路径、ZIP 目录层级、7z 默认输出目录和稳定页面布局。
- Core/CLI/SFX：失败或取消时保留已有输出、归档校验与错误传播，以及安全依赖更新。变更细节见 [CHANGELOG](../CHANGELOG.md)。

Win11 一级菜单的实验 [PR #33](https://github.com/GEELINX-LTD/geezipx/pull/33) 独立评估。本轮安装包采用已接入的传统 Explorer 菜单。

## 2. 本地质量检查

本轮启动交接修复已通过以下检查；若候选代码继续变化，按影响范围重新验证：

- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace --exclude geezipx-gui --all-features`
- [x] `cargo clippy -p geezipx-gui --all-targets --all-features -- -D warnings`
- [x] `cargo test -p geezipx-gui --lib --all-features`（194 项通过）
- [x] 在 `crates/gui-tauri` 执行 `npm test`（112 项通过）
- [x] 同目录执行 `npx tsc --noEmit`、`npm run build`
- [x] Svelte autofixer 检查，无新增问题

启动交接回归包含两个先失败、再修复的场景：订阅前通知已丢失但队列仍有请求；查询失败时已经收到下一次通知。另覆盖重复通知、确认失败、独立请求、异步压缩与销毁后的迟到结果。实现约束见 [GUI 规划](GUI_MVP_PLAN.md#33-代码复查后的行为约束)。

## 3. 候选包构建与追溯

- [ ] 将通过检查的修复整理到可审查提交，并将候选分支推送至远端。
- [ ] 针对候选分支触发 `.github/workflows/gui-windows.yml`，记录 run URL 与完整 `head_sha`，确认 GUI Rust 测试及 NSIS 构建通过。
- [ ] 核对 artifact `geezipx-gui-windows-x86_64` 同时包含非空的 NSIS 安装程序和独立 `geezipx-gui.exe`。
- [ ] 下载本次 run 的产物并计算 SHA256；确认运行提交与候选提交一致。
- [ ] 对同一候选提交运行 `.github/workflows/release.yml`，保持 `dry_run: true`；核对三平台 CLI、GUI 产物及 `SHA256SUMS`。

测试记录至少包含：提交 SHA、Actions run URL、产物文件名与 SHA256、系统版本、逐项结果及失败复现步骤。版本抬升前的测试包可能仍显示 v0.7.4，应按提交 SHA 和构建记录识别。

## 4. Windows 10 / 11 原生验收

在独立测试目录准备包含中文、空格的路径、多个文件与文件夹、普通 ZIP/7z，以及带密码的 ZIP/7z。写入类用例使用新的目标路径。

| 场景 | 操作与通过标准 | 结果 |
| --- | --- | --- |
| 冷启动连续打开 | 完全退出后连续双击多个不同归档；每个归档都出现，条目内容正确 | 待验收 |
| 挂载期间交接 | 启动应用后立即双击归档或执行右键压缩，多次重复；不漏接、不重复压缩 | 待验收 |
| 热启动连续动作 | 主窗口已打开时连续打开归档并使用“压缩为…”；动作均接收，文件列表完整 | 待验收 |
| COM 混合多选 | 同时选择文件、文件夹并执行“压缩为 ZIP”和“压缩为…”；所有选区项都进入归档，中文路径正确 | 待验收 |
| 快捷解压 | 应用关闭与已运行两种状态下执行“解压到当前文件夹”；显示独立进度窗口，目标目录正确，完成后可关闭 | 待验收 |
| 加密与错误 | 正确密码成功；错误密码、损坏归档显示错误；关闭密码提示不会残留运行中的任务 | 待验收 |
| 取消与并发 | 同时运行两个不同任务，取消或关闭其中一个；另一任务继续，已有输出不被失败操作破坏 | 待验收 |
| 目录与导航 | Windows 分隔符 ZIP 保持目录层级；7z 单顶层目录不多套一层；选择目录时提取全部后代 | 待验收 |
| 设置与升级 | 关闭菜单或单项后立即生效；从旧版覆盖安装仍保留选择；主窗口几何可恢复 | 待验收 |
| 卸载 | 菜单、GeeZipX COM CLSID、sentinel 与关联清理；其他应用的扩展名注册保留；Explorer 菜单刷新 | 待验收 |

Win11 的传统菜单位于“显示更多选项”（Shift+F10）。构建成功与上述 Explorer/安装实测分别记录。

## 5. macOS / Linux 桌面验收

- [ ] macOS：安装 DMG，验证 Finder 双击/打开方式、拖到 Dock、冷/热启动连续打开、中文路径与原生文件选择。
- [ ] Linux：运行候选 AppImage 或安装 deb，验证桌面启动、文件关联、拖入/拖出、原生文件选择与窗口恢复。
- [ ] 三平台：抽查普通与加密 ZIP/7z、取消、独立任务进度和错误提示；记录具体系统与架构。

## 6. 正式发布

在候选构建、原生验收及版本发布决定完成后执行：

1. 统一 workspace、core 依赖版本、GUI `package.json` / lockfile、Tauri 配置与 `Cargo.lock`，把 CHANGELOG 的本轮内容归入正式版本及日期。
2. 核对发布提交、必要检查和验收记录；需要 crates.io 发布时按 core → CLI 的依赖顺序执行。
3. 创建版本 tag，跟踪 release workflow，核对三平台产物和校验文件并验证下载。
4. 将构建/验收记录与已知限制放入 release notes，更新规划中的正式版本。

保留的能力限制见 [技术架构](TECH_ARCHITECTURE.md#10-当前实现限制)，GUI 格式范围见 [GUI 规划](GUI_MVP_PLAN.md#31-支持的格式)。
