# GeeZipX Phase 1 — CLI MVP 任务拆分

> 总周期估计：**10-12 周**（单人全职开发）。  
> 里程碑结构：4 个里程碑，每个里程碑对应可发布的增量。

---

## 里程碑总览

| 里程碑 | 主题 | 周期 | 产出 |
|--------|------|------|------|
| M1 | 项目骨架 + 核心引擎库 | 第 1-4 周 | `geezipx-core` lib crate，zip/tar/gz 基础读写 |
| M2 | CLI 基本命令 | 第 5-7 周 | `geezipx` binary，三个子命令可用 |
| M3 | 流式/进度/兼容性打磨 | 第 8-10 周 | 进度条、管道、格式检测、跨平台测试 |
| M4 | CI/测试/发布 | 第 11-12 周 | CI 全线通过、crates.io 发布、文档站 |

---

## M1：项目骨架 + 核心引擎库（第 1-4 周）

### 目标
建立 Cargo Workspace，完成 `geezipx-core` 库的架构落地，实现 ZIP 和 tar.gz 的基础读写能力（内存模式，暂不流式）。

### M1-1：Workspace 初始化
- **任务**：创建 Cargo Workspace，建立 `core/` 和 `cli/` 目录骨架。
- **文件**：
  - `/Cargo.toml` — workspace 定义，resolver = "2"
  - `/core/Cargo.toml` — lib crate `geezipx-core`，初始依赖 `thiserror`、`log`
  - `/cli/Cargo.toml` — bin crate `geezipx`，初始依赖 `geezipx-core`
  - `core/src/lib.rs` — 公开模块声明
  - `cli/src/main.rs` — 最小入口 `fn main() { println!("hello"); }`
- **验收标准**：
  - `cargo build` 通过
  - `cargo clippy` 无 warning
- **预估**：0.5 天

### M1-2：错误类型定义
- **任务**：实现 `error` 模块，定义 `GeeZipError` 枚举。
- **文件**：
  - `core/src/error.rs` — 包含全部错误变体（Io, Format, UnsupportedFormat, Cancelled, Crypto, PathTraversal, ClobberDenied）
  - 派生 `thiserror::Error` + 手动实现 `Display`（带用户友好的错误信息格式）
- **验收标准**：
  - `GeeZipError` 实现 `std::error::Error` + `Send + Sync`
  - 单元测试覆盖每种变体的消息格式
- **预估**：1 天

### M1-3：格式检测模块
- **任务**：实现 `detect` 模块，基于魔数字节检测归档格式。
- **文件**：
  - `core/src/detect.rs` — `detect_format()` 函数和 `ArchiveFormat` 枚举
  - 魔数匹配：ZIP(50 4B 03 04), gzip(1F 8B), zstd(28 B5 2F FD), xz(FD 37 7A 58 5A 00)
  - tar 降级到 `.tar` 扩展名匹配
- **验收标准**：
  - 传入 ZIP 魔数返回 `ArchiveFormat::Zip`
  - 传入 gzip 魔数返回 `ArchiveFormat::Gzip`
  - 未知魔数返回 `ArchiveFormat::Unknown`
- **预估**：1 天

### M1-4：ZIP 读写基础
- **任务**：实现 ZIP 格式的 `ArchiveReader` / `ArchiveWriter` trait。
- **文件**：
  - `core/src/archive/mod.rs` — `ArchiveReader` 和 `ArchiveWriter` trait 定义
  - `core/src/archive/zip.rs` — 基于 `zip` crate 的 read-only 和 write 实现
- **验收标准**：
  - 能读 ZIP 所有 entry 的信息（文件名、大小、压缩大小、CRC）
  - 能提取 entry 到 `&mut dyn Write`
  - 能创建 ZIP 并将文件条目写入
  - 单元测试：压缩 → 解压 → 比对原始内容
- **依赖**：M1-2（错误类型）
- **预估**：3 天

### M1-5：tar.gz 读写基础
- **任务**：实现 tar 和 gzip 组合的读写能力。
- **文件**：
  - `core/src/archive/tar_impl.rs` — tar（无压缩）
  - `core/src/archive/gz_impl.rs` — 单文件 gzip
  - `core/src/archive/targz.rs` — tar + gzip 组合
