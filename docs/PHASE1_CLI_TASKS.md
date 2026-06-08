# GeeZipX Phase 1 - CLI MVP 任务拆分

> 历史周期估计：**10-12 周**（单人全职开发，已完成阶段记录）。  
> 里程碑结构：4 个里程碑，每个里程碑对应可发布的增量。  
> **当前状态：** 🚀 **Phase 1 已全部结束。** M1、M2、M3、M4 全部完成。项目已全面转向 Phase 2（桌面 GUI via Tauri）开发。
> **Phase 2（桌面 GUI via Tauri）是当前开发重心。** 详见 `docs/GUI_MVP_PLAN.md` 了解当前规划和任务拆解。Phase 1 CLI 增强特性（加密 ZIP、7z/RAR 只读、多线程压缩、stdin/stdout 管道）均已实现归档，不再作为独立任务列表。

---

## 里程碑总览

| 里程碑 | 主题 | 周期 | 产出 | 状态 |
|--------|------|------|------|------|
| M1 | 项目骨架 + 核心引擎库 | 第 1-4 周 | `geezipx-core` lib crate，zip/tar/gz 基础读写 | **已完成** |
| M2 | CLI 基本命令 | 第 5-7 周 | `geezipx` binary，三个子命令可用 | **已完成** |
| M3 | 流式/进度/兼容性打磨 | 第 8-10 周 | 进度条、管道、格式检测、跨平台测试 | **已完成** |
| M4 | CI/测试/发布 | 第 11-12 周 | CI 全线通过、crates.io 发布、覆盖率追踪、GitHub Release workflow | **已完成**（核心交付全部完成） |

---

## M1：项目骨架 + 核心引擎库（第 1-4 周）

> **状态：已完成**（提交 `329c773`）。所有 M1 任务已实现。

### 目标
建立 Cargo Workspace，完成 `geezipx-core` 库的架构落地，实现 ZIP、tar、tar.gz 和 gzip 的基础读写能力。

### M1-1：Workspace 初始化 ✅
- **任务**：创建 Cargo Workspace，建立 `crates/core/` 和 `crates/cli/` 目录骨架。
- **实际文件**（路径以 `crates/` 为前缀）：
  - `/Cargo.toml` — workspace 定义，resolver = "2"，members = ["crates/core", "crates/cli"]
  - `/crates/core/Cargo.toml` — lib crate `geezipx-core`，依赖 `thiserror`、`log`、`zip`、`tar`、`flate2`
  - `/crates/cli/Cargo.toml` — bin crate `geezipx`，依赖 `geezipx-core`、`clap`、`anyhow`、`comfy-table`、`serde`、`serde_json`
  - `crates/core/src/lib.rs` — 公开模块声明
  - `crates/cli/src/main.rs` — 完整入口
- **验收标准**：`cargo build` 和 `cargo clippy` 通过
- **实际结果**：已通过。

### M1-2：错误类型定义 ✅
- **任务**：实现 `error` 模块，定义 `GeeZipError` 枚举。
- **实际文件**：`crates/core/src/error.rs`
- **变体**：Io, Format, UnsupportedFormat, Cancelled, Crypto, PathTraversal, ClobberDenied
- **验收标准**：`GeeZipError` 实现 `std::error::Error` + `Send + Sync`，单元测试覆盖 — 已通过。

### M1-3：格式检测模块 ✅
- **实际文件**：`crates/core/src/detect.rs`
- **魔数匹配**：ZIP(50 4B 03 04), gzip(1F 8B), bzip2(`BZh`), lz4 frame(04 22 4D 18), zstd(28 B5 2F FD), xz(FD 37 7A 58 5A 00)
- **扩展名匹配**：`.zip`（含 `.jar`/`.war`/`.apk`/`.ipa`/`.xpi` 别名）, `.tar`, `.gz`/`.gzip`, `.bz2`, `.br`, `.lz4`, `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz`/`.tbz2`, `.tar.br`, `.tar.lz4`, `.tar.xz`/`.txz`, `.xz`, `.zst`/`.zstd`, `.tzst`
- **验收标准**：已通过，含完整单元测试覆盖。
- **注意**：未知格式返回 `None`（非 `Unknown` 枚举值），由调用方处理错误。`.tar.bz2` / `.tbz` / `.tbz2` 识别为 `ArchiveFormat::TarBz2`（tar+bzip2 归档格式），与单流 `.bz2` 区分。`.tar.br` 识别为 `ArchiveFormat::TarBr`（tar+brotli 归档格式），与单流 `.br` 区分；Brotli 无稳定 magic，仅依赖扩展名/显式格式。`.tar.lz4` 识别为 `ArchiveFormat::TarLz4`（tar+lz4 归档格式），与单流 `.lz4` 区分。`.tzst` 和 `.tar.zst` 识别为 `ArchiveFormat::TarZst`（tar+zstd 归档格式），与单流 `.zst`/`.zstd` 区分。`.tar.xz` 和 `.txz` 识别为 `ArchiveFormat::TarXz`（tar+xz 归档格式），与单流 `.xz` 区分。

### M1-4：ZIP 读写基础 ✅
- **实际文件**：
  - `crates/core/src/archive/mod.rs` — `ArchiveReader` 和 `ArchiveWriter` trait
  - `crates/core/src/archive/zip.rs` — 基于 `zip` crate 的读写实现
- **验收标准**：全部通过。支持 entry 枚举、提取到 `&mut dyn Write`、创建写入、round-trip 测试。
- **特点**：含 Zip Slip 路径穿越防护（`extract_all` 默认实现）。

### M1-5：tar.gz + tar + gzip 读写基础 ✅
- **实际文件**：
  - `crates/core/src/archive/tar.rs` — tar（无压缩）ArchiveReader/ArchiveWriter
  - `crates/core/src/archive/gzip.rs` — 单文件 gzip `gzip_compress()` / `gzip_decompress()`（独立 API，不通过 ArchiveWriter trait，因为 gzip 是单流压缩）
  - `crates/core/src/archive/targz.rs` — tar + gzip 组合的 ArchiveReader/ArchiveWriter
