# GeeZipX — 技术架构文档

## 1. 架构概览

GeeZipX 采用 Rust Cargo Workspace 分层架构，核心引擎库与前端（CLI / GUI）解耦，当前仓库实际结构如下：

```text
geezipx/
├── Cargo.toml
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── archive/       # 各归档/压缩格式实现
│   │       ├── config.rs      # CompressOptions 等参数模型
│   │       ├── detect.rs      # 魔数字节 + 扩展名检测
│   │       ├── error.rs       # GeeZipError
│   │       ├── io.rs          # ProgressReader / ProgressWriter / ProgressEvent
│   │       └── test.rs        # 完整性验证与测试辅助
│   ├── cli/
│   │   ├── src/
│   │   │   ├── commands/      # compress / decompress / list / test / completions
│   │   │   ├── render/        # 进度条与终端输出
│   │   │   └── signal.rs      # Ctrl+C 取消处理
│   │   └── tests/             # CLI 集成测试
│   └── gui-tauri/
│       ├── src/
│       │   ├── bridge.ts
│       │   ├── main.ts
│       │   └── style.css
│       └── src-tauri/
│           ├── src/
│           │   ├── commands/
│           │   ├── lib.rs
│           │   └── state.rs
│           └── tauri.conf.json
├── docs/
├── scripts/
└── .github/workflows/
```

### 分层原则

```text
┌─────────────┐  ┌─────────────────┐
│  cli (bin)  │  │  gui-tauri       │  ← 前端层：终端 UI / Tauri GUI
└──────┬──────┘  └────────┬─────────┘
       │                  │
       └────────┬─────────┘
                │ depends on
        ┌───────▼──────────┐
        │  core (lib)       │  ← 核心引擎：归档/压缩/检测/错误/进度
        │  ─ 无终端依赖     │
        │  ─ 无 Tauri 依赖  │
        │  ─ 可被多前端复用 │
        └──────────────────┘
```

设计目标：

- core 只保留格式逻辑、I/O 包装、错误模型与安全检查；
- CLI 负责参数解析、TTY 进度、stdout/stderr 呈现；
- Tauri GUI 负责图形交互、任务管理、事件桥接；
- RAR / CAB / ASAR / DEB / ISO / CPIO / ZPAQ 在当前版本中保持只读语义（`list` / `decompress` / `test`）；LZH/LHA 已支持 store-only 写入 MVP；7z 已支持基础读写与 AES-256 密码写入。

## 2. 模块设计

### 2.1 `core/archive` — 归档格式读写抽象

`crates/core/src/archive/mod.rs` 定义了当前真实的共享 trait 语义：

```rust
pub trait ArchiveReader: Send {
    fn format(&self) -> ArchiveFormat;
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>>;
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64>;
    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport>;
    fn extract_all_with_cancel(
        &mut self,
        dest: &Path,
        overwrite: bool,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<ExtractReport>;
    fn set_password(&mut self, _password: &str) -> GeeZipResult<()>;
}

pub trait ArchiveWriter: Send {
    fn format(&self) -> ArchiveFormat;
    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()>;
    fn finish(self: Box<Self>) -> GeeZipResult<u64>;
    fn add_directory(&mut self, _path: &Path) -> GeeZipResult<()>;
}
```

要点：

- 多文件归档格式通过 `ArchiveReader` / `ArchiveWriter` 统一。
- `finish(self: Box<Self>) -> GeeZipResult<u64>` 负责结束写入并返回写出的总字节数。
- `extract_all(..., overwrite)` 与 `extract_all_with_cancel(..., overwrite, ...)` 是当前真实批量提取入口。
- 读取路径的密码接口由各 reader 覆盖；写入加密当前由 ZIP 与 7z writer 分别支持 AES-256。

当前模块职责：

