# GeeZipX GUI MVP 规划

> **阶段**：Phase 2（当前开发阶段）
>
> **前置依赖**：Phase 1 CLI 已完成并成熟，core 引擎库 API 稳定，crates.io 已发布。
>
> **已完成（v0.7.0）**：GUI 应用骨架、core 引擎桥接、归档浏览器、选择性提取、文件预览、
> 7z 多线程解压、ISO Joliet+Rock Ridge、UDF 读写、WIM/ISZ 读写、UU/XXE 编码写入、
> 分卷压缩创建、设置页面（5 标签页）、Toast 通知、页面过渡动画、键盘快捷键、
> Windows 右键菜单集成。
>
> **当前状态**：独立 `gui-windows.yml` 已可手动构建 Windows GUI；`release.yml` 已配置三平台 GUI bundle 构建并上传 `.AppImage` / `.dmg` / `.msi`。首个真实 tag release 仍待实战验证。
>
> **格式覆盖**：全部可评估格式已完成评估（见 `docs/PRD.md` §5.1-5.2）。所有 P1/P2 格式待做项已在上一个版本完成。

---

## 1. 目标

提供桌面 GUI 作为 CLI 的配套界面，让不熟悉命令行的用户也能完成日常压缩/解压缩操作。

- 以 Tauri 为壳层，复用已有 Rust core 引擎。
- MVP 聚焦于文件选择 -> 格式配置 -> 执行 -> 进度反馈 -> 完成的闭环体验。
- 支持 macOS、Linux 桌面、Windows 三平台原生打包。

## 2. 非目标（MVP 明确不做）

- 右键菜单集成
- 自动更新
- 云同步
- 插件系统
- 分卷压缩
- RAR 创建 — 受 UnRAR 许可限制，仅保持只读
- 文件管理器集成（双面板、标签页等）
- 批量任务队列（仅单次任务，后续可扩展）
- 高级命令行偏好配置（当前仅 GUI 设置页面的选项，不做 CLI 级配置暴露）

## 3. MVP 功能范围

### 3.1 支持的格式

| 操作 | 当前格式 | 目标扩展 |
|------|----------|----------|
| 解压缩 | 上述所有格式 + RAR / CAB / ASAR / DEB / CPIO / WIM / ARJ / ACE / ARC / ALZ / UU / UUE / XXE / Z / LZ / IMG / BIN / AES / UDF / ISZ | UU/UUE/XXE 编码写入（P1）、分卷压缩创建（P2）。其余已评估格式永久排除（见 PRD §5.2） |

> 完整格式目标清单见 `docs/PRD.md` 第 5.1 节。新增格式按 feature gate 引入，不要求当前版本一次性完成。

GUI 中 7z 已支持基础创建与 AES-256 密码写入，LZH/LHA 已支持 lh0-lh7 压缩写入（通过 oxiarc-lzhuf），ZIPX 作为 ZIP-compatible alias 可创建、浏览、测试与提取。ISO 已支持创建与读取（ISO 9660 Level 1 + Joliet 读取）。ZPAQ 已支持创建与读取。SFX 已支持创建 ZIP self-extracting 可执行文件。CPIO 已支持创建与读取（newc/odc）。CAB 已支持创建与读取（MSZIP 写入）。ASAR 已支持创建与读取。DEB 已支持创建与读取。WIM 已支持创建与读取（无压缩写入，XPRESS/LZX/LZMS 解压）。ISZ 已支持创建与读取（块压缩 ISO 包装）。RAR / ARJ / ACE / ARC / ALZ / UU / UUE / XXE / Z 已支持只读浏览、测试与提取。LZ（Lzip）已支持单流压缩/解压。

### 3.2 核心功能

1. **文件选择**
   - 系统原生文件对话框（单文件/多文件/文件夹）
   - 拖拽文件/文件夹到窗口