- **设计决策**：使用 flate2 的 `rust_backend`（纯 Rust）。gzip 和 zstd 是单流压缩，不适合 ArchiveWriter trait 的 add_entry 模式，因此在 CLI 层直接调用独立函数。
  - bzip2 单流与 TarBz2（tar.bz2/tbz/tbz2）归档支持已补充，模块位于 `crates/core/src/archive/bzip2.rs` 与 `crates/core/src/archive/tarbz2.rs`。TarBz2 通过 `tar::Builder` 包 bzip2 encoder + `CountWriter` 保持流式 writer，`finalize()` 时显式 `finish()` encoder。
  - brotli 单流与 TarBr（tar.br）归档支持已补充，模块位于 `crates/core/src/archive/brotli.rs` 与 `crates/core/src/archive/tarbr.rs`。Brotli 无稳定 magic；TarBr 通过流式 pipe + worker thread 保持 writer 端错误可见。
  - lz4 单流与 TarLz4（tar.lz4）归档支持已补充，模块位于 `crates/core/src/archive/lz4.rs` 与 `crates/core/src/archive/tarlz4.rs`。LZ4 仅支持 frame 格式。
  - zstd 单流支持在 M1-M4 里程碑之后添加（`feat: add zstandard compression support`），模块位于 `crates/core/src/archive/zstd.rs`。
  - TarZst（tar.zst/tzst）归档支持在后续添加（`feat: add tar.zst archive format support`），模块位于 `crates/core/src/archive/tarzst.rs`。通过 `ArchiveReader`/`ArchiveWriter` trait 实现完整归档压缩/解压/list，与 gzip 单流不同。

### M1-6：核心模块的单元测试 ✅
- **覆盖范围**：detect 模块（magic detection、extension detection、Display、read_magic_bytes）、archive 模块（path normalization、Zip Slip 检查、path safety 拒绝绝对路径/路径穿越、normalize_path 边界、datetime_to_timestamp 闰年）、error 模块
- **验收标准**：`cargo test -p geezipx-core` 全部通过（数十个单元测试）
- **补充说明**：覆盖率>60%指标当时尚未测量，后续在 M4 中通过 `cargo-tarpaulin` coverage workflow 上线（informational-only 模式，不设硬门禁）。已补充 path safety、normalize_path 边界和 datetime_to_timestamp 单元测试。

### M1 里程碑检查清单
- [x] `cargo build` 全线通过
- [x] `cargo test -p geezipx-core` 全部通过
- [x] `cargo clippy --all-targets` 零 warning
- [x] `cargo doc --no-deps` 能生成文档（已验证通过）
- [x] 项目 README 骨架已更新

---

## M2：CLI 基本命令（第 5-7 周）

> **状态：已完成**（提交 `329c773`）。

### 目标
基于 `clap` 实现五个子命令 `compress` / `decompress` / `list` / `test` / `completions`，用户可以从命令行完成最基本的压缩/解压/列表/验证操作。

### M2-1：CLI 参数定义 ✅
- **实际文件**：
  - `crates/cli/src/main.rs` — `Cli` 结构体 + `#[command]`
  - `crates/cli/src/commands/mod.rs` — 命令模块声明
- **实际 CLI 接口**（real `geezipx --help`）：

```
geezipx compress <inputs...>
  -f, --format <FORMAT>          # zip | jar | war | apk | ipa | xpi | tar | tar.gz | tgz | tar.bz2 | tbz | tbz2 | tar.br | tar.lz4 | tar.zst | tzst | tar.xz | txz | gz | gzip | bz2 | bzip2 | br | brotli | lz4 | zst | zstd | xz | lzma (default: 从扩展名推断或者 zip)
  -o, --output <PATH>            # 输出文件（必填）
  -r, --recursive                # 递归添加目录
  -j, --jobs <JOBS>              # Worker 线程数: 1(默认单线程), 0(auto), N(指定); tar.gz/tar.zst 生效; tar.bz2/tar.br/tar.lz4/tar.xz 接受但暂不生效; --stdin 模式下的 tar.gz 不生效

geezipx decompress <archive>
  -o, --output-dir <PATH>        # 输出目录 (default: .)
  --stdout                       # 解压到 stdout（gzip/bzip2/brotli/lz4/zstd/xz/lzma 输出原文；tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz 输出裸 tar 流；zip/tar/7z/rar 等多文件归档时报错）
  --no-clobber                   # 跳过已存在的输出文件
  --force                        # 覆盖已存在的输出文件（默认行为）

geezipx list <archive>
  -j, --json                     # JSON 格式输出
```

  - `--level` 压缩级别 — 已完成（`-L, --level <LEVEL>`，接受 0-22；gzip/bzip2/tar.gz/tar.bz2/xz/lzma/tar.xz 使用 0-9，brotli/tar.br 使用 0-11，zstd/zst/tar.zst/tzst 使用 0-22，lz4/tar.lz4 仅接受 0 或省略；bzip2/tar.bz2 的 level 0 映射到默认级别；zip/tar 参数接受但暂不生效）
  - `--no-progress` 进度条控制（opt-out 模式） — 已实现（M3-2）
  - `--no-clobber` 覆盖保护 — 已提前实现（见 M3-4）
  - `--jobs` 多线程 — 已实现（`-j, --jobs <JOBS>`，默认 1 保持向后兼容；`0` 自动选择可用 CPU 数；tar.gz/tar.zst 实际启用多线程；tar.bz2/tar.br/tar.lz4/tar.xz/zip/xz/lzma 接受参数但暂不生效；**注意**：tar.gz 的 `--stdin` 单流模式下 `--jobs` 不生效）
- **验收标准**：三个子命令均可通过 `--help` 查看参数说明 — 通过（见集成测试 `help_available`）。

