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

- **多格式支持** -- ZIP（含 ZIP 兼容别名 `.zipx`、`.jar`、`.war`、`.apk`、`.ipa`、`.xpi`）、TAR、TAR.GZ/TGZ、TAR.BZ2/TBZ/TBZ2、TAR.BR、TAR.LZ4、TAR.ZST/TZST、TAR.XZ/TXZ、GZIP/GZ、BZIP2/BZ2、Brotli/BR、LZ4、Zstandard/ZST、XZ、LZMA、7Z、LZH/LHA（读写；当前写入器为 store-only `-lh0-`），以及 RAR、CAB、ASAR、DEB、ISO、CPIO、ZPAQ（只读）。*规划扩展：更完整的 LZH/LHA 兼容、ISO 写入、ZPAQ 写入、SFX、WIM 等（详见 [docs/PRD.md](docs/PRD.md) 第 5.1 节）*
- **ZIPX 兼容支持** -- `.zipx` 已作为 ZIP 兼容容器/扩展名别名接入 `compress`、`list`、`test` 与 `decompress`。当前不承诺 WinZip 专有高级压缩方法、Deflate64 写入或完整 ZIPX method matrix。
- **流式 I/O** -- 大文件处理内存可控
- **实时进度条** -- 在 TTY 中显示速度、预计完成时间、逐文件状态
- **取消安全** -- Ctrl+C 优雅退出，自动清理未完成文件；双击强制退出
- **格式自动检测** -- 魔数字节识别 + 扩展名回退
- **压缩级别** -- gzip/tar.gz/xz/lzma/tar.xz 支持 `--level 0-9`；zstd/tar.zst 支持 `--level 0-22`
- **覆盖控制** -- `--no-clobber` 跳过已有文件，`--force` 强制覆盖
- **Zip Slip 防护** -- 所有归档格式都防护路径穿越攻击
- **JSON 输出** -- `list --json` 机器可读；`test --json` 适合程序化验证
- **Shell 补全** -- bash、zsh、fish、PowerShell、elvish
- **ZIP AES-256 加密** -- 可用 `--password`、`--password-file`、`--password-stdin` 创建加密 ZIP 归档
- **加密 7z/RAR 只读支持** -- 只读 `list`、`decompress`、`test` 可处理带密码的 7z/RAR 归档
- **ASAR 只读支持** -- `.asar` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 归档浏览与选择性提取；不支持 `compress`、归档写入、加密或密码输入。
- **DEB 只读支持** -- `.deb` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 对 `data.tar*` payload 的归档浏览与提取；不支持 `compress`、包写入、control scripts 提取、加密或密码输入。
- **CAB 只读支持** -- `.cab` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 归档浏览与提取。当前 MVP 面向单卷 cabinet，不支持 `compress`、cabinet 写入、加密/密码输入或多卷 cabinet set。
- **LZH/LHA 写入 MVP** -- `.lzh` / `.lha` 支持 CLI/GUI `compress`、`list`、`decompress`、`test`。当前写入器输出 store-only `-lh0-` 文件条目和 `-lhd-` 目录条目；`lh5`/`lh6`/`lh7` 压缩写入、加密/密码输入、多卷归档、扩展属性、长路径 / level 1-3 扩展 header 写入，以及单条目超过 4 GiB 仍不支持。
- **ISO 只读支持** -- `.iso` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 归档浏览与提取。当前 MVP 面向常见 ISO9660 / Rock Ridge / Joliet 数据 ISO，不支持 `compress`、镜像写入、加密或密码输入。
- **CPIO 只读支持** -- `.cpio` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 归档浏览与提取。当前 MVP 支持 `newc` / `odc`，仅通过扩展名识别，不支持 `compress`、`bin` / `crc` 变体、宿主 symlink/device 创建、加密或密码输入。 
- **ZPAQ 只读支持** -- `.zpaq` / `.zpq` 支持 CLI `list`、`decompress`、`test`，也支持 GUI 归档浏览与提取。当前 MVP 为只读实现，不支持归档创建或追加/更新工作流，单条目提取目前通过字节缓冲 helper 完成。
- **跨平台** -- Linux、macOS、Windows（三平台 CI）
- **单一二进制** -- 无运行时依赖，`cargo install` 即装即用
- **多线程压缩** -- tar.gz（gzp/pigz 风格）与 zstd/tar.zst（zstd 原生 NbWorkers）支持 `-j`/`--jobs` 并行压缩

