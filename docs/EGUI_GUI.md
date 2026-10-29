# GeeZipX Native（egui 桌面端）

GeeZipX Native 是与 Tauri 桌面端并行的独立 Rust 应用，包名和可执行文件名为 `geezipx-egui`。它使用 egui/eframe 0.33.3、Glow/OpenGL 渲染器和系统文件对话框，直接调用 `geezipx-core`；构建和运行不需要 Node.js、Tauri、WebView2 或 WebKitGTK。

本阶段目标是形成可实际使用的原生归档工作空间，完成基础压缩、解压、浏览、预览、验证和取消操作。性能收益尚未做与 Tauri 的同机对照测量，不宣称固定的内存、启动时间或吞吐提升。

## 运行和构建

需要 Rust stable（workspace MSRV 为 1.96）。

```sh
# 开发运行
cargo run -p geezipx-egui

# 打开归档；也可传入多个文件、文件夹创建压缩包
cargo run -p geezipx-egui -- /path/to/archive.zip
cargo run -p geezipx-egui -- /path/to/folder /path/to/report.pdf

# 优化构建
cargo build -p geezipx-egui --release --locked
# Linux/macOS: target/release/geezipx-egui
# Windows: target/release/geezipx-egui.exe

# 可选：去掉 RAR 后端
cargo run -p geezipx-egui --no-default-features
```

Linux 运行需要桌面显示服务、可用的 OpenGL 驱动，以及提供文件选择器的 XDG Desktop Portal。构建不依赖 GTK/WebKitGTK 开发包。Linux 同时启用 X11 和 Wayland。macOS/Windows 使用系统原生文件对话框。中文字体内嵌，因此无需联网或预装中文字体；许可证及 egui 默认字体的版权/许可见 `crates/gui-egui/assets/*.txt`，也可在侧栏“关于 GeeZipX”查看。

根目录 `cargo build` / `cargo test` 默认只选择 core、CLI，保持无桌面依赖的命令行构建。原生 GUI 通过 `-p geezipx-egui` 显式选择；`--workspace` 仍会包含两个 GUI 包。

## 界面与操作

采用蓝紫色主操作、低对比背景、细边框和统一的圆角/间距。导航、线条图标、主/次按钮、格式选项、复选框、文件行和空状态均通过 egui Painter 自绘；共享浅/深色语义配色，保留悬停、禁用、键盘焦点和辅助功能信息。

左侧为固定导航，中间集中浏览和选择，归档打开后右侧提供解压目标与操作；新建页将来源列表和归档设置并排放置。文件与设置分别滚动，主要压缩/解压按钮固定可见，支持最小 920×620 窗口。

- **打开与浏览**：点击“打开归档”、拖入单个归档，或将归档路径作为启动参数。可按目录进入、通过面包屑返回、按名称搜索及排序；Windows 反斜杠条目与隐式文件夹会合并为同一目录树。最近打开最多保存八条路径。
- **选择与解压**：勾选文件或目录；目录选择包含后代，搜索和翻页不清除已有选择。支持全选当前列表、清空选择、Shift 连选。选中后主按钮变为“解压选中项”，同时保留“解压全部”。默认输出到归档旁的独立目录；可修改目标或使用系统目录选择器。
- **覆盖策略**：默认跳过同名目标；必须显式勾选才替换已有文件。新建归档已有同名输出时也需要单独勾选替换。错误或取消时不会把不完整临时文件替换为最终文件。
- **新建压缩包**：使用独立的“添加文件”“添加文件夹”按钮，或在新建页面拖入多个来源。保留每个顶层来源名称，自动消除重叠来源；拒绝来自不同目录的同名根条目。可选格式、速度/压缩率档位、保存位置；ZIP/7z 可设置并确认密码。
- **验证与预览**：完整性验证会读完整个归档并验证引擎提供的校验，不写入解压目录。文本预览最多保留 512 KiB，显示截断说明；双击目录进入、双击文本文件预览。不执行归档内的程序。
- **进度与取消**：底部显示任务摘要和取消按钮，任务中心显示当前条目、已处理大小、速度。任务中心记录本次启动的耗时、结果、错误详情，并提供“在文件管理器中显示”。退出时若仍有任务，会询问是否取消并等待工作线程清理。
- **主题与隐私**：侧栏可切换浅/深色外观。仅保存外观和最近路径；密码不序列化，最近路径不保存文件内容，可在侧栏清空最近记录。压缩密码提交后清空；读取密码仅用于当前打开归档的会话及用户明确发起的任务。

键盘：`Ctrl/Cmd+O` 打开、`Ctrl/Cmd+N` 新建、`Ctrl/Cmd+A` 选择当前目录、`Ctrl/Cmd+F` 搜索、`Backspace` 上一级、`Escape` 取消当前任务或清空选择。编辑文本时不抢占全选、退格等输入操作。

Linux release 在虚拟桌面实跑的界面截图：

![浅色归档工作区](images/egui-archive-light.png)

![深色新建压缩包页面](images/egui-create-dark.png)

## 格式范围

