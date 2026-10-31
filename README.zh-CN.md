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

- **多格式支持** -- 30+ 种归档与流格式类别的压缩、解压、查看与验证（见下方格式表）
- **SFX（自解压）** -- 通过 `--sfx` 将 ZIP 归档包装为自解压可执行文件，支持 Linux、macOS、Windows
- **流式 I/O** -- 大文件处理内存可控
- **实时进度条** -- 在 TTY 中显示速度、预计完成时间、逐文件状态
- **取消安全** -- Ctrl+C 优雅退出，自动清理未完成文件；双击强制退出
- **格式自动检测** -- 魔数字节识别 + 扩展名回退
- **压缩级别** -- gzip/bzip2/tar.gz/tar.bz2/xz/lzma/tar.xz 支持 0-9；brotli/tar.br 支持 0-11；zstd/tar.zst 支持 0-22；LZH 0-9（lh0/lh4-lh7，4+ 使用 lh7）；lz4/tar.lz4 仅接受 0 或省略
- **覆盖控制** -- `--no-clobber` 跳过已有文件，`--force` 强制覆盖
- **Zip Slip 防护** -- 所有归档格式都防护路径穿越攻击
- **JSON 输出** -- `list --json` 机器可读；`test --json` 适合程序化验证
- **Shell 补全** -- bash、zsh、fish、PowerShell、elvish
- **ZIP / 7z AES-256 加密** -- 可用 `--password`、`--password-file`、`--password-stdin` 创建加密归档
- **7z 固实压缩** -- `--solid` 将所有文件合并压缩以提高压缩率
- **7z 方法选择** -- `--7z-method`（lzma2/lzma/bzip2/ppmd/deflate/copy）与 `--dict-size`
- **多卷输出** -- `--split-size` 将归档切分为编号分卷文件
- **加密归档读取** -- `list`、`decompress`、`test` 支持密码保护的 ZIP、7z、RAR
- **AES-256-GCM-SIV 加密容器** -- `.enc` 文件，Argon2id 密钥派生
- **ISZ 压缩 ISO 封装** -- 读写压缩 ISO 镜像
- **IMG / BIN 透传** -- 保持原始磁盘镜像数据不变
- **UU / UUE / XXE 文本编码** -- 编解码历史遗留文本编码文件
- **跨平台** -- Linux、macOS、Windows（三平台 CI）
- **单一二进制** -- `cargo install` 即装即用，无运行时依赖
- **多线程压缩** -- `-j`/`--jobs` 并行压缩（tar.gz 使用 gzp，zstd/tar.zst 使用 zstdmt）

### 格式支持