---

## 项目状态

第一阶段（CLI MVP）已经**全部完成并进入成熟阶段**。适用的子命令均已落地：读写格式（含 7z 与当前 LZH/LHA store-only MVP）支持 `compress`、`decompress`、`list`、`test`；只读 RAR/CAB/ASAR/DEB/ISO/CPIO/ZPAQ 支持 `list`、`decompress`、`test`。`completions` 子命令也已完成。crates.io 上已发布 `geezipx` 和 `geezipx-core` 包。
第二阶段（桌面 GUI via Tauri）**是当前开发重心**。

| 阶段 | 主题 | 状态 |
|------|------|------|
| 1 | CLI MVP | **已完成** -- crates.io 上已发布 `geezipx` 与 `geezipx-core` |
| 2 | 桌面 GUI (Tauri) | **开发中** -- v0.5.0 已包含归档浏览器、拖拽、进度显示、选择性提取等能力 |

详见 [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) 了解详细规划和剩余任务。

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
- 支持 C++17 的 C++ 编译器工具链（默认 RAR / ZPAQ 支持需要；可通过 `--no-default-features` 跳过）

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

# 使用 zstandard 压缩
geezipx compress hello.txt -f zst -o hello.txt.zst

# zstandard 解压到 stdout
geezipx decompress hello.txt.zst --stdout > output.txt

# 多线程 zstd 压缩（4 个 worker）
geezipx compress hello.txt -f zst -o hello.txt.zst -j 4

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

# stdin/stdout 管道示例（单流格式）
echo "Hello" | geezipx compress --stdin -f gz -o hello.gz
echo "Hello" | geezipx compress --stdin -f gz --stdout > hello.gz
cat hello.txt | geezipx compress --stdin -f zst -o hello.txt.zst
cat hello.txt.gz | geezipx decompress --stdin -f gz --stdout > restored.txt
cat hello.txt.gz | geezipx decompress --stdin -f gz -o outdir      # 输出为 outdir/output
geezipx compress hello.txt -f gz --stdout > hello.gz               # 文件 -> stdout

