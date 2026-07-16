# GeeZipX GUI MVP 规划

> 本文档描述 GeeZipX GUI 的架构与已完成功能。当前版本 v0.7.0。
>
> **前置依赖**：Phase 1 CLI 已完成，core 引擎库 API 稳定，crates.io 已发布。

---

## 1. 目标

桌面 GUI 作为 CLI 的配套界面，让不熟悉命令行的用户也能完成日常压缩/解压缩操作。

- 以 Tauri 为壳层，复用 Rust core 引擎。
- 聚焦文件选择 -> 格式配置 -> 执行 -> 进度反馈 -> 完成的闭环体验。
- 支持 macOS、Linux 桌面、Windows 三平台原生打包。

## 2. 非目标

- 自动更新、云同步、插件系统
- 文件管理器集成（双面板、标签页等）
- 批量任务队列（仅单次任务）
- 高级命令行偏好配置
- RAR 创建（受 UnRAR 许可限制，仅只读）

## 3. MVP 功能范围（当前状态）

### 3.1 支持的格式

所有操作依赖 core 引擎，完整清单见 `docs/PRD.md` §5.1-5.2。

- **完整读写**：ZIP（含 AES-256）、TAR 及其压缩变体（GZ/TGZ、BZ2/TBZ、BR、LZ4、XZ/TXZ、ZST/TZST）、GZ、BZ2、BR、LZ4、XZ、LZMA、LZ/Lzip、ZST、7z（AES-256）、CAB（MSZIP）、ASAR、DEB、CPIO（newc/odc）、LZH/LHA（lh0-lh7）、ISO、UDF、ZPAQ、WIM、ISZ、SFX（ZIP 自解压）、UU/UUE/XXE 编码写入
- **只读**：RAR、ARJ、ACE、ARC、ALZ、Z/Unix Compress
- **透传**：IMG、BIN
- **AES 加密容器**：`.enc`（AES-256-GCM-SIV + Argon2id）

### 3.2 核心功能

1. **文件选择** — 原生对话框 + 拖拽
2. **压缩任务** — 格式选择、自动推荐、压缩级别、密码（ZIP/7z AES-256）、输出目录
3. **解压缩任务** — 归档内容预览、选择性提取、覆盖策略、密码输入
4. **归档浏览器** — virtua VList 虚拟滚动（5 万行仅 ~25 行 DOM）、文件夹导航、最近路径 chips
5. **进度与结果** — 实时进度 + 速度/剩余时间、安全取消、Toast 通知
6. **文件预览** — 文本/图片条目在浏览器中预览
7. **设置页面** — 5 标签页 + 右键菜单管理，`tauri-plugin-store` 持久化
8. **多语言与主题** — zh-CN / en 实时切换；跟随系统/浅色/深色
9. **窗口状态持久化** — `tauri-plugin-window-state`

## 4. 架构边界

```text
┌─────────────────────────────────────┐
│      Tauri Frontend (TypeScript/Vite)  │
│  - 文件选择 / 拖拽 / 浏览 / 预览       │
│  - 任务状态与进度面板                 │
└──────────────┬──────────────────────┘
               │ invoke / event
┌──────────────▼──────────────────────┐
│    Tauri Rust Backend (thin bridge) │
│  compress / extract / list / test   │
│  preview / cancel / drag helpers    │
└──────────────┬──────────────────────┘
               │ reuse core APIs
┌──────────────▼──────────────────────┐
│         geezipx-core 引擎库          │
│  archive/*  detect.rs  error.rs     │
│  config.rs  io.rs  test.rs          │
└─────────────────────────────────────┘
```

**约束**：GUI 依赖 core，反向不允许。后端只做参数映射、任务生命周期、进度桥接与数据整形，不处理压缩逻辑。

## 5. Core API 复用策略

| Core 模块 | GUI 复用方式 |
| `io::{ProgressReader, Writer, Callback, Event}` | 进度计数与取消检查；后端转 Tauri 事件 |
| `detect::*` | 自动识别拖入文件与归档类型 |
| `config::CompressOptions` | 统一传递 level、jobs、password 等 |
| `error::GeeZipError` | 后端转用户可读字符串 |
| `archive::*` | 复用 `ArchiveReader` / `ArchiveWriter` |
| `test` | 归档完整性验证 |

### 5.1 Tauri command bridge

入口 `crates/gui-tauri/src-tauri/src/lib.rs`，核心命令：

