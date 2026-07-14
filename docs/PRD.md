# GeeZipX — 产品需求文档 (PRD)

## 1. 产品定位

GeeZipX 是一个高性能、跨平台压缩/解压缩工具，使用 Rust 开发。CLI 已进入成熟阶段，当前为第二阶段 — 基于 Tauri 的桌面 GUI（v0.7.0），复用已有 Rust core 引擎。

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

## 4. 核心场景

1. **批量归档**：将多个文件/目录打包为 `.tar.gz`、`.zip`，带进度显示。
2. **快速解包**：解压常见格式，自动检测格式，无需手动指定。
3. **格式转换**：将 `.tar.gz` 转为 `.zip`，或转为 `.zst` 以节省空间。
4. **管道集成**：`cat data.tar.gz | geezipx decompress --stdout | tar xf -`。
5. **加密压缩**：创建带密码的 ZIP / 7z AES-256 归档。
6. **桌面拖拽**（GUI）：拖入文件夹，选格式，完成压缩。

## 5. 格式支持

### 5.1 已支持格式

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
| WIM (.wim/.swm) | 读写 | XPRESS/LZX/LZMS 解压，无压缩写入 |
| DEB (.deb) | 读写 | ar 容器 + control.tar.gz + data.tar.* |
| ASAR (.asar) | 读写 | Electron 归档 |
| UDF (.udf) | 读写 | Universal Disk Format |
| ISZ (.isz) | 读写 | 块压缩 ISO 包装 |
| SFX (.exe) | 读写 | 自解压 ZIP (Linux/Windows/macOS) |
| UU (.uu/.uue) | 读写 | 自实现编解码器 |
| XXE (.xxe) | 读写 | 自实现编解码器 |
| AES (.enc) | 读写 | AES-256-GCM-SIV + Argon2id |
| IMG (.img/.ima) | 透传 | 原始磁盘镜像 |
| BIN (.bin) | 透传 | 原始二进制 |
| RAR (.rar) | 只读 | UnRAR 许可限制 |
| ARJ, ACE, ARC | 只读 | unarc-rs 适配器 |
| ALZ | 只读 | unalz-rs 适配器 |
| Z (.Z) | 只读 | Unix compress |

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

## 9. 路线图

```
Phase 1 (CLI MVP)                已完成并成熟

Phase 2 (Desktop GUI via Tauri)  当前阶段 (v0.7.0)
├── Tauri + TypeScript/Vite 项目骨架与 Core 命令桥接
├── 文件浏览器 + 拖拽导入归档 + 归档内容浏览
├── 压缩/解压任务管理 + 实时进度显示 + 安全取消
├── 加密归档密码输入 + 多语言 (中英文)
├── 窗口状态持久化 + 偏好设置 (5 标签页) + 默认行为配置
├── 自动/手动格式检测 (压缩时)
└── 平台原生打包 (AppImage/.dmg/.exe (NSIS 安装器))

Phase 3 (生态 + 发布)
├── Homebrew / winget / APT 仓库
├── 格式覆盖评估已完成 (§5.2)
└── UU/XXE 编码写入与分卷压缩创建均已完成
```

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
