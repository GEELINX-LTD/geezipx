# GeeZipX — 技术架构文档

## 1. 架构概览

采用 **Rust Cargo Workspace** 分层架构，核心引擎库与前端（CLI / GUI）完全分离。

```
geezipx/
├── Cargo.toml             # [workspace] 定义 crate 成员
├── core/                  # 核心引擎库 — 纯逻辑，无 I/O 绑定
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── archive/       # 各归档格式的读写实现
│       ├── io/            # 流式读/写/计数封装
│       ├── detect/        # 格式自动检测
│       ├── progress/      # 进度回调 trait 与指标
│       └── error/         # 统一错误类型
├── cli/                   # CLI 二进制 — clap + 进度渲染
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── commands/      # compress / decompress / list
│       └── render/        # 进度条、彩色输出
├── gui-tauri/             # Tauri 桌面应用 (Phase 3)
│   ├── Cargo.toml
│   ├── src-tauri/         # Rust 后端（很薄，调用 core）
│   └── src/               # 前端 (Vue/Svelte)
├── tests/                 # 集成测试（归档格式互操作）
│   ├── fixtures/          # 测试用小型归档文件
│   └── compress-decompress.rs
├── benches/               # 基准测试
│   └── throughput.rs
└── scripts/               # 构建/CI/发布脚本
```

### 分层原则

```
┌─────────────┐  ┌─────────────┐
│  cli (bin)  │  │ gui-tauri   │  ← 前端层：用户交互，不处理核心逻辑
│  clap + crossterm │  │ Tauri + FE │
└──────┬──────┘  └──────┬──────┘
       │                │
       └───────┬────────┘
               │ 依赖
       ┌───────▼────────┐
       │  core (lib)     │  ← 核心引擎层：所有压缩/解压/格式逻辑
       │  ─ 纯数据流     │
       │  ─ 无 I/O 假设  │
       │  ─ 无终端依赖   │
       └────────────────┘
```

> **为什么这样分？** core 库可以被 CLI 和 GUI 同时依赖，确保行为一致；core 不依赖终端特性，可以方便地在 Tauri 命令或 WebAssembly 中复用。

## 2. 模块设计

### 2.1 core/archive — 归档格式读写

每个格式一个独立模块，通过统一 trait `ArchiveReader` / `ArchiveWriter` 暴露。

```rust
// 伪代码示意 trait 定义
pub trait ArchiveReader: Send {
    fn format(&self) -> ArchiveFormat;
    fn entries(&mut self) -> Result<Vec<Entry>>;
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> Result<u64>;
    fn extract_all(&mut self, dest: &Path) -> Result<ExtractReport>;
}

pub trait ArchiveWriter: Send {
    fn format(&self) -> ArchiveFormat;
    fn add_entry(&mut self, path: &Path, reader: &mut dyn Read) -> Result<()>;
    fn finish(self: Box<Self>, writer: &mut dyn Write) -> Result<u64>;
}
```

| 模块 | 负责 | 依赖 crate |
|------|------|-----------|
| `archive::zip` | ZIP 读写（Store/Deflate/Deflate64） | `zip` |
| `archive::targz` | tar + gzip 组合 | `tar`, `flate2` |
| `archive::tar` | 纯 tar 打包 | `tar` |
| `archive::gzip` | .gz 单文件压缩 | `flate2` |
| `archive::xztar` | tar.xz 读取 | `xz2` (liblzma) |
| `archive::zsttar` | tar.zst 读写 | `zstd` (zstd-sys) |

### 2.2 core/io — 流式接口

关键抽象：`ProgressReader` 和 `ProgressWriter`，包裹任意 `Read + Write`，计数并调用进度回调。

```rust
pub struct ProgressReader<R: Read> {
    inner: R,
    total: Option<u64>,
    callback: Box<dyn Fn(ProgressEvent) + Send>,
}

pub struct ProgressEvent {
    pub current: u64,
    pub total: Option<u64>,
    pub phase: Phase,  // Reading | Writing | Hashing
}
```