| 命令 | 类型 | 参数 |
| `compress_archive` | `async -> Result<...>` | 压缩参数 + 进度回调 |
| `extract_archive` / `extract_entries` | `async -> Result<...>` | 路径 + 覆盖 + 密码 |
| `list_archive` | `async -> Result<Vec<EntryInfo>>` | 路径 + 密码 |
| `list_archive_stream` | `async (Channel<EntryChunk>) -> Result<()>` | 路径 + 密码 + 流通道 |
| `test_archive` | `async -> Result<...>` | 路径 + 密码 |
| `preview_entry` | `async -> Result<...>` | 路径 + 条目 |
| `cancel_task` | `fn -> Result<()>` | task_id |
| 辅助命令 | - | `get_formats`、文件关联、版本 |

## 6. 进度与取消

**进度推送**：core 的 `ProgressEvent` 由后端在 `commands/progress.rs` 包装为 `TaskProgressPayload`（含 `task_id`、`percent`、`bytes_per_second`、`current_entry`、`completed_entries`/`total_entries`），通过 `task:progress` 事件推送前端。

**取消机制**：每个任务注册 `Arc<AtomicBool>` 令牌至 `AppState`。`cancel_task` 置为取消态，`ProgressReader` / `ProgressWriter` 每次 I/O 前检查并抛出 `GeeZipError::Cancelled`。

## 7. 密码处理

ZIP 与 7z 支持创建 AES-256 加密归档。RAR 密码仅读取。密码仅作任务参数传递，不做持久化。前端提供显隐切换。

## 8. 覆盖策略

GUI 提取使用 `overwrite: bool` 参数，关闭时 core 通过 `ClobberDenied`/skip 保护已有文件。选择性提取与整包提取共享同一套逻辑。

## 9. 平台原生打包

| 平台 | bundle | 状态 |
| macOS | `.dmg` | `release.yml` 已配置并验证 |
| Linux | `.AppImage` | `release.yml` 已配置并验证 |
| Windows | `.exe (NSIS 安装器)` | `release.yml` 已配置；`gui-windows.yml` 可手动构建 |

打包配置在 `crates/gui-tauri/src-tauri/tauri.conf.json`。

## 10. 已完成功能概述（v0.7.0）

- **应用骨架** — Tauri v2 + Svelte 5 + Vite workspace member
- **Core 桥接** — 全部压缩/解压/列表/测试/预览/取消命令
- **前端 UI** — 主窗口布局、拖拽、格式选择、归档浏览器（虚拟滚动）、文件预览
- **进度管理** — 实时进度条、速度/剩余时间、安全取消、Toast 通知
- **设置页面** — 5 标签页，`tauri-plugin-store` 持久化
- **多语言与主题** — zh-CN / en 实时切换，三模式主题
- **窗口状态持久化** — 位置、大小恢复
- **Windows 右键菜单** — 运行时通过设置页动态管理，`HKCU\Software\Classes`（无需管理员），`SHChangeNotify` 即时刷新，sentinel 防安装器覆盖。Win11 显示于“显示更多选项”。
- **三平台打包** — `.AppImage` / `.dmg` / `.exe (NSIS 安装器)` 构建就绪

### 10.1 Windows 右键菜单运行时管理（当前开发分支，待发布/多选支持）

右键菜单从安装器一次性写入升级为运行时动态管理：

- **总开关** — 设置页 `shell_menu_enabled` 控制整体显示/隐藏。
- **四项动词** — `Extract here`（解压到当前文件夹）、`Extract to...`（解压到…）、`Compress as ZIP`（压缩为 ZIP）、`Compress as...`（压缩为…），可独立开关。
- **注册表位置** — `HKCU\Software\Classes`（每用户，无需管理员权限），与安装器 hooks.nsi 一致。
- **即时生效** — 保存后通过 `SHChangeNotify(SHCNE_ASSOCCHANGED)` 刷新 Explorer。
- **Sentinel 保护** — 写入 `HKCU\Software\Classes\GeeZipX\ShellMenu\Configured=1`，安装器升级时检测到则跳过默认注册，保留用户选择。
- **COM DelegateExecute 多选**（当前开发分支）— 压缩动词注册于 `AllFilesystemObjects` 父键（文件/目录/混合多选均支持），父子均声明 `MultiSelectModel=Player`。使用 COM `DelegateExecute` 而非静态 `"%*"` 命令——`%*` 在实机测试中确认传递空参数列表，不可作为支持方案。两个稳定 CLSID 通过 `LocalServer32` 指向同一 `geezipx-gui.exe`；Explorer 启动时传入 `-Embedding`，程序跳过 Tauri 以 COM 服务身份运行，实现 `IExecuteCommand` + `IObjectWithSelection` 接收 `IShellItemArray`。选中路径写入版本化二进制 action 文件（`.gzsa`，UTF-16LE，限 `%LOCALAPPDATA%\GeeZipX\ShellActions` 目录，上限 1 MiB / 10 000条 / 32 767 码元），正常 GUI 通过 `--shell-action-file` 参数读取。提取动词仍保持 `SystemFileAssociations` + `"%1"` 静态命令。
- **验证状态** — Linux 协议级单元测试通过（action 编解码、resolve 优先级、CLSID 映射、Embedding 检测）；原生 Windows COM 编译/NSIS 安装/Explorer 右键实测尚未完成。禁止 cfg stub 冒充 Windows 通过 CI。
- **Win11 限制** — 传统菜单项显示于"显示更多选项"(Shift+F10)，这是系统限制，需要 IExplorerCommand COM server / MSIX 包标识才能进入一级菜单。
- **跨平台** — 非 Windows 平台返回 `supported: false`，前端调用无报错。

