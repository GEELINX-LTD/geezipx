# GeeZipX

[![CI](https://github.com/GEELINX-LTD/geezipx/actions/workflows/ci.yml/badge.svg)](https://github.com/GEELINX-LTD/geezipx/actions/workflows/ci.yml)
[![Audit](https://github.com/GEELINX-LTD/geezipx/actions/workflows/deny.yml/badge.svg)](https://github.com/GEELINX-LTD/geezipx/actions/workflows/deny.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/geezipx.svg)](https://crates.io/crates/geezipx)

> **一款基于 Rust 构建的高性能跨平台压缩/解压缩命令行工具。**  
> 统一接口处理多种归档格式，CLI 优先，流式驱动。

[English](README.md)

---

## 特性

- **多格式支持** -- ZIP、TAR、TAR.GZ/TGZ、TAR.ZST/TZST、TAR.XZ/TXZ、GZIP/GZ、Zstandard/ZST、XZ、LZMA（读写）
- **流式 I/O** -- 大文件处理内存可控
- **实时进度条** -- 显示速度、预计完成时间、逐文件状态
- **取消安全** -- Ctrl+C 优雅退出，自动清理未完成文件；双击强制退出
- **格式自动检测** -- 魔数字节识别 + 扩展名回退
- **压缩级别** -- gzip/tar.gz/xz/lzma/tar.xz 支持 `--level 0-9`；zstd/tar.zst 支持 `--level 0-22`
- **覆盖控制** -- `--no-clobber` 跳过已有文件，`--force` 强制覆盖
- **Zip Slip 防护** -- 所有归档格式防护路径穿越攻击
- **JSON 输出** -- `list --json` 机器可读格式

- **完整性验证** -- `test --json` 验证归档完整性，支持 CRC-32 校验
- **Shell 补全** -- bash、zsh、fish、PowerShell、elvish
- **跨平台** -- Linux、macOS、Windows（三平台 CI）
- **单一二进制** -- 无运行时依赖，`cargo install` 即装即用
- **多线程压缩** -- zstd 和 tar.zst 支持 `-j`/`--jobs` 并行压缩
- **ZIP AES-256 加密** -- 支持 `--password`、`--password-file`、`--password-stdin`（仅限 ZIP 格式）
- **stdin/stdout 管道支持** -- `compress --stdin`/`--stdout` 和 `decompress --stdin`，支持 gzip/zstd/xz/lzma 单流格式

---

## 项目状态

第一阶段（CLI MVP）的**核心 CLI 功能已完成**。`compress`、`decompress`、`list`、`test`、`completions` 五个子命令对当前支持格式均正常工作。

| 里程碑 | 主题 | 状态 |
|--------|------|------|
| M1 | 项目骨架 + 核心引擎库 | ✅ 已完成 |
| M2 | CLI 基本命令 | ✅ 已完成 |
| M3 | 流式处理 / 进度 / 打磨 | ✅ 已完成 |
| M4 | CI / 测试 / 发布 | ✅ 已完成 |

### 后续待办

这些不是 CLI MVP 的阻塞项，但仍是文档中明确提到的后续工作：

- 覆盖率已追踪但尚未作为硬门禁；当前覆盖率低于 PRD 目标（整体 >80%、core >85%）。
- Criterion 基准已建立，并已加入手动性能回归阈值检查；稳定基线和强制比较数据仍待完善。
- PR 覆盖率注释、coverage badge 或 diff coverage 反馈尚未实现。
- Release workflow 已能为后续 `v*` tag 自动构建二进制；历史 release 可能仍只有源码包，需要逐个 release 页面确认。
- stdin/stdout 管道支持已完成（gzip/zstd/xz/lzma 单流格式）
- 显式符号链接跟踪、Windows 长路径开关等高级文件系统选项仍属于后续增强。

详细任务拆解参见 [`docs/PHASE1_CLI_TASKS.md`](docs/PHASE1_CLI_TASKS.md)。

---

## 安装

### 源码编译

```sh
git clone https://github.com/GEELINX-LTD/geezipx.git
cd geezipx
cargo build --release

# 二进制文件位于 ./target/release/geezipx
./target/release/geezipx --version
```

### 通过 cargo 安装

```sh
cargo install geezipx
```

### 预构建二进制

包含预构建产物的 release 会使用以下文件名：

| 平台 | 文件 |
|------|------|
| Linux (x86_64) | `geezipx-linux-x86_64.tar.gz` |
| macOS (x86_64)  | `geezipx-macos-x86_64.tar.gz` |
| Windows (x86_64) | `geezipx-windows-x86_64.zip` |

每个文件附带 `.sha256` 校验文件和合并的 `SHA256SUMS` 用于验证。

```sh
# 下载并验证（Linux 示例）
curl -LO https://github.com/GEELINX-LTD/geezipx/releases/latest/download/geezipx-linux-x86_64.tar.gz
curl -LO https://github.com/GEELINX-LTD/geezipx/releases/latest/download/geezipx-linux-x86_64.tar.gz.sha256
shasum -a 256 -c geezipx-linux-x86_64.tar.gz.sha256
tar -xzf geezipx-linux-x86_64.tar.gz
sudo mv geezipx /usr/local/bin/
```

> **注意：** Release workflow 已配置为在后续 `v*` tag release 自动上传预构建二进制。历史 release 可能仍只有源码包；请以具体 GitHub Release 页面为准。

### 前置条件

- [Rust](https://rustup.rs/) stable 工具链（参见 `.rust-toolchain.toml`）

---

## 快速上手

```sh
# 压缩文件为 ZIP
geezipx compress hello.txt -o hello.zip

# 指定压缩格式
geezipx compress hello.txt -f gzip -o hello.txt.gz

# 递归压缩目录为 tar.gz
geezipx compress mydir/ -r -f tar.gz -o mydir.tar.gz

# 解压归档（自动检测格式）
geezipx decompress hello.zip

# gzip 解压到 stdout

# 管道模式（stdin/stdout）
管道模式将数据通过 stdin 读取或写入 stdout，适合脚本链式调用。

```bash
# stdin -> 文件
echo "Hello" | geezipx compress --stdin -f gz -o hello.gz

# stdin -> stdout（完整管道）
echo "Hello" | geezipx compress --stdin -f gz --stdout > hello.gz
cat hello.txt.gz | geezipx decompress --stdin -f gz --stdout > restored.txt

# stdin -> 目录（输出文件固定名为 output）
cat hello.txt.gz | geezipx decompress --stdin -f gz -o outdir

# 文件 -> stdout
geezipx compress hello.txt -f gz --stdout > hello.gz
```

注意：管道模式仅支持 gzip/zstd/xz/lzma 单流格式，不支持 zip/tar/7z。

geezipx decompress hello.txt.gz --stdout > output.txt

# 使用 zstandard 压缩
geezipx compress hello.txt -f zst -o hello.txt.zst

# zstandard 解压到 stdout
geezipx decompress hello.txt.zst --stdout > output.txt

# 多线程 zstd 压缩（4 个 worker）
geezipx compress hello.txt -f zst -o hello.txt.zst -j 4

# 内建 glob 展开（压缩 src/ 下所有 .rs 文件）
geezipx compress src/**/*.rs -f tar.gz -o src-rs.tar.gz

# 递归压缩目录为 tar.zst
geezipx compress mydir -r -f tar.zst -o mydir.tar.zst

# 解压 tar.zst 归档
geezipx decompress mydir.tar.zst

# 查看 tar.zst 归档内容
geezipx list mydir.tar.zst

# 解压到指定目录
geezipx decompress archive.tar.gz -o /tmp/out

# 解压时跳过已存在文件
geezipx decompress archive.zip --no-clobber

# 解压时强制覆盖
geezipx decompress archive.zip --force

# 查看归档内容
geezipx list archive.zip

# JSON 格式查看归档内容
geezipx list archive.tar.gz --json

# 验证归档完整性
geezipx test archive.zip

# JSON 格式验证
geezipx test archive.tar.gz --json
```

---

## 使用说明

### 全局选项

| 选项 | 说明 |
|------|------|
| `--no-progress` | 禁用进度条（适合脚本中使用） |
| `-v`, `--verbose` | 逐文件日志输出 |

### `compress` — 创建归档

```sh
geezipx compress <输入文件...> -o <输出文件> [选项]
```

| 选项 | 说明 |
|------|------|
| `-o`, `--output` | 输出文件路径 **（必填）** |
| `-f`, `--format` | 格式：`zip`、`tar`、`tar.gz`、`tgz`、`gz`、`gzip`、`tar.zst`、`tzst`、`zst`、`zstd`、`tar.xz`、`txz`、`xz`、`lzma`（省略时从扩展名推断，默认 zip） |
| `-r`, `--recursive` | 递归添加目录 |
| `-L`, `--level` | 压缩级别 0-9（gzip/tar.gz/xz/tar.xz，默认 6）；0-22（zstd/zst/tar.zst/tzst，默认使用 zstd 默认级别） |
| `-j`, `--jobs` | Worker 线程数：1（默认，单线程）、0（自动使用全部 CPU）或 N（显式指定）。当前仅对 zstd/tar.zst 生效；其他格式接受但暂不使用，便于向前兼容 |
|| `--password` | 使用 AES-256 加密 ZIP 归档（仅限 ZIP 格式）。使用 `--password-file` 从文件读取密码，或使用 `--password-stdin` 从标准输入读取。三者互斥。脚本中建议使用 `--password-file` 或 `--password-stdin` 以避免密码暴露在进程列表中 |

### `decompress` — 解压归档
|| `--stdin` | 从 stdin 读取未压缩数据（仅 gzip/zstd/xz/lzma；需配合 `--format`；与输入文件互斥） |
|| `--stdout` | 将压缩结果写入 stdout（仅 gzip/zstd/xz/lzma；需配合 `--format`；与 `--output` 互斥） |

```sh
geezipx decompress <归档文件> [选项]
```

自动通过魔数字节检测格式（扩展名作为回退）。

| 选项 | 说明 |
|------|------|
| `-o`, `--output-dir` | 输出目录（默认：当前目录） |
| `--stdout` | 解压到 stdout（仅 gzip/zstd/xz/lzma 单流格式；tar.gz、tar.zst、tar.xz 等多文件归档会报错） |
|| `--stdin` | 从 stdin 读取压缩数据（仅 gzip/zstd/xz/lzma；需配合 `--format`；与归档文件互斥） |
|| `-f`, `--format` | 归档/流格式（使用 `--stdin` 时必填） |
| `--no-clobber` | 跳过已存在的文件 |
| `--force` | 覆盖已存在的文件（默认行为；与 `--no-clobber` 互斥） |
|| `--password` | 解密加密 ZIP 归档的密码（AES-256）。使用 `--password-file` 从文件读取，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

### `list` — 查看归档内容

```sh
geezipx list <归档文件> [选项]
```

以表格形式显示文件路径、大小、压缩后大小、压缩率和修改时间。

| 选项 | 说明 |
|------|------|
| `-j`, `--json` | 以 JSON 数组格式输出 |
| `--password` | 解密加密归档（ZIP/7z）的密码。使用 `--password-file` 从文件读取密码，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

> **危险路径警告**：归档中的危险路径（绝对路径、路径穿越条目、Windows 设备路径）会输出警告到 stderr；stdout/JSON 输出保持干净不受影响。

### `test` — 验证归档完整性

```sh
geezipx test <归档文件> [选项]
```

逐条完整读取归档内所有条目，确认归档结构完好。
ZIP 格式额外支持 CRC-32 校验。
损坏的归档会导致退出码非零。

| 选项 | 说明 |
|------|------|
| `-j`, `--json` | 以 JSON 格式输出，包含 `ok` 布尔字段 |
|| `--password` | 验证加密 ZIP 归档的密码。使用 `--password-file` 从文件读取，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

### `completions` — 生成 Shell 补全脚本

```sh
geezipx completions <SHELL>    # 别名：geezipx comp <SHELL>
```

支持的 shell：`bash`、`zsh`、`fish`、`powershell`、`elvish`。

示例：

```sh
# Bash
geezipx completions bash > /usr/local/share/bash-completion/completions/geezipx

# Zsh
geezipx completions zsh > /usr/local/share/zsh/site-functions/_geezipx

# Fish
geezipx completions fish > ~/.config/fish/completions/geezipx.fish
```

安装后重启 shell 或 source 该文件，即可对子命令、选项和参数实现 Tab 补全。

---

## 项目结构

```
geezipx/
├── AGENTS.md               # AI 代理协作指南
├── CHANGELOG.md            # 发布变更日志
├── Cargo.toml              # Workspace 根定义
├── crates/
│   ├── core/               # 压缩/解压缩核心引擎库
│   │   └── src/
│   │       ├── archive/    # ZIP、TAR、TAR.GZ、TAR.ZST、TAR.XZ、GZIP、ZSTD、XZ、LZMA 等格式实现
│   │       ├── config.rs   # 压缩选项（CompressOptions、--jobs/--level）
│   │       ├── detect.rs   # 格式检测（魔数字节 + 扩展名）
│   │       ├── error.rs    # 统一错误类型（GeeZipError）
│   │       └── io.rs       # 流式 I/O 封装（ProgressReader 等）
│   └── cli/                # CLI 二进制（clap 驱动，core 的薄壳）
│       └── src/
│           ├── commands/   # compress / decompress / list / test
│           ├── render/     # 进度条渲染
│           └── signal.rs   # Ctrl+C 取消处理
├── docs/                   # 产品与架构文档
├── scripts/                # 构建、CI 和互操作测试脚本
├── crates/cli/tests/       # CLI 集成测试和流式 smoke 测试
├── .github/workflows/      # CI、审计和基准测试工作流
├── deny.toml               # cargo-deny 安全审计配置
└── .rust-toolchain.toml    # Rust 工具链固定
```

### 架构

GeeZipX 采用分层 workspace 架构：

```
┌─────────────┐  ┌─────────────┐
│  cli (bin)  │  │  gui-tauri  │  ← 前端层（CLI / 未来 GUI）
└──────┬──────┘  └──────┬──────┘
       │                │
       └───────┬────────┘
               │   依赖
       ┌───────▼────────┐
       │  core (lib)     │  ← 核心引擎：所有归档/压缩逻辑
       │  ─ 纯数据流     │     - 无 I/O 假设
       │  ─ 无终端依赖   │     - 可被 CLI 和未来 Tauri GUI 复用
       └────────────────┘
```

核心库通过 `ArchiveReader` / `ArchiveWriter` 统一 trait 处理所有格式逻辑。CLI 仅处理参数解析和进度显示。这一设计确保了同一压缩引擎可在未来的 Tauri 桌面 GUI 中复用，无需重复开发。

---

## 开发指南

### 前置条件

- Rust stable（通过 [rustup](https://rustup.rs/) 安装）

### 构建与测试

```sh
# 构建所有 workspace crate
cargo build

# 运行全部测试（单元测试 + 集成测试）
cargo test --workspace --all-features

# Release 构建
cargo build --release

# 检查代码格式化
cargo fmt --all --check

# 运行 clippy（严格模式）
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 生成文档并将 rustdoc warning 视为错误
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
```

### 运行基准测试

```sh
# 验证基准测试可编译
cargo bench --no-run -p geezipx-core

# 运行完整基准测试
cargo bench -p geezipx-core
```

基准覆盖 gzip 吞吐量（4 种级别 × 2 种大小）和归档吞吐量（tar.gz、ZIP round-trip）。

### 互操作测试

```sh
# 使用系统 tar、unzip、gzip 进行标准检查
bash scripts/check-interop.sh

# 重型压力测试（256 MB gzip、1000 文件 tar.gz）
GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh
```

```sh
# CI 专用流式 smoke（默认 cargo test 不运行，需显式 --ignored）
cargo test -p geezipx --test streaming_smoke -- --test-threads=1 --ignored
```

### Release 构建验证

```sh
# 提交前质量门禁
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --workspace --all-features && \
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items && \
cargo build --release --workspace
```

---

## 路线图

### 第一阶段（CLI MVP）— 当前 ✓

核心功能已全部实现并验证：

- [x] ZIP / TAR / TAR.GZ / TAR.ZST / TAR.XZ / GZIP / ZSTD / XZ / LZMA 读写
- [x] 流式 I/O，内存占用可控
- [x] indicatif 进度条
- [x] Ctrl+C 优雅取消
- [x] 自动格式检测（魔数字节 + 扩展名）
- [x] 覆盖保护（`--no-clobber` / `--force`）
- [x] Zip Slip 路径穿越防护
- [x] Shell 补全（5 种 shell）
- [x] `list --json` 机器可读输出

- [x] `list` 危险路径警告（stderr，不影响 stdout/JSON 输出）
- [x] `test` 归档完整性验证（ZIP CRC-32、TAR 结构校验，支持 JSON 输出）
- [x] 单流格式 `test` 支持：GZIP、ZSTD、XZ、LZMA
- [x] 300+ 测试（单元 + 集成 + 互操作 + 流式 smoke）
- [x] 三平台 CI（Linux/macOS/Windows）
- [x] cargo-deny 安全审计
- [x] Streaming smoke 与 rustdoc warning CI 守卫
- [x] Criterion 基准测试
- [x] **crates.io 发布**

### 第二阶段（CLI 增强）— 部分已完成 / 规划中

- zstd/tar.zst 多线程压缩（`-j`/`--jobs`，zstd 原生 NbWorkers）— **已完成**
- xz / LZMA 读写 — **已完成**
- Zstandard 读写 — **已完成**
- tar.zst / tar.xz 归档格式读写 — **已完成**
- ZIP AES-256 加密 -- 支持 `--password`、`--password-file`、`--password-stdin`
- 分卷压缩
- 7z 只读支持
- RAR 只读支持
- tar.gz、zip、xz 等更多格式的多线程压缩
- 面向脚本场景的真正 stdin 管道输入
- 稳定 benchmark 基线与强制性能回归门禁

### 第三阶段（桌面 GUI）— 未来

- 基于 Tauri 的桌面应用
- 拖拽压缩
- 批量操作任务队列
- 右键菜单集成
- 平台原生安装包（Homebrew、winget、APT）

### 第一阶段不做的事项

GUI、7z 写入、RAR 创建、云同步、插件系统、自动更新——将在后续阶段评估。

---

## 配置

GeeZipX 遵循 [XDG 基础目录规范](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)。目前所有配置通过命令行选项完成，无需配置文件。

---

## 贡献指南

欢迎贡献代码！请先阅读 [AGENTS.md](AGENTS.md) 了解开发规范和协作指南。

提交 PR 前请确认：

1. 全部测试通过：`cargo test --workspace --all-features`
2. Clippy 零警告：`cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. 代码格式正确：`cargo fmt --all --check`
4. 文档检查通过：`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`
5. 如果变更影响公共 API 或 CLI 行为，同步更新文档

---

## 许可证

本项目基于 MIT 许可证发布，详见 [LICENSE](LICENSE)。