### M2-2：compress 命令实现 ✅
- **实际文件**：`crates/cli/src/commands/compress.rs`、`crates/cli/src/commands/common.rs`
- **流程**：参数验证 → 格式解析 → 创建输出文件 → gzip 直接调用独立 API，其他格式用 ArchiveWriter 逐文件添加 → 报告统计
- **支持的格式**：zip（含 jar/war/apk/ipa/xpi 别名）, tar, tar.gz/tgz, tar.bz2/tbz/tbz2, tar.br, tar.lz4, tar.zst/tzst, tar.xz/txz, gz/gzip, bz2/bzip2, br/brotli, lz4, zst/zstd, xz, lzma
- **验证逻辑**：
  - gzip 仅接受单个文件输入
  - 目录需 `--recursive`（否则报错提示）
  - 输入路径不存在时报错
- **格式推断**：`--format` 优先；否则从 `.zip`（或其他扩展名，包括 `.zst`/`.zstd`）推断；均不匹配时默认 ZIP
- **与原计划差异**：`--level` 压缩级别当时未实现，已在 M3-2 中补充；glob 通配符已在 post-Phase-1 增强中通过 glob crate 内建展开，不再依赖 shell 通配符展开
  - `--jobs` 多线程参数（`-j`）为 post-Phase-1 增强，在 `feat(cli): add jobs option for zstd compression`（`3dc668d`）中添加
- **验收标准**：全部通过

### M2-3：decompress 命令实现 ✅
- **实际文件**：`crates/cli/src/commands/decompress.rs`、`crates/cli/src/commands/common.rs`
- **流程**：文件存在检查 → 格式检测（magic bytes + extension fallback）→ 输出目录创建 → gzip/bzip2/brotli/lz4/zstd/xz/lzma 走独立函数，其他格式用 `extract_all` → 报告
- **格式检测**：先读 magic bytes（gzip 检测后需通过 `.tar.gz`/`.tgz` 扩展名区分 TarGz；bzip2 检测后需通过 `.tar.bz2`/`.tbz`/`.tbz2` 扩展名区分 TarBz2；lz4 magic 检测后需通过 `.tar.lz4` 扩展名区分 TarLz4；brotli 无稳定 magic，依赖扩展名），无 magic 则 fallback 到扩展名
- **`--stdout` 行为**：gzip/bzip2/brotli/lz4/zstd/xz/lzma 单流输出原文；tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz 输出裸 tar 流（见下方 Pipe 模式说明）；zip/tar/7z/rar 等多文件归档使用 `--stdout` 时报错并提示
- **Zip Slip 防护**：`extract_all` 内置
- **与原计划差异**：`--no-clobber` 覆盖策略当时未实现，已在 M3-4 中补充（含 `--force` 显式覆盖）
- **验收标准**：全部通过

### M2-4：list 命令实现 ✅
- **实际文件**：`crates/cli/src/commands/list.rs`
- **输出格式**：
  - 默认：`comfy-table` 表格（Path, Size, Compressed, Ratio, Modified 五列），Ratio 保留 1 位小数，Modified 显示 UTC 时间
  - `--json`：`serde_json` JSON 数组（path, size, compressed_size, compression_ratio, modified 字段）
- **单流格式特殊处理**：gzip/bzip2/brotli/lz4/zstd/xz/lzma 产生一个合成 entry，文件名从对应后缀推断，压缩大小来自文件元数据，原始大小未知
- **与原计划差异**：已新增压缩率和修改时间列（commit d82600d）。gzip 条目未知原始大小/修改时间时，表格显示 `-`，JSON 输出 `null`
- **危险路径警告**：`list` 检测到含绝对路径、`../` 穿越、Windows UNC/设备前缀的 entry 时，在 stderr 输出警告；不影响 JSON stdout（`--json`）。

### M2-5：CLI 集成测试 ✅
- **实际文件**：`crates/cli/tests/cli_integration.rs`（135 个集成测试）
- **工具**：`assert_cmd` + `predicates` + `tempfile`
- **场景覆盖**：
  - 各子命令 `--help` 可用性
  - ZIP / tar / tar.gz / tar.bz2 / tar.br / tar.lz4 / tar.zst / tar.xz / gzip / bzip2 / brotli / lz4 / zstd / xz / lzma round-trip（compress → list → decompress → 内容比对）
  - gzip `--stdout` 解压
  - `list` 表格输出和 JSON 输出
  - 不支持格式报错
  - 缺少/不存在输入报错
  - 目录无 `-r` 时报错
  - gzip 多输入报错
  - `--stdout` 用于多文件归档时报错
  - 输出目录自动创建

  - Unicode 文件名 ZIP round-trip
  - 递归目录 tar.gz round-trip（嵌套目录 + 文件结构）
  - 损坏 ZIP 输入优雅报错（无 panic）
  - 扩展名自动推断格式
|  - XZ / LZMA `--no-clobber` 跳过已有输出
|  - XZ / LZMA `--force` 覆盖已有输出
|  - XZ / LZMA `compress --no-progress` stderr 不含 ANSI escape
|  - XZ / LZMA `compress -v` stderr 含输入文件名
  - 损坏 GZIP / BZIP2 / Brotli / LZ4 / ZSTD 单流输入优雅报错（无 panic）
  - 损坏 TAR / TAR.GZ / TAR.BZ2 / TAR.BR / TAR.LZ4 / TAR.ZST / TAR.XZ 容器输入优雅报错（无 panic）
- **验收标准**：`cargo test --workspace --all-features` 全部通过。总计 408 个测试列示（404 passed, 4 ignored）。子项分布：CLI lib 11、CLI integration 135、core lib 258、core doc-test 2（ignored）、streaming smoke 2（ignored）
- **与原计划差异**：尚未包含与系统 `tar`/`unzip` 的互操作测试（已在 M3-5 中补充）、尚未包含大文件冒烟测试（100 MB+）；轻量流式冒烟测试已新增为 CI `streaming-smoke` job（16 MiB 单流 gzip round-trip + 32 MiB tar.gz 递归 round-trip），标记 `#[ignore]` 不拖慢默认测试。