- **设计决策**：tar 格式只打包不压缩，gzip 层包裹 tar 流。Phase 1 使用 flate2 的 `rust_backend`（纯 Rust 实现，无 C 依赖）。
- **验收标准**：
  - `tar cvf` 风格：创建 tar，添加文件，验证结构
  - `tar.gz` 创建和提取：先用 gzip 压缩 tar 流，解压时反向
  - 单元测试：`compress("hello") → gz → decompress → "hello"`
- **依赖**：M1-2
- **预估**：3 天

### M1-6：核心模块的单元测试
- **任务**：补全 M1-4 和 M1-5 的单元测试，包括边界情况。
- **场景**：
  - 空 ZIP/tar 的读写
  - 包含子目录的归档（保持路径结构）
  - 大文件（> 1 GB）在内存模式下的行为（应触发适度警告或限制）
  - 损坏归档的错误返回
- **验收标准**：
  - `cargo test -p geezipx-core` 全部通过
  - M1 模块行覆盖率 > 60%
- **预估**：2 天

### M1 里程碑检查清单
- [ ] `cargo build` 全线通过
- [ ] `cargo test -p geezipx-core` 全部通过，覆盖率 > 60%
- [ ] `cargo clippy --all-targets` 零 warning
- [ ] `cargo doc --no-deps` 能生成文档
- [ ] 项目 README 骨架已更新

---

## M2：CLI 基本命令（第 5-7 周）

### 目标
基于 `clap` 实现三个子命令 `compress` / `decompress` / `list`，用户可以从命令行完成最基本的压缩/解压操作。

### M2-1：CLI 参数定义
- **任务**：用 `clap` derive API 定义命令行结构。
- **文件**：
  - `cli/src/main.rs` — `Cli` 结构体 + `#[command]`
  - `cli/src/commands/mod.rs` — 命令模块声明
- **子命令参数**：

```
geezipx compress <inputs...>     # 输入文件/目录
  -f, --format <FORMAT>          # zip | tar | tar.gz | tgz (default: zip)
  -o, --output <PATH>            # 输出文件
  -l, --level <0-9>              # 压缩级别 (default: 6)
  -r, --recursive                # 递归添加目录
  -p, --progress                 # 显示进度条

geezipx decompress <archive>     # 归档文件
  -o, --output-dir <PATH>        # 输出目录 (default: .)
  --stdout                       # 解压到标准输出
  --no-clobber                   # 不覆盖已有文件
  -p, --progress                 # 显示进度条

geezipx list <archive>           # 归档文件
  -j, --json                     # JSON 格式输出
```

- **验收标准**：
  - `geezipx --help` 输出完整
  - 参数解析正确的所有子命令组合
  - 非法参数给出清晰错误
- **预估**：2 天

### M2-2：compress 命令实现
- **任务**：连接 CLI 参数与 core 引擎，实现压缩流程。
- **文件**：
  - `cli/src/commands/compress.rs` — `execute_compress()` 函数
- **流程**：
  1. 解析路径（支持通配符 glob）
  2. 打开输出文件
  3. 根据 `--format` 创建对应 `ArchiveWriter`
  4. 遍历输入文件，逐个添加
  5. 调用 `finish()`，关闭 writer
- **验收标准**：
  - `geezipx compress file.txt -f zip -o out.zip` 创建有效的 ZIP
  - `geezipx compress src/ -r -f tar.gz -o src.tar.gz` 递归打包
  - 生成的 ZIP 可被系统 `unzip` 解压
  - 生成的 tar.gz 可被 `tar xzf` 解压
- **依赖**：M1-4, M1-5, M2-1
- **预估**：3 天

### M2-3：decompress 命令实现
- **任务**：连接 CLI 参数与 core 引擎，实现解压流程。
- **文件**：
  - `cli/src/commands/decompress.rs` — `execute_decompress()` 函数
