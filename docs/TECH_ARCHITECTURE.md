# GeeZipX — 技术架构文档

## 1. 架构概览

GeeZipX 采用 Rust Cargo Workspace 分层架构，核心引擎库与前端（CLI / GUI）解耦。workspace MSRV 为 Rust 1.99，根目录 `rust-toolchain.toml` 和各 CI/发布工作流固定使用 Rust 1.99.0，包含 rustfmt 与 Clippy，保持本地与远端检查一致。当前仓库实际结构如下：

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
├── docs/
├── scripts/
└── .github/workflows/
```

### 分层原则

CLI 与 egui GUI 是两个独立前端，都单向依赖 core。原生 GUI 使用 eframe/Glow 直接绘制。根目录默认 Cargo 成员为 core/CLI，桌面端使用 `-p geezipx-egui` 显式选择。

```text
CLI ──────┐
          ├──→ core（归档、压缩、检测、错误、进度与安全 I/O）
egui GUI ─┘
```

设计目标：

- core 只保留格式逻辑、I/O 包装、错误模型与安全检查；
- CLI 负责参数解析、TTY 进度、stdout/stderr 呈现；
- egui GUI 负责独立的原生交互、共享进度快照与后台任务；不实现压缩编解码；
- RAR 在当前版本中保持只读语义（UnRAR 许可限制）。WIM 支持无压缩写入与 XPRESS 读取；LZX 保留实验实现，但尚未通过 wimlib 样本互操作验证，LZMS 返回不支持错误。ISZ 为块压缩 ISO 包装，使用单流引擎读写。
- ARJ/ACE/ARC/ALZ/Z 创建不在当前目标内；ALZ 通过 MIT `unalz` 索引加自有 `File` 接入单卷 Store/Deflate/ZipCrypto，保持 `ArchiveReader: Send` 与有界提取；Unix compress `.Z` 支持流式 LZW 解码；UU/UUE/XXE 支持流式编码与解码。
- DEB、CAB、ASAR、CPIO（newc/odc）、LZH/LHA（lh0-lh7）、7z、ISO、ZPAQ、UDF 具备读写实现，但各有性能/变体边界；`.enc` 为认证后处理的缓冲容器，`.aes` 另有只读兼容器。IMG/BIN 的创建仍为显式透传，CLI 已能提取 cooked/raw ISO/UDF 光盘文件，不能称为通用磁盘镜像完整读写。

## 2. 模块设计

### 2.1 `core/archive` — 归档格式读写抽象

`crates/core/src/archive/mod.rs` 定义了当前真实的共享 trait 语义：

```rust
pub trait ArchiveReader: Send {
    fn format(&self) -> ArchiveFormat;
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>>;
    fn entries_with_cancel(&mut self, is_cancelled: &dyn Fn() -> bool) -> GeeZipResult<Vec<Entry>>;
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64>;
    fn extract_entries(
        &mut self,
        entries: &[Entry],
        output: &mut EntryOutput<'_>,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<()>;
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
- `entries_with_cancel` 允许 path-based reader 在元数据扫描期间检查取消；CPIO 复用上游 newc/odc 头解析，seek 跳过内容并验证声明区间，TAR/LZH 同样按头和位置扫描。
- `visit_entries` 是 object-safe 顺序访问接口，visitor 返回 `false` 可停止；兼容默认实现仍用临时文件适配旧 reader。提取、预览和 GUI 验证改用 `extract_entries`：已知选区通过 `EntryOutput` 提供写入闭包，顺序 reader 一次访问，兼容 reader 只调用所选条目的 `extract` 并直接写入，避免为了筛选而解码全部条目。CPIO 单次打开选区提取。部分底层库仍有整项缓冲。
- GUI `ArchiveIndex` 共享元数据、源文件身份/长度/修改时间与未加密单流解码缓存；后续提取/预览复用索引，来源变化要求重新打开，验证绕过解码缓存重新读取原文件。加密单流不跨操作缓存明文。
- `add_entry_with_size` 默认委托原有输入 API，TAR 系列覆盖为已知长度流式写入；`ExactSizeReader` 拒绝来源在处理过程中缩短或增长。
- 读取路径的密码接口由各 reader 覆盖；写入加密当前由 ZIP 与 7z writer 分别支持 AES-256。

当前模块职责：

| 模块 | 当前职责 |
|------|----------|
| `archive::zip` | ZIP/ZIPX 读写，Deflate/XZ(method95) 创建与 AES-256；启用多种标准 ZIP codec；统一写入路径分隔符 |
| `archive::compound` | CFB/MSI/OLE 原始 streams 的 Read+Seek 提取；打开前验证64MiB FAT估算预算，目录等仍由库驻内存；不解释 MSI 数据库 |
| `archive::egg` | 单卷非 solid 未加密 EGG 的 Store/Deflate/BZip2/LZMA 流式块与 CRC 校验 |
| `archive::aes` | 旧 `.enc` 缓冲认证容器及 `.aes` AES Crypt v0–v3 双遍认证解密 |
| `archive::disk_image` | cooked 2048/raw 2352 单轨扇区视图，IMG/BIN 复用 ISO/UDF reader |
| `archive::tar` | 纯 TAR 读写 |
| `archive::targz` | TAR.GZ / TGZ 读写；`--jobs > 1` 时启用并行 gzip |
| `archive::tarbz2` | TAR.BZ2 / TBZ / TBZ2 读写 |
| `archive::tarbr` | TAR.BR 读写 |
| `archive::tarlz4` | TAR.LZ4 读写（LZ4 frame） |
| `archive::tarzst` | TAR.ZST / TZST 读写 |
| `archive::tarxz` | TAR.XZ / TXZ 读写，多流解码与可选 liblzma 多线程编码 |
| `archive::tarlz` | TLZ/TAR.LZMA/TAR.LZ 只读，LZMA Alone 或 Lzip 外层 |
| `archive::gzip` | GZIP/GZ 单流压缩/解压 helper |
| `archive::bzip2` | BZIP2/BZ2 单流压缩/解压 helper |
| `archive::brotli` | Brotli/BR 单流压缩/解压 helper |
| `archive::lz4` | LZ4 单流压缩/解压 helper（frame only） |
| `archive::zstd` | ZSTD/ZST 单流压缩/解压 helper |
| `archive::xz` | XZ/LZMA 单流压缩/解压 helper |
| `archive::lz` | LZ（Lzip）压缩使用 `lzma-rust2`；读取由严格成员封装+liblzma LZMA decoder实现，校验头/CRC/长度并设256MiB解码预算 |
| `archive::asar` | ASAR读写；读取缓存有上限的JSON索引并按需seek payload，流式验证可选SHA-256全文件/分块integrity；创建将输入暂存磁盘，按Electron校验语义生成索引并有界顺序输出 |
| `archive::deb` | DEB 读写（ar 容器 + control.tar.gz + data.tar.*） |
| `archive::cab` | CAB 读写（`compress` / `list` / `extract` / `test`；reader 基于 `cab` crate，writer 基于 `cab` crate 实现 MSZIP 压缩写入） |
| `archive::cpio` | CPIO 读写（`compress` / `list` / `extract` / `test`；reader/writer 基于 `cpio-archive` crate，MVP 支持 `newc` / `odc`，不创建宿主 symlink/device/FIFO/socket） |
| `archive::lzh` | LZH/LHA读写；reader基于`delharc`，writer使用维护的`geezipx-lzhuf`（依赖别名`oxiarc-lzhuf`）支持lh0/lh4–lh7；64KiB缓冲暂存原文与压缩结果，连续字典/位流编码，按项输出并保留CRC16/存储回退 |
| `archive::iso` | ISO 读写（`compress` / `list` / `extract` / `test`）；reader 基于 `isomage`（ISO9660/Rock Ridge/Joliet），writer 基于 MIT `geezipx-iso` fork（上游hadris-iso 1.2.1，Level 1/Joliet/Rock Ridge）；`iso::writer`以单个匿名文件暂存输入，再以64KiB缓冲格式化/输出临时镜像 |
| `archive::udf` | UDF reader采用64KiB seek缓冲；`udf::writer`将输入暂存为单个匿名文件，`geezipx-udf` FileSource按区间读取并格式化可seek临时镜像，最后以64KiB缓冲输出；默认UDF1.02、CS0 Unicode、单项创建上限4GiB，目录元数据驻内存 |
| `archive::zpaq` | ZPAQ读写；维护`geezipx-zpaq`（别名`zpaq_rs`）新增named Read+Write接口，原文匿名文件暂存、64KiB I/O、64MiB元数据预算、共享solid模型1–3；兼容级别4/5使用模型3。创建和读取验证段SHA-1，读取仍缓冲整包/整项；流式分段包可提取，真实7.15 journal仅列出逻辑文件，fragment/index内容重建待实现 |
| `archive::nsis` | NSIS 3 Unicode只读；PE节表/后附载荷边界与可选CRC校验，64MiB元数据/字典预算，Store/Deflate/BZip2/LZMA；solid只解码一次到匿名文件，non-solid按项seek，静态路径经共享安全输出；不执行脚本/插件 |
| `archive::seven_zip` | 7z 读写；默认 non-solid LZMA2，支持多方法选择、solid 模式与 AES-256 密码写入 |
| `archive::rar` | RAR 只读（`list` / `extract` / `test`，feature-gated） |
| `archive::wim` | WIM 无压缩写入（文件数据暂存临时磁盘，64 KiB 读入并增量 SHA-1）；XPRESS 读取，LZX 兼容性待修复，LZMS 未实现；默认第一映像；feature-gated |
| `archive::isz` | ISZ 有界块创建/读取；IszImage 提供单块缓存 Read+Seek，CLI 通过 ISO reader 直接列出/提取内容；--stdout 还原 ISO |
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
- PEA使用16字节前缀识别；NSIS通过可seek探测验证PE节表和后附NSIS头，不把任意MZ/EXE视为安装器；ZIP SFX仍单独验证ZIP中央目录。
- ZPAQ 当前不做浅层 magic sniff，避免把可选解析器耦合到前 8 字节的启发式判断中。

> **格式扩展方向**：`ArchiveFormat` 枚举随新增格式逐步扩展。新格式检测优先使用魔数字节；若魔数无定义（如 lzma），依赖扩展名回退或用户显式指定。ZIP 兼容别名（`.jar`/`.war`/`.apk`/`.ipa`/`.xpi`）映射到 `ArchiveFormat::Zip`；`.zipx` 映射到 `ArchiveFormat::Zipx`，创建时选择 XZ，读取共用 ZIP reader。新增格式枚举值需同步更新所有 `match` 分支的完整性检查。完整格式目标见 `docs/PRD.md` 第 5.1 节。

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
| `zip` 9.1.3+ | ZIP/ZIPX 流式读写；Deflate/XZ 写入及 AES-256；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；core 与 SFX stub 共用 workspace 版本 |
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
| `delharc` 0.8 / `geezipx-lzhuf` 0.1 | LZH/LHA/PMA读取及lh0/lh4–lh7写入；支持库派生自oxiarc-lzhuf0.3.5，独立版本+path依赖，持续位流/字典的有界Read编码，需在core发布前独立发布 |
| `isomage` 2.1 | ISO9660 / Rock Ridge / Joliet seek 读取 |
| `geezipx-iso` 0.1.0 | `crates/vendor/hadris-iso`中的MIT fork，上游hadris-iso 1.2.1；保留`hadris_iso`库名/依赖别名，增加FileSource文件区间输入、实文件补零、目录记录不跨扇区及向前SUSP续接布局；独立可发布包，发布core前先发布该依赖，不依赖仅本地生效的patch |
| `geezipx-udf` 0.1.0 | `crates/vendor/hadris-udf`中的MIT fork，上游hadris-udf 1.2.1；保留`hadris_udf`库名/依赖别名，FileSource区间避免文件数据clone，目录与父FID精确布局、多段短AD、CS0和LVID计数/CRC；显式版本+path，发布core前先发布该依赖。旧UTF-8文件名仅按旧producer/空charset标记兼容，标准CS0/8按Latin-1解码 |
| `cpio-archive` 0.10 | CPIO 读写支持（`newc` / `odc` 读写；MPL-2.0，作为未修改 Cargo 依赖使用） |
| `geezipx-zpaq` 0.1 | 派生自zpaq_rs1.0.4（保留Rust库名别名），独立版本/path包；有界named-reader solid创建、段校验和核对、FFI错误/析构保护及GCC LTO修复；optional/default-enabled，需C++17；core发布前先发布支持库 |
| `sevenz-rust2` 0.21 | 7z 读写，支持方法选择、solid 模式和 AES-256 密码写入 |
| `lzxd` 0.2 | WIM LZX 解压（pull 依赖） |
| `xpress-huffman` 0.1 | WIM XPRESS 解压（pull 依赖） |
| `sha1` 0.10 | WIM 资源 SHA-1 增量摘要；仅用于格式完整性 |
| `tempfile` 3 | 7z/CPIO 有界内存后溢写及 CAB/DEB/ISZ/WIM 等数据磁盘暂存；失败或销毁时自动清理 |
| `unegg` 0.2.1 | 仅 EGG 元数据解析；负载用现有编解码库流式读取并校验块 CRC，避开依赖 LZMA 整块缓冲 |
| `cfb` 0.15 | MSI/OLE 化合物原始流读取，Read+Seek 按需复制 mini-stream/普通流；不解释安装数据库 |
| `aescrypt-rs` =0.2.0-rc.18 | AES Crypt v0–v3，只读双遍认证后解密；通用 Read 输入暂存密文，seek API 可直接读取 |
| `base64` 0.22 / `ciborium` 0.2 | AES 容器 armor/CBOR 预检；限制头部大小、嵌套与 KDF 成本 |
| `geezipx-nsis` 0.1 | Apache-2.0支持库，派生自nsis0.6.0并保留lib/依赖别名`nsis`；只复用有界解码头的元数据视图和流式NSIS BZip2块解码，不调用整包载入/提取接口。独立版本+path，发布core前先发布支持库 |
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
- GUI 后台任务把 core 进度更新到 `Arc<Mutex<Progress>>` 最新快照；
- 界面按有限频率唤醒并读取快照，不按读取块积累事件。

### 用户取消

- CLI：`signal.rs` 中的取消令牌与 core 的 `is_cancelled()` 联动；
- core：`extract_all_with_cancel()` 在处理 entry 之前检查取消，并使用 `CancellableWriter` 在写入阶段再次检查；
- GUI：每个 `Job<T>` 持有独立取消状态；界面一次只运行一个后台操作，取消和退出等待任务清理完成。
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
- `doc`：`cargo doc --workspace --no-deps --document-private-items`；
- `clippy`：三平台矩阵执行 `cargo clippy --workspace --all-targets --all-features -- -D warnings`；
- `test`：三平台矩阵执行 `cargo test --workspace --all-features`；macOS 主 CI 使用 `--no-default-features -F rar,wim,sfx`，完整 GUI 后端另由原生工作流验证；
- `build`：三平台矩阵构建 CLI 候选并执行原生 CLI/SFX 验收；
- `interop`、`streaming-smoke`、`bench-compile`、`bench-regression` 为补充质量检查；
- egui 参与主 CI workspace 检查与测试；三平台优化构建、SFX 与 Windows 安装器验证由 `gui-egui.yml` 负责。

### 7.2 其他工作流

- `deny.yml`：`cargo-deny` 安全审计；
- `coverage.yml`：覆盖率观测（informational-only）；
- `bench.yml`：基准基线 / 手动 benchmark；
- `gui-egui.yml`：egui 三平台检查、测试、优化构建、归档打包与 Windows 安装器验证；PR 和手动运行可直接触发，`main` 推送统一由 Release 复用，避免重复构建；
- `release.yml`：CLI + GUI bundles 发布构建工作流，`main` 推送自动发布 beta。

安全依赖基线：ZIP 在 workspace 中统一为 9.1.3+，`unarc-rs` 升至 0.8，消除旧 ZIP 的传递依赖；Zstandard 升至 0.14.3+，同时更新 `zstd-safe` / `zstd-sys`。Tokio 最低版本为 1.53.3，`Cargo.lock` 同步更新 `ctutils` 等传递依赖。Audit 继续执行 `deny.toml` 的安全公告、许可证、来源和依赖规则，`advisories.ignore` 保持为空。

### 7.3 Release / GUI bundle workflow

`.github/workflows/release.yml` 当前已配置：

- CLI 三平台产物：`.tar.gz` / `.zip` + `.sha256`；
- GUI 复用 `gui-egui.yml`：Linux x86_64、macOS arm64 `.tar.gz`，Windows x86_64 `.zip` 与 NSIS 安装器；压缩包附带许可证、字体许可和提交标识；
- `consolidate` job 检查六份 CLI/GUI 压缩包的原始摘要与 Windows 安装器，再生成 `SHA256SUMS`；
- `scripts/release_plan.py` 根据事件、workspace 版本和 run number 区分 main beta、正式 tag 和手动演练；main 生成 `v<version>-beta.<run_number>`，正式 tag 必须匹配 workspace 版本；
- main/beta 使用 Windows 测试签名，正式 `vX.Y.Z` tag 使用生产签名；手动运行只构建产物；
- 全部原生构建和安装器验收通过后，创建 Release 草稿并上传经过校验的文件；beta 额外核对上传文件名、大小、状态与可用摘要后自动公开，设置 `prerelease=true`、`make_latest=false`，不修改最新正式版，不发布 crates.io；
- 正式 `vX.Y.Z` tag 保留 GitHub Release 草稿，人工核对后发布；
- main 新推送取消旧的开发构建，手动演练与正式 tag 运行独立；每个候选版本仍可通过 `workflow_dispatch` 的 `dry_run: true` 构建和校验，再完成原生验收，见 `docs/RELEASE_CHECKLIST.md`。

## 8. 原生 GUI 接入边界

桌面界面通过后台任务与 `engine` 适配层调用 `geezipx-core`。界面负责操作状态、参数映射、进度和错误展示，格式编解码与安全提取留在 core。系统集成由 `gui-egui/src/integration/` 按平台注册当前用户入口，`launch.rs` 保留原生路径编码并解析打开/压缩/解压动作；注册、状态查询和系统命令均在后台执行。Windows 使用 `windows-registry` 写入独立 ProgID、Capabilities 和传统级联菜单；压缩多选经 `IExecuteCommand` / `IObjectWithSelection` 本地 COM server 获取完整路径数组。Windows 11 另由独立的 `geezipx-shell` 轻量 DLL 实现 `IExplorerCommand`、子命令枚举与 COM class factory，通过按用户注册的签名稀疏 MSIX 提供一级菜单，DLL 在包 COM surrogate 中运行；不加载 core、eframe 或格式引擎。菜单状态只读取 Shell 属性与文件名，不探测或解码归档。`gui-egui/src/integration/modern.rs` 在后台调用 WinRT PackageManager 注册/移除身份包。两种菜单共用 `geezipx-shell` 的动作枚举及有界 UTF-16 选区文件，把完整选区交付新 GUI 进程；该进程在 eframe/字体初始化前处理 COM、选区文件与维护参数。Linux 使用 XDG desktop/MIME、KDE ServiceMenu 和 Nautilus/Nemo 脚本；macOS 生成带 `open` handler 的 AppleScript 启动器，通过 LaunchServices 设置格式默认项并注册 Finder Automator 服务。系统入口都回到应用已有 engine 任务，界面层不实现编解码。

## 9. egui/eframe 桌面实现

产品显示名统一为 GeeZipX；已有 `APP_ID`、ProgID、MSIX 身份及 `native-preferences` 保留以兼容既有关联和偏好。Windows `gui-egui/build.rs` 使用仅构建时依赖 `winresource` 0.1.34 将多尺寸 ICO 与版本信息写入 EXE；安装器、Shell 菜单和 DefaultIcon 共用该资源，`test-egui-windows-resources.ps1` 直接验证图标组/图标资源与名称。

`app/settings.rs` 将外观与系统集成分为独立页面，各页使用单层有界滚动并保留固定提示/操作栏。外观即时更新偏好；系统集成分别持有 OS `Snapshot` 与用户待应用选择，刷新与失败只更新状态，成功应用/移除才同步选择。格式按名称/扩展名过滤，整行点击保留辅助功能与键盘响应，搜索不修改隐藏选项。关闭未应用修改及移除注册使用页内确认，后台任务运行时禁用重复操作。

`crates/gui-egui` 使用 egui/eframe 0.33.3 + Glow、rfd 系统文件选择器。所有阻塞文件工作在 `Job<T>` 后台线程执行；结果通过容量 1 的 channel 返回，进度使用 `Arc<Mutex<Progress>>` 最新快照和限频唤醒，不积累读取块事件。取消状态属于每个任务，界面一次只启动一个后台操作；退出时等待取消清理。

`i18n.rs` 维护简体中文/English 文案与 `Language` 枚举；`tr!` 在编译期检查键及两种语言的格式参数。设置页即时切换语言，保存到 `native-preferences`；旧设置缺失语言或语言值未知时使用简体中文。后台 `Control` 持有任务启动时的语言，避免依赖全局可变语言或改变 core。归档文件名、路径和 core/系统错误详情不参与翻译。

原生端通过 `fonts.rs` 的 fontdb 在启动时发现系统字体并缓存目录及默认字面，`app/settings.rs` 提供“跟随系统 / 内置 Noto / 已安装字体”的可搜索选择器，保存在现有 `native-preferences` 中；用户选择只在启动及变更时加载，已卸载/不可读的字体回退至默认字体链。Windows 优先 Segoe UI/微软雅黑、macOS 优先系统无衬线/苹方、Linux 使用 Fontconfig 的通用字体配置。等宽文本保留独立的系统等宽字面；缺失中文字形逐字回退到内嵌 Noto Sans SC 2.004（SIL OFL 1.1）。加载前使用与 egui 相同的 ab_glyph 解析器验证字面，保留 TTC 字面索引，不可读/不可解析的字体自动跳过；字体发现与磁盘读取不进入重绘路径。系统字体仅在本机使用，不随应用重新分发。

主窗口在无系统装饰的透明原生窗口内绘制，普通窗口使用 12 点圆角，最大化/全屏时恢复直角；egui 根背景、标题栏、状态栏和 Modal 遮罩共同保持外角透明，Linux 由桌面合成器完成混合。`window_frame.rs` 负责主题一致的标题栏、边框及窗口控制，通过 eframe `ViewportCommand` 请求原生移动、最小化和最大化，Windows/Linux 使用原生拖动缩放，macOS 使用有最小尺寸约束的位置/尺寸更新；标题栏和边缘交互位于应用 Modal 之上，关闭动作接回复用任务退出确认。菜单、状态栏和工作区使用嵌套 panel，预留标题栏空间。其余界面由 `app/chrome.rs` 的菜单/工具栏、`app/workspace.rs` 的路径/搜索与紧凑文件表格、`app/folders.rs` 的可调整宽度目录树、底部状态/进度组成；创建、解压、任务详情分别在 `app/create.rs`、`app/extract.rs`、`app/tasks.rs` 按需打开 Modal。对话框提交使用独立状态和选择快照，失败后恢复原目标/来源/选择及非密码选项，密码在每次提交后清空。主操作独立于可滚动设置区域；输入框编辑期间不抢占导航和全选快捷键，Esc 不取消后台任务。窗口默认 1000×680、最小 760×480（逻辑像素）。

归档目录索引在后台建立，按名称、大小、压缩后大小和类型的排序结果预计算并共享 `Arc<Row>`；名称搜索只在条件变化时过滤当前目录缓存，绘制使用 29 逻辑像素的可见行虚拟列表。左侧目录树复用缓存目录索引，仅在展开/导航时以显式栈生成展开节点，虚拟绘制并自动展开当前路径的祖先；搜索和排序不改变树结构。子目录表格额外绘制 `..` 文件夹行，独立处理双击返回，不加入选择或解压模型；框选与滚动索引计入该行偏移。点击、Ctrl/Cmd、Shift 和键盘范围选择共享模型；切换目录清空选择，搜索保留选择并报告隐藏项。预览限制为 512 KiB，密码不进入持久化，外观与最多八条最近路径使用 eframe 存储。

原生端 `engine` 只编排 core 的 reader/writer、进度、来源收集和安全输出。压缩通过同目录临时归档提交；解压复用 core `ExtractionDir`，预先拒绝危险名称与输入归档别名覆盖。`engine/formats.rs` 覆盖 core 全部格式构造，RAR/WIM/ZPAQ 默认启用；Unix 工作流关闭 C++ LTO、Windows 使用静态 CRT。`engine/streams.rs` 把单流格式映射为一个条目并委托 core 编解码，先在匿名临时文件完整解码/认证，再提供真实长度与读取流，不实现格式算法。IMG/BIN 通过 core 光盘 reader 浏览 ISO/UDF，ISZ 经 core `IszImage` 接 ISO reader，完整性测试先使用 core `isz_verify` 校验全部外层块，避免仅遍历文件遗漏未访问的损坏块。创建列表包含全部可写格式，单流/镜像只接受单个普通文件；ZIP SFX 使用 core `write_zip_sfx` 在最终提交前生成，`SfxTarget::is_available` 控制缺失 stub 的目标不可选。

原生端 `export::DeferredExport` 按归档/选区快照生成描述和交付根映射，悬停不读取归档或创建目标。Windows `app/native_drag/windows.rs` 在工作线程跟踪释放，通过命中窗口所属的可见 Shell view、UI Automation 行及 Shell item 文件系统属性解析目标；不能可靠解析时使用目录选择器，放回源窗口或 Esc 取消。不再发布虚拟文件内容给 Explorer；结果通过 `DropOutcome::ExtractTo` 回到 `app/interaction.rs`，由普通 `Job` 调用 `DeferredExport::extract_to` → `engine::extract_mapped` → core `extract_entries` 直接落盘，统一进度、跳过、取消与记录。macOS 保留 `NSFilePromiseProvider`，Linux X11 保留 `XdndDrop` 后的 URI 请求及必要暂存；原生 Wayland 暂不支持。Explorer 标签页、桌面、不同视图与 DPI 需要原生验收，编译不等于目标解析已兼容全部窗口。目录双击按稳定条目 ID 配对点击，Windows 使用系统双击间隔；导航快捷键方向键由 Painter 绘制。底部提示不改变列表起点。

`gui-egui.yml` 使用 `ubuntu-24.04`、`windows-latest`、`macos-latest`，覆盖 Linux x86_64、Windows x86_64、macOS arm64 的检查、测试、release 编译与原生程序压缩包；Windows 额外交付 Shell DLL 与签名稀疏 MSIX，使用 `crates/gui-egui/windows/installer.nsi` 打包按用户安装的 NSIS。`configure-egui-windows-signing.ps1` 在编译前固定 Publisher；`build-egui-windows-shell.ps1` 检查 DLL 嵌入身份，调用 MakeAppx/SignTool 打包和验证签名。`scripts/test-egui-windows-integration.ps1` 保留其他应用注册和受保护的 UserChoice；`test-egui-windows-explorer.ps1` 从已注册的包激活 `IExplorerCommand`，验证枚举、适用状态、中文多选 ZIP 与解压。工作流检查 EXE/DLL/MSIX 校验和、许可证、提交标识、快捷方式、原位升级与卸载清理。升级先由已安装 EXE 注销身份包，替换后修复原有设置；卸载先删除身份包再删除 DLL。`release.yml` 复用该工作流并收集产物，正式 tag 要求受信任代码签名凭据，普通 CI/dry-run 使用临时测试签名。使用说明、完整格式范围与已知限制见 [EGUI_GUI.md](EGUI_GUI.md)。

安装器的 `windows/installer-theme.nsh` 复用 `theme.rs` 浅色主题的背景、文字和强调色，并使用从同一应用图标生成的页眉和侧栏图案。菜单选择页在证书页之前；`MenuChoice` 控制是否进入测试证书页，返回并取消菜单时清除证书安装选择。调用 `--repair-shell-integration --menu=0|--menu=1` 只改变菜单并保留已有格式选择，新安装不隐式关联所有格式。目录页通过 NSIS `InstallDirRegKey` 读取当前用户卸载项 `InstallLocation`，并在进入页面时显示已检测到的原安装目录；`/D=` 可显式覆盖，Windows CI 验收无 `/D=` 的自定义目录升级。

安装器的中英 `nsDialogs` 证书页说明自签名测试证书为何影响 Windows 11 一级菜单，以及不安装时可继续使用的归档功能。证书安装默认关闭、卸载移除默认开启；静默安装显式使用 `/MENU=1 /INSTALLCERT=1`，静默卸载用 `/REMOVECERT=0` 保留证书。`windows/manage-test-certificate.ps1` 校验公钥指纹、自签名测试主题和代码签名用途，通过 .NET `X509Store` API 操作证书而不依赖 PowerShell `Cert:` 驱动器；仅对证书操作通过 UAC 提权，并只写 `LocalMachine\TrustedPeople`，提权后重新检查已有信任，避免认领等待期间由其它来源添加的证书；新增指纹记录在当前安装器卸载项的 `InstallerCertificates` 子键中，升级保留，已有信任不认领，按用户选择逐项移除。`scripts/test-egui-windows-certificates.ps1` 在临时 Windows runner 中验收默认行为、证书归属、升级、保留与移除。

## 10. 当前实现限制

| 项目 | 当前边界与后续验证 |
|------|--------------------|
| 原生桌面与安装 | 编译、单元测试和 Wine 注册表测试不能替代 Explorer、NSIS、Finder、Launch Services 实测；按 [发布验收清单](RELEASE_CHECKLIST.md) 记录候选提交与结果 |
| WIM | 无压缩写入与 XPRESS 读取可用；LZX 的独立 wimlib 样本尚未通过，LZMS、分割 SWM 与第一映像以外提取未实现 |
| 内存预算 | ISO/UDF/ISZ、CAB、DEB、CPIO 创建改用有界块/磁盘暂存；ISO/UDF临时空间约输入+镜像，RAM随元数据增长，tmpfs会占系统内存；UDF读取使用64KiB seek缓冲，UU/XXE CLI逐行处理。ASAR创建为有界暂存/顺序输出并设64MiB索引预算，LZH创建为按项暂存/有界连续编码；ZPAQ创建为磁盘暂存/共享solid模型及64MiB元数据预算；旧 `.enc`、部分第三方 reader 仍有整包/整项缓冲；WIM metadata/目录驻内存，不能保证所有格式低于固定上限 |
| UDF支持库配置 | core/CLI使用同步std/read/write，独立27项单测及package通过；额外async/write组合与上游1.2.1同样因生成类型/时间方法冲突失败，待分离共享描述符和I/O绑定；workspace all-features不代表该被排除库的async组合已支持 |
| AES 容器 | `.enc` 支持经验证的非流式容器，拒绝不安全 schema 和超额 KDF；`.aes` 用 AES Crypt v0–v3 双遍认证后解密，非seek输入暂存密文。两者格式不兼容，创建 `.aes` 不会回退生成 `.enc` |
| 新读取预算 | EGG累计索引64MiB；CFB打开前FAT估算64MiB（不含目录等总开销）；Lzip/LZMA Alone/TLZ解码256MiB。超过当前预算明确拒绝，并非所有合法大字典/容器均可读；可配置预算和其它累计元数据计费待补。互操作证据见[专项验收](FORMAT_PARITY_ACCEPTANCE.md) |
| PEA读取 | `archive/pea.rs`及子模块实现单卷/单流未加密Store/块Deflate；Read+Seek、64KiB缓冲、64MiB元数据计费预算，打开时顺序验证后建立索引，提取按对象seek并复核。使用adler2、crc32fast及RustCrypto校验实现，不运行Pea工具；文件名支持UTF-8/ASCII；egui 已桥接该只读子集 |
| 系统集成 | 当前用户格式关联与压缩/解压快捷操作；Windows 默认应用需系统确认，Windows 11 的一级菜单依赖签名身份包，Windows 10 和“显示更多选项”保留传统菜单；Linux 覆盖 KDE 与 Nautilus/Nemo，macOS 通过用户启动器及 Finder 服务；其它文件管理器和原生交互仍需平台验收 |
| 组事务与持久性 | 多文件提取按条目发布，多卷不能整组原子提交；普通失败会清理，不保证强杀、断电或所有文件系统语义下的完整回滚 |
| NSIS读取 | `archive/nsis.rs`及子模块实现现代NSIS 3 Unicode静态文件/目录映射，Store/Deflate/BZip2/LZMA的solid与non-solid；PE范围、可选CRC和精确流结束校验。64KiB I/O、64MiB元数据与LZMA字典预算，solid匿名磁盘暂存；有界编解码内存不随单项长度增长，临时磁盘随solid原文增长。ANSI/旧版/Park、BCJ、自定义打包stub、插件/运行时文件和卸载器重建不在范围；egui 已桥接该只读子集 |
| 格式范围 | ALZ/EGG/PEA/NSIS部分只读；Z/PMA/TLZ/AES Crypt/Compound 已接入；BH/DAA1.0 暂缓。逐项证据、性能债务及后续方案见 [格式审查](FORMAT_PARITY_AUDIT.md)；egui 已桥接当前 core 格式，各格式的 core 子集限制仍适用 |
| 桌面性能与可访问性 | 仍需超大归档、键盘焦点、屏幕阅读器与原生拖放实测 |

Cargo workspace 成员、feature 与依赖版本以根目录及各 crate 的 `Cargo.toml`、`Cargo.lock` 为准；本文不复制完整配置。
