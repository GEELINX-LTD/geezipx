# GeeZipX — 产品需求文档 (PRD)

## 1. 产品定位

GeeZipX 是一个高性能、跨平台压缩/解压缩工具，使用 Rust 开发。

第一阶段 CLI 已开发完成并进入成熟阶段。当前阶段基于 Tauri 提供 macOS/Linux/Windows 桌面 GUI，复用已有 Rust core 引擎。

> **核心理念**：压缩操作通常位于自动化脚本、服务器运维、CI/CD 流水线中，CLI 是先发价值；GUI 提供附加便捷，不牺牲底层性能。

## 2. 目标用户

| 用户群 | 使用场景 | 优先度 |
|--------|----------|--------|
| 开发者和运维人员 | CLI 集成脚本、CI/CD、批量归档处理 | P0 |
| 技术用户 / 终端爱好者 | 日常命令行解包、加密压缩、格式转换 | P1 |
| 桌面普通用户（GUI 阶段） | 拖拽压缩、右键菜单、图形化浏览 | P2 |

## 3. 平台范围

| 平台 | 形式 | CLI 阶段 | GUI 阶段 |
|------|------|---------|---------|
| Linux (x86_64, aarch64) | 终端 CLI | 原生二进制 | Tauri 桌面 |
| macOS (x86_64, arm64) | 终端 CLI + 桌面 | 原生二进制 / Homebrew | Tauri .dmg |
| Windows (x86_64) | 终端 CLI + 桌面 | 原生二进制 / winget | Tauri .msi |
| Linux 桌面 | GUI | — | Tauri .AppImage/.deb |

## 4. 核心场景

1. **批量归档**：用户将多个文件/目录打包为 `.tar.gz`、`.zip`，带进度显示。
2. **快速解包**：用户解压常见格式，自动检测格式，无需手动指定。
3. **格式转换**：用户将 `.tar.gz` 转为 `.zip`，或转为 `.zst` 以节省空间。
4. **管道集成**：`cat data.tar.gz | geezipx decompress --stdout | tar xf -`。
5. **加密压缩**：用户创建带密码的 ZIP/AES 归档（高级功能）。
6. **桌面拖拽**（GUI 阶段）：拖入文件夹 → 选格式 → 压缩完成。

## 5. MVP 范围（Phase 1 — CLI MVP）

> **当前状态**：基础 `compress`、`decompress`、`list`、`test`、`completions` 子命令已实现，
> 支持 zip/tar/tar.gz/gzip 格式。以下表格标记了各特性的完成状态。

严格限定为 **命令行高性能压缩/解压缩**：

| 特性 | 说明 | 状态 |
|------|------|------|
| 格式支持 | `.tar.gz`, `.tgz`, `.zip`, `.tar`, `.gz`, `.tar.zst`, `.tzst`, `.zst`, `.zstd`, `.tar.xz`, `.txz`, `.xz`, `.lzma`（读/写） | **已完成** (9 格式) |
| 流式处理 | 文件流读写，内存占用与文件大小解耦 | **已完成** |
| 进度显示 | 进度条，支持 `--progress` / `--no-progress` | **已完成** |
| 格式自动检测 | 根据文件魔数（magic bytes）自动检测归档格式 | **已完成** |
| 压缩级别 | `--level 0-9`（gzip/tar.gz/xz/lzma/tar.xz）；`--level 0-22`（zstd/tar.zst） | **已完成** |
| 多线程压缩 | tar.gz/tar.zst 支持 `-j`/`--jobs` 多线程并行（tar.gz: pigz-style via gzp；tar.zst: zstd native NbWorkers）；tar.xz 接受参数但暂不生效（xz2 未暴露多线程 API）；**注意**：tar.gz 的 `--jobs` 在 `--stdin` 单流模式下不生效（仅归档模式有效） | **已完成** |
| 标准管道 | `--stdout` 支持 gzip/zstd/xz/lzma 单流输出原文；tar.gz/tar.zst/tar.xz 输出裸 tar 流；`--stdin` 支持从 stdin 读取（单流及 tar-based 格式）；zip/tar/7z/rar 等多文件归档使用 `--stdout`/`--stdin` 时报错 | **已完成** |
| 递归操作 | `-r` 递归添加目录，保持目录结构 | **已完成** |
| 覆盖保护 | `--no-clobber` / `--force` 覆盖策略 | **已完成** |
| 列表功能 | 表格 + JSON 输出，支持所有当前格式 | **已完成** |