- **流程**：
  1. 打开归档文件
  2. 自动检测格式（`detect_format`）
  3. 创建对应 `ArchiveReader`
  4. 提取所有 entry 到 `--output-dir`
  5. 处理 `--no-clobber` / `--stdout`
- **验收标准**：
  - `geezipx decompress out.zip` 解压到当前目录
  - `geezipx decompress out.zip -o /tmp/out` 解压到指定目录
  - `geezipx decompress archive.tar.gz --stdout > data` 管道输出
  - 自动检测：无需指定格式即可解压 ZIP 和 tar.gz
- **依赖**：M1-3, M1-4, M1-5, M2-1
- **预估**：2 天

### M2-4：list 命令实现
- **任务**：列出归档内容，表格输出。
- **文件**：
  - `cli/src/commands/list.rs`
  - 使用 `comfy-table` 格式化，`--json` 模式用 `serde_json`
- **验收标准**：
  - `geezipx list archive.zip` 输出表格（文件名、大小、压缩大小、压缩率、修改时间）
  - `geezipx list archive.tar.gz -j` 输出 JSON 数组
  - `geezipx list unknown.xyz` 报错 `unsupported format`
- **依赖**：M1-3, M1-4, M1-5, M2-1
- **预估**：1.5 天

### M2-5：CLI 集成测试
- **任务**：为 CLI 子命令编写集成测试，测试真实文件流。
- **工具**：`assert_cmd` + `predicates` + `tempfile`
- **场景**：
  - 压缩 → 解压 → 比对原始文件 hash
  - `tar.gz` 与系统 `tar` 互操作
  - 非法参数的错误输出
  - `--stdout` 与管道组合
  - 大文件（100 MB+）的简单冒烟测试
- **验收标准**：
  - `cargo test` 全线通过
  - `cargo test --test '*'` 包含集成测试
- **预估**：2 天

### M2 里程碑检查清单
- [ ] `geezipx compress` / `decompress` / `list` 三个子命令可用
- [ ] ZIP 和 tar.gz 双向与原生工具互操作
- [ ] 自动格式检测工作
- [ ] 集成测试覆盖主要场景
- [ ] `cargo build --release` 生成稳定二进制

---

## M3：流式/进度/兼容性打磨（第 8-10 周）

### 目标
实现流式管线（大文件不占内存）、进度显示、格式兼容性增强。

### M3-1：流式 I/O 封装
- **任务**：实现 `ProgressReader` / `ProgressWriter` 和流式管线。
- **文件**：
  - `core/src/io/mod.rs` — `ProgressReader<R>`, `ProgressWriter<W>`, `ProgressEvent`
- **设计**：
  - `ProgressReader` 包裹 `Read` trait，每次 read 调用更新计数
  - `ProgressWriter` 包裹 `Write` trait，每次 write 更新计数
  - 通过 `total: Option<u64>` 支持未知总大小（管道模式）
  - CLI 调用方传入进度回调闭包
- **验收标准**：
  - `ProgressReader` 读取后计数字节与文件实际大小一致
  - `ProgressWriter` 写入后计数一致
  - 10 GB 大文件压缩时内存占用 < 256 MB
- **预估**：3 天

### M3-2：进度条实现
- **任务**：CLI 进度渲染，基于 `indicatif` 和 `crossterm`。
- **文件**：
  - `cli/src/render/progress.rs` — 实现 `ProgressCallback` trait
- **设计细节**：
  - 默认样式：`[{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})`
  - 管道/非 tty 下自动禁用
  - `--no-progress` 强制禁用
  - `--verbose` 输出逐文件日志（代替进度条）
  - 传输速度：基于滑动窗口（最近 5 秒），每 250ms 刷新
- **验收标准**：
  - tty 下压缩/解压显示实时进度条
  - 管道模式下输出简洁逐行日志
  - 速度显示合理（MB/s 单位）
- **依赖**：M2-2, M2-3, M3-1
- **预估**：3 天

### M3-3：用户取消（Ctrl+C 优雅退出）
- **任务**：SIGINT 处理，优雅关闭当前操作。
- **文件**：
  - `cli/src/render/cancel.rs` — 信号处理 + 共享取消标志
  - `core/src/progress.rs` — `is_cancelled()` 检查点