流式管线示例：

```
[File] → ProgressReader ──→ [Decompressor] ──→ ProgressWriter → [File]
         ↑                    ↑                    ↑
    (计数+回调)          (flate2::read::DeflateDecoder)  (计数+回调)
```

对于 `tar.gz`，管线为四段：

```
[.tar.gz] → [GzDecoder] → [TarArchive] → [entry-by-entry] → [FileWriter]
               ↑               ↑               ↑              ↑
          (流式解压)      (tar 解包)      (逐个 entry)    (带进度写)
```

### 2.3 core/detect — 格式自动检测

基于文件魔数字节的匹配引擎，轻量无外部依赖。

```rust
pub enum ArchiveFormat {
    Zip, Tar, Gzip, Xz, Zstd, Raw, Unknown(Vec<u8>),
}

pub fn detect_format(reader: &mut dyn Read) -> Result<ArchiveFormat>;
```

魔数表：

| 格式 | 魔数 (hex) |
|------|-----------|
| ZIP | `50 4B 03 04` |
| ZIP (empty) | `50 4B 05 06` |
| gzip | `1F 8B` |
| zstd | `28 B5 2F FD` |
| xz | `FD 37 7A 58 5A 00` |
| tar | 无魔数，fallback 到 `.tar` 扩展名 |

### 2.4 core/error — 统一错误模型

单一错误类型 `GeeZipError`，全库通用：

```rust
pub enum GeeZipError {
    // I/O 错误，携带上下文
    Io { source: io::Error, context: String },
    // 格式错误（头损坏、CRC 不匹配等）
    Format { message: String, format: ArchiveFormat },
    // 不支持格式
    UnsupportedFormat(Vec<u8>),  // 包含前 8 字节魔数
    // 用户取消
    Cancelled,
    // 密码错误 / 加密相关
    Crypto { message: String },
    // 路径逃逸（Zip Slip 防护）
    PathTraversal { entry: String, target: String },
    // 覆盖保护
    ClobberDenied { path: String },
}

impl Error for GeeZipError { /* ... */ }
```

设计原则：
- 所有错误都做到 `Send + Sync`。
- 每条错误信息应包含：**出错位置 + 原因 + 建议**。
- 示例：`error: cannot extract 'foo/../../etc/passwd': path traversal detected, use --unsafe to bypass`

### 2.5 core/progress — 进度回调

定义 trait，cli 和 gui 各自实现：

```rust
pub trait ProgressCallback: Send {
    fn update(&mut self, event: ProgressEvent);
    fn is_cancelled(&self) -> bool;
    fn set_label(&mut self, label: String);
}
```

CLI 实现：`crossterm` + `indicatif` 渲染 tqdm 风格进度条。
GUI 实现：通过 Tauri `emit` 事件推送进度到前端。

### 2.6 cli/commands — 命令分发

使用 `clap` v4 derive API，三个子命令：

| 子命令 | 主要参数 | 核心流程 |
|--------|---------|---------|
| `compress` | `<inputs...>` `--format` `-o` `--level` `-r` `--progress` | 收集文件 → 创建 ArchiveWriter → 写入 |
| `decompress` | `<archive>` `-o` `--stdout` `--no-clobber` `--force` `--progress` | 检测格式 → 创建 ArchiveReader → 解包 |
| `list` | `<archive>` | 检测格式 → 读取 entries → 表格输出 |

### 2.7 cli/render — 输出渲染

- 进度条：`indicatif` 的 `ProgressBar` + `ProgressStyle` 自定义模板。
- 列表输出：`comfy-table` 格式化表格。
- 颜色/样式：`owo-colors` 或 `colored`，仅在 tty 且未禁用时启用。

## 3. 关键依赖（Phase 1）