535:|| 归档完整性验证 | `test` 子命令，不解压到磁盘验证归档完整性，支持 `--json` 输出。ZIP 逐 entry CRC-32 校验，TAR 验证结构/截断/压缩层，TAR.GZ/TAR.ZST/TAR.XZ 组合校验。单流格式包括 GZIP/ZSTD/XZ/LZMA。退出码 0/1 | **已完成** |
| 测试覆盖 | 356 个测试（core 239 + CLI lib 11 + CLI integration 106），覆盖率追踪已启用，当前基线 overall ~74%、core archive ~64% | **已完成**（覆盖率 workflow 为观测性/信息性，优先补真实高风险/回归路径） |
| 三平台 CI | GitHub Actions：三平台矩阵（ubuntu/macos/windows），push/PR/tag/manual 触发 | **大部分完成** (M4) |

> **扩展格式识别**：魔数检测已支持 xz（`FD 37 7A 58 5A 00`）和 zstd（`28 B5 2F FD`）。xz 和 lzma 单流压缩/解压已支持（`geezipx-core` via `xz2` crate）；`.tar.xz`/`.txz` 识别为 tar+xz 完整归档格式，与单流 `.xz` 区分。zstd 单流压缩/解压已支持（`geezipx-core` via `zstd` crate）；`.tar.zst`/`.tzst` 识别为 tar+zstd 归档格式。lzma 无固定魔数，仅通过扩展名/显式格式识别。

## 6. 非目标（Phase 1 明确不做）

以下能力在 CLI 阶段明确不做，部分可能作为 GUI 阶段后续考虑：

- 7z 格式写入 — 格式复杂，后续评估
- 分卷压缩 — 后续评估
- 右键菜单集成 — GUI 阶段后续考虑
- 自动更新 — GUI 阶段后续考虑
- 云同步 — 非目标
- 插件系统 — 非目标
- 可视化对比 / 压缩率图表 — GUI 阶段后续考虑

## 7. 功能需求（Feature Requirements）

### FR-1: 压缩
- `geezipx compress [input]... --format zip -o output.zip`
- `geezipx compress [input]... --format tar.gz -o output.tar.gz`
- 支持通配符：`geezipx compress src/*.rs --format zip -o src.zip`
- `-r` 递归添加目录
- `--level N` 控制压缩级别（gzip/tar.gz: 0-9, zstd: 0-22）

### FR-2: 解压缩
- `geezipx decompress archive.zip` — 自动检测格式并解压到当前目录
- `geezipx decompress archive.tar.zst -o /tmp/out` — 指定输出目录（支持 tar.gz、tar.zst、tar.xz 等归档格式）
| `geezipx decompress archive.tar.gz --stdout` — 解压到标准输出：tar.gz/tar.zst/tar.xz 输出裸 tar 流；gzip/zstd/xz/lzma 输出原文；**zip/tar/7z/rar 等多文件归档使用会报错**

### FR-3: 格式自动检测
- 检查文件头部魔数字节
- 无魔数时 fallback 到扩展名匹配
- 均无法识别时报错：`error: unsupported or unknown format`

### FR-4: 进度显示
- 默认开启（tty 下），`--no-progress` 禁用
- 管道模式下自动禁用进度条
- 显示：文件名、已处理大小/总大小(PERCENT)、速度、ETA

### FR-5: 列出归档内容
- `geezipx list archive.zip`
- 显示：文件名、原始大小、压缩后大小、压缩率、修改时间

### FR-6: 流式管道

### FR-7: 归档完整性验证
- `geezipx test <archive>`：不解压到磁盘，逐 entry 读取验证归档。
- 退出码 `0` 表示全部通过，`1` 表示存在损坏。
- `--json`：输出 JSON 格式的详细验证结果。
- 支持的格式：ZIP、TAR、TAR.GZ、TAR.ZST、TAR.XZ、GZIP、ZSTD、XZ、LZMA。
- 各格式校验方式：
  - ZIP：逐 entry 触发 CRC-32 校验。
  - TAR：验证头结构、截断和压缩层，无 per-file CRC。
  - 加密 ZIP AES-256 password 保护已支持：`compress --password / --password-file / --password-stdin`。其它格式暂不支持
