# AGENTS.md

> 桌面 GUI 统一使用 `crates/gui-egui` 的 egui/eframe + Rust，复用 `geezipx-core`。

---

本文件为 GeeZipX 项目的 AI 编码代理协作说明。所有代理在本仓库内工作时，应优先遵守本文档；若与用户的最新明确指令冲突，以用户最新指令为准。

## 项目概述

GeeZipX 是一个使用 Rust 开发的跨平台压缩/解压缩工具。

产品路线：

1. **第一阶段：高性能 CLI（已完成）**
   - 先开发稳定、高性能、脚本友好的命令行工具。
   - 重点支持 macOS、Linux 终端、Windows 终端。
   - 核心目标是性能、可靠性、流式处理和跨平台一致性。
2. **第二阶段：桌面 GUI（当前阶段）**
   - 以 egui/eframe 原生 GUI（`crates/gui-egui`）作为唯一桌面端，说明见 `docs/EGUI_GUI.md`。
   - GUI 应复用已有 Rust core 引擎，不应重复实现压缩逻辑。
   - 桌面目标平台为 macOS、Linux 桌面、Windows。

相关规划文档：

- `docs/PRD.md`：产品需求文档。
- `docs/TECH_ARCHITECTURE.md`：技术架构文档。
- `docs/EGUI_GUI.md`：egui 原生桌面端的运行、能力范围、架构和限制。

在实现功能前，应优先阅读以上文档。

## 技术路线

### 语言与平台

- 主语言：Rust。
- 第一阶段（CLI）：已在 v0.1.0 达成并保持成熟稳定；当前正式版本为 v0.7.4，当前开发候选为 0.9.0，待发布改动见 `CHANGELOG.md` 的 Unreleased。
- 第二阶段（GUI）：唯一桌面端使用 egui/eframe，复用 Rust core；界面与格式范围见 `docs/EGUI_GUI.md`。
- 目标平台：
  - macOS
  - Linux 终端
  - Linux 桌面
  - Windows

### 推荐架构

项目应采用 Rust workspace 分层架构：

```text
geezipx/
├── Cargo.toml
├── crates/
│   ├── core/        # 压缩/解压缩核心引擎库
│   ├── cli/         # 命令行入口
│   └── gui-egui/    # egui/eframe 原生桌面应用
├── docs/
└── AGENTS.md
```


## 开发原则

### 1. CLI-first

任何功能设计都应先考虑 CLI 场景：

- 是否适合脚本调用；
- 是否有清晰的退出码；
- 是否能输出机器可读结果；
- 是否支持大文件；
- 是否能在无 GUI 环境运行。

GUI 不应成为 core 或 CLI 的前置依赖。

### 2. Core 与界面解耦

压缩/解压缩逻辑必须放在 core 层。

CLI 和 GUI 只负责：

- 参数解析；
- 用户交互；
- 进度展示；
- 错误展示；
- 调用 core。

不要在 CLI 或 GUI 中重复实现归档格式处理、流式管道或格式检测逻辑。

### 3. 流式优先

压缩/解压缩实现应优先使用流式处理：

- 不要一次性将大文件完整读入内存；
- 使用 `Read` / `Write` / `BufReader` / `BufWriter` 等接口；
- 设计进度统计时避免破坏流式特性；
- 大文件处理能力是核心非功能需求。

### 4. 跨平台一致性

实现文件系统相关功能时必须考虑：

- Windows 路径分隔符和长路径；
- Unix 文件权限；
- 符号链接；
- 文件覆盖策略；
- 中文路径和 Unicode 文件名；
- 文件锁和权限错误；
- 大小写敏感差异。

平台差异应封装在专门模块中，避免散落在业务逻辑里。

### 5. 格式支持循序渐进

当前已支持：