# tar-based 管道示例（stdin/stdout 传输裸 tar 流）
cat raw.tar | geezipx compress --stdin -f tar.gz -o archive.tar.gz
tar cf - mydir/ | geezipx compress --stdin -f tar.zst -o mydir.tar.zst
geezipx decompress archive.tar.gz --stdout | tar tf -
geezipx decompress archive.tar.xz --stdout > raw.tar
```

注意：管道模式支持 gzip/zstd/xz/lzma 单流格式和 tar.gz/tar.zst/tar.xz 裸 tar 流，不支持 zip/tar/7z/rar 等多文件归档。

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
| `-o`, `--output` | 输出文件路径（除非使用 `--stdout`，否则必填） |
| `-f`, `--format` | 格式：`zip`、`zipx`、`jar`、`war`、`apk`、`ipa`、`xpi`、`tar`、`tar.gz`、`tgz`、`tar.bz2`、`tbz`、`tbz2`、`tar.br`、`tar.lz4`、`tar.zst`、`tzst`、`tar.xz`、`txz`、`7z`、`gz`、`gzip`、`bz2`、`bzip2`、`br`、`brotli`、`lz4`、`zst`、`zstd`、`xz`、`lzma`（省略时从扩展名推断，默认 zip） |
| `-r`, `--recursive` | 递归添加目录 |
| `-L`, `--level` | 压缩级别 0-9（gzip/bzip2/tar.gz/tar.bz2/xz/tar.xz，默认 6）；0-11（brotli/tar.br）；0-22（zstd/zst/tar.zst/tzst，默认使用 zstd 默认级别）；`lz4`/`tar.lz4` 仅接受 `0` 或省略 |
| `-j`, `--jobs` | Worker 线程数：1（默认，单线程）、0（自动使用全部 CPU）或 N（显式指定）。tar.gz（gzp 并行 gzip）和 zstd/tar.zst（zstd 原生 NbWorkers）实际启用多线程；tar.xz/zip/xz/lzma 接受但不生效（向前兼容）。**注意**：tar.gz 的 `--stdin` 单流模式下不生效（仅归档模式有效） |
| `--password` | 使用 AES-256 加密 ZIP 归档（仅限 ZIP 格式）。使用 `--password-file` 从文件读取密码，或使用 `--password-stdin` 从标准输入读取。三者互斥。脚本中建议使用 `--password-file` 或 `--password-stdin` 以避免密码暴露在进程列表中 |
| `--stdin` | 从 stdin 读取未压缩数据或裸 tar 流（gzip/bzip2/brotli/lz4/zstd/xz/lzma 和 tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz；需配合 `--format`；与输入文件互斥） |
| `--stdout` | 将压缩结果写入 stdout（gzip/bzip2/brotli/lz4/zstd/xz/lzma 和 tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz 裸 tar 流；需配合 `--format`；与 `--output` 互斥） |

### `decompress` — 解压归档

```sh
geezipx decompress <归档文件> [选项]
```

自动通过魔数字节检测格式（扩展名作为回退）。

| 选项 | 说明 |
|------|------|
| `-o`, `--output-dir` | 输出目录（默认：当前目录） |
| `--stdout` | 解压到 stdout：gzip/zstd/xz/lzma 输出原文；tar.gz/tar.zst/tar.xz 输出裸 tar 流；zip/tar/7z/rar 等多文件归档会报错 |
| `--stdin` | 从 stdin 读取压缩数据或压缩 tar 流（gzip/zstd/xz/lzma 和 tar.gz/tar.zst/tar.xz；需配合 `--format`；与归档文件互斥） |
| `-f`, `--format` | 归档/流格式（使用 `--stdin` 时必填） |
| `--no-clobber` | 跳过已存在的文件 |
| `--force` | 覆盖已存在的文件（默认行为；与 `--no-clobber` 互斥） |
| `--password` | 解密加密归档的密码（ZIP AES-256、7z AES-256、RAR）。使用 `--password-file` 从文件读取，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

### `list` — 查看归档内容

```sh
geezipx list <归档文件> [选项]
```

以表格形式显示文件路径、大小、压缩后大小、压缩率和修改时间。

| 选项 | 说明 |
|------|------|
| `-j`, `--json` | 以 JSON 数组格式输出 |
| `--password` | 解密加密归档（ZIP/7z/RAR）的密码。使用 `--password-file` 从文件读取密码，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

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
| `--password` | 验证加密归档的密码（ZIP AES-256、7z AES-256、RAR）。使用 `--password-file` 从文件读取，或使用 `--password-stdin` 从标准输入读取。三者互斥 |

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

```text
geezipx/
├── AGENTS.md               # AI 代理协作指南
├── CHANGELOG.md            # 发布变更日志
├── Cargo.toml              # Workspace 根定义
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── archive/    # 各类归档/容器实现
│   │       ├── config.rs   # 压缩选项（level/jobs/password）
│   │       ├── detect.rs   # 格式检测（魔数字节 + 扩展名）
│   │       ├── error.rs    # 统一错误类型（GeeZipError）
│   │       ├── io.rs       # ProgressReader / ProgressWriter / ProgressEvent
│   │       └── test.rs     # 归档完整性辅助逻辑
│   ├── cli/
│   │   ├── src/
│   │   │   ├── commands/   # compress / decompress / list / test / completions
│   │   │   ├── render/     # 终端进度与输出渲染
│   │   │   └── signal.rs   # Ctrl+C 取消处理
│   │   └── tests/          # CLI 集成测试与流式 smoke 测试
│   └── gui-tauri/
│       ├── src/
│       │   ├── bridge.ts   # 前端与 Tauri 的桥接类型/辅助函数
│       │   ├── main.ts     # 当前 Tauri 前端主逻辑（TypeScript/Vite）
│       │   └── style.css   # GUI 样式
│       └── src-tauri/
│           ├── src/
│           │   ├── commands/
│           │   ├── lib.rs
│           │   └── state.rs
│           └── tauri.conf.json
├── docs/                   # 产品与架构文档
├── scripts/                # 构建、CI、benchmark、互操作脚本
└── .github/workflows/      # CI、审计、覆盖率、benchmark、release 工作流
```

### 架构

GeeZipX 采用分层 workspace 架构：

```text
┌─────────────┐  ┌─────────────────┐
│  cli (bin)  │  │  gui-tauri       │  ← 前端层（CLI / Tauri GUI）
└──────┬──────┘  └────────┬─────────┘
       │                  │
       └────────┬─────────┘
                │ 依赖
        ┌───────▼──────────┐
        │  core (lib)       │  ← 核心引擎：归档/压缩逻辑
        │  ─ 纯数据流       │     - 不承载终端/UI 逻辑
        │  ─ 可复用 API     │     - 被 CLI 与 GUI 共同复用
        └──────────────────┘