| Crate | 用途 | 理由 |
|-------|------|------|
| `clap` v4 | CLI 参数解析 | 行业标准，derive API，自动补全 |
| `zip` 2.x | ZIP 格式读写 | Rust 生态最成熟的 ZIP 库 |
| `tar` 0.4 | tar 归档包 | 与 `flate2`/`xz2`/`zstd` 组合使用 |
| `flate2` 1.x | gzip/deflate | Rust 原生实现，支持 `miniz_oxide`（无 C 依赖） |
| `xz2` | xz 解压 | 绑定 liblzma，可选 feature |
| `zstd` | zstd 压缩 | Facebook 标准，可选 feature |
| `indicatif` | 进度条 | 功能丰富的终端进度条 |
| `crossterm` | 终端控制 | 跨平台 tty/尺寸检测 |
| `comfy-table` | 表格输出 | 简洁的表格格式化 |
| `serde` + `serde_json` | 序列化 | `--json` 输出模式 |
| `snapbox` / `assert_cmd` | 集成测试 | 测试 CLI 二进制行为 |
| `criterion` | 基准测试 | Rust 标准基准框架 |

Feature flags 设计：

```toml
[features]
default = ["zip", "gz", "xz", "zstd"]
zip = ["zip_crate"]
gz = ["flate2"]
xz = ["xz2"]        # 依赖 liblzma 系统库
zstd = ["zstd"]     # 依赖 zstd 系统库
static-xz = ["xz2/static"]
static-zstd = ["zstd/static"]
```

用户可以通过 `cargo install geezipx --no-default-features --features zip,gz` 选择裁剪，避免安装系统库。

## 4. 进度与取消机制

### 进度流

```
Reader(File) → ReadStream(Reader) → Decompress → WriteStream(Writer) → File
                    ↑                              ↑
               (计数, emit)                   (计数, emit)
                    │
               ┌────┴─────┐
               │ Ticker (250ms) │ ← 节流回调，避免每字节触发
               └──────────┘
```

- 每 250ms 计算速度（基于滑动窗口）并发出进度事件。
- 标准输出被检测到是 pipe 时，不输出进度条，改为 `--verbose` 简单日志。

### 用户取消

- `ProgressCallback::is_cancelled()` 方法，默认每读取 64KB 检查一次。
- CLI 下：支持 Ctrl+C（`SIGINT` 处理），收到信号后设置取消标志。
- 取消后：已完成 entry 保留，当前进行中的 entry 回滚，输出报告。

## 5. 跨平台文件系统差异处理

| 差异 | 处理策略 |
|------|---------|
| 路径分隔符 | 内部统一使用 `/`，写入时转换为平台分隔符 `Path::join` |
| 文件权限 | 仅 Linux/macOS 保存执行位；Windows 忽略，GUI 阶段加 ACL 映射 |
| 符号链接 | 默认跳过，`--follow-symlinks` 跟踪；Windows 需启用 SeCreateSymbolicLinkPrivilege |
| 长路径 (Windows) | 默认开启 `\\?\` 前缀；可选 `--win-longpaths` |
| 文件名非法字符 | Windows 替换 `\ / : * ? " < > |` 为 `_`，其他平台保留 |
| 时间戳 & 时区 | 统一使用 UTC 存储，恢复时使用平台系统时间 |
| 字符编码 | 归档内文件名 UTF-8；CP437 ZIP 兼容转换 |

## 6. 测试策略

| 层级 | 工具 | 内容 |
|------|------|------|
| 单元测试 | 内联 `#[cfg(test)]` | 每个模块的基础逻辑（魔数检测、路径安全、错误转换） |
| 集成测试 | `/tests/*.rs` | 真实文件压缩→解压→对比 hash；CLI 命令调用 |
| 格式互操作测试 | 脚本 + 原生工具 | 用 Info-ZIP / GNU tar 创建归档，GeeZipX 解压；反之 |
| 基准测试 | `/benches/*.rs` | `criterion` 吞吐量、内存峰值、启动时间 |
| 模糊测试 | `cargo-fuzz`（Phase 2） | 格式鲁棒性，确保恶意文件不会 panic |

