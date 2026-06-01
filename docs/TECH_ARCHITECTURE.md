# GeeZipX — 技术架构文档

## 1. 架构概览

采用 **Rust Cargo Workspace** 分层架构，核心引擎库与前端（CLI / GUI）完全分离。

```
geezipx/
├── Cargo.toml             # [workspace] 定义 crate 成员
├── crates/
│   ├── core/              # 核心引擎库 — 纯逻辑，流式 I/O
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── archive/       # 各归档格式的读写实现 (zip/tar/tar.gz/gzip)
│   │       ├── detect.rs      # 格式自动检测（魔数 + 扩展名）
│   │       ├── error.rs       # 统一错误类型
│   │       └── io.rs          # 流式读/写/计数/进度封装
│   └── cli/                   # CLI 二进制 — clap + 进度渲染
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── commands/      # compress / decompress / list / completions
│           └── render/        # 进度条
├── docs/
├── scripts/               # 构建/CI/互操作性测试脚本
├── CHANGELOG.md
├── deny.toml
├── LICENSE
├── rustfmt.toml
└── .rust-toolchain.toml
```

> 注：`gui-tauri/` 目录将在 Phase 3 桌面 GUI 阶段创建。

### 分层原则

```
┌─────────────┐  ┌─────────────┐
│  cli (bin)  │  │ gui-tauri   │  ← 前端层：用户交互，不处理核心逻辑
│  clap       │  │ Tauri + FE │
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
| `archive::zip` | ZIP 读写（Store/Deflate） | `zip` |
| `archive::targz` | tar + gzip 组合 | `tar`, `flate2` |
| `archive::tar` | 纯 tar 打包 | `tar` |
| `archive::gzip` | .gz 单文件压缩 | `flate2` |

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
    Zip, Tar, Gzip, TarGz, Xz, Zstd, Unknown,
}

pub fn detect_format(reader: &mut dyn Read) -> Result<ArchiveFormat>;
```

魔数表：

| 格式 | 魔数 (hex) |
|------|-----------|
| ZIP | `50 4B 03 04` |
| ZIP (empty) | `50 4B 05 06` |
| gzip | `1F 8B` |
| tar.gz | 同 gzip (`1F 8B`)，配合 `.tar.gz`/`.tgz` 扩展名 |
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
    fn is_cancelled(&self) -> bool;  // 默认返回 false
}
```

CLI 实现：`indicatif` 渲染 tqdm 风格进度条，可被 `--no-progress` 禁用。
GUI 实现：通过 Tauri `emit` 事件推送进度到前端。

### 2.6 cli/commands — 命令分发

使用 `clap` v4 derive API，四个子命令：

| 子命令 | 主要参数 | 核心流程 |
|--------|---------|---------|
| `compress` | `<inputs...>` `--format` `-o` `--level` `-r` | 收集文件 → 创建 ArchiveWriter → 写入 |
| `decompress` | `<archive>` `-o` `--stdout` `--no-clobber` `--force` | 检测格式 → 创建 ArchiveReader → 解包 |
| `list` | `<archive>` `--json` | 检测格式 → 读取 entries → 表格/JSON 输出 |
| `completions` | `<shell>` | 生成指定 Shell 的自动补全脚本 |

全局参数：`--no-progress`（禁用进度条）、`--verbose`（逐文件日志）。

### 2.7 cli/render — 输出渲染

- 进度条：`indicatif` 的 `ProgressBar` + `ProgressStyle` 自定义模板。
- 列表输出：`comfy-table` 格式化表格。

## 3. 关键依赖（Phase 1）
Phase 1 实际依赖（以 `crates/core/Cargo.toml` 和 `crates/cli/Cargo.toml` 为准）：

#### core 依赖

| Crate | 用途 |
|-------|------|
| `zip` 2.x | ZIP 格式读写（`default-features = false`，仅 `deflate`） |
| `tar` 0.4 | tar 归档包 |
| `flate2` 1.x | gzip/deflate（`rust_backend` — 纯 Rust，无 C 依赖） |
| `thiserror` 2 | 错误类型 derive |
| `log` 0.4 | 日志门面 |

##### dev-dependencies

| Crate | 用途 |
|-------|------|
| `tempfile` 3 | 测试临时目录 |
| `criterion` 0.5 | 基准测试（`html_reports`） |

#### cli 依赖

| Crate | 用途 |
|-------|------|
| `geezipx-core` | workspace 依赖 |
| `clap` v4 | CLI 参数解析（derive API） |
| `clap_complete` 4 | Shell 自动补全生成 |
| `anyhow` 1 | 二进制层错误传播 |
| `indicatif` 0.17 | 终端进度条 |
| `comfy-table` 7 | 表格输出 |
| `serde` + `serde_json` 1 | `--json` 输出序列化 |
| `ctrlc` 3 | Ctrl+C 信号处理 |

##### dev-dependencies

| Crate | 用途 |
|-------|------|
| `assert_cmd` 2 | CLI 二进制集成测试 |
| `predicates` 3 | CLI 输出断言 |
| `tempfile` 3 | 测试临时目录 |

> **与早期草案的变化**：Phase 1 不包含 `xz2`/`zstd`/`crossterm`/`owo-colors`/`env_logger`/`snapbox`。xz/zstd 的 `ArchiveFormat` 枚举变体虽已定义（格式检测占位），但读写实现留待 Phase 2。当前 core 也不使用 feature flags 进行条件编译——zip 和 flate2 为必选依赖。

## 4. 进度与取消机制

### 进度流

Reader(File) → ProgressReader → Decompress → ProgressWriter → File
                    ↑                            ↑
               (计数, callback)             (计数, callback)
            （回调在每次 read/write 后触发；
             CLI 端 `indicatif` 内部负责自己的渲染节流）
```

