# GeeZipX — 产品需求文档 (PRD)

## 1. 产品定位

GeeZipX 是一个高性能、跨平台压缩/解压缩工具，使用 Rust 开发。采用 **CLI-first** 策略：先打磨出命令行工具核心体验，等 CLI 成熟后，再基于 Tauri 提供 macOS/Linux/Windows 桌面 GUI。

> **核心理念**：压缩操作通常位于自动化脚本、服务器运维、CI/CD 流水线中，CLI 是先发价值；GUI 提供附加便捷，不牺牲底层性能。

## 2. 目标用户

| 用户群 | 使用场景 | 优先度 |
|--------|----------|--------|
| 开发者和运维人员 | CLI 集成脚本、CI/CD、批量归档处理 | P0 |
| 技术用户 / 终端爱好者 | 日常命令行解包、加密压缩、格式转换 | P1 |
| 桌面普通用户（GUI 阶段） | 拖拽压缩、右键菜单、图形化浏览 | P2 |

## 3. 平台范围

| 平台 | 形式 | CLI 阶段 | GUI 阶段 |
|------|------|---------|---------|
| Linux (x86_64, aarch64) | 终端 CLI | 原生二进制 | Tauri 桌面 |
| macOS (x86_64, arm64) | 终端 CLI + 桌面 | 原生二进制 / Homebrew | Tauri .dmg |
| Windows (x86_64) | 终端 CLI + 桌面 | 原生二进制 / winget | Tauri .msi |
| Linux 桌面 | GUI | — | Tauri .AppImage/.deb |

## 4. 核心场景

1. **批量归档**：用户将多个文件/目录打包为 `.tar.gz`、`.zip`，带进度显示。
2. **快速解包**：用户解压常见格式，自动检测格式，无需手动指定。
3. **格式转换**：用户将 `.tar.gz` 转为 `.zip`，或转为 `.zst` 以节省空间。
4. **管道集成**：`cat data.tar.gz | geezipx decompress --stdout | tar xf -`。
5. **加密压缩**：用户创建带密码的 ZIP/AES 归档（高级功能）。
6. **桌面拖拽**（GUI 阶段）：拖入文件夹 → 选格式 → 压缩完成。

## 5. MVP 范围（Phase 1 — CLI MVP）

> **当前状态**：基础 `compress`、`decompress`、`list` 子命令已实现（提交 `329c773`），
> 支持 zip/tar/tar.gz/gzip 格式。以下表格标记了各特性的完成状态。

严格限定为 **命令行高性能压缩/解压缩**：

| 特性 | 说明 | 状态 |
|------|------|------|
| 格式支持 | `.tar.gz`, `.tgz`, `.zip`, `.tar`, `.gz`, `.zst`, `.zstd`（读/写） | **已完成** (5 格式) |
| 流式处理 | 文件流读写，内存占用与文件大小解耦 | **未完成** (M3) |
| 进度显示 | 进度条，支持 `--progress` / `--no-progress` | **未完成** (M3) |
| 格式自动检测 | 根据文件魔数（magic bytes）自动检测归档格式 | **已完成** |
| 压缩级别 | `--level 0-9`（gzip/tar.gz）；`--level 0-22`（zstd） | **已完成** |
| 标准管道 | `--stdout` 支持 gzip 解压到标准输出 | **部分完成** |
| 递归操作 | `-r` 递归添加目录，保持目录结构 | **已完成** |
| 覆盖保护 | `--no-clobber` / `--force` 覆盖策略 | **已完成** |
| 列表功能 | 表格 + JSON 输出，支持所有当前格式 | **已完成** |
| 测试覆盖 | 131 个测试（核心单元测试 + 23 个 CLI 集成测试），覆盖率未测量 | **已完成**（功能测试） |
| 三平台 CI | GitHub Actions：三平台矩阵（ubuntu/macos/windows），push/PR/tag/manual 触发 | **大部分完成** (M4) |

> **扩展格式识别**：魔数检测已支持 xz（`FD 37 7A 58 5A 00`）和 zstd（`28 B5 2F FD`），枚举值已预定义。zstd 单流压缩/解压已支持（`geezipx-core` via `zstd` crate）；xz 的压缩/解压推迟到后续版本实现。

## 6. 非目标（Phase 1 明确不做）

- 桌面 GUI / Tauri — 留到 Phase 2+
- 7z 格式 — 格式复杂，三期考虑
- 分卷压缩 — 二期考虑
- 图形化文件浏览 — GUI 阶段
- 右键菜单集成 — GUI 阶段
- 可视化对比 / 压缩率图表 — GUI 阶段

## 7. 功能需求（Feature Requirements）