| 格式 | 扩展名 | 读 | 写 | 说明 |
|--------|-----------|:----:|:-----:|-------------|
| ZIP / ZIPX | `.zip`, `.zipx`, `.jar`, `.war`, `.apk`, `.ipa`, `.xpi` | ✓ | ✓ | AES-256；ZIPX 创建使用 XZ method 95，未覆盖全部专有方法 |
| TAR | `.tar` | ✓ | ✓ | |
| GZIP / TAR.GZ | `.gz`, `.gzip`, `.tar.gz`, `.tgz` | ✓ | ✓ | 级别 0-9；tar.gz 使用 gzp 并行引擎 |
| BZIP2 / TAR.BZ2 | `.bz2`, `.bzip2`, `.tar.bz2`, `.tbz`, `.tbz2` | ✓ | ✓ | 级别 0-9 |
| Brotli / TAR.BR | `.br`, `.brotli`, `.tar.br` | ✓ | ✓ | 级别 0-11 |
| LZ4 / TAR.LZ4 | `.lz4`, `.tar.lz4` | ✓ | ✓ | 仅级别 0（存储） |
| ZSTD / TAR.ZST | `.zst`, `.zstd`, `.tar.zst`, `.tzst` | ✓ | ✓ | 级别 0-22；原生 zstdmt 多线程 |
| XZ / TAR.XZ | `.xz`, `.tar.xz`, `.txz` | ✓ | ✓ | 级别 0-9 |
| LZMA | `.lzma` | ✓ | ✓ | 级别 0-9 |
| LZ / Lzip | `.lz` | ✓ | ✓ | LZMA 容器，含 CRC-32 校验 |
| TLZ | `.tlz`, `.tar.lzma`, `.tar.lz` | ✓ | ✗ | TAR + LZMA Alone 或 Lzip |
| 7Z | `.7z` | ✓ | ✓ | AES-256；solid；LZMA2/LZMA/BZIP2/PPMD/DEFLATE |
| ISO 9660 | `.iso` | ✓ | ✓ | Level 1/Joliet/Rock Ridge；磁盘暂存、有界创建 |
| UDF | `.udf` | ✓ | ✓ | UDF 1.02、CS0 Unicode；磁盘暂存、有界创建 |
| ZPAQ | `.zpaq`, `.zpq` | 部分 | ✓ | 流式分段包可提取；journal 仅列出；模型 1–3，级别 4/5 使用模型 3 |
| LZH / LHA | `.lzh`, `.lha` | ✓ | ✓ | lh0/lh4–lh7 创建，级别 0–9（4+ 使用 lh7）；有界暂存、CRC16 |
| PMA | `.pma` | ✓ | ✗ | PMarc pm0/pm1/pm2；CRC16 |
| CPIO | `.cpio` | ✓ | ✓ | newc/odc |
| ASAR | `.asar` | ✓ | ✓ | Electron 归档；磁盘暂存与 SHA-256 integrity |
| CAB | `.cab` | ✓ | ✓ | 仅单卷；不支持加密 |
| DEB | `.deb` | ✓ | ✓ | data.tar\* payload；不支持加密 |
| WIM / SWM | `.wim`, `.swm` | ✓ | ✓ | 未压缩写入、XPRESS 读取；LZX 兼容性待修复，LZMS 未实现 |
| ISZ | `.isz` | ✓ | ✓ | 有界块创建/读取；直接提取 ISO 内容，暂不支持加密/分卷 |
| SFX | `.exe` | ✓ | 按目标 | 自解压 ZIP；创建需非空目标 stub |
| RAR | `.rar` | ✓ | ✗ | 只读（许可限制）；支持解密 |
| PEA | `.pea` | 部分 | ✗ | 单卷/单流未加密 Store/Deflate PCOMPRESS0–3；三层校验，UTF-8/ASCII 名称 |
| NSIS 安装器 | `.exe`, `.nsis` | 部分 | ✗ | NSIS 3 Unicode Store/Deflate/BZip2/LZMA，solid/non-solid；静态路径、可选 CRC，不执行脚本/插件 |
| 化合物 / MSI | `.msi`, `.msp`, `.mst`, `.cfb`, `.ole` | ✓ | ✗ | 原始 CFB streams，不解析 MSI 数据库或自动展开 CAB |
| AES Crypt | `.aes` | ✓ | ✗ | v0–v3；认证后输出，有界缓冲 |
| AES 加密容器 | `.enc` | ✓ | ✓ | AES-256-GCM-SIV + Argon2id；认证后输出，当前整体缓冲 |
| IMG / IMA | `.img`, `.ima` | 光盘 | 透传 | cooked/raw ISO/UDF 内容提取；不支持 FAT/NTFS/分区盘 |
| BIN | `.bin` | 光盘 | 透传 | ISO/UDF cooked 2048 或单轨 raw 2352 内容；无音轨/CUE |
| UU / UUE | `.uu`, `.uue` | ✓ | ✓ | 文本编码/解码 |
| XXE | `.xxe` | ✓ | ✓ | 文本编码/解码 |
| Z / Unix Compress | `.Z` | ✓ | ✗ | 有界流式 LZW；原格式无校验和 |
| ARJ / ACE / ARC | `.arj`, `.ace`, `.arc` | ✓ | ✗ | unarc-rs 适配器 |
| ALZ | `.alz` | 部分 | ✗ | 单卷 Store/Deflate 与 ZipCrypto；修改版 BZip2、其它方法及分卷未支持 |
| EGG | `.egg` | 部分 | ✗ | 单卷非 solid 未加密 Store/Deflate/BZip2/LZMA；逐块 CRC/长度校验 |