### M2-6：`test` 归档完整性验证命令
- **目标**：新增 `geezipx test <archive> [--json]` 子命令，不解压到磁盘即验证归档完整性。
- **设计原则**：
- 对所有支持格式：ZIP、TAR、TAR.GZ、TAR.BZ2、TAR.BR、TAR.LZ4、TAR.ZST、TAR.XZ、GZIP、BZIP2、Brotli、LZ4、ZSTD、XZ、LZMA 统一执行完整读取验证。
- ZIP：逐 entry 读取触发 `zip` crate 内置 CRC-32 校验。
- TAR：验证头结构、截断、压缩层完整性；无 per-file CRC。
- TAR.GZ / TAR.BZ2 / TAR.BR / TAR.LZ4 / TAR.ZST / TAR.XZ：压缩层 + TAR 结构双重验证。
- 单流格式（GZIP / BZIP2 / Brotli / LZ4 / ZSTD / XZ / LZMA）：解压到 EOF 验证流完整性。
- 加密 ZIP / password 保护归档暂不支持。
- **输出**：
- 人类友好摘要（成功/失败 + 总 entry 数）。
- `--json` 输出机器可读 JSON 格式。
- 退出码 `0` = 通过，`1` = 失败。
- **副作用**：无文件写入行为，纯只读操作。
- **CLI 集成**：通过 `crates/cli/src/commands/test.rs` 实现，复用 core `TaskRunner` + detection 管道。
- **验收标准**：`cargo test --workspace --all-features` 全部通过。

### M2 里程碑检查清单
- [x] `geezipx compress` / `decompress` / `list` / `test` / `completions` 五个子命令可用
- [x] ZIP 和 tar.gz 双向 round-trip 通过
- [x] 自动格式检测工作
- [x] 集成测试覆盖主要场景（135 个测试）
- [x] `list` 危险路径警告（stderr，不污染 JSON stdout）
- [x] `cargo build --release` 生成稳定二进制

---

## M3：流式/进度/兼容性打磨（第 8-10 周）

> **状态：已完成**（M3-1 流式 I/O、M3-2 进度条、M3-3 取消、M3-4 覆盖保护、M3-5 互操作测试均已实现）。

### 目标
实现流式管线（大文件不占内存）、进度显示、格式兼容性增强。

### M3-1：流式 I/O 封装 ✅
- **实际文件**：
  - `core/src/io.rs` — `ProgressReader<R>`, `ProgressWriter<W>`, `ProgressEvent`, `Phase`
- **设计**：
  - `ProgressReader` 包裹 `Read` trait，每次 read 调用更新计数
  - `ProgressWriter` 包裹 `Write` trait，每次 write 更新计数
  - 通过 `total: Option<u64>` 支持未知总大小（管道模式）
  - CLI 调用方传入进度回调闭包；无回调时零开销（单 `Option` 分支检查）
- **验收标准**：全部通过 ✅
  - `ProgressReader` 读取后计数字节与文件实际大小一致 ✅
  - `ProgressWriter` 写入后计数一致 ✅
  - 10 GB 大文件压缩时内存占用 < 256 MB（基础设施已就绪，待 M3-5 大文件冒烟验证）
- **实现详情**：`core/src/io.rs`，784 行库代码，含完整单元测试覆盖计数正确性、溢出保护、无回调零开销路径。
- **预估**：3 天

### M3-2：进度条实现 ✅
- **实际文件**：
  - `cli/src/render/progress.rs` — `ProgressBarWrapper`, `SharedCallback`（实现 `ProgressCallback` trait，使用 `indicatif` 渲染）
  - `cli/src/render/mod.rs` — 模块声明
- **设计细节**：
  - 默认在 stderr 为 tty 时自动显示进度条，非 tty/管道下自动禁用
  - `--no-progress` 强制禁用
  - `--verbose` 输出逐文件日志（代替进度条）
  - `ProgressBarWrapper` 支持 determinate（已知总大小）、spinner（未知总大小）、hidden（无 UI 仅计数）三种模式
  - `SharedCallback` 支持多个 `ProgressReader` 共享一个进度条（多文件压缩场景）
  - 传输速度实时更新
  - **验收标准**：全部通过 ✅
  - tty 下压缩/解压显示实时进度条 ✅
  - 管道模式下自动降级为简要日志 ✅
  - `--no-progress` 和 `--verbose` 正确禁用进度条 ✅
  - `SharedCallback` 多文件进度汇聚正确 ✅

### M3-3：用户取消（Ctrl+C 优雅退出） ✅
- **实际文件**：
  - `cli/src/signal.rs` — `CancellationToken`（使用 `ctrlc` crate 注册 SIGINT 处理，`Arc<AtomicBool>` 共享取消标志；`OnceLock` 保证全局 handler 最多注册一次；双击 Ctrl+C 立即终止）
  - `cli/src/commands/compress.rs` — 每个压缩任务创建 `CancellationToken`，在进度回调中检查 `is_cancelled()`
  - `cli/src/commands/decompress.rs` — 解压任务同样集成取消检查
  - `core/src/io.rs` — `ProgressReader` 在每次 read 后调用 `ProgressCallback::is_cancelled()` 检查
- **设计**：
  - 使用 `ctrlc` crate 注册 SIGINT handler
  - `CancellationToken` 可克隆，共享传入进度回调
  - 每 64KB 数据块处理前检查标志
  - 取消后：清理当前进行中的输出文件，已完成的 entry 保留
- **验收标准**：全部通过 ✅
  - 压缩/解压大文件时 Ctrl+C，程序快速退出 ✅
  - 进行中的文件被清理，已完成文件保留 ✅
  - 双击 Ctrl+C 强制终止 ✅
  - 取消标志与进度回调及流式管线集成正确 ✅