```

核心库通过统一的 `ArchiveReader` / `ArchiveWriter` trait 以及单流 helper 处理格式逻辑。CLI 和 Tauri GUI 只负责参数映射、用户交互与进度展示，不重复实现压缩/解压逻辑。

---

## 开发指南

### 前置条件

- Rust stable（通过 [rustup](https://rustup.rs/) 安装）
- 支持 C++17 的 C++ 编译器工具链（默认 RAR / ZPAQ 支持需要；可通过 `--no-default-features` 跳过）

### RAR 支持

RAR 归档支持为**只读**且**默认启用**。[`unrar`](https://crates.io/crates/unrar) crate 链接了 RARLAB freeware
[UnRAR 源码](https://www.rarlab.com/rar_add.htm)（需要 C++ 编译器）。

```sh
# 默认构建（RAR 已包含）
cargo build --release

# 运行所有测试（RAR 已包含）
cargo test --all-features
```

如果需要在没有合适 C++ 工具链的环境中构建（不包含默认的 RAR / ZPAQ 支持）：

```sh
cargo build --release --no-default-features
cargo test --no-default-features
```

> **注意**：`cargo publish` 和 `cargo install` 默认包含 RAR 与 ZPAQ 支持。
> 如果无法满足 C++ 编译器要求，可使用 `--no-default-features` 构建。

### ASAR 支持

ASAR 归档当前为**只读**支持。GeeZipX 在 CLI 中可对 `.asar` 执行 `list`、`decompress`、`test`，Tauri GUI 会将其作为归档打开到 Archive Browser 中进行浏览与选择性提取。

当前不支持：

- `compress` / 创建 ASAR
- 归档写入或原地更新
- 加密 / 密码访问

```sh
geezipx list app.asar
geezipx test app.asar
geezipx decompress app.asar -o out/
```

### DEB 支持

DEB 包当前为**只读**支持，并遵循 `dpkg-deb -c` / `dpkg-deb -x` 风格的 payload 语义。GeeZipX 在 CLI（`list`、`decompress`、`test`）和 Tauri GUI Archive Browser 中默认只查看/提取包内的 `data.tar*` 成员；`control.tar.*` 脚本与元数据在这一阶段会被有意忽略。

当前不支持：

- `compress` / 创建 `.deb` 包
- 包写入或原地更新
- control script 提取或执行
- 加密 / 密码访问

```sh
geezipx list package.deb
geezipx test package.deb
geezipx decompress package.deb -o out/
```

### LZH/LHA 支持

LZH/LHA 归档已支持 CLI/GUI `compress`、`list`、`decompress`、`test`。当前写入器是 store-only MVP：普通文件写成 `-lh0-`，目录写成 `-lhd-`；提取时仍会在 `delharc` 归一化路径前先校验原始 LZH 路径字节，因此 `../`、绝对路径、UNC 路径与 Windows drive-relative 名称都会被拒绝。

当前不支持：

- `lh5` / `lh6` / `lh7` 压缩写入
- 加密 / 密码访问
- 多卷归档
- 扩展属性与更丰富的历史元数据
- 长路径与 level 1/2/3 扩展 header 写入
- 单条目超过 4 GiB

```sh
geezipx compress hello.txt -f lzh -o archive.lzh
geezipx list archive.lzh
geezipx test archive.lha
geezipx decompress archive.lzh -o out/
```

### ISO 支持

ISO 镜像当前为**只读**支持。GeeZipX 在 CLI 中可对 `.iso` 执行 `list`、`decompress`、`test`，Tauri GUI Archive Browser 也可浏览并提取这类镜像。当前 MVP 面向常见 ISO9660 / Rock Ridge / Joliet 数据 ISO，明确不承诺 UDF-only 介质、多卷镜像或完整 El Torito boot 元数据工作流。

当前不支持：

- `compress` / 创建 ISO 镜像
- 镜像写入或原地更新
- 加密 / 密码访问
- 超出当前只读 MVP 的更广泛磁盘镜像能力

```sh
geezipx list image.iso
geezipx test image.iso
geezipx decompress image.iso -o out/
```

### CPIO 支持

CPIO 归档当前为**只读**支持。GeeZipX 在 CLI 中可对 `.cpio` 执行 `list`、`decompress`、`test`，Tauri GUI Archive Browser 也可浏览并提取这类归档。当前 MVP 支持 `newc` 与 `odc`，并刻意保持为仅扩展名识别（不做浅层文件级 magic 自动判断）；提取时不会在宿主文件系统上创建 symlink、硬链接、device、FIFO、socket 等特殊对象。

当前不支持：

- `compress` / 创建 CPIO 归档
- 超出当前 MVP 的 `bin` / `crc` 变体
- 在宿主文件系统上创建 symlink、硬链接、device、FIFO、socket 等特殊对象
- 加密 / 密码访问

```sh
geezipx list archive.cpio
geezipx test archive.cpio
geezipx decompress archive.cpio -o out/
```

### ZPAQ 支持

ZPAQ 归档当前为**只读**支持。GeeZipX 在 CLI 中可对 `.zpaq` / `.zpq` 执行 `list`、`decompress`、`test`，Tauri GUI Archive Browser 也可浏览并提取这类归档。当前 MVP 明确**不**实现归档创建、追加/更新语义、密码访问、版本选择等需要单独写路径设计的 journaling 工作流。

实现说明：

- GeeZipX 通过默认启用的可选 `zpaq` feature 接入 `zpaq_rs`。
- `zpaq_rs` 需要支持 C++17 的编译器和 Rust 1.85+；GeeZipX 当前 workspace 工具链高于该最低要求，但构建阶段仍需要 C++ 编译器。
- 单条目提取当前经过 `zpaq_rs` 的字节缓冲 helper，因此 GeeZipX 暂不对 ZPAQ 的逐条目完全流式提取做过度承诺。

当前不支持：

- `compress` / 创建 ZPAQ 归档
- 归档写入、追加/原地更新、版本选择
- 加密 / 密码访问
- 超出当前只读 MVP 的更广泛 ZPAQ journaling 工作流

```sh
geezipx list backup.zpaq
geezipx test backup.zpq
geezipx decompress backup.zpaq -o out/
```

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

### 基准测试

Criterion 基准测试框架已配置并可用于手动运行：

```sh
# 验证基准测试可编译
cargo bench --no-run -p geezipx-core