- ZIP（含 AES-256 加密及 `jar`/`war`/`apk`/`ipa`/`xpi` 别名）；
- ZIPX（XZ method 95 创建；读取共用 ZIP codecs，未覆盖所有专有方法）；
- TAR；
- TAR.GZ / TGZ；
- TAR.BZ2 / TBZ / TBZ2；
- TAR.BR；
- TAR.LZ4；
- TAR.XZ / TXZ；
- TAR.ZST / TZST；
- GZIP / GZ；
- BZIP2 / BZ2；
- Brotli / BR；
- LZ4；
- XZ；
- LZMA；
- LZ / Lzip（读写，LZMA 容器）；
- Zstandard / ZST / ZSTD；
- 7z（读写，含 AES-256 加密）；
- RAR（只读）；
- CAB（读写）；
- ASAR（读写，Electron 归档）；
- DEB（读写，Debian 包）；
- LZH / LHA（读写，lh0-lh7）；
- PMA（只读，pm0/pm1/pm2）；
- TLZ / TAR.LZMA / TAR.LZ（只读，LZMA Alone 或 Lzip 外层）；
- ISO（读写，含 Joliet Unicode 文件名 + Rock Ridge POSIX 属性）；
- UDF（读写，Universal Disk Format DVD/Blu-ray 文件系统）；
- CPIO（读写，newc/odc）；
- ZPAQ（创建与流式分段包读写；内置solid模型1–3，兼容级别4/5使用模型3；标准journal仅列出，内容重建待实现）；
- WIM（无压缩读写，XPRESS 解压；LZX 互操作待修复，LZMS 与分割 SWM 未实现）；
- ISZ（有界块创建/读取，CLI 可直接列出及提取 ISO 内文件；加密/分卷暂不支持）；
- SFX（自解压 ZIP 读写；创建需要对应目标的非空嵌入式 stub）；
- ARJ（只读，unarc-rs 适配器）；
- ACE（只读，unarc-rs 适配器）；
- ARC（只读，unarc-rs 适配器）；
- UU / UUE（读写，自实现编解码器）；
- XXE（读写，自实现编解码器）；
- IMG / IMA、BIN（CLI 提取 cooked/raw ISO/UDF 光盘内容；创建和显式 core copy helper 仍为透传，无 FAT/NTFS/分区盘）；
- 化合物 / MSI / OLE（只读，原始 CFB streams，不解释 MSI 数据库）；
- EGG（部分只读，单卷非 solid 未加密 Store/Deflate/BZip2/LZMA）；
- PEA（部分只读，单卷/单流未加密 Store/Deflate PCOMPRESS0–3，三层校验，UTF-8/ASCII 名称；加密/HMAC、ECC、分卷和旧 ANSI 代码页未支持）；
- NSIS（部分只读，NSIS 3 Unicode Store/Deflate/BZip2/LZMA，solid 与 non-solid；静态路径及可选 CRC 校验，solid 磁盘暂存，不执行脚本或插件）；
- AES Crypt（.aes，只读，v0–v3，认证后输出）；
- AES 加密容器（.enc，AES-256-GCM-SIV + Argon2id）；

ALZ 支持单卷 Store/Deflate 与 ZipCrypto，修改版 BZip2/其它方法及分卷暂缓；Z / Unix Compress 支持有界流式 LZW 解码。

CLI 已支持 `compress --split-size` 分卷创建。后续格式扩展由实际需求驱动，当前优先处理 GUI 迭代与已知兼容性限制。egui 已桥接当前 core 全部格式，默认启用 RAR/WIM/ZPAQ，创建/单流/镜像与只读变体限制见 `docs/EGUI_GUI.md`。

Core/CLI 以用户给定的 Bandizip 清单为对齐目标，逐项实现/性能审查结果见 `docs/FORMAT_PARITY_AUDIT.md`。BH、DAA 1.0 本轮暂缓并记录具体证据及路线；PMA 已实现，EGG/PEA/NSIS 部分实现，不能继续列为永久排除。其它范围见 `docs/PRD.md` §5.2；RAR 写入仍受许可限制，ZIPX 专有方法及各格式变体的限制需分别说明。

当前阶段不承诺完整替代 7-Zip / WinRAR。

## 代码约定

### Rust 约定

- 使用稳定版 Rust 1.99；workspace MSRV 为 1.99，`rust-toolchain.toml` 与 CI 固定使用 1.99.0（含 rustfmt、Clippy）。
- 公共 API 应有清晰错误类型。
- 优先使用显式类型和可读性强的结构。
- 避免不必要的 `unsafe`。
- 新增复杂逻辑时应补充测试。
- 错误处理优先使用结构化错误，而不是简单字符串。

建议依赖方向：

- CLI 参数：`clap`。
- 进度显示：`indicatif`。
- 错误定义：`thiserror`。
- 应用级错误传播：`anyhow`，仅限二进制入口或边界层。
- ZIP：`zip`。
- TAR：`tar`。
- GZIP/Deflate：`flate2`。

具体依赖选择应参考 `docs/TECH_ARCHITECTURE.md`。

### 模块边界

实际 core 层结构：

```text
core/src/
├── archive/     # zip/tar/7z/rar 等归档容器实现
├── config.rs    # CLI 配置参数模型
├── detect.rs    # 格式检测（魔数 + 扩展名）
├── error.rs     # 统一错误类型 GeeZipError
├── io.rs        # ProgressReader / ProgressWriter 流式封装
└── test.rs      # 测试工具函数
```