2. **压缩任务**
   - 选择目标格式（dropdown/radio group）
   - 自动格式检测（压缩时根据文件类型自动推荐）
   - 配置压缩级别（通过 slider 或 dropdown）
   - 支持密码（AES-256 for ZIP / 7z）
   - 可选：输出目录

3. **解压缩任务**
   - 拖拽或选择压缩包
   - 预览归档内容（列表模式，文件名/大小/压缩比）
   - 设置解压目标目录
   - 覆盖策略：提示/跳过/覆盖全部
   - 支持密码输入

4. **进度与结果**
   - 实时进度条 + 处理速度/剩余时间
   - 取消按钮（安全取消）
   - 完成通知（Toast / 状态栏消息）
   - 错误展示（可读错误消息）

### 3.3 省略功能（MVP 后考虑）

- 压缩率图表/对比
- 批量任务队列
- 文件搜索/过滤
- 预设配置保存
- 历史记录

## 4. 架构边界

```text
┌─────────────────────────────────────┐
│      Tauri Frontend (TypeScript/Vite)  │
│  当前实现：src/main.ts + i18n + style.css  │
│  - 文件选择 / 拖拽 / 浏览 / 预览       │
│  - 任务状态与进度面板                 │
└──────────────┬──────────────────────┘
               │ invoke / event
┌──────────────▼──────────────────────┐
│    Tauri Rust Backend (thin bridge) │
│  src-tauri/src/commands/*           │
│  - compress_archive                 │
│  - extract_archive / extract_entries│
│  - list_archive / test_archive      │
│  - preview_entry / drag helpers     │
│  - cancel_task                      │
└──────────────┬──────────────────────┘
               │ reuse core APIs
┌──────────────▼──────────────────────┐
│         geezipx-core 引擎库          │
│  archive/*  detect.rs  error.rs     │
│  config.rs  io.rs  test.rs          │
│  - 纯业务逻辑，不依赖 Tauri          │
└─────────────────────────────────────┘
```

**关键约束**：

- `geezipx-gui`/`crates/gui-tauri/src-tauri` 依赖 `geezipx-core`，反向依赖不允许。
- GUI Rust 后端只做参数映射、任务生命周期管理、进度桥接与前端数据整形。
- 前端不直接处理压缩格式细节；所有实际归档操作都经由 Tauri command bridge。
- 各格式创建/读取能力同 core 层（见 `docs/PRD.md` §5.1-5.2）。

## 5. Core API 复用策略

GUI 直接复用 core 的以下能力，不重复实现压缩/解压逻辑：

| Core 模块 | GUI 复用方式 |
|-----------|--------------|
| `core::archive::*` | 复用 `ArchiveReader` / `ArchiveWriter` trait；各格式能力见 PRD §5.1-5.2 |
| `core::io::{ProgressReader, ProgressWriter, ProgressCallback, ProgressEvent}` | 进度计数与取消检查；由 GUI 后端转成 Tauri 事件 |
| `core::detect::{detect_format, detect_from_extension, read_magic_bytes}` | 自动识别拖入文件与归档类型 |
| `core::config::CompressOptions` | 统一传递 level、jobs、password 等参数 |
| `core::error::GeeZipError` | 后端转换为用户可读字符串并返回前端 |
| `core::test` | 归档完整性验证逻辑，供 `test_archive` 调用 |

### 5.1 Tauri command bridge

当前后端命令入口位于 `crates/gui-tauri/src-tauri/src/lib.rs`：

```rust
#[tauri::command]
async fn compress_archive(...) -> Result<CompressArchiveResult, String>;

#[tauri::command]
async fn extract_archive(...) -> Result<ExtractArchiveResult, String>;

#[tauri::command]
async fn extract_entries(...) -> Result<ExtractArchiveResult, String>;

#[tauri::command]
async fn list_archive(...) -> Result<Vec<EntryInfo>, String>;

#[tauri::command]
async fn test_archive(...) -> Result<TestArchiveResult, String>;

#[tauri::command]
async fn preview_entry(...) -> Result<PreviewEntryResult, String>;

#[tauri::command]
fn cancel_task(task_id: String, state: State<'_, AppState>) -> Result<(), String>;
```