Rust 命令：`get_shell_menu_state` → `ShellMenuState`、`set_shell_menu(enabled, verbs)`

### 10.2 `list_archive` 流式推送（v0.7.0）

大型归档列表通过 Tauri IPC `Channel` 分批推送，避免一次性阻塞：

- **后端**（`commands/list.rs`）：每批 500 条（`STREAM_CHUNK_SIZE = 500`），发送 `EntryChunk { entries, chunk_index, total_chunks, total_entries }`
- **前端**（`archiveStore.svelte.ts`）：`listArchiveStream()` 接收每个 chunk 渐进追加到 `entries`，条目到达即可渲染
- **效果**：数万条条目的归档即时展示，零等待，UI 无卡顿

```rust
const STREAM_CHUNK_SIZE: usize = 500;

pub struct EntryChunk {
    pub entries: Vec<EntryInfo>,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub total_entries: usize,
}

#[tauri::command]
pub async fn list_archive_stream(
    archive_path: String,
    password: Option<String>,
    on_chunk: Channel<EntryChunk>,
) -> Result<(), String>;
```

```typescript
// 前端调用
export async function listArchiveStream(
  archivePath: string,
  onChunk: (chunk: EntryChunk) => void,
  password?: string,
): Promise<void> {
  const channel = new Channel<EntryChunk>();
  channel.onmessage = onChunk;
  await invoke("list_archive_stream", { archivePath, password, onChunk: channel });
}
```

## 11. 设置页面架构

Svelte 5 runes 模式，5 标签页，`tauri-plugin-store` 持久化到 `settings.json`。

### 11.1 文件结构

```text
crates/gui-tauri/src/
├── pages/SettingsPage.svelte        # 主页面
├── stores/settingsStore.svelte.ts   # 读写 + 默认值
├── stores/settingsGuard.svelte.ts   # 未保存导航保护
├── bridge.ts                        # GeeZipXSettings 类型
└── i18n/locales/{en,zh-CN}.json     # i18n key
```

后端辅助命令：`get_formats`、`get_file_associations` / `set_file_association` / `open_association_settings`、`get_version`。

### 11.2 标签页

| 标签页 | 设置项 |
| **通用** | locale（`'en' \| 'zh-CN'`）、默认输出目录、覆盖策略、完成后行为 |
| **压缩** | 默认格式、压缩级别、递归 |
| **外观** | 主题（`system \| light \| dark`），即时 `data-theme` 切换 |
| **文件关联** | 格式扩展名列表 + 绑定状态；macOS 复选框直绑，Windows 引导跳转系统设置 |
| **关于** | 应用名 + 动态版本号、技术栈、GitHub 链接 |

### 11.3 GeeZipXSettings 类型

```typescript
export interface GeeZipXSettings {
  locale: 'en' | 'zh-CN';
  default_output_dir: string | null;
  overwrite_strategy: 'prompt' | 'skip' | 'overwrite';
  default_format: string;
  default_level: number | null;
  recursive: boolean;
  theme: 'system' | 'light' | 'dark';
  on_complete: 'nothing' | 'open_output';
  shell_menu_enabled: boolean;
  shell_menu_verbs: ShellMenuVerb[];  // 'extract' | 'extract_here' | 'compress_zip' | 'compress'
}
```

### 11.4 数据流

```text
SettingsPage ― loadAll()/saveAll() ──► tauri-plugin-store (settings.json)
    └── dirty 检测 ( $derived ) → settingsGuard → TabBar 拦截导航
    └── 保存后即时应用 theme + locale

CompressPage / ExtractControls ── settingsStore.get(key) ──► 预填表单
```

各设置项在 CompressPage（压缩参数）、ExtractControls（解压参数）、App.svelte（快速压缩路径）中按需读取。
