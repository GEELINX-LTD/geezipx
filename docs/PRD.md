# GeeZipX — 产品需求文档 (PRD)

## 1. 产品定位

GeeZipX 是一个高性能、跨平台压缩/解压缩工具，使用 Rust 开发。CLI 已进入成熟阶段，桌面端采用两条并行路线：保留现有 Tauri GUI，新增独立设计的 egui/eframe 原生 GUI；两者都复用 Rust core 引擎。当前正式版本为 v0.7.4，egui 桌面端为未发布功能，详见 [原生桌面端说明](EGUI_GUI.md)。原生端以减少 WebView 依赖和界面开销为目标，尚未完成同机性能对照测量。

> **核心理念**：压缩操作通常位于自动化脚本、服务器运维、CI/CD 流水线中，CLI 是先发价值；GUI 提供附加便捷，不牺牲底层性能。

## 2. 目标用户

| 用户群 | 使用场景 | 优先度 |
|--------|----------|--------|
| 开发者和运维人员 | CLI 集成脚本、CI/CD、批量归档处理 | P0 |
| 技术用户 / 终端爱好者 | 日常命令行解包、加密压缩、格式转换 | P1 |
| 桌面普通用户 | 拖拽压缩、图形化浏览（GUI 阶段） | P2 |

## 3. 平台范围

| 平台 | CLI | GUI |
|------|-----|-----|
| Linux (x86_64, aarch64) | 原生二进制 | Tauri .AppImage/.deb |
| macOS (x86_64, arm64) | 原生二进制 / Homebrew | Tauri .dmg |
| Windows (x86_64) | 原生二进制 / winget | Tauri .exe (NSIS 安装器) |

另提供 `geezipx-egui` 原生桌面可执行文件，目标为 Windows/macOS/Linux；当前配置 Linux x86_64、Windows x86_64、macOS arm64 独立 CI 编译，Windows 另提供按用户安装的 NSIS 测试安装包。原生工作区支持鼠标框选与选区拖出解压，拖出使用 Windows/macOS 系统拖放或 Linux X11/XWayland，原生 Wayland 拖出暂不支持。文件关联和系统右键菜单尚未接入。

## 4. 核心场景

1. **批量归档**：将多个文件/目录打包为 `.tar.gz`、`.zip`，带进度显示。
2. **快速解包**：解压常见格式，自动检测格式，无需手动指定。
3. **格式转换**：将 `.tar.gz` 转为 `.zip`，或转为 `.zst` 以节省空间。
4. **管道集成**：`cat data.tar.gz | geezipx decompress --stdout | tar xf -`。
5. **加密压缩**：创建带密码的 ZIP / 7z AES-256 归档。
6. **桌面拖拽**（GUI）：拖入文件夹，选格式，完成压缩。

## 5. 格式支持

### 5.1 已支持格式

以下为 core/CLI 格式能力。Tauri 桌面端范围见 `docs/GUI_MVP_PLAN.md` §3.1，其 `get_formats` 根据创建解析器与读取工厂报告能力；egui 桌面端使用独立的适配清单，范围见 `docs/EGUI_GUI.md`。