## 7. CI/CD（GitHub Actions）

```
Matrix:
  - os: ubuntu-latest, macos-latest, windows-latest
  - rust: stable, msrv (1.80)

流程:
  1. cargo check + clippy (全线)
  2. cargo test (全线)
  3. cargo build --release (全线)
  4. 集成测试 (对比 hash)
  5. cargo deny check advisories (安全审计)
  6. 二进制上传至 workflow artifact

Release:
  - Tag semver 触发 publish
  - cargo publish crates.io
  - GitHub Release + artifact attach
  - 后续：Homebrew tap / winget PR
```

## 8. Tauri 后续接入方式（Phase 3 占位）

当进入桌面 GUI 阶段时：

```
gui-tauri/
├── Cargo.toml               # [package] name = "geezipx-gui"
├── src-tauri/
│   ├── Cargo.toml           # 依赖 core, tauri, tauri-build
│   ├── src/
│   │   └── main.rs          # Tauri 命令调用 core 库
│   │       └── #[tauri::command] fn compress(...) -> Result<...>
│   └── tauri.conf.json
└── src/                     # 前端 SPA (Vue 3 / Svelte 5)
    ├── App.vue
    ├── components/
    │   ├── FileBrowser.vue
    │   ├── ProgressPanel.vue
    │   └── FormatSelector.vue
    └── stores/
        └── task.ts          # 压缩任务状态管理
```

关键点：
- GUI 的 Rust 后端非常薄：仅仅把 `core` 库的方法暴露为 Tauri 命令。
- 进度通过 `tauri::Window::emit` 推送到前端，前端用 WebSocket/EventSource 风格监听。
- 取消通过 Tauri 事件反向传递到 core 的 `is_cancelled()`。
- 右键菜单、文件关联、自动启动等平台集成在 Tauri 配置中声明。

## 9. 关键技术风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| `zip` crate 对 Deflate64 支持不完整 | 部分 ZIP 无法解压 | Phase 2 回退到系统 `unzip`；社区 PR |
| liblzma / zstd 系统库交叉编译困难 | CI 构建易失败 | 提供 `static-*` feature；MUSL 目标 |
| 大文件进度精度受限于预扫描 | 压缩前需要遍历目标文件计算总大小 | Phase 1 接受首次扫描开销；Phase 2 用 `rayon` 并行 |
| Windows 下符号链接/长路径不一致 | 功能受限 | 清晰文档说明限制；渐进式支持 |
| 不同平台 gzip 压缩默认级别差异 | 产生不同二进制 | CI 强制 `--level` 保证一致性；默认值文档说明 |

## 附录：Cargo Workspace 配置建议

```toml
# /geezipx/Cargo.toml
[workspace]
resolver = "2"
members = ["core", "cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
authors = ["GeeZipX Contributors"]
```

```toml
# /geezipx/core/Cargo.toml
[package]
name = "geezipx-core"
version.workspace = true
edition.workspace = true

[dependencies]
zip = { package = "zip", version = "2", optional = true }
tar = "0.4"
flate2 = { version = "1", optional = true, default-features = false, features = ["rust_backend"] }
xz2 = { version = "0.1", optional = true }
zstd = { version = "0.13", optional = true, default-features = false }
thiserror = "2"
log = "0.4"

[features]
default = ["zip", "gz"]
zip = ["zip_crate"]
gz = ["flate2"]
xz = ["xz2"]
zstd = ["zstd"]
```

```toml
# /geezipx/cli/Cargo.toml
[package]
name = "geezipx"
version.workspace = true
edition.workspace = true

[dependencies]
geezipx-core = { path = "../core" }
clap = { version = "4", features = ["derive"] }
indicatif = "0.17"
crossterm = "0.28"
comfy-table = "7"
owo-colors = "4"
anyhow = "1"
log = "0.4"
env_logger = "0.11"
```

> 注意：以上 `Cargo.toml` 版本号仅作示例，需以 crates.io 上的实际最新稳定版本为准。
