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
│   │       ├── sfx.rs         # 自解压 ZIP 创建（feature-gated）
│   │       └── test.rs        # 完整性验证与测试辅助
│   ├── cli/
│   │   ├── src/
│   │   │   ├── commands/      # compress / decompress / list / test / completions
│   │   │   ├── render/        # 进度条与终端输出
│   │   │   └── signal.rs      # Ctrl+C 取消处理
│   │   └── tests/             # CLI 集成测试
│   ├── gui-egui/             # 独立 egui/eframe 原生桌面端
│   │   ├── assets/           # 内嵌中文字体与许可
│   │   ├── src/
│   │   │   ├── app/          # 三个工作页面与交互状态
│   │   │   ├── browser.rs    # 缓存目录索引、共享行数据与选择
│   │   │   ├── engine/       # core reader/writer 适配与安全 I/O 编排
│   │   │   ├── job.rs        # 后台线程、取消与进度快照
│   │   │   └── theme.rs      # 独立浅/深色主题
│   │   └── tests/            # 原生端实际文件操作回归
│   └── gui-tauri/
│       ├── src/
│       │   ├── App.svelte     # 主窗口布局
│       │   ├── pages/         # 压缩、解压、设置等页面
│       │   ├── stores/        # 归档、任务与设置状态
│       │   ├── bridge.ts      # Tauri API
│       │   ├── main.ts        # 窗口入口
│       │   └── app.css        # 全局样式
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

CLI、Tauri GUI 和 egui GUI 分别是独立前端，都单向依赖 core；两个 GUI 互不依赖。原生 egui 使用 eframe/Glow 直接绘制，不经过 WebView 或 Tauri IPC。根目录默认 Cargo 成员为 core/CLI，桌面端必须显式选择包。

```text
CLI ──────────┐
Tauri GUI ────┼──→ core（归档、压缩、检测、错误、进度与安全 I/O）
egui GUI ─────┘
```

设计目标：

- core 只保留格式逻辑、I/O 包装、错误模型与安全检查；
- CLI 负责参数解析、TTY 进度、stdout/stderr 呈现；
- Tauri GUI 负责图形交互、任务管理、事件桥接；
- egui GUI 负责独立的原生交互、共享进度快照与后台任务；不实现压缩编解码；
- RAR 在当前版本中保持只读语义（UnRAR 许可限制）。WIM 支持无压缩写入与 XPRESS 读取；LZX 保留实验实现，但尚未通过 wimlib 样本互操作验证，LZMS 返回不支持错误。ISZ 为块压缩 ISO 包装，使用单流引擎读写。
- ARJ/ACE/ARC/ALZ/Z 写入经评估永久排除（见 PRD §5.2）；ALZ/Z 仅保留格式识别，尚未实现操作支持；UU/UUE/XXE 已支持编码与解码。
- DEB、CAB、ASAR、CPIO（newc/odc）、LZH/LHA（lh0-lh7）、7z（含高级方法选择）、ISO、ZPAQ、UDF、AES、IMG、BIN 均已实现完整读写。

## 2. 模块设计

### 2.1 `core/archive` — 归档格式读写抽象

`crates/core/src/archive/mod.rs` 定义了当前真实的共享 trait 语义：

```rust
pub trait ArchiveReader: Send {
    fn format(&self) -> ArchiveFormat;
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>>;
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64>;
    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport>;
    fn visit_entries(
        &mut self,
        visitor: &mut dyn FnMut(&Entry, &mut dyn Read) -> GeeZipResult<bool>,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<()>;
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
    fn add_entry_with_size(&mut self, path: &Path, reader: &mut dyn Read, size: u64) -> GeeZipResult<()>;
    fn finish(self: Box<Self>) -> GeeZipResult<u64>;
    fn add_directory(&mut self, _path: &Path) -> GeeZipResult<()>;
}
```

要点：

