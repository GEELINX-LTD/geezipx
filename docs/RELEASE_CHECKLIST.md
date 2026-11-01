# 原生桌面发布验收清单

本清单面向下一候选版本。当前正式版本为 v0.7.4；当前源码使用 egui/eframe 作为唯一 GUI，包名与可执行文件名为 `geezipx-egui`。历史版本的发布记录保留在 CHANGELOG 中，不作为当前候选已验收的证据。

每次验收记录完整源码 SHA、workflow run URL、系统版本与架构、产物 SHA-256、逐项结果及未完成项。编译、单元测试和安装器静默检查通过后，仍需完成对应平台的原生交互验收。

## 本地质量门禁

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test -p geezipx-egui --no-default-features --locked
cargo deny check
```

- [ ] 以上检查在候选提交通过，记录命令与结果。
- [ ] 使用优化构建验证实际文件操作：`cargo build -p geezipx-egui --release --locked`。Unix 设置 `CXXFLAGS=-fno-lto`；Windows 使用静态 CRT，详见 [原生端指南](EGUI_GUI.md)。
- [ ] 核对 core/CLI 格式子集、独立样本结果与暂缓项，见 [格式审查](FORMAT_PARITY_AUDIT.md)；GUI 格式能力见 [原生端格式表](EGUI_GUI.md#格式范围)。

## 三平台构建与产物

- [ ] 针对候选提交触发 `.github/workflows/gui-egui.yml`，确认 Linux x86_64、Windows x86_64、macOS arm64 的检查、测试、SFX stub 与 release 构建通过。
- [ ] 核对 `geezipx-egui-<platform>` artifact 中的可执行文件、许可证及 `docs/EGUI_GUI.md`。
- [ ] 核对 `geezipx-egui-<platform>-release` artifact 中的原生程序压缩包与原始 SHA-256；解包确认程序、LICENSE、NOTICE、字体许可和提交标识完整。
- [ ] Windows 安装包 artifact `geezipx-egui-windows-x86_64-installer` 包含非空 `GeeZipX-Native_<version>_x64-setup.exe`、`SHA256SUMS.txt` 与 `build-info.json`，提交 SHA 与候选一致。
- [ ] macOS GUI 为 arm64，内嵌自解压 stub 仍为 x86_64；将二者的测试范围分别记录，不把 GUI 架构作为 SFX 架构证据。

## Windows 安装与卸载

- [ ] 安装器简体中文/英文可用，按用户安装无需管理员权限；默认目录为 `%LOCALAPPDATA%\Programs\GeeZipX Native`。
- [ ] 在含中文与空格的安装目录运行，确认应用哈希、LICENSE、字体许可、提交标识、开始菜单入口和卸载项正确。
- [ ] 通过开始菜单或 EXE 启动，打开 Unicode 路径归档；升级后仍可启动并保留外观与最近路径。
- [ ] “工具 → 设置… → 系统集成”按所选格式注册 Open With/默认应用，手动在 Windows 默认应用确认后双击打开；注册/移除不写受保护的 `UserChoice`，保留其他应用的 OpenWith 值。
- [ ] Windows 11 一级右键菜单显示“GeeZipX”子菜单；单个归档提供压缩及三种解压操作，普通文件/目录和多选只提供适用的压缩操作。Windows 10 与“显示更多选项”保留五种传统入口；中文/空格路径多选压缩生成一个含完整选区的归档，打开归档和三种解压目标正确，错误/密码/覆盖/取消可恢复。
- [ ] EXE、`geezipx_shell.dll` 和 `GeeZipXShell.msix` 同时交付，DLL 身份 Publisher 与 MSIX 及签名证书一致；测试包明确附带测试公钥，正式发布使用受信任代码签名证书且不自动导入信任。
- [ ] 已启用集成的升级修复指向新的安装路径，原位升级在替换 DLL 前注销旧身份包；卸载移除当前用户身份包、自身 ProgID、Capabilities、Shell 设置、静态菜单和 COM CLSID，保留其它应用注册、用户偏好及用户自行放入的文件。

工作流增加了静默安装/原位升级/卸载、注册所有格式、传统及打包 COM 双选生成 ZIP、一级菜单命令状态与真实解压、注册归属检查。上述自动检查随本次变更接入，不能视为已经完成 Windows 11 视觉验收；可交互安装、默认应用确认、一级菜单实际显示与系统拖放仍需 Windows 实机验证。

## 原生交互与归档操作

以下项目分别在 Windows、macOS、Linux 桌面记录结果：

- [ ] 默认及最小窗口、浅/深主题、高 DPI：标题栏拖动、双击最大化、还原、四边/四角缩放、关闭与任务退出确认正常；对话框和下拉选择不遮住主要按钮。
- [ ] 界面语言：简体中文默认，切换 English 立即生效且重启保留；旧偏好正常迁移，760×480 下菜单/设置/新建/解压可用，Unicode 文件名和路径保持原样。
- [ ] 中文字体与输入法：缺少系统中文字形时离线后备可用；搜索/选择字体、重启保留与已卸载字体回退正常。
- [ ] 打开归档、目录树、连续双击目录与 `..`、面包屑、返回/前进、搜索和排序正常；大归档列表滚动与选择响应可用。
- [ ] 鼠标框选、Ctrl/Cmd 切换、Shift 连选及键盘范围选择正常；选择性提取不误含同名前缀目录。
- [ ] 拖入文件/目录创建归档，ZIP/ZIPX/7z 密码与确认输入正确；Enter 提交、来源编辑、错误后重试正常，密码不持久化。
- [ ] 全部/选择性提取、默认跳过与明确覆盖、中文名称/空目录正常；错误密码、损坏输入和取消保留已有目标并清理临时文件。
- [ ] 单流、ISO/UDF/IMG/BIN/ISZ、只读格式与可用 SFX 目标在声明范围内通过真实文件验证，不把原样复制计为镜像内容提取。
- [ ] 文本预览上限、完整性验证、阶段进度、吞吐、取消、任务记录与完成通知正常；通知不移动列表布局。
- [ ] Windows 自主管理拖出：Explorer 当前目录空白处、子文件夹、桌面（空白处、隐藏图标、OneDrive/重定向、用户/公共桌面文件夹图标）、多个窗口/标签页及不同 DPI 均指向实际命中目标；虚拟/未知目标显示目录选择器。松手前、Esc、放回源窗口或取消选择时不解压、不创建目标。
- [ ] Windows 大 TAR 与压缩 TAR 拖出：只出现 GeeZipX 解压进度和取消，不启动 Explorer 复制任务；中文/空目录、多选去重、同名跳过、输入自覆盖保护正常。取消清理当前未完成文件，已完成文件保留；关闭等待任务清理。
- [ ] TAR/LZH/CPIO 大条目列表不读取 payload，重复提取/预览不重复列目录；单流缓存复用且源变化失效，测试重新校验来源，加密单流不延长解锁生命周期。
- [ ] macOS Finder 文件承诺、Linux X11/XWayland Xdnd：一次拖动在目标松手后才解压，取消或未接收时不读取归档；大文件、多选与中文目录内容一致。
- [ ] 系统集成设置在最小窗口和两种语言下可操作，读取/更新状态不冻结界面；Linux 验证 MIME 默认项、KDE 菜单及 Nautilus/Nemo 脚本，macOS 验证首次/再次双击启动器及 Finder 服务。关闭操作后进度和取消行为与界面内任务一致；移除仅恢复仍属于本应用的默认项并保留后来选择的其它程序。
- [ ] 原生文件选择器、OpenGL 驱动、输入法和显示缩放正常；Linux 同时验证 X11 与 Wayland 的常规窗口操作。

原生 Wayland 拖出、Linux/macOS 安装器、屏幕阅读器验收及其它未实现项见 [原生端限制](EGUI_GUI.md)。只读格式和部分格式变体继续受 core 限制，不把本清单计为全格式互操作保证。

## 发布演练与正式发布

- [ ] 使用 `.github/workflows/release.yml` 的 `workflow_dispatch`、`dry_run: true`，记录 run URL 与源码 SHA；确认它复用原生 GUI 工作流。
- [ ] 校验三份 CLI 压缩包、三份 egui 压缩包、Windows NSIS 安装器及各自 SHA-256，`scripts/verify-release-artifacts.sh` 成功生成 `SHA256SUMS`。当前 egui 产物是 Linux/macOS `.tar.gz` 与 Windows `.zip`/NSIS `.exe`。
- [ ] 下载 `geezipx-release-artifacts`，重新核对压缩包内可执行文件、许可证、提交标识及实际运行结果。
- [ ] 统一 workspace、core 依赖、原生 GUI 与支持库版本以及 `Cargo.lock`，将 CHANGELOG 内容归入正式版本与日期。
- [ ] 按需先发布 core 所依赖的独立支持库，再发布 core/CLI；流程见仓库发布约定。
- [ ] 获得发布授权后创建 `v*` tag；工作流生成 GitHub Release 草稿，人工核对文件与校验和后再发布。

记录具体候选的证据后再勾选；本清单不表示候选已经通过验收。剩余边界见 [技术架构](TECH_ARCHITECTURE.md#10-当前实现限制)。