> **ZIPX 说明**：创建使用 XZ method 95；读取共用 ZIP codecs，未覆盖全部 WinZip 专有方法。
> **WIM 写入**：WIM 写入器存储未压缩数据。如需压缩写入请使用 wimlib。
> **格式限制详细说明见代码注释与 docs/PRD.md。**

---

## 项目状态

第一阶段（CLI MVP）已经**全部完成并进入成熟阶段**。所有支持格式均已实现对应子命令。`completions` 子命令也已完成。crates.io 上已发布 `geezipx` 和 `geezipx-core` 包。
第二阶段（桌面 GUI）**是当前开发重心**。Tauri 桌面端和独立的 **GeeZipX Native**（egui/eframe）共用 Rust core。Tauri 的功能和格式范围见 [GUI 指南](docs/GUI_MVP_PLAN.md)，原生端的独立设计、构建和能力范围见 [原生端指南](docs/EGUI_GUI.md)。

| 阶段 | 主题 | 状态 |
|------|------|------|
| 1 | CLI MVP | **已完成** -- crates.io 上已发布 `geezipx`（v0.7.4）与 `geezipx-core` |
| 2 | 桌面 GUI (Tauri) | **开发中** -- v0.7.4 已包含归档浏览器、拖拽、进度显示、选择性提取、文本/十六进制预览、侧边栏导航、设置面板、Toast 通知、Windows 右键菜单 |
| 2 | 桌面 GUI (egui) | **未发布** -- 独立原生应用；使用 `cargo run -p geezipx-egui` 启动，独立三平台构建工作流 |

### 原生 GUI（egui）

```sh
cargo run -p geezipx-egui
cargo run -p geezipx-egui -- /path/to/archive.zip
cargo build -p geezipx-egui --release --locked
```

原生端使用 Rust、egui/eframe 与 OpenGL，提供系统文件选择器和内嵌中文字体，无需 Node.js、Tauri 或 WebView。`cargo build` 默认只构建 core/CLI，桌面端需显式选择包。当前产物为独立可执行文件，安装器和文件关联尚未接入；尚无与 Tauri 的同机性能对照测量。

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

> **发布产物：** [v0.7.4](https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.7.4) 已包含 Linux、macOS、Windows 的 CLI 归档与桌面 GUI 安装包；具体文件和架构以各版本的 Release 页面为准。

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

# 使用 Brotli 压缩
geezipx compress hello.txt -f brotli -o hello.txt.br

# 使用 zstandard 压缩
geezipx compress hello.txt -f zst -o hello.txt.zst

# zstandard 解压到 stdout
geezipx decompress hello.txt.zst --stdout > output.txt

# 多线程 zstd 压缩（4 个 worker）
geezipx compress hello.txt -f zst -o hello.txt.zst -j 4

# 递归压缩目录为 tar.lz4
geezipx compress mydir -r -f tar.lz4 -o mydir.tar.lz4

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
cat raw.tar | geezipx compress --stdin -f tar.bz2 -o archive.tar.bz2
cat raw.tar | geezipx compress --stdin -f tar.br -o archive.tar.br
cat raw.tar | geezipx compress --stdin -f tar.lz4 -o archive.tar.lz4
geezipx decompress archive.tar.gz --stdout | tar tf -
geezipx decompress archive.tar.bz2 --stdout > raw.tar
geezipx decompress archive.tar.br --stdout > raw.tar
geezipx decompress archive.tar.lz4 --stdout > raw.tar
geezipx decompress archive.tar.xz --stdout > raw.tar

# 创建自解压 ZIP（当前平台）
geezipx compress mydir/ -r -o myapp.zip --sfx

# 创建自解压 ZIP 指定目标平台
geezipx compress mydir/ -r -o myapp.exe --sfx --sfx-target windows

# 创建 AES 加密容器
geezipx compress secret.txt -f aes --password mypass -o secret.enc