- 多文件归档格式通过 `ArchiveReader` / `ArchiveWriter` 统一。
- `archive::extraction::ExtractionDir` 用 `cap-std` 的目录句柄限制条目路径，拒绝符号链接和 Windows 重解析点；GUI、默认 reader 和 ZIP 批量解压复用该输出层。
- 文件先流式写入同目录临时文件，读取或取消失败会清理临时文件；成功后才替换目标。禁止覆盖模式在发布文件时再次以原子方式保护已存在的目标。
- `finish(self: Box<Self>) -> GeeZipResult<u64>` 负责结束写入并返回写出的总字节数。
- `extract_all(..., overwrite)` 与 `extract_all_with_cancel(..., overwrite, ...)` 是当前真实批量提取入口。
- `visit_entries` 是兼容现有 reader 的 object-safe 单次遍历接口，visitor 返回 `false` 可停止。ZIP/TAR 系列/7z 提供流式覆盖；默认实现使用临时文件适配旧 reader，部分后端本身仍有内存缓冲。
- `add_entry_with_size` 默认委托原有输入 API，TAR 系列覆盖为已知长度流式写入；`ExactSizeReader` 拒绝来源在处理过程中缩短或增长。
- 读取路径的密码接口由各 reader 覆盖；写入加密当前由 ZIP 与 7z writer 分别支持 AES-256。

当前模块职责：

| 模块 | 当前职责 |
|------|----------|
| `archive::zip` | ZIP 读写，支持 ZIP AES-256 创建与读取；写入时将 Windows 路径分隔符统一为 `/`，目录条目保留单个末尾 `/` |
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
| `archive::lz` | LZ（Lzip）单流压缩/解压 helper（通过 `lzma-rust2` lzip feature） |
| `archive::asar` | ASAR 读写（`compress` / `list` / `extract` / `test`）；reader/writer 基于 `asar` crate） |
| `archive::deb` | DEB 读写（ar 容器 + control.tar.gz + data.tar.*） |
| `archive::cab` | CAB 读写（`compress` / `list` / `extract` / `test`；reader 基于 `cab` crate，writer 基于 `cab` crate 实现 MSZIP 压缩写入） |
| `archive::cpio` | CPIO 读写（`compress` / `list` / `extract` / `test`；reader/writer 基于 `cpio-archive` crate，MVP 支持 `newc` / `odc`，不创建宿主 symlink/device/FIFO/socket） |
| `archive::lzh` | LZH/LHA 读写；reader 基于 `delharc`，writer 使用 `oxiarc-lzhuf` 支持 lh0 存储与 lh4-lh7 压缩，CLI 级别 0-4 映射到这些方法 |
| `archive::iso` | ISO 读写（`compress` / `list` / `extract` / `test`）；reader 基于 `isomage`（ISO9660/Rock Ridge/Joliet），writer 基于 `hadris-iso`（ISO 9660 Level 1） |
| `archive::zpaq` | ZPAQ 读写（`compress` / `list` / `extract` / `test`）；writer 基于 `zpaq_rs::archive_from_entries`，压缩级别 1-5；不含增量 journaling/dedup |
| `archive::seven_zip` | 7z 读写；默认 non-solid LZMA2，支持多方法选择、solid 模式与 AES-256 密码写入 |
| `archive::rar` | RAR 只读（`list` / `extract` / `test`，feature-gated） |
| `archive::wim` | WIM 无压缩写入（文件数据暂存临时磁盘，64 KiB 读入并增量 SHA-1）；XPRESS 读取，LZX 兼容性待修复，LZMS 未实现；默认第一映像；feature-gated |
| `archive::isz` | ISZ — 块压缩 ISO 包装，单流引擎实现（读/写）；不实现 `ArchiveReader`/`ArchiveWriter` trait |
| `sfx` | 自解压 ZIP 创建，`write_zip_sfx` 流式拼接 stub 与 ZIP；只允许非空的已嵌入目标 stub；运行时校验 ZIP/ZIP64 定位并用目录句柄安全提取；feature-gated |
| `volume` | 数字分卷发现、缺卷检测、跨卷读取/查找与分卷写入；CLI 在临时目录暂存完整卷组，发布时拒绝覆盖已有分卷 |