| 模块 | 当前职责 |
|------|----------|
| `archive::zip` | ZIP 读写，支持 ZIP AES-256 创建与读取 |
| `archive::tar` | 纯 TAR 读写 |
| `archive::targz` | TAR.GZ / TGZ 读写；`--jobs > 1` 时启用并行 gzip |
| `archive::tarbz2` | TAR.BZ2 / TBZ / TBZ2 读写 |
| `archive::tarbr` | TAR.BR 读写 |
| `archive::tarlz4` | TAR.LZ4 读写（LZ4 frame） |
| `archive::tarzst` | TAR.ZST / TZST 读写 |
| `archive::tarxz` | TAR.XZ / TXZ 读写 |
| `archive::gzip` | GZIP/GZ 单流压缩/解压 helper |
| `archive::bzip2` | BZIP2/BZ2 单流压缩/解压 helper |
| `archive::brotli` | Brotli/BR 单流压缩/解压 helper |
| `archive::lz4` | LZ4 单流压缩/解压 helper（frame only） |
| `archive::zstd` | ZSTD/ZST 单流压缩/解压 helper |
| `archive::xz` | XZ/LZMA 单流压缩/解压 helper |
| `archive::asar` | ASAR 只读（`list` / `extract` / `test`） |
| `archive::deb` | DEB 只读（`data.tar*` payload 视图） |
| `archive::cab` | CAB 只读（`list` / `extract` / `test`；path-based 读取，当前面向单卷 cabinet） |
| `archive::cpio` | CPIO 只读（`list` / `extract` / `test`；path-based 读取，MVP 支持 `newc` / `odc`，不创建宿主 symlink/device/FIFO/socket） |
| `archive::lzh` | LZH/LHA 读写（`compress` / `list` / `extract` / `test`）；reader 基于 `delharc`，writer 为项目内实现（store-only level-0：文件 `-lh0-`，目录 `-lhd-`）|
| `archive::iso` | ISO 只读（`list` / `extract` / `test`，`isomage` 解析 ISO9660/Rock Ridge/Joliet） |
| `archive::zpaq` | ZPAQ 只读（`list` / `extract` / `test`，`zpaq_rs` 提供列表/条目读取；单条目提取当前可能经字节缓冲） |
| `archive::seven_zip` | 7z 读写（`list` / `extract` / `test` / `compress`）；当前 writer 为基础 MVP（默认 non-solid LZMA2，支持 AES-256 密码写入） |
| `archive::rar` | RAR 只读（`list` / `extract` / `test`，feature-gated） |

> **注**：上表仅列出当前已实现的格式模块。项目长期规划支持更多格式（详见 `docs/PRD.md` 第 5.1 节），
> 新增格式遵循相同的 `ArchiveReader` / `ArchiveWriter` trait 接口和 feature gate 策略，归档模块数量随阶段逐步扩展。

### 2.2 `core/io` — 流式进度与取消包装

进度相关能力集中在 `crates/core/src/io.rs` 中实现，并不存在独立的 progress 子模块：

```rust
pub enum Phase {
    Reading,
    Writing,
    Hashing,
}

pub struct ProgressEvent {
    pub current: u64,
    pub total: Option<u64>,
    pub phase: Phase,
}

pub trait ProgressCallback: Send {
    fn update(&mut self, _event: ProgressEvent) {}
    fn is_cancelled(&self) -> bool { false }
}

pub struct ProgressReader<R: Read> {
    inner: R,
    bytes_read: u64,
    total: Option<u64>,
    callback: Option<Box<dyn ProgressCallback>>,
}

pub struct ProgressWriter<W: Write> {
    inner: W,
    bytes_written: u64,
    total: Option<u64>,
    callback: Option<Box<dyn ProgressCallback>>,
}
```

实际行为：

- `ProgressReader` / `ProgressWriter` 在每次成功 `read` / `write` 后发出 `ProgressEvent`；
- 在每次 I/O 之前先调用 `is_cancelled()`；
- 无回调时只有一次 `Option` 分支检查；
- GUI 层在此基础上封装更丰富的任务级进度 payload。

### 2.3 `core/detect` — 格式检测