# 运行完整基准测试
cargo bench -p geezipx-core
```

> **注意**：基准测试仅作为参考信息（advisory）。GitHub-hosted runner 性能波动大，硬性阈值不可靠。不进一步推进 benchmark 基线或 CI 性能门禁。

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

### 第一阶段（CLI MVP）— 已完成并成熟 ✓

所有核心能力与适用格式的子命令均已实现并验证：

- [x] ZIP / TAR / 7Z / TAR.GZ / TAR.BZ2 / TAR.BR / TAR.LZ4 / TAR.ZST / TAR.XZ / GZIP / BZIP2 / Brotli / LZ4 / ZSTD / XZ / LZMA 读写
- [x] LZH / LHA store-only 读写 MVP（`compress`、`list`、`decompress`、`test`）
- [x] RAR / CAB / ASAR / DEB / ISO / CPIO / ZPAQ 只读支持（`list`、`decompress`、`test`）
- [x] 流式 I/O，内存占用可控
- [x] `indicatif` 进度条
- [x] Ctrl+C 优雅取消
- [x] 自动格式检测（魔数字节 + 扩展名）
- [x] 覆盖保护（`--no-clobber` / `--force`）
- [x] Zip Slip 路径穿越防护
- [x] Shell 补全（5 种 shell）
- [x] `list --json` 机器可读输出
- [x] `test` 归档完整性验证（支持 JSON 输出）
- [x] 400+ 测试（单元 + 集成 + 互操作 + 流式 smoke）
- [x] 三平台 CI（Linux/macOS/Windows）
- [x] cargo-deny 安全审计
- [x] Criterion 基准测试（advisory，无硬门禁）
- [x] crates.io 发布
- [x] 多线程压缩（`-j`/`--jobs` for tar.gz, zstd/tar.zst）
- [x] ZIP AES-256 密码加密
- [x] stdin/stdout 管道（单流 + tar-based 格式）

### 第二阶段（桌面 GUI via Tauri）— 当前开发重心（v0.5.0）

- [x] Tauri v2 项目骨架 + TypeScript/Vite 前端
- [x] Core 引擎桥接（Tauri commands）
- [x] 归档浏览器 + 文件关联（含只读 `.cab` / `.asar` / `.deb` / `.iso` / `.cpio` / `.zpaq` 打开、浏览、提取流程，以及 `.lzh` / `.lha` 的浏览/提取与 store-only 写入流程）
- [x] 选择性提取
- [x] 内联预览（文本 + 十六进制）
- [x] 拖入应用与拖出条目
- [x] 侧边栏导航与最近路径 chips
- [x] 加密归档密码输入（ZIP AES-256、7z、RAR）
- [x] 实时进度显示（速度 + 剩余时间）
- [x] 取消安全的任务执行
- [x] GUI bundle CI 已配置：独立 `gui-windows.yml` 用于 Windows 构建，`release.yml` 用于 `.AppImage`、`.dmg`、`.msi` 产物
- [ ] GUI bundle 的首次 tag release 端到端验证
- [ ] 窗口状态持久化与更多打磨项

详见 [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) 了解详细规划和任务拆解。

### 第三阶段（未来）

- [ ] 平台原生安装渠道（Homebrew、winget、APT）
- **格式扩展** — 分阶段推进，详见 [docs/PRD.md](docs/PRD.md) 第 5.1 节完整目标清单
  - 压缩扩展：7z 高级写入能力（加密/调优）、更完整的 LZH/LHA 兼容（`lh5`/`lh6`/`lh7`、元数据、多卷）、ISO 写入、ZPAQ 写入、ZIPX 高级方法矩阵评估、SFX
  - 解压扩展：WIM
  - 历史/专有格式：ARJ、ACE、ARC、ALZ（通过适配器评估）
  - 容器/衍生格式：JAR、WAR、APK、IPA、XPI（复用 ZIP 引擎）
  - 磁盘镜像：IMG、ISZ、UDF
  - 更多格式由用户需求与社区反馈驱动

### 明确不做（当前阶段）

右键菜单集成、自动更新、云同步、插件系统、分卷压缩、7z 高级写入能力（密码/高级编码器/更深优化待后续阶段）、RAR 创建（受许可限制保持只读）。更多格式扩展详见 [docs/PRD.md](docs/PRD.md) 第 5.1 节及第 6.2 节交付策略。

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