- **设计**：
  - 使用 `ctrlc` crate 或 `tokio::signal`（如果后续需要异步）
  - 取消标志用 `Arc<AtomicBool>` 共享
  - 每 64KB 数据块处理前检查标志
  - 取消后：打印已处理的文件数，退出码 130
- **验收标准**：
  - 压缩大文件时 Ctrl+C，程序在 1 秒内退出
  - 不留下损坏的临时文件
  - 已完成的 entry 保留在输出中
- **预估**：2 天

### M3-4：覆盖保护与路径安全
- **任务**：`--no-clobber`、路径穿越防护、Windows 兼容处理。
- **文件**：
  - 主要在 `cli/src/commands/decompress.rs` 和 `core/src/archive/` 各实现中
- **特性**：
  - Zip Slip 攻击防护：检查 entry 路径解析后是否在目标目录外
  - 覆盖保护：文件已存在时跳过（`--no-clobber`）或覆盖（默认行为）
  - Windows 路径兼容：非法字符替换、长路径 `\\?\` 前缀
- **验收标准**：
  - 恶意 ZIP（含 `../../etc/passwd` 条目）提取时被拒绝并报错
  - `--no-clobber` 模式下跳过已有文件
  - Windows 上包含 `:` 的文件名创建正常
- **依赖**：M2-3
- **预估**：2 天

### M3-5：多格式互操作与兼容性测试
- **任务**：系统测试兼容性，确保与原生工具互操作。
- **文件**：
  - `tests/compress-decompress.rs` — 扩展已有集成测试
  - `scripts/check-interop.sh` — 原生工具对比脚本
- **测试场景**：
  - **ZIP 兼容**：Info-ZIP `zip` / `unzip` vs GeeZipX
  - **tar 兼容**：GNU `tar` vs GeeZipX
  - **gzip 兼容**：GNU `gzip` / `gunzip` vs GeeZipX
  - **跨格式**：`tar.gz → 解压 → 重新压缩为 ZIP` 内容一致
  - **大文件**：5 GB 文件压缩/解压无内存泄漏
  - **多文件**：10,000 个小文件的 tar 归档处理
- **修复**：根据测试结果修复兼容性问题
- **验收标准**：
  - 所有互操作测试通过
  - GeeZipX 产生的归档可被原生工具 100% 正常使用
- **预估**：3 天

### M3 里程碑检查清单
- [ ] 大文件（10 GB+）压缩/解压内存 < 256 MB
- [ ] 进度条实时显示，管道模式正确 fallback
- [ ] Ctrl+C 优雅退出，不留下临时文件
- [ ] Zip Slip 防护有效
- [ ] 与系统 tar / unzip 100% 互操作
- [ ] `cargo clippy` 零 warning

---

## M4：CI/测试/发布（第 11-12 周）

### 目标
建立三平台 CI、代码质量门禁、性能基准、首次 crates.io 发布。

### M4-1：GitHub Actions CI（三平台 Matrix）
- **文件**：`.github/workflows/ci.yml`
- **Matrix**：`os: [ubuntu-latest, macos-latest, windows-latest]` + `rust: [stable, 1.80.0]`
- **Job 步骤**：
  1. 缓存 (actions/cache) — `~/.cargo` 和 `target/`
  2. `cargo check` + `cargo clippy --all-targets`
  3. `cargo test --all-targets`
  4. `cargo test --test '*'`（集成测试）
  5. `cargo build --release`
  6. `cargo deny check advisories`
  7. 上传二进制 artifact
- **验收标准**：
  - 6 个 runner（3 OS × 2 Rust）全部绿色
  - 总 CI 时间 < 20 分钟
- **预估**：2 天

### M4-2：代码质量门禁
- **配置**：
  - `.github/workflows/lint.yml` — clippy + rustfmt check
  - `deny.toml` — `cargo-deny` 配置（advisories + licenses + bans）
  - `.clippy.toml` — 自定义 clippy 规则（如有需要）
  - `.github/workflows/coverage.yml` — 覆盖率报告（`cargo-tarpaulin` for Linux）
- **验收标准**：
  - `cargo deny check` 通过（无高危 advisory）
  - 覆盖率 > 80%
  - PR 自动标记覆盖率变化
- **预估**：1.5 天

### M4-3：性能基准测试
- **文件**：`/benches/throughput.rs`
- **基准场景**（criterion）：
  - ZIP compress: 100 MB 文件
  - ZIP decompress: 100 MB 归档
  - tar.gz compress: 100 MB 文件目录
  - tar.gz decompress: 100 MB 归档
  - 启动时间 `geezipx list small.zip`
  - 内存峰值：大文件流式处理
- **验收标准**：
  - 吞吐量不低于原生工具 90%
  - 启动时间 < 50 ms
  - criterion CI comparison（与 `main` 分支对比不退化超过 5%）
- **预估**：2 天

### M4-4：README 与文档
- **文件**：
  - `/README.md` — 项目介绍、安装、用法示例、Roadmap 链接
  - `/docs/README.md` — 文档索引
- **内容要求**：
  - 徽章（CI status, crates.io, license）
  - 安装方式（cargo install, 下载二进制）
  - 快速上手（3 条最常用命令）
  - 与竞品性能对比表
  - 链接到 PRD、技术架构、Phase 1 任务文档
- **验收标准**：
  - `README.md` 可在 GitHub 仓库首页正确渲染
  - 安装指南在三大平台均可操作
- **预估**：1.5 天

### M4-5：crates.io 发布
- **任务**：首次公开版本发布。
- **前置检查**：
  - `cargo publish --dry-run` 通过
  - 确认 `Cargo.toml` 中 license、description、keywords、categories、readme 字段完整
  - `cargo package --list` 确认包含正确文件
  - 二进制体积检查（release + strip < 15 MB as target）
- **发布**：
  - 先 publish `geezipx-core`
  - 再 publish `geezipx`
- **标签与 Release**：
  - `git tag v0.1.0` → `git push --tags`
  - GitHub Release draft，attach 三平台二进制
- **验收标准**：
  - `cargo install geezipx` 可安装并正常运行
  - GitHub Releases 页面可见
- **预估**：1 天

### M4 里程碑检查清单
- [ ] GitHub Actions CI 三平台全线绿色
- [ ] 代码覆盖率 > 80%
- [ ] 基准测试覆盖率 >= 4 个核心场景
- [ ] `cargo deny` 无高危 advisory
- [ ] `cargo install geezipx` 可用
- [ ] GitHub Releases v0.1.0 发布

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
                                                                       M4-5 发布
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

### 发布 v0.1.0 前验证的项目

- [ ] **安装测试**：在空白 Ubuntu/macOS/Windows 环境执行 `cargo install geezipx`
- [ ] **核心场景冒烟测试**：
  - [ ] `geezipx compress file.txt -f zip -o test.zip`
  - [ ] `geezipx decompress test.zip`
  - [ ] `geezipx list test.zip`
  - [ ] `geezipx compress dir/ -r -f tar.gz -o dir.tar.gz`
  - [ ] `geezipx decompress dir.tar.gz`
- [ ] **管道测试**：`geezipx decompress archive.tar.gz --stdout | sha256sum`
- [ ] **进度测试**：`geezipx compress bigfile.iso -f zip -o big.zip -p`
- [ ] **取消测试**：运行压缩任务时按 Ctrl+C，确认快速退出
- [ ] **大文件测试**：5 GB+ 文件流式处理
- [ ] **互操作测试**：`unzip -t test.zip`，`tar tzf dir.tar.gz`
- [ ] **路径安全测试**：尝试解压包含 `../../etc/passwd` 的恶意归档
- [ ] **帮助信息**：`geezipx help compress` 等子命令帮助页面完整
- [ ] **文档检查**：README 在所有平台渲染正确

### 发布后
- [ ] crates.io 页面更新
- [ ] GitHub Release note 编写
- [ ] 公告（可选）：Twitter / Reddit / 博客