`crates/core/src/detect.rs` 的当前接口是：

```rust
pub enum ArchiveFormat {
    Zip,
    Tar,
    Gzip,
    Bzip2,
    Brotli,
    Lz4,
    TarGz,
    TarBz2,
    TarBr,
    TarLz4,
    Xz,
    Zstd,
    TarZst,
    Lzma,
    TarXz,
    SevenZip,
    Rar,
    Asar,
    Deb,
    Cab,
    Lzh,
    Iso,
    Unknown,
}

pub fn detect_format(data: &[u8]) -> Option<ArchiveFormat>;
pub fn detect_from_extension(path: &Path) -> Option<ArchiveFormat>;
pub fn read_magic_bytes<R: Read>(reader: &mut R) -> io::Result<Vec<u8>>;
```

检测策略：

- ZIP / gzip / bzip2 / lz4 frame / zstd / xz / 7z / RAR / CAB 优先用魔数字节；
- ASAR / DEB / LZH / LHA / ISO / CPIO / ZPAQ 以及 `tar`、`tar.gz`、`tar.bz2`、`tar.br`、`tar.lz4`、`tar.zst`、`tar.xz` 依赖扩展名回退或显式格式；CAB 还支持 `MSCF` magic，CPIO 刻意不做文件级 magic sniff。
- `read_magic_bytes()` 仅读取前 `MAGIC_DETECT_SIZE` 字节，供调用方自行决定后续缓存与回放策略。
- ZPAQ 当前不做浅层 magic sniff，避免把可选解析器耦合到前 8 字节的启发式判断中。

> **格式扩展方向**：`ArchiveFormat` 枚举随新增格式逐步扩展。新格式检测优先使用魔数字节；若魔数无定义（如 lzma），依赖扩展名回退或用户显式指定。ZIP 兼容别名（`.zipx`/`.jar`/`.war`/`.apk`/`.ipa`/`.xpi`）统一映射到 `ArchiveFormat::Zip`。新增格式枚举值需同步更新所有 `match` 分支的完整性检查。完整格式目标见 `docs/PRD.md` 第 5.1 节。

### 2.4 `core/error` — 统一错误模型

当前核心错误类型仍统一为 `GeeZipError`，主要覆盖：

- `Io { source, context }`
- `Format { message, format }`
- `UnsupportedFormat(Vec<u8>)`
- `Cancelled`
- `Crypto { message }`
- `PathTraversal { entry, target }`
- `ClobberDenied { path }`

设计要求：

- 错误值可跨线程传递（`Send + Sync`）；
- 错误消息包含上下文；
- 路径穿越错误保持保守拒绝策略，不提供当前 CLI 中不存在的“绕过开关”。

### 2.5 `cli/commands` — 命令分发

CLI 当前子命令为：

| 子命令 | 主要参数 | 核心流程 |
|--------|----------|----------|
| `compress` | `<inputs...>` `--format` `-o` `-L/--level` `-j/--jobs` `-r` | 选择目标格式 → 多文件归档走 `ArchiveWriter`，单流格式走对应 helper |
| `decompress` | `<archive>` `-o` `--stdout` `--no-clobber` `--force` | 检测格式 → 打开 reader / decoder → 提取到目录或 stdout |
| `list` | `<archive>` `--json` | 检测格式 → 读取 `entries()` → 表格/JSON 输出 |
| `test` | `<archive>` `--json` | 检测格式 → 读取全部 entry 验证完整性 → 结果输出 |
| `completions` | `<shell>` | 生成指定 shell 的补全脚本 |

补充说明：

- 全局 `--no-progress` 用于关闭 TTY 进度条；
- `--verbose` 输出逐文件日志；
- ZIP AES-256 创建仅在 `compress` + ZIP 路径可用；
- ZIP 与 7z 当前都支持 AES-256 密码创建；7z / RAR 的读取路径（`list` / `decompress` / `test`）同样支持密码输入。

### 2.6 `cli/render` — 输出渲染