> **注**：上表仅列出当前已实现的格式模块。全部格式评估结果见 `docs/PRD.md` §5.1-5.2，
> 新增格式遵循相同的 `ArchiveReader` / `ArchiveWriter` trait 接口和 feature gate 策略。

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
    Lz,
    TarXz,
    SevenZip,
    Rar,
    Cab,
    Asar,
    Deb,
    Lzh,
    Iso,
    Cpio,
    Zpaq,
    Wim,
    Arj,
    Ace,
    Arc,
    Alz,
    Z,
    Udf,
    Uu,
    Xxe,
    Img,
    Aes,
    Bin,
    Isz,
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
- 普通文件压缩和单流解压通过 `common::PendingOutput` 写入同目录临时文件，校验/收尾成功后才发布；失败与 Ctrl+C 保留已有目标并清理临时文件。路径预检覆盖规范路径、递归输入和 Unix 硬链接。
- `decompress` 汇总中存在条目错误即返回失败；有效空归档返回成功。递归压缩拒绝内部符号链接，避免循环跟随。
- `--password-stdin` 与 `--stdin` 互斥；不支持密码的格式在读取输入或发布输出之前报错。

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
| `zip` 9.1.3+ | ZIP 读写（启用 `deflate`、`aes-crypto`）；core 与 SFX stub 共用 workspace 版本 |
| `tar` 0.4 | TAR 容器读写 |
| `flate2` 1.x | gzip/deflate（纯 Rust backend） |
| `bzip2` 0.6 | bzip2 / tar.bz2 |
| `brotli` 8 | brotli / tar.br |
| `lz4_flex` 0.13 | lz4 frame / tar.lz4 |
| `gzp` 0.11 | tar.gz 并行 gzip 压缩 |
| `xz2` 0.1 | xz / lzma / tar.xz |
| `zstd` 0.14.3+ | zstd / tar.zst，多线程支持 `zstdmt` |
| `cab` 0.6 | CAB 读写支持（单卷 cabinet 浏览、提取、验证、MSZIP 压缩创建） |
| `asar` 0.3 | ASAR 读写支持（Electron 归档，JSON header + 内容写入） |
| `ar` 0.9 | ar 归档读写（DEB 包解析与创建） |
| `delharc` 0.6 / `oxiarc-lzhuf` 0.3 | LZH/LHA 读取及 lh0、lh4-lh7 写入 |
| `isomage` 2.1 | ISO 读取（ISO9660 / Rock Ridge / Joliet 解析与流式读取）；`hadris-iso` 1.1 提供 ISO 9660 Level 1 写入 |
| `cpio-archive` 0.10 | CPIO 读写支持（`newc` / `odc` 读写；MPL-2.0，作为未修改 Cargo 依赖使用） |
| `zpaq_rs` 1.0 | ZPAQ 读写支持（writer 基于 `archive_from_entries`，压缩级别 1-5；optional, default-enabled；需 Rust 1.85+ 与 C++17 编译器） |
| `sevenz-rust2` 0.21 | 7z 读写，支持方法选择、solid 模式和 AES-256 密码写入 |
| `lzxd` 0.2 | WIM LZX 解压（pull 依赖） |
| `xpress-huffman` 0.1 | WIM XPRESS 解压（pull 依赖） |
| `sha1` 0.10 | WIM 资源 SHA-1 增量摘要；仅用于格式完整性 |
| `tempfile` 3 | WIM 文件数据磁盘暂存；失败或销毁时自动清理 |
| `base64` 0.22 / `ciborium` 0.2 | AES 容器 armor/CBOR 预检；限制头部大小、嵌套与 KDF 成本 |
| `unarc-rs` 0.8 | ARJ / ACE / ARC 只读适配；使用已修复安全公告的 ZIP 依赖 |
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
- GUI：`AppState::register_task` 拒绝空白或重复任务 ID，取消仅更新现有令牌，结束时按令牌身份移除，防止旧任务误删新任务；前端收到终态事件后收束 UI。
- I/O 包装用可识别的 `GeeZipError::Cancelled` 作为 `io::Error` 载荷，避免 `Interrupted` 被 `io::copy` / `read_to_end` 自动重试；错误转换保留取消语义，压缩终结和 flush 同样检查取消。

## 5. 跨平台文件系统策略

