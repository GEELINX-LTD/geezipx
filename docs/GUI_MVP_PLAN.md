# GeeZipX GUI MVP 规划

> **阶段**：Phase 2（当前开发阶段）
>
> **前置依赖**：Phase 1 CLI 已完成并成熟，core 引擎库 API 稳定，crates.io 已发布。

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
- 7z 写入 / RAR 创建
- 文件管理器集成（双面板、标签页等）
- 批量任务队列（仅单次任务，后续可扩展）
- 多语言 i18n（仅英文初始版）
- 深色/浅色主题切换（沿用系统主题）

## 3. MVP 功能范围

### 3.1 支持的格式

| 操作 | 格式 |
|------|------|
| 压缩 | ZIP、TAR、TAR.GZ、GZIP、ZSTD、TAR.ZST、XZ、TAR.XZ、LZMA |
| 解压缩 | 上述所有格式 + 7z 只读 + RAR 只读 |

### 3.2 核心功能

1. **文件选择**
   - 系统原生文件对话框（单文件/多文件/文件夹）
   - 拖拽文件/文件夹到窗口

2. **压缩任务**
   - 选择目标格式（dropdown/radio group）
   - 配置压缩级别（通过 slider 或 dropdown）
   - 支持密码（AES-256 for ZIP）
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

```
┌─────────────────────────────────────┐
│        Tauri Frontend (WebView)      │
│  Vue 3 / Svelte 5 SPA               │
│  - 文件选择交互                       │
│  - 格式/参数 UI                      │
│  - 进度监听 & 实时显示                │
│  - 任务状态管理                       │
└──────────────┬──────────────────────┘
               │ Tauri IPC (invoke / event)
┌──────────────▼──────────────────────┐
│      Tauri Rust Backend (thin)       │
│  #[tauri::command] 桥接层            │
│  - compress_cmd(args) -> task_id     │
│  - decompress_cmd(args) -> task_id   │
│  - list_archive(path) -> contents    │
│  - cancel_task(task_id)              │
└──────────────┬──────────────────────┘
               │ Rust API calls
┌──────────────▼──────────────────────┐
│        geezipx-core 引擎库            │
│  archive/compress/detect/fs/         │
│  progress/pipeline/task              │
│  - 纯业务逻辑，不依赖 Tauri          │
└─────────────────────────────────────┘
```

**关键约束**：
- `geezipx-gui` 依赖 `geezipx-core`，反之不成立。
- core 引擎不引入任何 Tauri 依赖。
- GUI Rust 后端仅做参数映射、进度桥接、任务生命周期管理，不实现压缩逻辑。
- 前端不直接调用压缩库——所有操作通过 Tauri command bridge 转发。

## 5. Core API 复用策略

GUI 直接复用 core 的以下 API，不重复实现：

| Core 模块 | 复用方式 |
|-----------|---------|
| `core::task::CompressTask` / `DecompressTask` | GUI 构造任务结构体，传递参数 |
| `core::progress::Event::Progress { current, total }` | Tauri event emit 转发进度 |
| `core::pipeline::Pipeline` | GUI 触发运行，监听取消信号 |
| `core::detect::detect_format` | 自动识别拖入文件的格式 |
| `core::archive::ZipArchive` / `TarArchive` / ... | list 预览内容 |
| `core::error::Error` | 序列化为字符串展示给用户 |

### 5.1 Tauri Command Bridge 设计

```rust
// gui-tauri/src-tauri/src/commands.rs

#[tauri::command]
async fn compress(
    app: AppHandle,
    files: Vec<String>,
    format: String,       // "zip" | "tar" | "tar.gz" | ...
    level: Option<u32>,
    password: Option<String>,
    output_dir: Option<String>,
    jobs: Option<u32>,
) -> Result<String, String> {
    // 1. 创建 CompressTask
    // 2. 在 tokio spawn_blocking 中执行
    // 3. 通过 app.emit("progress", event) 推送进度
    // 4. 返回 task_id
}

#[tauri::command]
async fn decompress(
    app: AppHandle,
    archive: String,
    output_dir: Option<String>,
    password: Option<String>,
    overwrite: Option<bool>,
) -> Result<String, String> {
    // 类似 compress
}

#[tauri::command]
async fn list_archive(path: String, password: Option<String>) -> Result<Vec<EntryInfo>, String> {
    // 调用 core detect + list
}

#[tauri::command]
async fn cancel_task(task_id: String) -> Result<(), String> {
    // 设置取消标志
}
```