- 进度条：`indicatif`；
- 列表渲染：`comfy-table`；
- JSON：`serde` + `serde_json`；
- Ctrl+C：`ctrlc` + `signal.rs` 中的取消令牌。

## 3. 关键依赖

当前依赖以 `crates/core/Cargo.toml` 和 `crates/cli/Cargo.toml` 为准：

### core 依赖

| Crate | 用途 |
|------|------|
| `zip` 2.x | ZIP 读写（启用 `deflate`、`aes-crypto`） |
| `tar` 0.4 | TAR 容器读写 |
| `flate2` 1.x | gzip/deflate（纯 Rust backend） |
| `bzip2` 0.6 | bzip2 / tar.bz2 |
| `brotli` 8 | brotli / tar.br |
| `lz4_flex` 0.11 | lz4 frame / tar.lz4 |
| `gzp` 0.11 | tar.gz 并行 gzip 压缩 |
| `xz2` 0.1 | xz / lzma / tar.xz |
| `zstd` 0.13 | zstd / tar.zst，多线程支持 `zstdmt` |
| `delharc` 0.6 | LZH/LHA 只读 reader 支持（writer 为项目内实现，store-only level-0） |
| `isomage` 2.1 | ISO 只读支持（ISO9660 / Rock Ridge / Joliet 解析与流式读取） |
| `cpio-archive` 0.10 | CPIO 只读支持（`newc` / `odc` 读取；MPL-2.0，作为未修改 Cargo 依赖使用） |
| `zpaq_rs` 1.0 | ZPAQ 只读支持（optional, default-enabled；需 Rust 1.85+ 与 C++17 编译器） |
| `sevenz-rust2` 0.21 | 7z 读写支持（当前 writer 走默认 non-solid LZMA2，可选 AES-256 密码写入） |
| `unrar` 0.5.8 | RAR 只读支持（optional, default-enabled） |
| `thiserror` 2 | 错误定义 |
| `log` 0.4 | 日志门面 |

### cli 依赖

| Crate | 用途 |
|------|------|
| `geezipx-core` | 核心引擎 workspace 依赖 |
| `clap` v4 | 参数解析 |
| `clap_complete` 4 | Shell 补全 |
| `anyhow` 1 | 边界层错误传播 |
| `indicatif` 0.17 | TTY 进度条 |
| `comfy-table` 7 | 表格渲染 |
| `serde` / `serde_json` | JSON 输出 |
| `ctrlc` 3 | Ctrl+C 信号处理 |
| `glob` 0.3 | 输入模式展开 |

## 4. 进度与取消机制

### 进度流

```text
Reader/File → ProgressReader → Decoder/ArchiveReader → ProgressWriter → Output/File
                 │                    │                     │
                 └──── update() ──────┴──── update() ───────┘
```

当前行为：

- CLI 在 TTY 下默认显示进度条；`--no-progress` 可禁用；
- 非 TTY / pipe 输出默认不渲染 ANSI 进度条；
- GUI 后端将 core 事件转换成 `task:progress` 事件；
- GUI 事件节流由 `TaskProgressEmitter` 负责（时间与字节步长双阈值）。

### 用户取消

- CLI：`signal.rs` 中的取消令牌与 core 的 `is_cancelled()` 联动；
- core：`extract_all_with_cancel()` 在处理 entry 之前检查取消，并使用 `CancellableWriter` 在写入阶段再次检查；
- GUI：`cancel_task` 把对应 `Arc<AtomicBool>` 标记为取消，前端收到终态事件后收束 UI。

## 5. 跨平台文件系统策略

| 主题 | 当前策略 |
|------|----------|
| 路径安全 | 提取前统一执行 Zip Slip/path traversal 检查 |
| 路径分隔符 | 归档内统一使用 `/`；落盘时交给 `Path` 处理 |
| 覆盖策略 | 通过 `overwrite: bool` / `--no-clobber` / `--force` 控制 |
| Unicode 文件名 | 作为一等场景测试，ZIP 同时兼顾 CP437 兼容 |
| 长路径 / 符号链接 | 采取保守策略并在文档中明确限制；当前 CLI 未暴露 `--follow-symlinks` 或 `--win-longpaths` |