| 格式 | 能力 | 备注 |
|------|------|------|
| ZIP (.zip/.zipx/.jar/.war/.apk/.ipa/.xpi) | 读写 | AES-256 加密 |
| TAR (.tar) | 读写 | 无压缩容器 |
| TAR.GZ (.tar.gz/.tgz) | 读写 | 多线程并行 (-j) |
| TAR.BZ2 (.tar.bz2/.tbz/.tbz2) | 读写 | — |
| TAR.BR (.tar.br) | 读写 | — |
| TAR.LZ4 (.tar.lz4) | 读写 | — |
| TAR.XZ (.tar.xz/.txz) | 读写 | — |
| TAR.ZST (.tar.zst/.tzst) | 读写 | 多线程并行 |
| GZIP (.gz/.gzip) | 读写 | — |
| BZIP2 (.bz2) | 读写 | — |
| Brotli (.br) | 读写 | 无稳定魔数，依赖扩展名 |
| LZ4 (.lz4) | 读写 | LZ4 frame |
| ZSTD (.zst/.zstd) | 读写 | — |
| XZ (.xz) | 读写 | — |
| LZMA (.lzma) | 读写 | — |
| 7Z (.7z) | 读写 | AES-256，多方法可选，--solid |
| CAB (.cab) | 读写 | MSZIP 压缩，单卷 |
| LZH/LHA (.lzh/.lha) | 读写 | lh0-lh7 |
| LZ (.lz) | 读写 | Lzip 格式 |
| ISO (.iso) | 读写 | ISO 9660 Level 1 |
| CPIO (.cpio) | 读写 | newc/odc |
| ZPAQ (.zpaq/.zpq) | 读写 | 级别 1-5 |
| WIM (.wim/.swm) | 读写（有限制） | 无压缩写入、XPRESS 读取；LZX 兼容性待修复，LZMS 未实现 |
| DEB (.deb) | 读写 | ar 容器 + control.tar.gz + data.tar.* |
| ASAR (.asar) | 读写 | Electron 归档 |
| UDF (.udf) | 读写 | Universal Disk Format |
| ISZ (.isz) | 读写 | 块压缩 ISO 包装 |
| SFX (.exe) | 按构建目标提供 | 自解压 ZIP；缺少或为空的目标 stub 明确报错 |
| UU (.uu/.uue) | 读写 | 自实现编解码器 |
| XXE (.xxe) | 读写 | 自实现编解码器 |
| AES (.enc) | 读写 | AES-256-GCM-SIV + Argon2id；仅验证后的非流式容器 |
| IMG (.img/.ima) | 透传 | 原始磁盘镜像 |
| BIN (.bin) | 透传 | 原始二进制 |
| RAR (.rar) | 只读 | UnRAR 许可限制 |
| ARJ, ACE, ARC | 只读 | unarc-rs 适配器 |
| ALZ | 未实现操作支持 | 仅保留扩展名识别 |
| Z (.Z) | 未实现操作支持 | 仅保留扩展名识别 |

### 5.2 永久排除格式

以下格式经评估后明确不做：

| 格式 | 排除原因 |
|------|----------|
| BH, PMA, PEA, EGG | crates.io 无可用的 Rust 库；格式规范未公开或严重不完整；无已知活跃用户群 |
| CPIO bin/crc | `cpio-archive` crate 不支持；1970 年代遗留格式，无现代使用场景 |
| I00/I01 (分卷 ISO) | 极其小众，未来可复用通用分卷框架实现 |
| ARJ/ACE/ARC/ALZ 写入 | 适配器为只读 API，创建需逆向 1990s 专有格式，用户群极小 |
| Z 写入 | 2026 年无创建 .Z 文件的合理场景 |
| ZIPX 高级方法 | WinZip 专有压缩方法无公开规范与 Rust 实现 |
| RAR 写入 | UnRAR 许可限制 |

## 6. 阶段目标与边界

### 6.1 当前阶段明确不做

- RAR 创建（永久排除，许可限制）
- 自动更新
- 云同步
- 插件系统
- 可视化对比 / 压缩率图表

### 6.2 格式支持交付策略

- **依赖策略**：优先 Rust 原生 crate，仅在无合适实现时评估外部工具或系统库。
- **Feature gate**：每种格式按独立 feature 引入，用户可按需编译。
- **优先级**：由用户需求与社区反馈驱动。
- **读/写分离**：一种格式可先实现只读，写入能力后续补充。
- **历史格式**：通过适配器层或自实现编解码器渐进接入。

## 7. 功能需求

### FR-1: 压缩
`geezipx compress [input]... --format <fmt> -o <output>` 命令，支持通配符和 `-r` 递归添加目录。`--level N` 控制压缩级别（gzip/bzip2/xz/lzma: 0-9，brotli: 0-11，zstd: 0-22）。`--password` / `--password-file` / `--password-stdin` 支持加密 ZIP 和 7z AES-256。

### FR-2: 解压缩
`geezipx decompress <archive>` 自动检测格式并解压到当前目录；`-o <dir>` 指定输出目录；`--stdout` 将单流格式输出到标准输出，tar-based 格式输出裸 tar 流。多文件归档（zip/tar/7z/rar）使用 `--stdout` 时报错。

### FR-3: 格式自动检测
优先检查文件头部魔数字节，无魔数时回退到扩展名匹配，均无法识别时报错退出。

### FR-4: 进度显示
TTY 下默认显示进度条（文件名、处理大小、百分比、速度、ETA），`--no-progress` 禁用，管道模式下自动禁用。