### FR-1: 压缩
- `geezipx compress [input]... --format zip -o output.zip`
- `geezipx compress [input]... --format tar.gz -o output.tar.gz`
- 支持通配符：`geezipx compress src/*.rs --format zip -o src.zip`
- `-r` 递归添加目录
- `--level N` 控制压缩级别（gzip/tar.gz: 0-9, zstd: 0-22）

### FR-2: 解压缩
- `geezipx decompress archive.zip` — 自动检测格式并解压到当前目录
- `geezipx decompress archive.tar.gz -o /tmp/out` — 指定输出目录
- `geezipx decompress archive.zip --stdout` — 解压到标准输出

### FR-3: 格式自动检测
- 检查文件头部魔数字节
- 无魔数时 fallback 到扩展名匹配
- 均无法识别时报错：`error: unsupported or unknown format`

### FR-4: 进度显示
- 默认开启（tty 下），`--no-progress` 禁用
- 管道模式下自动禁用进度条
- 显示：文件名、已处理大小/总大小(PERCENT)、速度、ETA

### FR-5: 列出归档内容
- `geezipx list archive.zip`
- 显示：文件名、原始大小、压缩后大小、压缩率、修改时间

### FR-6: 流式管道
- 支持 stdin/stdout 作为输入输出
- `geezipx decompress --stdout < archive.tar.gz | tar xf -`

## 8. 非功能需求（Non-Functional Requirements）

| 需求 | 目标 | 度量方式 |
|------|------|---------|
| 性能 | 压缩/解压速度不低于同格式原生工具(pigz/unzip) 90% | 基准测试（hyperfine） |
| 内存 | 流式模式下内存占用 < 256 MB | 大文件（10 GB）测试 |
| 二进制体积 | 静态链接 < 15 MB, MUSL < 20 MB | `du -sh` |
| 启动时间 | < 50 ms（首次命令到输出） | hyperfine |
| 跨平台一致性 | 同版本在三大平台产生相同输出 | CI hash 对比 |
| 错误信息质量 | 用户无需查手册即可理解错误 | 人工审查 |
| 代码覆盖率 | 核心引擎 > 85%，整体 > 80% | cargo-tarpaulin / grcov |

## 9. 路线图

```
Phase 1 (MVP — CLI)        ← 当前阶段
├── M1 项目骨架 + 核心引擎库    ── ✅ 已完成
├── M2 CLI 基本命令            ── ✅ 已完成
├── M3 流式/进度/兼容性打磨     ── ⬜ 未开始
└── M4 CI/测试/发布            ── 🔶 大部分完成

Phase 2 (CLI 增强)
├── 读取 7z（只读）、RAR（只读）
├── 多线程压缩（rayon）
├── 分卷压缩
├── 密码加密 ZIP (AES-256)
└── 性能吞吐优化

Phase 3 (Tauri GUI)
├── Tauri + Vue/Svelte 外壳
├── 文件浏览器 + 拖拽支持
├── 压缩/解压任务队列
├── 右键菜单集成
└── 平台安装包分发

Phase 4 (生态)
├── Homebrew / winget / APT 仓库
├── Shell 补全
├── Nushell / fish / zsh 插件
└── 性能排行榜
```

## 10. 成功指标

| 指标 | 目标值 | 测量方式 |
|------|--------|---------|
| GitHub Stars | Phase1 > 200, Phase3 > 1000 | 计数 |
| 日活跃 CLI 用户 | Phase1 > 50 | 遥测（opt-in） |
| 格式兼容性 | 与 Info-ZIP/GNU tar 100% 互操作 | 集成测试 |
| CI 通过率 | 主干 > 99% | GitHub Actions |
| 用户报告 P0 bug 数 | 每月 < 3 | Issue tracker |
| 代码覆盖率 | > 80% | cargo-tarpaulin |

## 11. 竞品参照

| 工具 | 优点 | GeeZipX 差异化 |
|------|------|---------------|
| `tar` + `gzip` | 生态标准，功能完整 | 跨平台一致体验、进度、格式统一 CLI、单一二进制 |
| `unzip` / `zip` | 广泛使用 | 一个工具搞定所有格式，进度+流式 |
| `7z` / `p7zip` | 高压缩率 | Rust 安全内存、现代 CLI 设计、Tauri GUI |
| `bandizip`/`keka` | GUI 体验好 | 免费开源、跨平台 CLI |

---

## 附录 A：术语表

| 术语 | 说明 |
|------|------|
| 流式处理 | 边读边处理，不将整个文件加载到内存 |
| 魔数字节 | 文件头部固定签名，用于识别文件格式 |
| 分卷压缩 | 将大归档拆分为多个小文件 |
| tqdm | Python 工具库，迭代时显示进度条 |