## 6. 测试策略

| 层级 | 当前内容 |
|------|----------|
| core 单元测试 | 路径安全、时间戳、进度包装、格式检测、错误映射 |
| CLI 集成测试 | 压缩/解压/列表/完整性验证、JSON 输出、密码与覆盖策略 |
| 互操作测试 | `scripts/check-interop.sh` 与系统工具交叉验证 |
| 流式 smoke 测试 | 大文件/长耗时路径通过单独测试入口执行 |
| benchmark | Criterion + advisory regression check |

## 7. CI/CD（GitHub Actions）

### 7.1 主 CI：core / CLI 质量门禁

主 CI 位于 `.github/workflows/ci.yml`，当前事实：

- 触发：`main` push、`v*` tag push、`pull_request`、`workflow_dispatch`；
- `fmt`：`cargo fmt --all --check`；
- `doc`：`cargo doc --workspace --exclude geezipx-gui --no-deps --document-private-items`；
- `clippy`：三平台矩阵执行 `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`；
- `test`：三平台矩阵执行 `cargo test --workspace --exclude geezipx-gui --all-features`；
- `build`：三平台矩阵执行 `cargo build --release --workspace --exclude geezipx-gui`；
- `interop`、`streaming-smoke`、`bench-compile`、`bench-regression` 为补充质量检查；
- GUI 不在主 CI 的 clippy/test/build 门禁里，GUI 构建由独立 workflow 负责。

### 7.2 其他工作流

- `deny.yml`：`cargo-deny` 安全审计；
- `coverage.yml`：覆盖率观测（informational-only）；
- `bench.yml`：基准基线 / 手动 benchmark；
- `gui-windows.yml`：独立 Windows GUI 手动构建工作流；
- `release.yml`：CLI + GUI bundles 发布构建工作流。

### 7.3 Release / GUI bundle workflow

`.github/workflows/release.yml` 当前已配置：

- CLI 三平台产物：`.tar.gz` / `.zip` + `.sha256`；
- GUI 三平台 bundle：Linux `.AppImage`、macOS `.dmg`、Windows `.msi`；
- `consolidate` job 会校验所需 artifacts 并生成 `SHA256SUMS`；
- `release` job 在 `v*` tag push 时把 CLI 与 GUI artifacts 上传到 GitHub Release；
- 当前状态统一表述为“已配置，待首个真实 tag release 实战验证”。

## 8. Tauri GUI 接入方式（Phase 2 当前阶段）

```text
crates/gui-tauri/
├── src/
│   ├── bridge.ts
│   ├── main.ts
│   └── style.css
└── src-tauri/
    ├── src/
    │   ├── commands/
    │   │   ├── app.rs
    │   │   ├── cancel.rs
    │   │   ├── compress.rs
    │   │   ├── drag.rs
    │   │   ├── extract.rs
    │   │   ├── extract_entries.rs
    │   │   ├── formats.rs
    │   │   ├── list.rs
    │   │   ├── preview_entry.rs
    │   │   ├── progress.rs
    │   │   └── test.rs
    │   ├── lib.rs
    │   └── state.rs
    └── tauri.conf.json
```

当前接入要点：

- GUI Rust 后端是 thin bridge：暴露 Tauri commands，调用 `geezipx-core`；
- 进度通过 `task:progress` 事件推送，文件关联/单实例通过 `opened-archives` 事件通知前端；
- 前端当前以 `src/main.ts` 为主，包含最近路径 chips、归档浏览器、任务状态管理；
- 选择性提取、条目预览、拖出归档条目等 GUI 专属交互都建立在 core 的只读/提取能力之上；
- 目前尚无完整设置系统或 i18n 框架，这些仍属于后续规划。

