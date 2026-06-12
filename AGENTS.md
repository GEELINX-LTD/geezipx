# AGENTS.md

> **Svelte 5 + AI Tools**: 本项目 GUI 使用 Svelte 5 和官方 Svelte AI 工具。创建/编辑 Svelte 文件时，优先使用 `svelte-file-editor` subagent。

## Svelte MCP 工具使用指南

本项目已配置 Svelte MCP server (`@sveltejs/mcp`)，可通过 MCP 工具访问官方 Svelte 5 文档和代码分析。

### 可用 Svelte MCP 工具

1. **list-sections** — 首先使用此工具发现所有可用的 Svelte 5 文档章节。在涉及 Svelte 话题时，始终先调用此工具查找相关章节。

2. **get-documentation** — 获取特定章节的完整文档。调用 `list-sections` 后，分析返回的文档章节（特别是 `use_cases` 字段），然后使用此工具获取与当前任务相关的所有章节。
   示例：`get-documentation("$state,$derived,$effect")`

3. **svelte-autofixer** — 分析 Svelte 代码并返回问题和建议。在编写 Svelte 代码后**必须**使用此工具，持续调用直到没有新问题。

4. **playground-link** — 生成 Svelte Playground 链接。仅在用户确认后调用，绝不用于已写入项目文件的代码。

### Svelte 5 关键规则（runes 模式，compilerOptions: { runes: true }）

- 仅对需要响应式的变量使用 `$state` rune；普通变量不加。
- 使用 `$derived` 而非 `$effect` 进行计算派生。
- 使用 `$derived.by(() => {...})` 处理复杂表达式。
- 始终使用 keyed `{#each items as item (item.id)}`。
- 使用 `$props()` 替代 Svelte 4 的 `export let`。
- 事件处理：`onclick={handler}`（无 `:` 前缀）。
- 大对象仅被重新赋值时，使用 `$state.raw` 替代 `$state`（避免深度代理开销）。
- `$effect` 中修改 state 无需额外处理，但注意避免无限循环。
- 组件样式自动 scoped；全局样式放在 `app.css`。

### 何时使用 svelte-file-editor subagent

以下情况**必须**使用 `svelte-file-editor` subagent（在独立上下文中运行，有 MCP 工具访问权限）：
- 创建新的 `.svelte` 组件
- 编辑现有 `.svelte` 文件
- 创建/编辑 `.svelte.ts` 或 `.svelte.js` 模块
- 审查 Svelte 代码质量

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
   - GUI 使用 Tauri 实现。
   - GUI 应复用已有 Rust core 引擎，不应重复实现压缩逻辑。
   - 桌面目标平台为 macOS、Linux 桌面、Windows。

相关规划文档：

- `docs/PRD.md`：产品需求文档。
- `docs/TECH_ARCHITECTURE.md`：技术架构文档。
- `docs/PHASE1_CLI_TASKS.md`：第一阶段 CLI MVP 任务拆分。
- `docs/GUI_MVP_PLAN.md`：第二阶段桌面 GUI MVP 规划和任务拆分。

在实现功能前，应优先阅读以上文档。

## 技术路线

### 语言与平台

- 主语言：Rust。
- 第一阶段（CLI）：已在 v0.1.0 达成并保持成熟稳定；当前项目整体版本已演进到 v0.5.0。
- 第二阶段（GUI）：Tauri，当前阶段（v0.5.0）。
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
│   └── gui-tauri/   # 当前 Tauri 桌面应用
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

CLI 和后续 Tauri GUI 只负责：

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

- ZIP（含 AES-256 加密、`zipx`/`jar`/`war`/`apk`/`ipa`/`xpi` 兼容别名）；
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
- Zstandard / ZST / ZSTD；
- 7z（读写，含 AES-256 加密）；
- RAR（只读）；
- CAB（只读）；
- ASAR（只读，Electron 归档）；
- DEB（只读，Debian 包）；
- LZH / LHA（读写，store-only）；
- ISO（只读）；
- CPIO（只读）；
- ZPAQ（只读）。

后续可扩展现：

- 分卷压缩；
- RAR 写入；
- LZH/LHA 压缩条目（lh5/lh6/lh7）；
- 完整 7z 多线程调优。

当前阶段不承诺完整替代 7-Zip / WinRAR。

## 代码约定

### Rust 约定

- 使用稳定版 Rust。
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

**core / CLI 默认质量门禁**

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings
cargo test --workspace --exclude geezipx-gui --all-features
```

**GUI 变更额外关注**

- `.github/workflows/gui-windows.yml`：独立 Windows GUI build workflow；
- `.github/workflows/release.yml`：已配置三平台 GUI bundle 构建与上传（`.AppImage` / `.dmg` / `.msi`），如涉及发布链路应结合 tag release 进行实战验证。

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
- 第一阶段任务状态或拆分变化：更新 `docs/PHASE1_CLI_TASKS.md`。
- 新增命令或参数：更新 CLI 使用说明或 README。

文档应保持具体、可执行，避免空泛描述。

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
   - 后续 Tauri GUI。
3. 如果需求不明确，应先提出具体澄清问题。

### 修改代码时

1. 优先小步提交式修改。
2. 不要一次性重写无关文件。
3. 不要引入与任务无关的大型依赖。
4. 不要把 GUI 依赖引入 core。
5. 不要让 core 依赖 CLI 或 Tauri。
6. 新增功能必须考虑测试。

### 完成任务后

应报告：

- 修改了哪些文件；
- 实现了什么；
- 如何验证；
- 是否有未完成事项或风险。

如果运行测试失败，应明确说明失败命令、错误摘要和可能原因。

## Tauri GUI 接入原则（当前阶段）

Tauri GUI 阶段的核心原则：

- CLI 已稳定，core API 足够清晰。
- 进度事件应可以被 GUI 消费。
- 任务应可以取消。
- 错误类型可以序列化为用户友好的消息。

Tauri 只应作为桌面壳层：

```text
Tauri frontend
      │
Tauri command bridge
      │
Rust core task API
      │
archive/compress pipeline
```

不要让 Tauri frontend 直接处理压缩格式细节。

## 非目标提醒（GUI 阶段明确不做）

以下能力在当前 GUI 阶段不做优先考虑：

- 右键菜单集成；
- 自动更新；
- 完整 7z 写入；
- RAR 创建；
- 分卷压缩；
- 云同步；
- 插件系统。

## 当前优先级

当前最高优先级：

1. **GUI 迭代（Tauri）** — 桌面 GUI 已达 v0.5.0，继续迭代增强，复用 core 引擎。详见 `docs/GUI_MVP_PLAN.md`。
2. **GUI 相关测试** — 仅针对 GUI 实现添加必要的测试；不主动推进 core/CLI 基准测试基线或覆盖率补测。
3. **文档同步** — GUI 实现过程中的文档跟随更新。

### 近期不主动推进

- **Benchmark 基线**：Criterion 基准框架已就绪，但 GitHub-hosted runner 性能波动大，不稳定。benchmark 回归为 advisory 模式，不设硬性门禁，不进一步投入。
- **覆盖率补测**：已有测试足够（400+），覆盖率工作流为 informational-only 观测性模式。新增测试仅在明确风险或回归场景按需补最小用例。

### 任何时候修改代码仍需遵守

- `cargo fmt --all --check`
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`
- `cargo test --workspace --exclude geezipx-gui --all-features`
- GUI 改动需额外关注 `gui-windows.yml` 与 `release.yml` 的构建/发布链路
- Core/CLI 解耦原则（core 不依赖 GUI，GUI 不实现压缩逻辑）