## 6. 进度与取消

### 6.1 进度推送

core 的 `ProgressEvent` 通过 channel 或 callback 暴露给 GUI 后端：

```rust
// core 侧
pub enum ProgressEvent {
    Started { total_bytes: u64 },
    Progress { current: u64, total: u64, rate: f64 },
    Message { text: String },
    Finished { result: Result<(), Error> },
}
```

GUI 后端在 `spawn_blocking` 中监听 core 的 channel，并通过 `app.emit("task:progress", payload)` 推送到前端。

### 6.2 取消机制

```rust
// core 侧已有
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn is_cancelled(&self) -> bool { ... }
}

// GUI 后端
#[tauri::command]
async fn cancel_task(task_id: String, state: State<'_, TaskState>) -> Result<(), String> {
    state.cancel_tokens.lock().unwrap().remove(&task_id);
    // 设置 token。core pipeline 检查 is_cancelled() 后提前返回。
}
```

### 6.3 前端事件监听

```typescript
// 前端 (TypeScript)
import { listen } from '@tauri-apps/api/event';

listen<ProgressPayload>('task:progress', (event) => {
    const { current, total, rate } = event.payload;
    progress.value = current / total;
    speed.value = formatSpeed(rate);
});
```

## 7. 密码处理

- 仅在需要密码的格式（ZIP AES-256、7z、RAR）展示密码输入框。
- 密码通过 Tauri invoke 以 string 参数传入 Rust 后端。
- 不持久化密码；每次任务独立输入。
- 提供"显示密码"切换（明文/遮掩切换）。

## 8. 覆盖策略

- 解压时默认检查目标文件是否存在。
- 发现冲突时弹出对话框提供选项：
  - 跳过（默认）
  - 覆盖
  - 全部跳过 / 全部覆盖
- core 的 `ClobberStrategy` 枚举已实现该逻辑，GUI 直接复用。

## 9. 平台原生打包

| 平台 | 格式 | 工具 |
|------|------|------|
| macOS | .dmg | tauri-bundler + create-dmg |
| Linux | .AppImage / .deb | tauri-bundler / linuxdeploy |
| Windows | .msi / .exe | tauri-bundler / NSIS |

> 打包配置在 Tauri 项目 `tauri.conf.json` 中声明。CI release workflow 为每个 tag 构建三平台安装包并发布到 GitHub Releases。

## 10. 首批任务拆分

### G1：项目骨架与 Tauri 集成

- [x] 调研结论：Tauri v2 + Vue 3 / Svelte 5 均可选用
- [ ] 创建 `gui-tauri/` workspace member
- [ ] 初始化 Tauri + Vue 3 项目模板
- [ ] 添加 `geezipx-core` 依赖到 `gui-tauri/Cargo.toml`
- [ ] 验证 `cargo build -p geezipx-gui` 通过
- [ ] 验证 Tauri dev 模式启动正常

### G2：Core 引擎桥接

- [ ] 实现 `compress_cmd` Tauri command
- [ ] 实现 `decompress_cmd` Tauri command
- [ ] 实现 `list_archive` Tauri command
- [ ] 实现 `cancel_task` Tauri command
- [ ] 进度事件通过 `app.emit` 推送到前端
- [ ] 验证压缩/解压缩端到端可用

### G3：前端基础 UI

- [ ] 主窗口布局（header + 任务区 + 状态栏）
- [ ] 文件选择对话框 + 拖拽支持
- [ ] 格式选择器 + 压缩级别 slider
- [ ] 密码输入组件
- [ ] 解压预览列表
- [ ] 覆盖策略对话框

### G4：进度与任务管理

- [ ] 实时进度条 + 速度/剩余时间显示
- [ ] 取消按钮（安全中断）
- [ ] 完成/错误通知
- [ ] 任务状态管理（前端 store）

### G5：平台打包与 CI

- [ ] `tauri.conf.json` 打包配置
- [ ] macOS .dmg 构建验证
- [ ] Linux .AppImage 构建验证
- [ ] Windows .msi 构建验证
- [ ] GitHub Actions release workflow（构建 + 上传三平台 artifacts）

### G6（MVP 后）：细节打磨

- [ ] 窗口状态持久化（位置、大小）
- [ ] 最近文件列表
- [ ] 拖拽时自动检测格式
- [ ] 性能优化（大文件列表预渲染等）