# 解密 AES 容器
geezipx decompress secret.enc --password mypass

# 创建 ISZ 压缩 ISO
geezipx compress mydir/ -r -f isz -o disk.isz

# UUencode 文件
geezipx compress data.bin -f uu -o data.uu

# 多卷 ZIP（每卷 100 MiB）
geezipx compress bigdir/ -r -f zip -o archive.zip --split-size 100M

# 7z 固实压缩 + LZMA2 64 MB 字典
geezipx compress mydir/ -r -f 7z -o archive.7z --solid --dict-size 64M
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
| `-f`, `--format` | 格式：`zip`、`zipx`、`jar`、`war`、`apk`、`ipa`、`xpi`、`tar`、`tar.gz`、`tgz`、`tar.bz2`、`tbz`、`tbz2`、`tar.br`、`tar.lz4`、`tar.zst`、`tzst`、`tar.xz`、`txz`、`gz`、`gzip`、`bz2`、`bzip2`、`br`、`brotli`、`lz4`、`zst`、`zstd`、`xz`、`lzma`、`lz`、`7z`、`rar`（只读）、`cab`、`asar`、`deb`、`lzh`、`lha`、`iso`、`udf`、`cpio`、`zpaq`、`zpq`、`wim`、`swm`、`uu`、`uue`、`xxe`、`isz`、`aes`、`img`、`ima`、`bin`（省略时从扩展名推断，默认 zip） |
| `-r`, `--recursive` | 递归添加目录 |
| `-L`, `--level` | 压缩级别：0-9（gzip/bzip2/tar.gz/tar.bz2/xz/lzma/tar.xz）；0-11（brotli/tar.br）；0-22（zstd/zst/tar.zst/tzst）；0-9（LZH：0=lh0, 1=lh4, 2=lh5, 3=lh6, 4+=lh7）；lz4/tar.lz4 仅接受 0 或省略 |
| `-j`, `--jobs` | Worker 线程数：1（默认）、0（自动）或 N。tar.gz（gzp）和 zstd/tar.zst（zstdmt）启用多线程 |
| `--password` | 使用 AES-256 加密 ZIP 或 7z 归档。可使用 `--password-file` 或 `--password-stdin` |
| `--7z-method` | 7z 压缩方法：`lzma2`（默认）、`lzma`、`bzip2`、`ppmd`、`deflate`、`copy` |
| `--dict-size` | LZMA2 字典大小（如 `16M`、`64M`、`256M`） |
| `--solid` | 启用 7z 固实压缩（小文件效果更佳） |
| `--no-encrypt-filenames` | 禁用 7z 文件名加密（密码设置时默认加密） |
| `--stdin` | 从 stdin 读取未压缩数据（单流和 tar-based 格式；需配合 `--format`） |
| `--stdout` | 将压缩结果写入 stdout（单流和 tar-based 格式；需配合 `--format`） |
| `--split-size` | 将输出切分为多卷（如 `100M`、`1G`）；`.NNN` 命名 |
| `--sfx` | 将 ZIP 包装为自解压可执行文件。与 `--stdout` 互斥 |
| `--sfx-target` | SFX 目标平台：`linux`、`windows`、`macos`（默认当前主机） |

### `decompress` — 解压归档

```sh
geezipx decompress <归档文件> [选项]
```

自动通过魔数字节检测格式（扩展名作为回退）。