原生端有自己的适配器清单，core/CLI 支持不代表原生端自动支持；Tauri 的清单也不代表原生端的清单。

| 能力 | 格式 |
| --- | --- |
| 创建、浏览、解压、验证 | ZIP、7z、TAR、TAR.GZ、TAR.BZ2、TAR.BR、TAR.LZ4、TAR.XZ、TAR.ZST；启用 `zpaq` feature 后另含 ZPAQ |
| 浏览、解压、验证 | RAR（默认 feature）、CAB、ASAR、CPIO、DEB、LZH/LHA、ISO、UDF、ARJ、ACE、ARC |
| ZIP 别名 | zipx/jar/war/apk/ipa/xpi 等由 core 检测为 ZIP；不增加 WinZip 专有方法支持 |
| 未接入 | 单流 gz/bz2/br/lz4/xz/lzma/lz/zst、AES `.enc`、WIM、ISZ、SFX 创建、IMG/BIN、UU/XXE、ALZ、Unix `.Z` |

ZPAQ 是可选后端，默认关闭。其 C++ LTO 在部分 GCC/LLVM 组合上不兼容；需要该格式时可使用 `cargo run -p geezipx-egui --features zpaq`，release 构建在 Unix 上可能需要 `CXXFLAGS=-fno-lto cargo build -p geezipx-egui --release --features zpaq`。Windows MSVC 启用 ZPAQ 时需统一静态 C++ 运行库；PowerShell 中先设置 `$env:RUSTFLAGS="-C target-feature=+crt-static"` 再构建，与独立原生工作流一致。

只提供 ZIP/7z 加密创建及 ZIP/7z/RAR 密码读取。7z 的 solid、字典和文件名加密等高级创建选项仍通过 CLI 使用。遇到未接入格式时，界面提示使用 CLI。

## 性能和安全实现

- 压缩、扫描、归档索引、提取、验证、预览和文件对话框运行于工作线程。当前只运行一个后台操作，避免归档、密码、目标路径和取消状态互串。
- 文件列表按可见行绘制，不为所有条目创建界面组件；目录名称/大小顺序在后台预计算，行数据通过 `Arc` 共享；搜索只在条件变化时线性过滤当前目录的缓存，不复制名称、重新排序或访问文件系统。进度以共享快照和有限频率唤醒传递，不为每个读取块排队事件；空闲时不持续请求重绘。
- ZIP 与 TAR 系列通过 core 新增的 `visit_entries` 单次遍历；7z 沿 core 数据块顺序读取，保留跨所有块的停止语义。TAR 系列通过 `add_entry_with_size` 流式写入已知长度的来源，长度变化会报错。
- 预览使用有界写入器；完整验证只保留缓冲块，结果统计采用饱和加法。其余格式继承 core 的实现：部分后端仍会缓冲完整条目，ZPAQ 等并不具备与 ZIP/TAR 相同的内存边界；不能把原生界面等同于所有格式的流式保证。
- 解压先验证归档所有名称，使用 core `ExtractionDir` 安全遍历与临时文件提交；拒绝目录穿越、符号链接/重解析点目标、输入归档自覆盖及其硬链接别名。压缩拒绝把输出放入输入目录，跳过符号链接和特殊文件。
- 取消为协作式：常规流式读写在块边界检查；个别后端的目录读取、原子编码步骤及最终落盘阶段可能需要等引擎返回。取消不是撤销，已完成提取的文件会保留。

## 验证与交付

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings
cargo test --workspace --exclude geezipx-gui --all-features
cargo test -p geezipx-egui --no-default-features
cargo build -p geezipx-egui --release --locked
cargo deny check
```

`crates/gui-egui/tests/operations.rs` 覆盖全部创建格式的 Unicode/空目录往返、ZIP/7z 密码、选中目录边界、Windows 分隔符、覆盖/跳过、重复/重叠来源、目标别名、取消清理、损坏 ZIP、预览上限及 Unix 链接防护。`crates/core/tests/streaming_visitor.rs` 覆盖 TAR 流式输入和 7z 多块停止。

`crates/gui-egui/src/app/tests.rs` 另覆盖各页面、压缩格式、深浅色和不同窗口尺寸的真实 egui 布局，防止嵌套横向布局产生负宽度，并检查缩放保留选择。Linux release 实跑已验证文件夹多选提取、文本预览和创建 ZIP；创建结果另通过 Python ZIP CRC/内容检查。

独立 `.github/workflows/gui-egui.yml` 对 Linux x86_64、Windows x86_64、macOS arm64 执行检查、测试、release 编译并上传裸可执行文件与许可证。可手动触发；它不上传到正式 Release，也不接管 Tauri 的 AppImage/DMG/NSIS 安装包流程。

本阶段不含多标签并发、拖出文件、归档内重命名/删除、二进制预览、分卷创建、自动更新、文件关联/系统右键菜单注册和原生安装器。暂未提供英文界面、屏幕阅读器验收或完整 DPI/输入法兼容性报告；正式发版前需在 Windows/macOS/Linux 实机验证拖放、系统文件选择器、输入法和显示缩放。