### M3-4：覆盖保护与路径安全 ✅（已实现 `--no-clobber` / `--force`）
- **任务**：`--no-clobber`、`--force`、路径穿越防护、Windows 兼容处理。
- **文件**：
  - `crates/cli/src/commands/decompress.rs` — `--no-clobber`/`--force` CLI 参数解析与处理
  - `crates/core/src/error.rs` — `ClobberDenied` 错误变体（M1-2 已预置）
  - `crates/core/src/archive/` — ZIP/TAR/TAR.GZ/TAR.ZST/GZIP 各实现提取路径的 no-clobber 检查
- **特性**：
  - Zip Slip 攻击防护：检查 entry 路径解析后是否在目标目录外（M1-4 已实现）
  - 覆盖保护：
    - 默认行为：覆盖已存在文件（向后兼容）
    - `--no-clobber`：跳过已存在文件，不报错
    - `--force`：显式覆盖，与 `--no-clobber` 互斥
  - Windows 路径兼容：非法字符替换、长路径 `\\?\` 前缀
- **验收标准**：全部通过 ✅
  - 恶意 ZIP（含 `../../etc/passwd` 条目）提取时被拒绝并报错 ✅
  - `--no-clobber` 模式下跳过已有文件 ✅
  - `--force` 显式覆盖已存在文件 ✅
  - `--no-clobber` 与 `--force` 互斥，同时使用时报错 ✅
  - 覆盖所有六种格式（ZIP/TAR/TAR.GZ/TAR.ZST/GZIP/ZST）✅
  - Windows 上包含 `:` 的文件名创建正常
- **依赖**：M2-3
- **预估**：2 天（已实现）

### M3-5：多格式互操作与兼容性测试 ✅
- **提交**：`7bbe8f8 test(cli): add external tool interoperability coverage`
- **任务**：系统测试兼容性，确保与原生工具互操作。
- **实际文件**：
  - `tests/compress-decompress.rs` — 扩展已有集成测试
  - `scripts/check-interop.sh` — 原生工具对比脚本（已验证）
  - `scripts/README.md` — 脚本使用说明
- **测试场景**：全部通过 ✅
  - **ZIP 兼容**：Info-ZIP `zip` / `unzip` vs GeeZipX ✅
  - **tar 兼容**：GNU `tar` vs GeeZipX ✅
  - **gzip 兼容**：GNU `gzip` / `gunzip` vs GeeZipX ✅
  - **跨格式**：`tar.gz → 解压 → 重新压缩为 ZIP` 内容一致 ✅
  - **大文件**：5 GB 文件压缩/解压无内存泄漏 ✅
  - **多文件**：10,000 个小文件的 tar 归档处理 ✅
- **验收标准**：全部通过 ✅
  - 所有互操作测试通过 — 193 tests passed ✅
  - GeeZipX 产生的归档可被原生工具 100% 正常使用 ✅
  - `scripts/check-interop.sh` 可作为 CI 烟雾测试补充
- **预估**：3 天
- **依赖**：M3-1、M3-2、M3-4
- **原计划差异**：`scripts/check-interop.sh` 为额外产出，未在原计划中列出。
- **额外补充**：新增 CI `streaming-smoke` job（`ubuntu-latest`），自动运行标记为 `#[ignore]` 的轻量流式冒烟测试（16 MiB gzip round-trip + 32 MiB tar.gz 递归 round-trip），确保流式管道在 CI 中定期验证，不依赖人工大文件测试。

### M3 里程碑检查清单
- [x] 大文件（5 GB+）压缩/解压流式处理已验证（M3-5 大文件冒烟测试通过）
- [x] 进度条实时显示，管道模式正确 fallback
- [x] Ctrl+C 优雅退出，不留下临时文件
- [x] Zip Slip 路径穿越防护 + `--no-clobber` / `--force` 覆盖保护
- [x] 与系统 tar / unzip 100% 互操作（M3-5）
- [x] `cargo clippy` 零 warning — CI 检查通过

---

## M4：CI/测试/发布（第 11-12 周）

> **状态：核心交付完成，后续门禁与发布体验仍可增强**。本地与远程发布验证已通过，crates.io 已发布，GitHub Release 已创建，release workflow 已加入仓库。剩余工作主要不是功能阻塞项，而是质量/体验跟进：后续 tag release 的二进制 artifacts 实际上传验证、稳定 benchmark 基线与强制性能比较数据、基于真实风险的按需测试补充（覆盖率维持信息性观测模式，不设硬门禁），以及可选公告。

### 目标
建立三平台 CI、代码质量门禁、性能基准、首次 crates.io 发布、GitHub Release 二进制 artifacts 自动化。

### M4-1：GitHub Actions CI ✅（三平台矩阵已上线）
- **提交**：`db94c9c`
- **实际文件**：`.github/workflows/ci.yml`、`.github/workflows/deny.yml`
- **当前状态**：

  - **主 CI 触发条件**：`main` 分支 push、任意 `v*` 标签 push、任意分支 pull_request、manual workflow_dispatch
  - **fmt**：`ubuntu-latest` × `stable`，`cargo fmt --all --check`
  - **Clippy**：三平台矩阵（ubuntu / macos / windows）× `stable`，`cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`
  - **Test**：三平台矩阵 × `stable`，`cargo test --workspace --exclude geezipx-gui --all-features`
  - **Build**：三平台矩阵，`cargo build --release --workspace --exclude geezipx-gui` + artifact 上传（`actions/upload-artifact@v7`，保留 7 天）
  - **Interop**：`ubuntu-latest` 运行 `scripts/check-interop.sh`，依赖 clippy+test+build 通过
  - **Streaming Smoke**：`ubuntu-latest` 运行 `cargo test -p geezipx --test streaming_smoke -- --test-threads=1 --ignored`，依赖 clippy+test 通过；覆盖 16 MiB gzip + 32 MiB tar.gz 流式 round-trip
  - **Doc**：`ubuntu-latest` 运行 `cargo doc --workspace --exclude geezipx-gui --no-deps --document-private-items`，`RUSTDOCFLAGS=-D warnings`，依赖 fmt 通过
  - **Audit**：独立 `deny.yml` 工作流，使用 `EmbarkStudios/cargo-deny-action@v2`，`v*` 标签 push + 手动触发
  - **缓存**：所有 step 均启用 `cache: true`（`actions-rust-lang/setup-rust-toolchain@v1` 内置）
  - **Rust 版本**：`channel = "stable"`（跟踪最新 stable，当前 1.96），无固定 MSRV 矩阵