补充命令还包括：

- `get_formats`：向前端暴露支持的格式列表；
- `get_opened_archives`：读取冷启动/文件关联传入的归档路径；
- `prepare_drag_entries` / `cleanup_drag_temp_dir` / `cleanup_stale_drag_temp_dirs`：支持从归档浏览器拖出条目到系统文件管理器。

## 6. 进度与取消

### 6.1 进度推送

core 侧的 `ProgressEvent` 只有流式层所需的最小信息：

```rust
pub struct ProgressEvent {
    pub current: u64,
    pub total: Option<u64>,
    pub phase: Phase, // Reading | Writing | Hashing
}
```

GUI 后端在 `src-tauri/src/commands/progress.rs` 中把它包装为更丰富的 `TaskProgressPayload`，并通过固定事件名 `task:progress` 推送给前端。payload 额外包含：

- `task_id` / `kind` / `status` / `stage`
- `percent` / `bytes_per_second`
- `current_entry`
- `completed_entries` / `total_entries`
- 用户可读 `message`

### 6.2 取消机制

- 每个 GUI 任务都会注册一个 `Arc<AtomicBool>` 取消令牌到 `AppState`。
- 前端调用 `cancel_task(task_id)` 后，后端将对应令牌置为取消态。
- `ProgressReader` / `ProgressWriter` 会在每次 I/O 前调用 `ProgressCallback::is_cancelled()`。
- 底层 `Interrupted` / `GeeZipError::Cancelled` 会被 GUI 层统一映射为"用户取消"状态。

### 6.3 前端监听

```typescript
import { listen } from '@tauri-apps/api/event';
import type { TaskProgressPayload } from './bridge';

listen<TaskProgressPayload>('task:progress', (event) => {
  const payload = event.payload;
  updateTaskProgress(payload);
});
```

## 7. 密码处理

- ZIP：支持创建 AES-256 加密归档，也支持浏览/测试/提取已加密 ZIP。
- 7z：支持创建 AES-256 加密归档，也支持浏览/测试/提取已加密 7z。
- RAR：密码仅支持读取路径（`list` / `test` / `extract`）。
- CPIO：支持读取与创建路径（`compress` / `list` / `test` / `extract`），支持 `newc` / `odc`，不含 `bin` / `crc`、密码访问或宿主 symlink/device/FIFO/socket 创建。
- 密码仅作为任务参数传递，不做持久化。
- 前端提供显隐切换，但不保存默认密码。

## 8. 覆盖策略

- GUI 提取相关命令使用显式 `overwrite: bool` 参数传给后端。
- 关闭覆盖时，core 会通过 `ClobberDenied`/skip 语义保护已有文件。
- 选择性提取与整包提取共享同一套覆盖逻辑与进度汇报。

## 9. 平台原生打包

| 平台 | 当前 bundle / workflow 状态 |
|------|-----------------------------|
| macOS | `release.yml` 已配置构建 `.dmg` 并上传 release artifacts；待真实 tag release 验证 |
| Linux | `release.yml` 已配置构建 `.AppImage` 并上传 release artifacts；待真实 tag release 验证 |
| Windows | `gui-windows.yml` 可手动构建 Windows GUI；`release.yml` 已配置 `.msi` release artifacts |

> 打包配置声明在 `crates/gui-tauri/src-tauri/tauri.conf.json`。当前文档统一以"已配置，待 tag release 实战验证"为准。

## 10. 首批任务拆分（当前回顾）

### G1：项目骨架与 Tauri 集成