| 主题 | 当前策略 |
|------|----------|
| 路径安全 | 提取前统一执行 Zip Slip/path traversal 检查 |
| 路径分隔符 | 归档内统一使用 `/`；落盘时交给 `Path` 处理 |
| 覆盖策略 | 通过 `overwrite: bool` / `--no-clobber` / `--force` 控制 |
| Unicode 文件名 | 作为一等场景测试；ZIP 优先 Unicode/UTF-8，对可无损解码且包含汉字的旧名称尝试 GB18030（含 GBK），保留 CP437 回退；统一名称索引并拒绝解码重名，未标代码页的旧名称仍有识别歧义 |
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
- GUI Rust 不在主 CI 的 clippy/test/build 门禁里；`gui-build` 运行前端 `npm test`、TypeScript 检查和生产构建，原生 Windows GUI 测试与打包由独立 workflow 负责。

### 7.2 其他工作流

- `deny.yml`：`cargo-deny` 安全审计；
- `coverage.yml`：覆盖率观测（informational-only）；
- `bench.yml`：基准基线 / 手动 benchmark；
- `gui-windows.yml`：独立 Tauri Windows GUI 手动构建工作流；
- `gui-egui.yml`：独立 egui 三平台检查、测试与可执行文件构建；
- `release.yml`：CLI + GUI bundles 发布构建工作流。

安全依赖基线：ZIP 在 workspace 中统一为 9.1.3+，`unarc-rs` 升至 0.8，消除旧 ZIP 的传递依赖；Zstandard 升至 0.14.3+，同时更新 `zstd-safe` / `zstd-sys`。GUI 的 Tokio 最低版本为 1.53.3，`Cargo.lock` 同步更新 `ctutils` 等传递依赖。Audit 继续执行 `deny.toml` 的安全公告、许可证、来源和依赖规则，`advisories.ignore` 保持为空。

### 7.3 Release / GUI bundle workflow

`.github/workflows/release.yml` 当前已配置：

- CLI 三平台产物：`.tar.gz` / `.zip` + `.sha256`；
- GUI 三平台 bundle：Linux `.AppImage` / `.deb`、macOS `.dmg`、Windows `.exe (NSIS 安装器)`；
- `consolidate` job 会校验所需 artifacts 并生成 `SHA256SUMS`；
- `release` job 在 `v*` tag push 时把 CLI 与 GUI artifacts 上传到 GitHub Release；
- v0.7.4 已实际发布上述三平台 CLI 与 GUI 产物；每个候选版本仍应通过 `workflow_dispatch` 的 `dry_run: true` 重新构建和校验，之后再决定发布，见 `docs/RELEASE_CHECKLIST.md`。

## 8. Tauri GUI 接入方式（Phase 2 当前阶段）

```text
crates/gui-tauri/
├── src/
│   ├── App.svelte
│   ├── QuickExtract.svelte
│   ├── pages/
│   ├── stores/
│   ├── bridge.ts
│   ├── main.ts
│   └── app.css
└── src-tauri/
    ├── src/
    │   ├── commands/
    │   │   ├── app.rs
    │   │   ├── associations.rs
    │   │   ├── cancel.rs
    │   │   ├── compress.rs
    │   │   ├── drag.rs
    │   │   ├── extract.rs
    │   │   ├── extract_entries.rs
    │   │   ├── formats.rs
    │   │   ├── list.rs
    │   │   ├── preview_entry.rs
    │   │   ├── progress.rs
    │   │   ├── shell_menu.rs
    │   │   └── test.rs
    │   ├── lib.rs
    │   └── state.rs
    └── tauri.conf.json
```

当前接入要点：