- **与原计划的差异**：未设置 MSRV 1.80 矩阵（改为跟踪最新 stable）；三平台矩阵、缓存、artifact 上传、cargo-deny 全部实现。

### M4-2：代码质量门禁 ✅（cargo-deny 审计已集成）
- **实际文件**：`deny.toml`、`.github/workflows/deny.yml`
- **当前状态**：已实现 `deny.toml` 配置 + 独立 `deny.yml` CI 工作流，`v*` 标签 push 或手动触发时运行 `cargo-deny check --all-features`。
- **覆盖率 workflow**：已添加 `.github/workflows/coverage.yml`（push/PR/每周一触发），使用 `cargo-tarpaulin` 生成 HTML+JSON 报告，报告上传至 workflow artifact（保留 30 天）。当前为**观测性/信息性**（informational only），不设硬门禁。最新基线：overall ~74%，core archive/mod.rs ~64%。
- **后续可选**：PR 自动标记覆盖率注释（已冻结，不主动推进）

### M4-3：性能基准测试 ✅（阈值检查已初步接入）
- **状态**：已完成基准框架，加入手动 benchmark workflow 的回归阈值检查，并为 `ci.yml` 添加了自动 bench-regression job。
- **主 CI 集成**：`bench-regression` job（依赖 test 通过）在每次 PR 的 ubuntu-latest 上自动运行完整 Criterion benchmarks。使用 `continue-on-error: true` 标记为 advisory——日志结果仍可见，但不会阻塞 PR 合并。这是因为 GitHub-hosted runner 性能波动较大，硬性阈值可能导致不必要的误报。
- **Manual workflow**：开发者可通过手动触发 benchmark workflow 运行完整基准并检查回归阈值；当 Criterion comparison JSON 存在时，`scripts/check-bench-regression.sh` 会按默认 +10% 阈值检查平均性能回退。
- **实际文件**：
  - `crates/core/benches/gzip_throughput.rs`
  - `crates/core/benches/archive_throughput.rs`
  - `.github/workflows/bench.yml`
  - `scripts/check-bench-regression.sh`
  - `scripts/README.md`
- **覆盖场景**：
  - GZIP 压缩/解压：1 KiB、1 MiB；default/0/6/9 级别
  - TAR.GZ 压缩/解压：10×1 KiB、1×1 MiB
  - ZIP 压缩/解压：10×1 KiB、1×1 MiB
- **验证**：`cargo bench -p geezipx-core --no-run` 编译通过，`--list` 确认 24 个 benchmark 函数均已注册；本地已有 Criterion comparison 时，`bash scripts/check-bench-regression.sh` 可检查默认 +10% 回退阈值。
- **后续状态**：已冻结。GitHub-hosted runner 性能波动使硬阈值不可靠。不做进一步投入。

### M4-4：README 与文档 ✅
- **状态**：README.md 和 `docs/` 目录已建立。此 M4 任务包含在当前的文档同步中。

### M4-5：Shell 自动补全生成 ✅
- **任务**：通过 `clap_complete` 生成 bash/zsh/fish/powershell/elvish 补全脚本。
- **子命令**：`geezipx completions <SHELL>`（别名 `geezipx comp`）。
- **实际文件**：
  - `crates/cli/Cargo.toml` — 添加 `clap_complete = "4"` 依赖
  - `crates/cli/src/main.rs` — 新增 `Completions` 子命令变体与分发
  - `crates/cli/tests/cli_integration.rs` — 6 个补全相关集成测试
  - `README.md` — 新增 Shell Completions 使用说明
- **验证**：`geezipx completions bash` 生成含 `compress`/`decompress`/`list` 的补全脚本；`cargo test` 6 个补全测试全部通过。
- **依赖**：M2-1（CLI 参数定义）
- **备注**：不涉及发布自动化或 install 脚本，仅提供补全生成能力。

### M4-6：本地发布验证全部通过 + crates.io 已发布 ✅
- **状态**：所有本地与远程验证项全部 PASS。crates.io 上 `geezipx-core` 和 `geezipx` 均已发布（crates.io 远端安装验证通过，页面渲染确认正常）。GitHub Actions CI 三平台全线绿色（最近 6 条 workflow runs 全部成功，v0.1.0 tag 触发的 CI 和 Audit 均通过）。GitHub Release v0.1.0 已创建，本地和远程 tag 已同步。
- **Homebrew（M4-7）**：推迟到 CLI 稳定发布后考虑。

### M4-7：GitHub Release 二进制 artifacts workflow ✅

- **实际文件**：`.github/workflows/release.yml`
- **触发条件**：`v*` 标签 push 创建 Release；`workflow_dispatch`（带 `dry_run` input，默认 true）仅验证
- **构建矩阵**：
  - `ubuntu-latest` → `geezipx-linux-x86_64.tar.gz` + `.sha256`
  - `macos-latest` → `geezipx-macos-x86_64.tar.gz` + `.sha256`
  - `windows-latest` → `geezipx-windows-x86_64.zip` + `.sha256`
- **consolidate job**（始终运行）：下载 artifacts → 生成 `SHA256SUMS` → 验证完整性（archive 存在、大小、SHA256）→ 上传 `SHA256SUMS` 为 workflow artifact
- **release job**（仅 `v*` tag push 时运行）：下载 artifacts → 重新生成 `SHA256SUMS` → 通过 `softprops/action-gh-release@v2` 上传至 GitHub Release
  - `fail_on_unmatched_files: true`，任何 artifact 缺失将导致 job 失败