| 选项 | 说明 |
|------|------|
| `-o`, `--output-dir` | 输出目录（默认：当前目录） |
| `--stdout` | 解压到 stdout：gzip/zstd/xz/lzma 输出原文；tar.gz/tar.zst/tar.xz 输出裸 tar 流；zip/tar/7z/rar 等多文件归档会报错 |
| `--stdin` | 从 stdin 读取压缩数据（gzip/bzip2/brotli/lz4/zstd/xz/lzma 和 tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz；以及 lz/isz/aes/img/bin/uu/xxe；需配合 `--format`） |
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
├── crates/
│   ├── core/               # 共享归档引擎、格式检测、安全 I/O 与进度
│   ├── cli/                # 命令、终端输出与集成测试
│   ├── gui-tauri/          # Svelte 5 前端与 Tauri Rust 桥接
│   ├── gui-egui/           # 独立 egui/eframe 原生桌面端
│   ├── sfx-stub/           # 自解压 ZIP 可执行程序
│   └── vendor/             # 本地修补的原生依赖
├── docs/                   # 产品范围、架构与桌面端指南
├── scripts/                # 构建、基准、互操作与发布脚本
└── .github/workflows/      # CI、审计、桌面构建与发布
```

CLI 与两个桌面端单向依赖共享 Rust core。格式逻辑由 core 实现，前端负责交互、参数映射和进度展示；两个桌面端互相独立。

- [产品范围与格式支持](docs/PRD.md)
- [技术架构与实现限制](docs/TECH_ARCHITECTURE.md)
- [Tauri 桌面端指南](docs/GUI_MVP_PLAN.md) / [原生桌面端指南](docs/EGUI_GUI.md)
- [发布验收清单](docs/RELEASE_CHECKLIST.md)

---

## 开发指南

### 前置条件

- Rust stable（通过 [rustup](https://rustup.rs/) 安装）
- 支持 C++17 的 C++ 编译器工具链（默认 RAR / ZPAQ 支持需要；可通过 `--no-default-features` 跳过）

### C++ 构建依赖

RAR、ZPAQ 读取和 ZPAQ 写入需要支持 C++17 的编译器：

```sh
# 默认构建（RAR + ZPAQ 通过 C++ 后端均已包含）
cargo build --release
cargo test --all-features

# 不含 C++ 后端构建
cargo build --release --no-default-features
cargo test --no-default-features
```

> **注意**：`cargo install geezipx` 默认包含 RAR 与 ZPAQ。
> 如果无法满足 C++ 编译器要求，使用 `--no-default-features` 构建。

### SFX 自解压支持

GeeZipX 支持将 ZIP 归档包装为自解压可执行文件。SFX stub 使用对应平台的 ZIP 数据 + 原生可执行头生成自包含的可执行文件。

| 参数值 | 平台 | 输出扩展名 |
|--------|------|-----------|
| `linux` | Linux (x86_64) | 无（可执行文件） |
| `windows` | Windows (x86_64) | `.exe` |
| `macos` | macOS (x86_64) | 无（可执行文件） |

```sh
# 创建当前平台的自解压归档
geezipx compress myapp/ -r -o myapp.zip --sfx