- GUI Rust 后端是 thin bridge：暴露 Tauri commands，调用 `geezipx-core`；
- `get_formats` 只报告当前 GUI 桥接能力：创建标志使用 `parse_gui_compress_format`，读取标志和 `open_reader` 共用 `reader_factory`。可选 feature 关闭时不会误报支持，回归测试逐一验证所有标记可创建的格式能通过 GUI 读取工厂解出原内容。core/CLI 的完整格式清单独立维护。
- 进度通过 `task:progress` 事件推送。主窗口启动交接采用 `LaunchAction` / `LaunchRequest` 进程内队列：`dispatch_resolved_action` 在检查或创建主窗口前入队，热启动只发送 `launch-requests-pending` 唤醒；前端注册通知与进度监听后调用 `get_launch_requests`，按请求 ID 去重并在处理或显示错误后调用 `acknowledge_launch_request`。快照读取不清空数据，确认仅影响调用窗口的对应请求；查询失败仍处理查询期间已收到的唤醒。队列不跨进程退出持久化。
- `list_archive_stream` — 流式归档条目列表（通过 Tauri IPC Channel 分批推送，500 条/块）；
- 右键“解压缩到当前文件夹”由 `dispatch_resolved_action` 在冷/热启动统一路由到 `extract-here-*` 独立进度窗口，`src/main.ts` 根据窗口 URL 挂载 `QuickExtract.svelte`；`main` 使用 `create: false`，仅普通动作按需创建。启动动作按窗口标签暂存，`get_shell_action` 只消费调用窗口的动作；进度继续用显式任务 UUID 隔离，独立窗口具备密码提示、取消及失败确认。窗口状态插件仅持久化主窗口几何，前端挂载后再显示对应窗口。快捷窗口 capability 同时授予 `core:window:allow-close` 和 `core:window:allow-destroy`：注册 `onCloseRequested` 后，Tauri 在未阻止关闭时会通过 `destroy` 完成销毁；运行中关闭仍先取消本窗口的任务，等待执行结束后关闭。
- 前端由 `src/main.ts` 挂载 `App.svelte` 或 `QuickExtract.svelte`，页面组件位于 `pages/`，归档、任务与设置状态由 `stores/` 管理；
- 归档浏览器每层只显示直接子项；对旧 ZIP 的 `\` 分隔符在目录索引和选择匹配时按 `/` 处理，文件预览/读取继续保留原始条目名称，避免破坏旧包的精确查找。
- 选择性提取、条目预览、拖出归档条目等 GUI 专属交互都建立在 core 的只读/提取能力之上；
- 前端已内置 i18n 支持（`en.json` / `zh-CN.json` 双语），由 `i18n/index.ts` 管理语言切换；
- 偏好设置/默认行为配置已在 v0.7.0 中实现（默认输出目录、覆盖策略等）。
- **Windows 右键菜单运行时管理**（已合入主线，待下一版发布和原生交互验收）— `commands/shell_menu.rs` 通过 `HKCU\Software\Classes` 动态管理四项 Explorer 右键动词。压缩父键注册于 `AllFilesystemObjects`（文件/目录/混合多选均支持），父子均声明 `MultiSelectModel=Player`；提取仍使用 `SystemFileAssociations` + `"%1"` 单归档路径。
  - **COM DelegateExecute 多选**（已合入主线）：压缩动词（`Compress`、`CompressZip`）通过 `DelegateExecute` 委托获得完整 `IShellItemArray`，而非静态 `"%*"` 命令——`%*` 在实机测试中确认传递空参数列表，不可作为支持方案。
  - **LocalServer32 双 CLSID**：两个稳定 GUID 各自注册 `HKCU\Software\Classes\CLSID\{...}\LocalServer32` 指向同一 `geezipx-gui.exe`。Explorer 通过 COM 启动时传入 `-Embedding`，程序跳过 Tauri 进入 STA 消息泵，实例化 `IExecuteCommand` + `IObjectWithSelection` 实现类，接收 `IShellItemArray`，将选中路径写入版本化二进制 action 文件（`.gzsa`, UTF-16LE，限 `%LOCALAPPDATA%\GeeZipX\ShellActions\` 目录内，上限 1 MiB / 10 000 条 / 32 767 码元），然后以 `--shell-action-file` 参数启动正常 GUI 进程消费。cold/warm 共用 `resolve_shell_action` 解析路径。action 文件读取后 best-effort 删除。
  - **提取动词**继续保持 `SystemFileAssociations` + `"%1"` 静态命令，不涉及 COM，无多选需求。
  - **Sentinel 保护**保持：`HKCU\Software\Classes\GeeZipX\ShellMenu\Configured=1`，NSIS fresh-install/uninstall 维护注册状态，upgrade 检测 sentinel 跳过默认注册保留用户选择。
  - **验证状态**：协议单元测试以及原生 Windows GUI 测试/NSIS 构建已有成功记录；下一候选版仍需按提交 SHA 重跑构建，并完成 Explorer、安装升级和卸载实测。禁止 cfg stub 冒充 Windows 通过 CI。
  - 非 Windows 平台返回 `supported: false`。Win11 限制：传统菜单项显示于"显示更多选项"(Shift+F10)。
- **Windows 文件关联**（待下一版发布）— `commands/associations.rs` 的 Windows 平台模块同样迁移至 `windows-registry`，通过程序化 API 管理 ProgID、`OpenWithProgids`、`Capabilities` 与 `RegisteredApplications`，不再依赖外部 `reg.exe` 进程。

| 风险 | 影响 | 当前缓解 |
|------|------|----------|
| 下一候选版尚未完成三平台发布演练 | 新变更仍可能暴露平台编译或打包问题 | 按候选提交运行 release dry-run，并核对 artifacts、SHA256 与原生验收记录 |
| 大文件压缩进度依赖预扫描总量 | 首次开始任务前会有扫描延迟 | UI 上明确扫描阶段 |
| Windows 长路径/符号链接差异 | 个别归档场景行为与 Unix 不完全一致 | 持续测试 + 文档限制说明 |
| RAR / ARJ / ACE / ARC 只读，ALZ / Z 尚未实现操作支持 | GUI 不能创建这些格式，且桥接范围小于 core/CLI | 以 PRD 和各 GUI 格式表分别说明范围 |
| WIM / ISZ writer 为标准模式外的实现 | WIM 为无压缩写入、ISZ 为单流引擎（非 ArchiveWriter trait） | 在文档中明确能力边界 |
| LZH/LHA writer 为缓冲写入（lh0-lh7 压缩均支持） | 不支持加密、多卷、extended header；单个 entry payload 会缓冲 | 在产品文档中明确限制范围 |
| Windows COM DelegateExecute 右键菜单仍需完整原生验收 | 已通过原生编译和 NSIS 构建，Explorer 选区传递、升级与卸载仍需实机确认 | 按 `docs/RELEASE_CHECKLIST.md` 记录候选提交与逐项验收结果 |
## 9. 独立 egui 原生 GUI

`crates/gui-egui` 使用 egui/eframe 0.33.3 + Glow、rfd 系统文件选择器，独立于 Tauri/Svelte。所有阻塞文件工作在 `Job<T>` 后台线程执行；结果通过容量 1 的 channel 返回，进度使用 `Arc<Mutex<Progress>>` 最新快照和限频唤醒，不积累读取块事件。取消状态属于每个任务，界面一次只启动一个后台操作；退出时等待取消清理。

原生端通过 `fonts.rs` 的 fontdb 在启动时发现系统字体并缓存目录及默认字面，`app/settings.rs` 提供“跟随系统 / 内置 Noto / 已安装字体”的可搜索选择器，保存在现有 `native-preferences` 中；用户选择只在启动及变更时加载，已卸载/不可读的字体回退至默认字体链。Windows 优先 Segoe UI/微软雅黑、macOS 优先系统无衬线/苹方、Linux 使用 Fontconfig 的通用字体配置。等宽文本保留独立的系统等宽字面；缺失中文字形逐字回退到内嵌 Noto Sans SC 2.004（SIL OFL 1.1）。加载前使用与 egui 相同的 ab_glyph 解析器验证字面，保留 TTC 字面索引，不可读/不可解析的字体自动跳过；字体发现与磁盘读取不进入重绘路径。系统字体仅在本机使用，不随应用重新分发。

主窗口由 `app/chrome.rs` 的菜单/工具栏、`app/workspace.rs` 的路径/搜索与紧凑文件表格、`app/folders.rs` 的可调整宽度目录树、底部状态/进度组成；创建、解压、任务详情分别在 `app/create.rs`、`app/extract.rs`、`app/tasks.rs` 按需打开 Modal。对话框提交使用独立状态和选择快照，失败后恢复原目标/来源/选择及非密码选项，密码在每次提交后清空。主操作独立于可滚动设置区域；输入框编辑期间不抢占导航和全选快捷键，Esc 不取消后台任务。窗口默认 1000×680、最小 760×480（逻辑像素）。

归档目录索引在后台建立，按名称、大小、压缩后大小和类型的排序结果预计算并共享 `Arc<Row>`；名称搜索只在条件变化时过滤当前目录缓存，绘制使用 29 逻辑像素的可见行虚拟列表。左侧目录树复用缓存目录索引，仅在展开/导航时以显式栈生成展开节点，虚拟绘制并自动展开当前路径的祖先；搜索和排序不改变树结构。子目录表格额外绘制 `..` 文件夹行，独立处理双击返回，不加入选择或解压模型；框选与滚动索引计入该行偏移。点击、Ctrl/Cmd、Shift 和键盘范围选择共享模型；切换目录清空选择，搜索保留选择并报告隐藏项。预览限制为 512 KiB，密码不进入持久化，外观与最多八条最近路径使用 eframe 存储。

原生端 `engine` 只编排 core 的 reader/writer、进度、来源收集和安全输出。压缩通过同目录临时归档提交；解压复用 core `ExtractionDir`，预先拒绝危险名称与输入归档别名覆盖。格式清单以 `engine/formats.rs` 为准，RAR 默认启用，ZPAQ 可选且默认关闭以避开原生 C++ LTO 兼容性问题。

原生端 `export::DeferredExport` 按归档/选区快照生成描述信息，不读取归档或创建暂存文件；只有系统确认放置、接收方请求内容后，才通过 `engine::extract` 准备一次并缓存结果。`app/interaction.rs` 管理内容坐标框选和拖放生命周期；`app/native_drag/windows.rs` 实现 OLE `IDataObject`、Unicode `FILEGROUPDESCRIPTORW`、`FILECONTENTS` 和 `IStream`，仅在 `IDropSource::QueryContinueDrag` 确认释放后提供内容；macOS 后端使用 `NSFilePromiseProvider` 与串行后台队列；Linux X11 后端在 `XdndDrop` 后响应 URI 请求。原生 Wayland 暂不支持。复制语义和暂存生命周期允许接收方异步读取。目录双击按稳定条目 ID 配对点击，Windows 使用系统双击间隔；导航快捷键方向键由 Painter 绘制。底部提示不改变列表起点。

独立 `gui-egui.yml` 覆盖 Linux x86_64、Windows x86_64、macOS arm64 的检查、测试、release 编译与裸可执行文件 artifacts；Windows 使用 `crates/gui-egui/windows/installer.nsi` 额外打包按用户安装的 NSIS，安装后验证 EXE 校验和、许可证、提交标识、快捷方式和卸载清理。独立安装目录与卸载注册项允许和 Tauri 并存；当前不接入正式 Release、系统右键菜单或文件关联。使用说明、完整格式范围与已知限制见 [EGUI_GUI.md](EGUI_GUI.md)。

## 10. 当前实现限制

| 项目 | 当前边界与后续验证 |
|------|--------------------|
| 原生桌面与安装 | 编译、单元测试和 Wine 注册表测试不能替代 Explorer、NSIS、Finder、Launch Services 实测；按 [发布验收清单](RELEASE_CHECKLIST.md) 记录候选提交与结果 |
| WIM | 无压缩写入与 XPRESS 读取可用；LZX 的独立 wimlib 样本尚未通过，LZMS、分割 SWM 与第一映像以外提取未实现 |
| 内存预算 | AES、ISZ 写入、UDF 映像读写、UU/XXE 和部分第三方适配器仍整体缓冲；WIM 文件内容暂存磁盘，但 metadata/目录信息驻内存；不能保证所有格式低于固定内存上限 |
| AES 容器 | 支持经过验证的非流式容器；拒绝存在认证漏洞的 streaming schema 和超出 KDF 策略的外部容器；流式 AES 仍需认证 framing 设计 |
| 系统关联恢复 | 多键注册尚无完整回滚；Linux 旧 handler 备份仅会话内有效，desktop ID 查找未完整遵循 XDG；安装关联表仍宽于 GUI 实际能力 |
| 组事务与持久性 | 多文件提取按条目发布，多卷不能整组原子提交；普通失败会清理，不保证强杀、断电或所有文件系统语义下的完整回滚 |
| 格式范围 | ALZ/Z 仅识别格式，未实现操作支持；两个 GUI 的适配清单分别维护，均不等同于 core/CLI 全部能力 |
| 桌面性能与可访问性 | 尚无 Tauri/egui 同机耗时和 RSS 对照基准；仍需超大归档、键盘焦点与原生拖放实测 |

Cargo workspace 成员、feature 与依赖版本以根目录及各 crate 的 `Cargo.toml`、`Cargo.lock` 为准；本文不复制完整配置。