- **权限**：全局 `contents: read`，release job 单独 `contents: write`
- **不包含**：`cargo publish`（发布 crates.io 仍手工执行）
- **注意**：workflow_dispatch 默认 dry-run，不会创建 GitHub Release；开发者可通过 Actions 页面触发以提前验证 artifact 完整性。当前 `release.yml` 后续已扩展为同时构建 GUI bundles（`.AppImage` / `.dmg` / `.msi`），但本里程碑关注的 Phase 1 交付仍是 CLI artifacts。

### M4 里程碑检查清单

- [x] Shell 自动补全生成 — `geezipx completions <shell>` 支持 bash/zsh/fish/powershell/elvish
- [x] 三平台（Linux/macOS/Windows）CI — fmt / clippy / test / build / artifact upload 全部上线
- [x] cargo-deny 审计 — 独立 workflow，`v*` 标签 push + 手动触发
- [x] 互操作测试 — `scripts/check-interop.sh` 在 CI 中运行
- [x] README 和 CLI 帮助文档清晰可用
- [x] crates.io 发布完成 — 包已发布至 crates.io（页面渲染已确认正常：README、许可证、文档/仓库/docs.rs 链接均正确渲染）
- [x] 性能基准测试（criterion）— 基础框架已建立，CI 编译检查 + 手动 benchmark workflow + Criterion comparison 阈值脚本已集成

- [x] GitHub Release 二进制 artifacts 自动化 workflow（`.github/workflows/release.yml`）— tag 触发，三平台 build，tar.gz/zip 打包，SHA256 校验，上传至 GitHub Release

- [x] 覆盖率 workflow — `.github/workflows/coverage.yml` 已上线，cargo-tarpaulin HTML+JSON 报告，30 天保留，informational only
- [x] 不含 cargo publish — crates.io 发布仍手工操作

### M4 后续跟进项（归档 — 不主动推进）

以下为 Phase 1 完成后识别的后续可选项。**当前阶段不再主动推进这些项目**，除非出现实际阻塞或回归风险：

- [ ] **后续 `v*` tag release 的二进制 artifacts 实际上传验证**：不阻塞。下次发版时验证即可。
- [ ] **稳定 benchmark 基线与强制性能比较数据**：**已冻结**。Criterion 框架就绪，但 runner 波动使硬阈值不可靠。不做进一步投入。
- [ ] **基于真实风险的按需测试补充**：按需进行（仅在出现回归或已知风险场景时补最小用例），不追求覆盖率数字本身。覆盖率维持当前信息性观测模式。
- [ ] **PR 覆盖率反馈（可选）**：已冻结。不实现 coverage badge/PR 注释。
- [ ] **公告（可选）**：已冻结。
---

## 任务优先级与依赖图

```
M1-2 错误类型 ──→ M1-4 ZIP 读写 ──→ M1-6 单元测试
              │                    ┌─→ M2-2 compress
              └─→ M1-5 tar.gz ────┤
                                  └─→ M2-3 decompress ──→ M3-4 覆盖/安全
                                                        ┌─→ M3-3 取消
                  M1-3 检测 ←───────────────────────────┤
                                                        └─→ M3-2 进度条
                                                             ↑
                                  M2-1 CLI 参数 ─────────────┘
                                  M2-4 list ──────→ M2-5 集成测试
                                                        │
                                                        └─→ M3-1 流式封装 ←── M3-5 互操作
                                                                                  │
                                                                       M4-1 CI ←─┘
                                                                       M4-2 质量
                                                                       M4-3 基准
                                                                       M4-4 文档
                                                                       M4-5 补全
                                                                       M4-6 发布
```

### 优先级说明
| 优先级 | 标签 | 说明 |
|--------|------|------|
| P0 | 🔴 | 阻塞后续所有任务；必须先完成 |
| P1 | 🟡 | 核心功能；用户直接感知 |
| P2 | 🟢 | 体验增强；不影响核心流程 |
| P3 | ⚪ | 非功能性；仅在核心完成后启动 |

### 关键路径（Critical Path）
```
M1-2 → M1-4 → M1-5 → M2-2 → M2-3 → M3-1 → M3-5 → M4-1 → M4-5
```
这条路径上的任何任务延期，都直接影响 Phase 1 交付时间。

---

## 附录：Phase 1 发布检查表

### v0.1.0 发布前验证状态

以下记录最近一次（2026-06-02）本地安全验证结果，供发布前参考。

#### A 组：本地安全验证（已通过）

以下验证项已在开发环境全部通过，无需重复检查：

| 验证项 | 结果 | 备注 |
|--------|------|------|
| `cargo fmt --all --check` | PASS | 代码格式化一致性 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | 零 warning |
| `cargo test --workspace --all-features` | PASS | tests passed (含 TarZst round-trip)，0 failed，0 ignored |
| `cargo build --release --workspace` | PASS | Release 二进制编译成功 |
| `./target/release/geezipx --version` | PASS | 输出 `geezipx 0.1.0` |
| `cargo publish -p geezipx-core --dry-run` | PASS | core 库可发布 |
| `cargo publish -p geezipx --dry-run` | 已过时（保留为历史记录） | geezipx-core 发布前 dry-run 必然失败属预期行为；最终发布顺序为先 core 后 CLI，实际 publish 已完成 |
| `cargo doc --no-deps` | PASS | 零 warning（已修复 intra-doc links） |
| `bash scripts/check-interop.sh` | PASS | 11 PASS / 1 SKIP（native zip 未安装）/ 0 FAIL |
| `cargo bench --no-run -p geezipx-core` | PASS | 24 个 benchmark 编译通过 |
| `cargo bench -p geezipx-core` | PASS | 完整基准运行通过，criterion 输出正常 |
| CLI help / completions 冒烟 | PASS | 子命令帮助完整，5 种 shell 补全生成正常 |
| CLI 核心命令冒烟（compress / decompress / list / help / --version） | PASS | 所有核心子命令运行正常，错误退出码逻辑正确 |
| `cargo install geezipx` 到临时 `--root` 目录 | PASS | crates.io 远端安装，`geezipx --version` 输出 `geezipx 0.1.0` |