# 指定目标平台
geezipx compress myapp/ -r -o myapp.exe --sfx --sfx-target windows
```

SFX 功能需启用 `sfx` feature（CLI 默认包含），且所选目标必须具有非空的嵌入式 stub；缺少时明确报错。发布工作流在各平台构建对应 stub。本地更新可执行 `cargo build -p geezipx-sfx-stub --release`，将产物复制到 `crates/core/stubs/` 对应目录后再构建 CLI。ZIP 载荷和提取文件通过临时文件流式处理；条目校验失败会保留已有目标。

### 格式详细说明

主要格式限制已在前文格式表中列出。以下为补充说明：

- **WIM**：写入器输出**未压缩** WIM（CompressionType::None），文件内容暂存到临时磁盘并增量计算 SHA-1。支持 XPRESS 读取；LZX 与 wimlib 样本的互操作尚未通过，LZMS 未实现。压缩写入请使用 wimlib 等工具。
- **ASAR / CAB / DEB 写入**：三者均支持写入。ASAR 和 CAB 创建单卷归档；DEB 写入 data.tar\* 载荷及必要元数据。
- **LZH/LHA 写入**：通过 `oxiarc-lzhuf` 支持 lh0（存储）至 lh7 压缩。CLI 级别 0→lh0, 1→lh4, 2→lh5, 3→lh6, 4+→lh7。单文件 >4 GiB 及扩展 header 元数据暂不支持。
- **ISO 写入**：写入 ISO 9660 Level 1 + Joliet。扩展 Rock Ridge/Joliet 创建者元数据在复制时保留。UDF 写入请使用独立的 `udf` 格式。
- **CPIO 写入**：支持 newc/odc。提取时不创建符号链接、设备、FIFO 或套接字。
- **ZPAQ 写入**：磁盘暂存、有界 I/O，跨成员共享 solid 模型；模型 1–3 可用，兼容级别 4/5 映射到模型 3，不等于原生 CLI 的预处理方法 4/5。段 SHA-1 会验证；读取仍整体缓冲并可能逐项重复解码。标准 journal 仅列出逻辑文件，fragment/index 重建尚未接入；可提取 GeeZipX 的流式分段包。
- **PEA / NSIS 读取**：均为有独立语料验证的只读子集，详见上表和[格式审查](docs/FORMAT_PARITY_AUDIT.md)。PEA 不支持加密/HMAC、ECC、分卷和旧 ANSI 名称；NSIS 不执行安装脚本/插件，不恢复运行时生成文件，旧版/ANSI/Park、BCJ、自定义打包 stub 及卸载器重建未覆盖。两个 GUI 需另行接入。
- **ISZ**：围绕 ISO 数据的单流压缩封装。`list` 显示合成条目而非单个文件。
- **AES `.enc`**：AES-256-GCM-SIV 加密 + Argon2id 密钥派生；打开时完成认证，`list` 和 `test` 同样验证完整容器。当前仍整体缓冲；尚未完整实现的流式容器明确拒绝。
- **IMG / BIN**：透传身份复制——数据原样传递，无压缩或转换。
- **UU / UUE / XXE**：遗留文本编码格式。支持解码（list/decompress/test）和编码（compress）。
- **RAR**：许可限制为只读。支持密码解密。

### 构建与测试

```sh
# 构建 core 与 CLI（默认 workspace 成员）
cargo build

# 运行不需要 Tauri 桌面依赖的 workspace 测试
cargo test --workspace --exclude geezipx-gui --all-features

# Release 构建
cargo build --release

# 检查代码格式化
cargo fmt --all --check

# 运行 clippy（严格模式，不含 Tauri）
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings

# 生成文档
cargo doc --no-deps
```

### 基准测试

Criterion 基准测试框架已配置并可用于手动运行：

```sh
cargo bench --no-run -p geezipx-core
cargo bench -p geezipx-core
```

> **注意**：基准测试仅作为参考信息。GitHub-hosted runner 性能波动大，硬性阈值不可靠。

### 互操作性测试

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
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings && \
cargo test --workspace --exclude geezipx-gui --all-features && \
cargo build --release --workspace --exclude geezipx-gui
```

### Release 产物预检

在打 tag 前，可通过 **workflow_dispatch** 手动触发 [Release workflow](.github/workflows/release.yml)（默认 `dry_run: true`）进行三平台构建、打包与校验。工作流任务摘要包含产物完整性检查（是否存在、大小、SHA256）与合并的 `SHA256SUMS` 文件。

---

## 路线图

CLI 已进入成熟阶段。当前重点是 Tauri 桌面端与独立 egui 桌面端、格式适配，以及下一版的原生桌面验收。具体范围和待办分别见 [Tauri 指南](docs/GUI_MVP_PLAN.md)、[原生端指南](docs/EGUI_GUI.md) 和 [发布验收清单](docs/RELEASE_CHECKLIST.md)。

后续分发渠道包括 Homebrew、winget 和 APT；格式扩展由用户需求与社区反馈驱动。

---

## 配置

GeeZipX 遵循 [XDG 基础目录规范](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)。目前所有配置通过命令行选项完成，无需配置文件。

---

## 贡献指南

欢迎贡献代码！请先阅读 [AGENTS.md](AGENTS.md) 了解开发规范和协作指南。

提交 PR 前请确认：

1. 测试通过：`cargo test --workspace --exclude geezipx-gui --all-features`
2. Clippy 零警告：`cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`
3. 代码格式正确：`cargo fmt --all --check`
4. 如果变更影响公共 API 或 CLI 行为，同步更新文档

---

## 许可证

本项目基于 MIT 许可证发布，详见 [LICENSE](LICENSE)。