### FR-5: 列出归档内容
`geezipx list <archive>` 显示文件名、原始大小、压缩后大小、压缩率、修改时间。支持表格和 `--json` 两种输出格式。

### FR-6: 流式管道
`--stdin` 从标准输入读取数据（需显式 `--format`），`--stdout` 输出到标准输出。单流格式直接输出原文，tar-based 格式解压缩层后输出裸 tar 流。ZIP/TAR/7z/RAR 等多文件归档不支持 stdin/stdout。

### FR-7: 归档完整性验证
`geezipx test <archive>` 不解压到磁盘，逐 entry 读取验证归档完整性。退出码 0（通过）/ 1（损坏）。`--json` 输出详细验证结果。支持的格式包括 ZIP（CRC-32）、TAR 及 TAR.GZ/TAR.BZ2/TAR.XZ/TAR.ZST（组合校验）、单流格式 GZIP/BZIP2/ZSTD/XZ/LZMA。

## 8. 非功能需求

| 需求 | 目标 | 度量方式 |
|------|------|---------|
| 性能 | 压缩/解压速度不低于同格式原生工具 90%（参考指标） | 基准测试 (hyperfine) |
| 内存 | 流式模式下 < 256 MB | 大文件 (10 GB) 测试 |
| 二进制体积 | 静态链接 < 15 MB, MUSL < 20 MB | `du -sh` |
| 启动时间 | < 50 ms（首次命令到输出） | hyperfine |
| 跨平台一致性 | 同版本三平台产生相同输出 | CI hash 对比 |
| 错误信息质量 | 用户无需查手册即可理解 | 人工审查 |
| 代码覆盖率 | 信息性观测指标，不设硬门禁 | cargo-tarpaulin / grcov |

内存目标适用于已实现流式处理的路径，不代表全部格式的现状保证。WIM 写入使用临时磁盘保存文件内容，metadata 仍驻留内存；AES、ISZ 写入、UDF 映像读写、UU/XXE 和部分第三方格式适配器仍存在整体缓冲，详见 [当前实现限制](TECH_ARCHITECTURE.md#10-当前实现限制)。

文件输出在编码或校验成功后才发布；失败与取消保留已有目标。多文件提取按条目发布，后续条目失败不会回滚此前成功的文件；多卷发布也不构成整组原子事务。

## 9. 路线图

```
Phase 1 (CLI MVP)                已完成并成熟

Phase 2 (Desktop GUI)            当前阶段（正式版 v0.7.4，下一版准备中）
├── Tauri：归档浏览、压缩/解压、预览、进度、取消、设置与系统集成
├── Tauri：AppImage / deb / DMG / NSIS 打包及下一版原生验收
├── egui：独立原生交互、后台任务与共享 core 格式适配
└── egui：Linux x86_64、Windows x86_64、macOS arm64 独立构建

Phase 3 (生态与分发)
└── Homebrew / winget / APT 仓库
```

近期重点是桌面端迭代、候选包构建与 Windows/macOS 原生验收，以及按需求补齐 GUI 格式桥接。启动动作交接回归已实现，候选提交仍需按 [发布验收清单](RELEASE_CHECKLIST.md) 验证；Win11 一级菜单未接入，不作为本轮发布前置。

## 10. 成功指标

| 指标 | 目标值 | 测量方式 |
|------|--------|---------|
| GitHub Stars | Phase1 > 200, Phase3 > 1000 | 计数 |
| 日活跃 CLI 用户 | Phase1 > 50 | 遥测 (opt-in) |
| 格式兼容性 | 与 Info-ZIP/GNU tar 100% 互操作 | 集成测试 |
| CI 通过率 | 主干 > 99% | GitHub Actions |
| 用户报告 P0 bug 数 | 每月 < 3 | Issue tracker |

## 11. 竞品参照

| 工具 | 优点 | GeeZipX 差异化 |
|------|------|---------------|
| `tar` + `gzip` | 生态标准，功能完整 | 跨平台一致体验、进度、格式统一 CLI、单一二进制 |
| `unzip` / `zip` | 广泛使用 | 一个工具搞定所有格式，进度 + 流式 |
| `7z` / `p7zip` | 高压缩率 | Rust 安全内存、现代 CLI 设计、Tauri GUI |
| `bandizip` / `keka` | GUI 体验好 | 免费开源、跨平台 CLI + GUI |