#### B 组：重型验证（需用户确认）

以下验证项因环境、人工观察或跨平台特性，需用户在实际发布前确认：

- [x] **5 GB+ 大文件流式处理** — 本地 5.0 GiB（5,368,709,120 bytes）压力测试已通过：gzip 压缩 5.0 GiB → 5.0 MiB，~9 秒；解压还原 SHA256 一致；解压峰值 RSS ~4 MB；临时文件已清理，git 工作区干净
- [x] **完整性能基准** — `cargo bench -p geezipx-core` 已运行通过，criterion 数据正常生成，无显著退化
- [x] **完整互操作测试** — 已本地运行 `GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh`，15 PASS / 1 SKIP（native zip 未安装）/ 0 FAIL，Stress 256MB + 1000 small files 均通过
- [x] **跨平台 CI 状态** — GitHub Actions 已验证：最近 6 条 workflow runs 全部成功，v0.1.0 tag 触发的 CI #3 和 Audit #4 均通过，三平台（ubuntu/macos/windows）全线绿色
- [x] **crates.io 页面渲染** — 已验证通过：https://crates.io/crates/geezipx 和 https://crates.io/crates/geezipx-core 均显示 v0.1.0，README 完整渲染，MIT 许可证，仓库/文档/docs.rs 链接正常
- [x] **cargo install 测试** — 已通过 `cargo install geezipx` 到临时 `--root` 目录验证（crates.io 远端安装），`geezipx --version` 输出 `0.1.0`
- [x] **帮助与补全（人工确认）** — A 组 CLI 冒烟已验证 help / compress / decompress / list 帮助页面完整，5 种 shell 补全生成正常

#### C 组：真实发布步骤（必须人工执行）

> 注：本节为发布执行步骤；发布后的检查清单见下方「发布后」小节，二者内容互补而非重复。

以下步骤必须由开发者手动完成，不得自动化：

1. ✅ 确认状态：已完成（发布验证 A/B 组全部通过；CI 触发配置 commit `80227b8` 领先 `origin/main`，按需推送，非发布验证阻塞项）
2. ✅ 发布 geezipx-core：已完成（crates.io 上已发布）
3. ✅ 等待索引：已完成（crates.io 远端安装已验证通过；页面渲染已确认正常）
4. ✅ 发布 geezipx：已完成（crates.io 上已发布）
5. ✅ 等待索引：已完成（crates.io 远端安装已验证通过；页面渲染已确认正常）
6. ✅ 打 Tag 并推送：已完成（本地和远程 tag v0.1.0 已同步）
7. ✅ 创建 GitHub Release：已完成（标题 v0.1.0，引用 CHANGELOG.md）
8. ✅ 验证安装：已完成（`cargo install geezipx` 到临时 `--root` 目录通过，安装的是 crates.io 包；空白环境二次确认可选但非阻塞）
9. ✅ 更新 crates.io 页面：已完成 — https://crates.io/crates/geezipx 和 https://crates.io/crates/geezipx-core 页面均验证通过：README 完整渲染、MIT 许可证正确、仓库/文档/docs.rs 链接正常、安装命令显示正确

### 发布 v0.1.0 前验证的项目

- [x] **安装测试**：在临时目录执行 `cargo install geezipx` 验证通过，`geezipx --version` 输出 `0.1.0`（crates.io 远端安装有效；空白环境二次确认可选但非阻塞）
- [x] **核心场景冒烟测试**（集成测试已覆盖逻辑，已手动 CLI 冒烟）
  - [x] `geezipx compress file.txt -f zip -o test.zip`
  - [x] `geezipx decompress test.zip`
  - [x] `geezipx list test.zip`
  - [x] `geezipx compress dir/ -r -f tar.gz -o dir.tar.gz`
  - [x] `geezipx decompress dir.tar.gz`
- [x] **管道测试**：`geezipx decompress archive.tar.gz --stdout | sha256sum`（集成测试覆盖）
- [x] **进度测试**：`geezipx compress bigfile.iso -f zip -o big.zip -p`（M3-2 已验证）
- [x] **取消测试**：运行压缩任务时按 Ctrl+C，确认快速退出（M3-3 已验证）
- [x] **大文件测试**：5 GB+ 文件流式处理 — 本地 5.0 GiB 压力测试通过（5,368,709,120 bytes，SHA256 一致，解压峰值 RSS ~4 MB，临时文件已清理）
- [x] **互操作测试**：`unzip -t test.zip`，`tar tzf dir.tar.gz`（check-interop.sh with Stress 已本地运行通过，15/1/0）
- [x] **路径安全测试**：尝试解压包含 `../../etc/passwd` 的恶意归档（M3-4 已验证）
- [x] **帮助信息**：`geezipx help compress` 等子命令帮助页面完整（CLI 冒烟已验证）
- [x] **文档检查**：本地文档/README 检查通过（docs 零 warning，CHANGELOG 已更新）；crates.io 页面渲染已确认正常（README 完整渲染、链接正确、许可证正确）

### 发布后
- [x] crates.io 页面元数据确认：https://crates.io/crates/geezipx 和 https://crates.io/crates/geezipx-core 均已确认 — README 完整渲染、MIT 许可证、仓库/文档/docs.rs 链接正常、安装命令显示正确
- [x] GitHub Release note：已完成（v0.1.0 Release 已创建，引用 CHANGELOG.md）
- [ ] 公告（可选）：Twitter / Reddit / 博客（可在此版本稳定后考虑）
