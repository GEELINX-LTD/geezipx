# 下一版发布验收清单

目标版本建议为 **v0.7.5**，以最终发布决定为准。当前正式版是 [v0.7.4](https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.7.4)，已发布三平台 CLI 与 GUI 产物；本清单用于验收后续未发布修复。

## 1. 本轮范围

- 启动交接：主窗口创建后、前端订阅前到达的归档或右键动作保留至确认；连续启动不覆盖，重复通知不重复执行。
- Windows：可配置右键菜单、COM 多选压缩、独立快捷解压窗口、窗口关闭权限和卸载清理。
- GUI：并发任务隔离、密码提示、原生路径、ZIP 目录层级、7z 默认输出目录和稳定页面布局。
- Core/CLI/SFX：失败或取消时保留已有输出、归档校验与错误传播，以及安全依赖更新。变更细节见 [CHANGELOG](../CHANGELOG.md)。
- Core/CLI 格式对齐：完整清单已完成一轮逐项实现/性能审查，新增能力、独立样本结果、部分兼容范围与暂缓项见 [格式审查](FORMAT_PARITY_AUDIT.md)。新增读取能力不代表两个 GUI 已接入，也不代表已完成三平台发布验证。

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

格式对齐分支已完成六轮实现/专项验收，最新候选`76b65a60fa9478f2661256369968ee275d245501`依次完成ASAR/LZH/ZPAQ创建内存改造及PEA/NSIS只读子集。[CI](https://github.com/GEELINX-LTD/geezipx/actions/runs/41201756766)及[Audit](https://github.com/GEELINX-LTD/geezipx/actions/runs/41201760473)均成功；x86-64三平台最小/基础/扩展feature测试、Clippy及原生CLI/SFX验收通过，Linux/Windows各77项、macOS73项。macOS扩展配置为RAR/WIM/SFX，沿用ZPAQ限制。六份产物/报告已下载，完整源码SHA/实际架构/二进制哈希与报告匹配，共有73项提取内容跨平台一致，Linux下载包复跑全部77项通过。测试数量、产物链接、SHA-256及各格式内存/独立互操作实测见[第六轮专项验收](FORMAT_PARITY_ACCEPTANCE.md#三平台最终候选与产物复核)。忽略项不计为通过；最终发布源码/依赖变更后仍应按影响范围重验。后续发布验收项如下：

- [x] 对格式对齐候选`76b65a6`执行Windows/macOS/Linux CLI构建与新增格式抽查，覆盖ZIPX/XZ、AES Crypt、PMA/TLZ、ALZ/EGG子集、UU/UUE/XXE、IMG/BIN/ISZ内容视图、ISO/UDF及ASAR/LZH/ZPAQ创建改造，以及PEA/NSIS只读子集。
- [x] AES Crypt官方CLI交叉核验63份v0–v2向量及官方新建v3；7份真实EGG通过上游记录，其中6份/22个文件与独立Go工具哈希一致；UltraISO默认zlib ISZ还原结果与官方逐字节一致。未覆盖变体和样本来源见专项验收。
- [x] CI构建时重建并嵌入三平台目标stub，实际创建/执行ZIP SFX；下载Linux候选包再次执行通过。后续stub源码/依赖变化仍需重验。

- [x] 修复已整理为独立提交，并将`feature/core-format-parity`候选分支推送远端；本次验证源码为`76b65a6`。
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

### 独立 egui 桌面端

本次紧凑文件管理器改造在 Linux 本地工作区完成以下验证；独立 `gui-egui.yml` 与 Windows/macOS 原生交互仍需对候选提交执行：

- [x] 工作区 fmt、Clippy、全部 feature 测试，以及 `cargo test -p geezipx-egui --no-default-features --locked`。
- [x] `cargo build -p geezipx-egui --release --locked`。
- [x] Xvfb 实跑目录导航/搜索、浅深色、760×480 窗口；选择文件夹提取包含后代且排除同名前缀目录。
- [x] 组件细节：菜单/右键菜单/工具提示的快捷键分栏、搜索清除、表单对齐、最小窗口对话框；回归覆盖 Windows/macOS 快捷键格式，Esc 关闭菜单保留选择，下拉框内 Enter 不提交任务。
- [x] 创建对话框的保存路径输入框按 Enter 生成 ZIP，独立检查 CRC、中文文件内容和空目录；同名输出失败后保留原文件和来源，修改目标后可直接重试。
- [x] 字体与设置：无系统字体、仅西文字体的离线中文回退，字体搜索/选择/预览/恢复默认、旧偏好迁移、重启保留和已卸载字体回退；Linux release 实窗验证及 760×480 深色布局通过，默认 RAR feature 的 Windows GNU 编译检查通过。
- [ ] Windows/macOS 字体验收：系统 UI/中文/等宽字体选择，手选安装字体的 TTC 字面索引、重启保留、卸载后回退，以及高 DPI 设置页显示。
- [ ] Windows/macOS：验证系统文件/目录选择器、Ctrl/Cmd/Shift 选择、键盘焦点、DPI 缩放和最小窗口对话框。
- [ ] 目标桌面：验证超大归档的打开、滚动、搜索，以及任务运行期间的导航响应。

- [x] 旧 GBK ZIP 的中文目录显示、双击导航、空白处框选；release 在 Xvfb 将三项选区通过 Xdnd 交给独立接收窗口，复制后的中文内容、空目录及其余所选文件核对通过。
- [x] 新增系统拖放依赖后的 Windows GNU 目标编译检查（关闭默认 RAR feature）；不能替代 Windows 原生工作流和 Explorer 交互验收。
- [x] 目录独立双击识别、快速嵌套/返回导航、拖动前零解压读取和放置后只解压一次的回归；Linux Xvfb 一次 Xdnd 拖动交付多选并核对内容；Windows COM 描述与释放后内容流用例在 Wine 通过，macOS 文件承诺模块已做 Apple 目标接口编译检查。
- [x] 左侧目录树展开/收起与单击导航、历史/面包屑同步、搜索时保持树和 `..`；`..` 双击返回并定位原目录，不参与框选/全选/解压。release 在 Xvfb 连续五轮进出子目录、760×480 窗口布局和单次拖出验证通过，截图已更新。
- [ ] Windows/macOS 原生验证：一次拖出单文件/多项/空目录、悬停不解压、取消拖动、松手后大文件交付、中文路径、覆盖提示及退出清理。

系统文件关联和 Explorer 右键注册尚未接入 egui；原生 Wayland 拖出暂不支持。上述 Tauri 系统集成验收不能替代独立 egui 能力验收。

## 6. 正式发布

在候选构建、原生验收及版本发布决定完成后执行：

1. 统一 workspace、core 依赖版本、GUI `package.json` / lockfile、Tauri 配置与 `Cargo.lock`，把 CHANGELOG 的本轮内容归入正式版本及日期。
2. 核对发布提交、必要检查和验收记录；需要crates.io发布时先确认`geezipx-iso`、`geezipx-udf`、`geezipx-lzhuf`、`geezipx-zpaq`和`geezipx-nsis`所需版本已发布并可从registry解析（对应`crates/vendor/hadris-iso/Cargo.toml`、`crates/vendor/hadris-udf/Cargo.toml`、`crates/vendor/oxiarc-lzhuf/Cargo.toml`、`crates/vendor/zpaq-rs/Cargo.toml`和`crates/vendor/nsis/Cargo.toml`独立发布），再按core → CLI的依赖顺序执行。支持库使用独立版本号；不能仅依赖本地path或workspace patch来验证发布可用性。
3. 创建版本 tag，跟踪 release workflow，核对三平台产物和校验文件并验证下载。
4. 将构建/验收记录与已知限制放入 release notes，更新规划中的正式版本。

保留的能力限制见 [技术架构](TECH_ARCHITECTURE.md#10-当前实现限制)，GUI 格式范围见 [GUI 规划](GUI_MVP_PLAN.md#31-支持的格式)。