- [x] 创建 `crates/gui-tauri/` 目录与 `crates/gui-tauri/src-tauri` workspace member
- [x] 初始化 Tauri v2 + TypeScript/Vite GUI 项目
- [x] 接入 `geezipx-core` 依赖并验证 `cargo build -p geezipx-gui`
- [x] 验证开发模式可启动

### G2：Core 引擎桥接

- [x] `compress_archive` / `extract_archive` / `list_archive` / `test_archive`
- [x] `cancel_task` 取消桥接
- [x] `preview_entry` / `extract_entries` / drag helpers
- [x] 后端通过 `task:progress` 发出 GUI 任务进度事件

### G3：前端基础 UI

- [x] 主窗口布局（导航 + 内容区 + 状态反馈）
- [x] 文件选择与拖拽导入
- [x] 格式选择、压缩级别、密码输入
- [x] 归档浏览器（目录树 / 列表 / 预览）
- [x] 最近路径 chips 与文件关联打开

### G4：进度与任务管理

- [x] 实时进度条 + 速度 / 剩余时间显示
- [x] 取消按钮（安全中断）
- [x] 完成 / 错误通知
- [x] 前端任务状态管理

### G5：平台打包与 CI

- [x] `tauri.conf.json` 打包配置
- [x] `gui-windows.yml` 独立 Windows GUI 构建工作流
- [x] `release.yml` 三平台 GUI bundle 构建与 artifact 上传配置（`.AppImage` / `.dmg` / `.msi`）
- [ ] 首次真实 tag release 的端到端验证

### G6：MVP 后打磨

- [x] 窗口状态持久化（位置、大小）— 通过 `tauri-plugin-window-state` v2.4.1 实现
- [x] 最近路径 / 最近归档 chips（当前以前端 `localStorage` 形式存在）
- [x] 拖拽时自动检测格式
- [x] 设置页面（5 标签页：通用 / 压缩 / 外观 / 文件关联 / 关于）
- [x] 多语言实时切换（zh-CN / en）
- [x] 主题切换（跟随系统 / 浅色 / 深色）
- [x] 通用设置：默认输出目录、覆盖策略、完成后行为
- [x] 压缩设置：默认格式、压缩级别、递归开关、默认密码
- [x] 文件关联管理（macOS 原生绑定 / Windows 引导设置）
- [x] 关于页面（动态版本号、技术栈、GitHub 链接）
- [x] 设置持久化（tauri-plugin-store，settings.json）
- [x] 未保存修改导航保护（settingsGuard）
- [x] 进度条 GPU 加速（transform: scaleX + will-change）
- [x] 大归档虚拟滚动（virtua VList，5 万行仅渲染可视 ~25 行 DOM）
- [x] 归档列表加载骨架屏（shimmer 动画）
- [x] archiveStore entries/selectedPaths 改用 $state.raw 消除深度代理开销
- [ ] 后端 list_archive 流式/分页推送（未来迭代）

---

## 11. 设置页面架构

设置页面使用 Svelte 5 runes 模式实现，包含 5 个标签页，通过 `tauri-plugin-store` 持久化到本地 `settings.json`。

### 11.1 文件结构

```text
crates/gui-tauri/src/
├── pages/SettingsPage.svelte       # 设置主页面（5 标签页布局）
├── stores/settingsStore.svelte.ts  # 设置读写 + 默认值定义
├── stores/settingsGuard.svelte.ts  # 未保存修改导航保护
├── bridge.ts                       # GeeZipXSettings 类型定义
└── i18n/locales/{en,zh-CN}.json    # 设置相关 i18n key
```

后端辅助命令（`crates/gui-tauri/src-tauri/src/commands/`）：

| 命令 | 文件 | 用途 |
|------|------|------|
| `get_formats` | `formats.rs` | 返回可压缩格式列表，填充「默认格式」下拉 |
| `get_file_associations` | `associations.rs` | 返回文件关联状态列表 |
| `set_file_association` | `associations.rs` | 绑定/解绑单个格式的文件关联 |
| `open_association_settings` | `associations.rs` | Windows 下打开系统设置页引导用户设为默认 |
| `get_version` | `app.rs` | 返回当前版本号，显示在「关于」标签页 |