实际 CLI 层结构：

```text
cli/src/
├── main.rs
├── commands/    # compress / decompress / list / test / completions
├── render/      # 进度渲染（progress.rs）和终端输出
└── signal.rs    # Ctrl+C 取消处理（CancellationToken）
```

## 测试与验证

实现代码后，应按变更范围执行检查：

**workspace 质量门禁（包含原生 GUI）**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

**GUI 变更额外关注**

- `.github/workflows/gui-egui.yml`：三平台原生 GUI 检查、构建与 Windows NSIS 安装/卸载验证；
- `.github/workflows/release.yml`：复用 GUI 工作流，发布三平台原生程序压缩包和 Windows NSIS 安装器；如涉及发布链路应进行 release dry-run 验证。

核心测试要求：

- ZIP round-trip：压缩后解压，文件内容一致；
- TAR.GZ round-trip；
- 空目录；
- 嵌套目录；
- Unicode 文件名；
- 大文件流式处理；
- 覆盖/不覆盖策略；
- 错误输入，例如损坏压缩包；
- Windows/macOS/Linux 三平台 CI。

## 文档约定

当实现或调整重要功能时，应同步更新相关文档：

- 产品范围变化：更新 `docs/PRD.md`。
- 架构、模块或依赖变化：更新 `docs/TECH_ARCHITECTURE.md`。
- 发布验证与剩余验收项：更新 `docs/RELEASE_CHECKLIST.md`。
- 新增命令或参数：更新 CLI 使用说明或 README。

文档应保持具体、可执行，避免空泛描述。

## 版本管理

- 交付新增功能或修复时按语义化版本推进开发版本：新增功能推进 minor，兼容性修复推进 patch；纯文档或 CI 调整无需单独升版。
- 同步根 `Cargo.toml` 的 workspace 版本、内部依赖版本要求和 `Cargo.lock` 中的应用包版本；独立发布的支持库按其自身变更推进版本。
- README、GUI 说明和发布清单应区分当前开发候选与最新正式版本。发布前保留 `CHANGELOG.md` 的 Unreleased，正式发布时再归档版本、日期和比较链接。
- 提交推送与正式发布分别授权；推进开发版本不自动创建 tag、发布 crates.io 包或 GitHub Release。

## Git 管理规范

本项目必须保持 Git 历史清晰、分支命名可读、提交粒度明确。不要创建一堆含义不明、临时性、随机命名的分支。

### Git 操作职责

涉及以下操作时，应优先交由 GitOperator / git subagent 执行或遵循其规范：

- 创建分支；
- 提交 commit；
- rebase / merge；
- 创建 tag；
- 创建 PR；
- 清理分支；
- 处理冲突。

除非用户明确要求，否则不要擅自提交代码或创建远程分支。

### 分支模型

默认稳定分支：

```text
main
```

常规开发从 `main` 拉出短生命周期工作分支。分支合并后应及时删除。

### 分支命名

分支名必须使用英文、小写、短横线分隔，格式如下：

```text
<type>/<scope-or-topic>
```

允许的 `type`：

- `feature/`：新增功能；
- `fix/`：修复 bug；
- `docs/`：文档变更；
- `refactor/`：不改变行为的重构；
- `test/`：测试相关；
- `ci/`：CI/CD 相关；
- `chore/`：工具、配置、维护类变更；
- `release/`：发布准备；
- `hotfix/`：紧急修复。

推荐示例：

```text
feature/cli-zip-compress
feature/cli-tar-gz-extract
fix/windows-long-path
fix/zip-unicode-filenames
docs/update-architecture
refactor/core-progress-events
test/archive-round-trip
ci/github-actions-matrix
chore/workspace-setup
```

禁止使用以下分支名：

```text
my-branch
test
wip
temp
new
fix
update
agent-work
chatgpt-changes
2026-05-31
```

### Commit message 规范

**Commit message 必须使用英文。**

提交格式采用 Conventional Commits：

```text
<type>(<scope>): <subject>
```

要求：

- `type` 使用小写英文；
- `scope` 使用小写英文，可选但推荐；
- `subject` 使用英文祈使句或简洁描述；
- 首行建议不超过 72 个字符；
- 不要使用中文 commit message；
- 不要使用 `update`、`fix stuff`、`misc changes`、`wip` 这类模糊描述；
- 每个 commit 应尽量只表达一个清晰意图。

允许的 commit `type`：

- `feat`：新增功能；
- `fix`：修复 bug；
- `docs`：文档变更；
- `refactor`：重构；
- `test`：测试；
- `ci`：CI/CD；
- `chore`：维护、配置、工具；
- `perf`：性能优化；
- `build`：构建系统或依赖；
- `style`：格式化或不影响逻辑的样式调整。