- `indicatif` 内部每 100ms 刷新一次渲染，不影响流式 I/O 性能。
- 标准输出被检测为 pipe 或无终端时不输出进度条，通过 `--no-progress` 禁用。

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

Matrix:
  - os: ubuntu-latest, macos-latest, windows-latest
  - toolchain: stable (单一版本)

Jobs (串行依赖):
  1. fmt           — cargo fmt --all --check
  2. clippy        — cargo clippy --workspace --all-targets --all-features -- -D warnings (全线，依赖 fmt)
  3. test          — cargo test --workspace --all-features (全线，依赖 fmt)
  4. build         — cargo build --release --workspace (全线，依赖 fmt+clippy+test)
                     → 产物上传至 workflow artifact (`geezipx-{os}-x86_64`)
  5. interop       — bash scripts/check-interop.sh (依赖 clippy+test+build)
  6. bench-compile — cargo bench --no-run -p geezipx-core (依赖 fmt)
```

> 注意：当前 CI 不包含 `cargo deny`（安全审计）或 `cargo publish`，这些将在 Phase 1 MVP 稳定后加入。发布流程目前仅通过手动 tag 触发 workflow artifact 生成。

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
| xz/zstd 的 C 库交叉编译困难（Phase 2 引入） | CI 构建可能复杂 | Phase 1 不包含 xz/zstd 读写，规避该风险 |
| 大文件进度精度受限于预扫描 | 压缩前需要遍历目标文件计算总大小 | Phase 1 接受首次扫描开销；Phase 2 用 `rayon` 并行 |
| Windows 下符号链接/长路径不一致 | 功能受限 | 清晰文档说明限制；渐进式支持 |
| 不同平台 gzip 压缩默认级别差异 | 产生不同二进制 | CI 强制 `--level` 保证一致性；默认值文档说明 |

## 附录：Cargo Workspace 配置

```toml
# /geezipx/Cargo.toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/geezipx/geezipx"
rust-version = "1.96"

[workspace.dependencies]
geezipx-core = { version = "0.1.0", path = "crates/core" }
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
zip = { version = "2", default-features = false, features = ["deflate"] }
tar = "0.4"
flate2 = { version = "1", default-features = false, features = ["rust_backend"] }

[dev-dependencies]
tempfile = "3"
criterion = { version = "0.5", features = ["html_reports"] }

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

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"

[[bin]]
name = "geezipx"
path = "src/main.rs"
```

> 注意：以上 `Cargo.toml` 版本号仅作示例，需以 crates.io 上的实际最新稳定版本为准。