### 11.2 标签页详情

#### 通用（General）

| 设置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| 语言 (locale) | `'en' \| 'zh-CN'` | `zh-CN` | 保存后即时生效 |
| 默认输出目录 | `string \| null` | `null` | 为空则每次提示选择；支持浏览按钮 + 清除 |
| 覆盖策略 (overwrite_strategy) | `'prompt' \| 'skip' \| 'overwrite'` | `prompt` | 解压时目标文件已存在时的行为 |
| 完成后行为 (on_complete) | `'nothing' \| 'open_output'` | `nothing` | 压缩/解压成功后是否打开输出目录 |

#### 压缩（Compression）

| 设置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| 默认格式 | `string` | `zip` | 下拉列表来自 `get_formats`，仅显示 `can_compress` 的格式 |
| 默认压缩级别 | `number \| null` | `null` | 0-22 数字输入，为空使用格式默认值 |
| 递归添加目录 | `boolean` | `true` | 压缩时是否递归包含子目录 |
| 默认密码 | `string \| null` | `null` | 预填加密/解压密码字段（明文存储于本地文件） |
| 记住密码 | `boolean` | `false` | 不勾选时保存会清除密码 |

#### 外观（Appearance）

| 设置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| 主题 (theme) | `'system' \| 'light' \| 'dark'` | `system` | 保存时立即通过 `data-theme` 属性切换 |

#### 文件关联（File Associations）

- 列出所有支持的归档格式，显示扩展名 chip、描述、绑定状态（默认/已注册/未绑定）
- macOS：直接通过复选框绑定/解绑
- Windows：复选框注册后需点击「设为默认」跳转系统设置完成绑定
- 按钮「打开系统设置」用于 Windows 下无法程序化设默认的场景

#### 关于（About）

- 应用名称 GeeZipX + 动态版本号（通过 `get_version` 命令获取）
- 产品描述 + 技术栈（Rust + Tauri 2 + Svelte 5）
- GitHub 仓库链接

### 11.3 GeeZipXSettings 类型

```typescript
// crates/gui-tauri/src/bridge.ts
export interface GeeZipXSettings {
  locale: 'en' | 'zh-CN';
  default_output_dir: string | null;
  overwrite_strategy: 'prompt' | 'skip' | 'overwrite';
  default_format: string;
  default_level: number | null;
  recursive: boolean;
  theme: 'system' | 'light' | 'dark';
  on_complete: 'nothing' | 'open_output';
  default_password: string | null;
  remember_password: boolean;
}
```

### 11.4 数据流

```text
SettingsPage.svelte
    │ 读取 ──► settingsStore.loadAll() ──► tauri-plugin-store (settings.json)
    │ 保存 ──► settingsStore.saveAll() ──► tauri-plugin-store (settings.json)
    │
    ├── 表单修改时 $derived dirty 检测
    ├── dirty 同步到 settingsGuard → TabBar 拦截导航
    └── 保存后即时应用 theme (data-theme) + locale (localeStore.switchLocale)

CompressPage / ExtractControls
    │ 读取 ──► settingsStore.get(key) ──► 预填默认值
    │
    └── 每个页面独立按需读取（get），不依赖 loadAll
```

### 11.5 与压缩/解压流程的集成

设置值在以下位置被消费：

- **CompressPage.svelte**：读取 `default_format`、`default_level`、`recursive`、`default_password`、`remember_password`、`default_output_dir`、`on_complete` 预填表单
- **ExtractControls.svelte**：读取 `overwrite_strategy`、`default_output_dir`、`default_password`、`remember_password`、`on_complete` 预填面板
- **App.svelte**（快速压缩路径）：拖拽 Zip 快速压缩时读取 `default_level`、`default_password`、`remember_password`、`recursive`