推荐 commit 示例：

```text
feat(cli): add zip compression command
feat(core): implement tar.gz extraction pipeline
fix(core): preserve unicode filenames in zip archives
fix(cli): return non-zero exit code on extraction failure
docs(agents): add git branch and commit conventions
test(core): add archive round-trip tests
ci: add cross-platform cargo test matrix
chore: initialize rust workspace
```

如需提交破坏性变更，使用 `!` 或正文说明：

```text
feat(core)!: change archive task API
```

### Commit 粒度

提交前必须检查：

```bash
git status
git diff
```

提交原则：

1. 不要提交与当前任务无关的文件。
2. 不要把格式化、重构、功能、文档混在同一个 commit，除非它们不可分割。
3. 文档变更使用 `docs(...)`。
4. 新功能使用 `feat(...)`。
5. Bug 修复使用 `fix(...)`。
6. 测试补充使用 `test(...)`。
7. CI 变更使用 `ci(...)`。
8. 如果测试未通过，不要声称提交已完成；必须在说明中标明失败原因。

### PR / 合并规范

创建 PR 时标题应同样使用英文 Conventional Commit 风格，例如：

```text
feat(cli): add zip compression command
```

PR 描述应包含：

- What changed；
- Why it changed；
- How it was tested；
- Known limitations or follow-ups。

合并前应确认：

- 分支名清晰；
- commit message 为英文；
- 无无关文件；
- 必要测试已运行；
- 文档已同步更新。

## 代理工作流程

### 开始任务前

1. 阅读与任务相关的文档。
2. 确认任务属于以下哪类：
   - 产品文档；
   - 架构设计；
   - core 实现；
   - CLI 实现；
   - 测试；
   - CI / 发布；
   - 桌面 GUI（egui/eframe）。
3. 如果需求不明确，应先提出具体澄清问题。

### 修改代码时

1. 优先小步提交式修改。
2. 不要一次性重写无关文件。
3. 不要引入与任务无关的大型依赖。
4. 不要把 GUI 依赖引入 core。
5. 不要让 core 依赖 CLI 或 GUI。
6. 新增功能必须考虑测试。

### 完成任务后

应报告：

- 修改了哪些文件；
- 实现了什么；
- 如何验证；
- 是否有未完成事项或风险。

如果运行测试失败，应明确说明失败命令、错误摘要和可能原因。

## GUI 接入原则（当前阶段）

- CLI 已稳定，GUI 复用 core API，不重复实现编解码。
- 阻塞操作放在后台任务，进度通过共享快照和限频唤醒传递。
- 取消属于具体任务，退出时等待清理；错误转换成用户可理解的消息。
- 密码仅在当前操作内使用，不进入持久化。

```text
egui/eframe 界面
      │
后台任务与 engine 适配
      │
Rust core API
      │
archive/compress pipeline
```

## 非目标提醒（GUI 阶段明确不做）

以下能力在当前 GUI 阶段不做优先考虑：

- 自动更新；
- RAR 创建（许可限制）；
- 分卷压缩；
- 云同步；
- 插件系统。

## 当前优先级

当前最高优先级：

1. **GUI 迭代（egui）** — 当前正式版本为 v0.7.4，当前开发候选为 0.9.0；当前源码的唯一桌面端为 egui/eframe，复用 core 引擎。运行、构建与能力范围见 `docs/EGUI_GUI.md`，候选验收见 `docs/RELEASE_CHECKLIST.md`。
   - `cargo run -p geezipx-egui`；默认 Cargo 包仍是 core/CLI。三平台构建工作流为 `gui-egui.yml`，正式发布复用该工作流。
2. **GUI 相关测试** — 仅针对 GUI 实现添加必要的测试；不主动推进 core/CLI 基准测试基线或覆盖率补测。
3. **文档同步** — GUI 实现过程中的文档跟随更新。

### 近期不主动推进

- **Benchmark 基线**：Criterion 基准框架已就绪，但 GitHub-hosted runner 性能波动大，不稳定。benchmark 回归为 advisory 模式，不设硬性门禁，不进一步投入。
- **覆盖率补测**：已有测试足够（400+），覆盖率工作流为 informational-only 观测性模式。新增测试仅在明确风险或回归场景按需补最小用例。

### 任何时候修改代码仍需遵守

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- GUI 改动需额外关注 `gui-egui.yml` 与 `release.yml` 的构建/发布链路
- Core/CLI 解耦原则（core 不依赖 GUI，GUI 不实现压缩逻辑）