- 当前已支持：
  - `--stdout` 将 gzip/zstd/xz/lzma 等**单流格式**解压到标准输出，便于脚本串联。
  - `--stdout` 将 tar.gz/tar.zst/tar.xz 等 **tar-based 压缩归档**解压缩层并输出裸 tar 流，便于 `decompress --stdout archive.tar.gz | tar tf -` 管道串联。
  - `--stdin` 从标准输入读取数据：`compress --stdin -f tar.gz < raw.tar` 接收裸 tar 流并做外层压缩；`decompress --stdin -f gz --stdout` 实现完整管道。
- 当前限制：
  - **zip/tar(无压缩)/7z/rar** 等多文件归档使用 `--stdout` 或 `--stdin` 仍会报错。
  - `--stdin` 模式下需显式 `--format` 指定格式。
  - tar.gz 的 `--stdin` 模式下 `--jobs` 不生效（gzp 并行 gzip 仅归档模式下有效）。

## 8. 非功能需求（Non-Functional Requirements）

| 需求 | 目标 | 度量方式 |
|------|------|---------|
| 性能 | 压缩/解压速度不低于同格式原生工具(pigz/unzip) 90%（参考指标） | 基准测试（hyperfine）|
| 内存 | 流式模式下内存占用 < 256 MB | 大文件（10 GB）测试 |
| 二进制体积 | 静态链接 < 15 MB, MUSL < 20 MB | `du -sh` |
| 启动时间 | < 50 ms（首次命令到输出） | hyperfine |
| 跨平台一致性 | 同版本在三大平台产生相同输出 | CI hash 对比 |
| 错误信息质量 | 用户无需查手册即可理解错误 | 人工审查 |
| 代码覆盖率 | 信息性观测指标（不设硬门禁）。仅针对真实风险/回归场景按需补测，不追求覆盖数字本身 | cargo-tarpaulin / grcov |

## 9. 路线图

```
Phase 1 (MVP — CLI)              ← ✅ 已完成并成熟
├── M1 项目骨架 + 核心引擎库     ── ✅ 已完成
├── M2 CLI 基本命令               ── ✅ 已完成
├── M3 流式/进度/兼容性打磨      ── ✅ 已完成
├── M4 CI/测试/发布              ── ✅ 已完成
└── CLI 增强特性                 ── ✅ 已完成
    ├── 多线程压缩 (-j/--jobs)
    ├── 加密 ZIP (AES-256)
    ├── 7z 只读 / RAR 只读
    └── stdin/stdout 管道

Phase 2 (Desktop GUI via Tauri)  ← 🚀 当前阶段
├── Tauri + Vue/Svelte 项目骨架
├── Core 引擎命令桥接 (command bridge)
├── 文件浏览器 + 拖拽支持
├── 压缩/解压任务管理
├── 实时进度显示 (Tauri event emit)
├── 取消安全的任务执行
├── 加密归档密码输入
└── 平台原生打包 (AppImage/.dmg/.msi)

Phase 3 (生态)
├── Homebrew / winget / APT 仓库
└── 按需扩展格式支持
```

## 10. 成功指标

| 指标 | 目标值 | 测量方式 |
|------|--------|---------|
| GitHub Stars | Phase1 > 200, Phase3 > 1000 | 计数 |
| 日活跃 CLI 用户 | Phase1 > 50 | 遥测（opt-in） |
| 格式兼容性 | 与 Info-ZIP/GNU tar 100% 互操作 | 集成测试 |
| CI 通过率 | 主干 > 99% | GitHub Actions |
| 用户报告 P0 bug 数 | 每月 < 3 | Issue tracker |
    | 代码覆盖率 | 参考指标（不设硬门禁：按真实风险场景按需补测） | cargo-tarpaulin |

## 11. 竞品参照

| 工具 | 优点 | GeeZipX 差异化 |
|------|------|---------------|
| `tar` + `gzip` | 生态标准，功能完整 | 跨平台一致体验、进度、格式统一 CLI、单一二进制 |
| `unzip` / `zip` | 广泛使用 | 一个工具搞定所有格式，进度+流式 |
| `7z` / `p7zip` | 高压缩率 | Rust 安全内存、现代 CLI 设计、Tauri GUI |
| `bandizip`/`keka` | GUI 体验好 | 免费开源、跨平台 CLI |

---

## 附录 A：术语表

| 术语 | 说明 |
|------|------|
| 流式处理 | 边读边处理，不将整个文件加载到内存 |
| 魔数字节 | 文件头部固定签名，用于识别文件格式 |
| 分卷压缩 | 将大归档拆分为多个小文件 |
| tqdm | Python 工具库，迭代时显示进度条 |