| 风险 | 影响 | 当前缓解 |
|------|------|----------|
| GUI bundle 发布尚未经历真实 tag release 演练 | 发布路径可能存在流程性缺口 | 保守表述为“已配置，待验证” |
| 大文件压缩进度依赖预扫描总量 | 首次开始任务前会有扫描延迟 | UI 上明确扫描阶段 |
| Windows 长路径/符号链接差异 | 个别归档场景行为与 Unix 不完全一致 | 持续测试 + 文档限制说明 |
| RAR / CAB / ASAR / DEB / ISO / CPIO / ZPAQ 仍为只读 | GUI 不能创建这些格式 | 在产品文档中明确范围 |
| LZH/LHA writer 为 store-only level-0 缓冲写入 | 不支持 lh5/lh6/lh7 压缩、加密、多卷、extended header；单个 entry payload 会缓冲 | 在产品文档中明确限制范围 |
## 附录：Cargo Workspace 配置

```toml
# /geezipx/Cargo.toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/cli", "crates/gui-tauri/src-tauri"]

[workspace.package]
version = "0.5.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/GEELINX-LTD/geezipx"
rust-version = "1.96"

[workspace.dependencies]
geezipx-core = { version = "0.5.0", path = "crates/core", default-features = false }
```

```toml
# /geezipx/crates/core/Cargo.toml
[package]
name = "geezipx-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Compression/decompression core engine for GeeZipX"
readme = "../../README.md"
keywords = ["compression", "decompression", "archive", "zip", "gzip"]
categories = ["compression"]

[dependencies]
thiserror = "2"
log = "0.4"
zip = { version = "2", default-features = false, features = ["deflate", "aes-crypto"] }
tar = "0.4"
flate2 = { version = "1", default-features = false, features = ["rust_backend"] }
xz2 = { version = "0.1", features = ["static"] }
zstd = { version = "0.13", features = ["zstdmt"] }
sevenz-rust2 = { version = "0.21", default-features = false, features = ["aes256", "bzip2", "ppmd", "deflate"] }
# Optional: RAR read-only (feature-gated, requires C++ compiler)
unrar = { version = "0.5.8", optional = true }
# Optional: ZPAQ read-only (feature-gated, requires Rust 1.85+ and a C++17 compiler)
zpaq_rs = { version = "1.0", optional = true }

[features]
rar = ["dep:unrar"]
zpaq = ["dep:zpaq_rs"]
default = ["rar", "zpaq"]

[dev-dependencies]
tempfile = "3"
criterion = { version = "0.5", features = ["html_reports"] }
sevenz-rust2 = { version = "0.21", default-features = false, features = ["compress", "util", "aes256", "bzip2", "ppmd", "deflate"] }

[lib]
name = "geezipx_core"
path = "src/lib.rs"

[[bench]]
name = "gzip_throughput"
harness = false

[[bench]]
name = "archive_throughput"
harness = false
```

```toml
# /geezipx/crates/cli/Cargo.toml
[package]
name = "geezipx"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "GeeZipX CLI — high-performance compression/decompression tool"
readme = "../../README.md"
keywords = ["compression", "decompression", "archive", "cli", "zip"]
categories = ["command-line-utilities", "compression"]

[dependencies]
geezipx-core.workspace = true
anyhow = "1"
clap = { version = "4", features = ["derive"] }
clap_complete = "4"
comfy-table = "7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
indicatif = "0.17"
ctrlc = "3"
glob = "0.3"

[features]
rar = ["geezipx-core/rar"]
zpaq = ["geezipx-core/zpaq"]
default = ["rar", "zpaq"]

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
sevenz-rust2 = { version = "0.21", default-features = false, features = ["compress", "util", "aes256", "bzip2", "ppmd", "deflate"] }

[[bin]]
name = "geezipx"
path = "src/main.rs"
```

> 注意：以上 `Cargo.toml` 版本号仅作示例，需以 crates.io 上的实际最新稳定版本为准。
