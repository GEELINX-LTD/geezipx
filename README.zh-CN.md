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

- **多格式支持** -- ZIP、TAR、TAR.GZ/TGZ、GZIP/GZ（读写）
- **流式 I/O** -- 大文件处理内存可控
- **实时进度条** -- 显示速度、预计完成时间、逐文件状态
- **取消安全** -- Ctrl+C 优雅退出，自动清理未完成文件；双击强制退出
- **格式自动检测** -- 魔数字节识别 + 扩展名回退
- **压缩级别** -- `--level 0-9`，gzip/tar.gz 生效
- **覆盖控制** -- `--no-clobber` 跳过已有文件，`--force` 强制覆盖
- **Zip Slip 防护** -- 所有归档格式防护路径穿越攻击
- **JSON 输出** -- `list --json` 机器可读格式
- **Shell 补全** -- bash、zsh、fish、PowerShell、elvish
- **跨平台** -- Linux、macOS、Windows（三平台 CI）
- **单一二进制** -- 无运行时依赖，`cargo install` 即装即用

---

## 项目状态

第一阶段（CLI MVP）**已完成**。`compress`、`decompress`、`list`、`completions` 四个子命令对四种格式均正常工作。

| 里程碑 | 主题 | 状态 |
|--------|------|------|
| M1 | 项目骨架 + 核心引擎库 | 已完成 |
| M2 | CLI 基本命令 | 已完成 |
| M3 | 流式处理 / 进度 / 打磨 | 已完成 |
| M4 | CI / 测试 / 发布 | 大部分完成（发布就绪） |

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

每个 [GitHub Release](https://github.com/GEELINX-LTD/geezipx/releases) 均提供预编译二进制：

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

> **注意：** 从后续 release 开始，二进制文件将自动上传至 Releases 页面。v0.2.0 仅提供 crates.io 和 GitHub 源码。

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
geezipx decompress hello.txt.gz --stdout > output.txt

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
| `-f`, `--format` | 格式：`zip`、`tar`、`tar.gz`、`tgz`、`gz`、`gzip`（省略时从扩展名推断，默认 zip） |
| `-r`, `--recursive` | 递归添加目录 |
| `-L`, `--level` | 压缩级别 0-9（仅 gzip/tar.gz，默认 6） |

### `decompress` — 解压归档

```sh
geezipx decompress <归档文件> [选项]
```

自动通过魔数字节检测格式（扩展名作为回退）。

| 选项 | 说明 |
|------|------|
| `-o`, `--output-dir` | 输出目录（默认：当前目录） |
| `--stdout` | 解压到 stdout（仅 gzip；多文件归档会报错） |
| `--no-clobber` | 跳过已存在的文件 |
| `--force` | 覆盖已存在的文件（默认行为；与 `--no-clobber` 互斥） |

### `list` — 查看归档内容

```sh
geezipx list <归档文件> [选项]
```

以表格形式显示文件路径、大小、压缩后大小、压缩率和修改时间。

| 选项 | 说明 |
|------|------|
| `-j`, `--json` | 以 JSON 数组格式输出 |

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
│   │       ├── archive/    # ZIP、TAR、TAR.GZ、GZIP 格式实现
│   │       ├── detect.rs   # 格式检测（魔数字节 + 扩展名）
│   │       ├── error.rs    # 统一错误类型（GeeZipError）
│   │       └── io.rs       # 流式 I/O 封装（ProgressReader 等）
│   └── cli/                # CLI 二进制（clap 驱动，core 的薄壳）
│       └── src/
│           ├── commands/   # compress / decompress / list
│           ├── render/     # 进度条渲染
│           └── signal.rs   # Ctrl+C 取消处理
├── docs/                   # 产品与架构文档
├── scripts/                # 构建、CI 和互操作测试脚本
├── tests/                  # 集成测试
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

# 生成文档
cargo doc --no-deps
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

### Release 构建验证

```sh
# 提交前质量门禁
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --workspace --all-features && \
cargo build --release --workspace
```

---

## 路线图

### 第一阶段（CLI MVP）— 当前 ✓

核心功能已全部实现并验证：

- [x] ZIP / TAR / TAR.GZ / GZIP 读写
- [x] 流式 I/O，内存占用可控
- [x] indicatif 进度条
- [x] Ctrl+C 优雅取消
- [x] 自动格式检测（魔数字节 + 扩展名）
- [x] 覆盖保护（`--no-clobber` / `--force`）
- [x] Zip Slip 路径穿越防护
- [x] Shell 补全（5 种 shell）
- [x] `list --json` 机器可读输出
- [x] 200+ 测试（单元 + 集成 + 互操作）
- [x] 三平台 CI（Linux/macOS/Windows）
- [x] cargo-deny 安全审计
- [x] Criterion 基准测试
- [x] **crates.io 发布**

### 第二阶段（CLI 增强）— 规划中

- zstd/tar.zst 多线程压缩（`-j`/`--jobs`，zstd 原生 NbWorkers）— **已完成**
- xz / LZMA 读写
- Zstandard 读写
- 加密 ZIP（AES-256）
- 分卷压缩
- 7z 只读支持
- RAR 只读支持

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
4. 如果变更影响公共 API 或 CLI 行为，同步更新文档

---

## 许可证

本项目基于 MIT 许可证发布，详见 [LICENSE](LICENSE)。
