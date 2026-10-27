# GeeZipX 代码复查与优化报告

本次针对 Rust core、CLI、Tauri 桥接与原生集成、Svelte 前端、SFX、构建和相关文档进行了分层复查。重点是能够复现的正确性、数据保护、取消和并发问题，以及有明确复杂度或内存收益的优化。变更保持 core 与 CLI/GUI 解耦；未更换归档架构或新增 GUI 压缩实现。

复查结合现有测试、修复前失败用例、真实子进程、损坏输入、独立格式工具和最终构建检查。通过自有 writer/reader 的 round-trip 不等同于格式兼容，SFX 源码通过测试也不等同于嵌入的可执行文件已更新。这两类问题在本次均有实际发现和处理。

## 已修复的问题

### 文件输出、取消和 CLI

| 问题 | 修复与验证 |
| --- | --- |
| 输出路径的文本比较未覆盖规范路径、递归输入内输出与 Unix 硬链接，提前创建输出可能截断输入 | 写入前检查输入/输出身份；输出使用同目录临时文件，编码成功后才发布；覆盖别名、硬链接和递归目录回归 |
| 压缩、单流解压或 GUI 压缩失败/取消时可能破坏已有输出 | CLI 共用 `PendingOutput`，GUI 压缩使用同目录暂存；错误和取消清理临时文件，保留既有目标；损坏 GZIP、错误密码和初始化失败均有回归 |
| Ctrl+C 标志初始化顺序导致信号处理器未安装；内部 `process::exit` 跳过清理 | 先初始化处理器，业务层返回取消，入口在析构后返回 130；真实 SIGINT 子进程测试 |
| 取消错误使用 `Interrupted`，被 `io::copy` / `read_to_end` 当作可重试中断 | I/O 错误携带可识别的 `GeeZipError::Cancelled`，跨错误转换保留取消语义；编码收尾与 flush 同样检查取消 |
| 解压条目错误只打印但返回成功；有效空归档被当作失败 | 有条目错误即返回失败，空归档允许成功；成功条目仍保留 |
| 单流解压、分卷解压可能覆盖自己的源文件 | 在实际写入前检查归档/各源分卷与输出路径冲突 |
| stdin 的格式分派不一致，密码和压缩数据争用同一个 stdin | 统一已有 core codec 调用，支持可用的 IMG/BIN/Lzip 流，校验密码适用范围，拒绝 `--stdin` 与 `--password-stdin` 冲突 |
| 递归输入跟随内部符号链接可能形成循环或包含非预期目录 | 收集输入时明确拒绝内部符号链接，在输出创建前报错 |
| 分卷创建覆盖旧卷或混合旧尾卷，错误时留下不完整的新集合 | 完整卷组先暂存，发布时拒绝覆盖已有数字卷，普通失败回滚本次已发布文件；支持 TAR/TAR.BR 分卷识别回归 |
| 格式/参数校验过晚，以及部分已实现格式遗漏 list/test/decompress 分派 | 提前校验 split/SFX/7z 参数；补齐可用的只读与单流分派、JSON 输出和密码验证 |

CLI 的大量重复单流分支已收敛为共享流程。归档编码、解码仍调用 core；CLI 不承担格式实现。

### 格式处理、分卷和 SFX

| 问题 | 修复与验证 |
| --- | --- |
| GZIP 仅处理第一个 member，忽略后续内容和校验失败 | 使用多 member 解码；覆盖连续 member、空 member、后续 CRC 损坏和截断 |
| AES 依赖接受未认证的流结束标志，可把 256 字节静默截断为 64 字节；畸形 nonce 会 panic | 在调用依赖前验证完整非流式容器，拒绝存在漏洞的 streaming schema；真实篡改样本、二进制/armor 和正常 AES/XChaCha 容器回归 |
| AES 头部、CBOR 递归、KDF 成本与密文长度缺乏实际边界 | 限制头部、递归、salt/nonce 长度、KDF 参数及组合成本；拒绝重复键、错算法、缺 tag、截断与尾随数据；`list`/`test` 同样完成认证 |
| ISZ 全量读取压缩输入，信任块几何和输出长度，末尾零块越界；4 MiB 写入块超出 22 位指针能力 | 解码按表项逐块读取并限制解码长度；验证边界和 codec 尾部；写入块改为 2 MiB；覆盖 level-0 全块与损坏表 |
| WIM 资源范围可读到相邻资源，长度运算下溢；压缩块表位置和含义错误 | 检查描述符和文件范围，按实际格式读取前置累计偏移，逐块限量解码；独立 XPRESS 样本验证 |
| WIM 目录偏移可循环/重复递归，畸形元数据被接受或导致溢出 | 检查安全块、名称、流、长度和偏移；跟踪访问集合，限制深度为 256；边界、循环和 FILETIME 回归 |
| WIM 自有 writer/reader 共用错误的 dentry 布局，round-trip 掩盖互操作失败 | 改为标准 102 字节固定 dentry、包含自身的长度、名称终止符、无名根目录和正确 part/chunk 元数据；保留旧 GeeZipX 无压缩布局读取兼容 |
| WIM 未检查元数据和文件内容 SHA-1，损坏数据可能通过 `test` | 元数据解析前核验资源摘要；提取时增量核验摘要；损坏内容和损坏元数据均有回归；旧版本全零 metadata hash 保持兼容 |
| WIM 写入保存所有文件的 `Vec<u8>`，手写 SHA-1 额外复制完整资源 | 文件数据改为临时磁盘 spool，64 KiB 读入并增量计算成熟库的 SHA-1；内存仅保留目录信息；覆盖失败输入回退、排序和大条目 |
| UU/XXE 解码静默接受截断行、非法字符和缺失 end | 严格检查头部、行长度、字母表和结束标记；规范 writer 的零长度行和 end 分隔；空内容、45/46 字节边界和畸形输入回归 |
| UDF 只列出根目录，嵌套查找按 basename 匹配，writer 将路径扁平化；提取反复复制整张映像并信任 extent 长度 | 正确递归读取、按完整路径查找和创建目录树；目录循环/深度检查；借用映像并按 extent 流式输出，检查图像范围、信息长度和分配描述符；嵌套同名文件、空目录、embedded/long allocation 和损坏 extent 回归 |
| UDF writer 把 formatter 的最终 seek 位置当作映像长度，截掉后续目录元数据 | 使用可增长的缓冲和 formatter 返回的扇区总数确定输出长度；嵌套空目录回归不再读取到截断映像 |
| 分卷 reader 空读改变位置、逐次求前缀和、负 seek 溢出，缺中间卷仍继续 | 空读无副作用；累计偏移一次构建，二分查找定位；检查有符号 seek、短卷和卷组缺口；迭代跨越空卷 |
| 分卷 writer 在后续卷失败时丢失已写字节数，错误重试跳卷或空写死循环 | 每次返回实际底层短写数量，成功打开后才提交卷索引，检查索引溢出；失败重试回归 |
| SFX 直接拼接条目路径，可越界写入；校验失败可破坏已有目标 | 路径校验加 capability 目录句柄，按条目暂存后发布；恶意路径、外部符号链接和 CRC 失败保留目标回归 |
| SFX 手工定位 ZIP 存在未检查减法、ZIP64 和整包内存问题；可执行程序内嵌 ZIP 签名干扰库的偏移扫描 | 流式拼接/读取 ZIP，校验 EOCD/ZIP64，并从计算后的载荷偏移构造 reader；实际生成的 Linux SFX 成功提取 |
| 修复源码后仍会嵌入旧 SFX 二进制，空 stub 也能被视作支持目标 | 更新 Linux 和 Windows 嵌入 stub；缺少/为空的目标明确拒绝；发布仍由各平台 workflow 构建本机 stub |

静态复查覆盖已接入的容器、压缩 TAR、单流 codec 和 legacy adapter；深度按手工解析、分配边界、完整性与路径风险分配。UU/XXE/UDF 的新增用例另与原始实现对照，五项安全回归在原始实现上均按预期失败。大 extent 样本没有在可能巨额分配的旧实现上运行。新增外部 WIM 样本与原生 SFX 执行用于补足内部 round-trip。

独立兼容性证据：生成的未压缩 WIM 经 `wimlib-imagex verify` 成功验证，提取内容与原文件逐字节相同。仓库新增由 wimlib 1.14.4 生成的 XPRESS 小样本及来源说明，测试覆盖 Unicode、空文件/目录、压缩块和内容摘要，避免只依赖本项目 writer。

### GUI、并发与原生集成

| 问题 | 修复与验证 |
| --- | --- |
| 重复任务 ID 覆盖取消目标，取消即移除 ID，旧任务结束误删新任务；panic/被丢弃 IPC 可泄漏注册 | `register_task` 拒绝空白和重复 ID；注册 RAII 由实际 blocking worker 持有，结束按 token 身份清理；取消只置位 |
| 文件预览将整个条目写入内存 | 仅保留最多 512 KiB，超限停止提取并区分取消和真实错误 |
| 选择条目时重复扫描选择集合 | 单次构建选择路径集合；对条目祖先执行查找，避免 entries × selections 的重复比较 |
| 新归档加载前无 tab/loading UI，关 tab 后残留 promise，旧 Channel/预览结果覆盖新状态 | 尽早返回 tab 元数据；使用存活 tab 去重、加载与预览代次隔离；真实 Svelte runtime 回归 |
| `$state.raw` 内普通 Map 原地变更不触发订阅，目录 getter 反复扫描 | 使用 `SvelteMap`；缓存目录和输出建议派生值；50,000 条目验证选择变化不重建目录结果 |
| 设置值未校验，异步保存读取被修改对象并交错写入 | 统一类型/枚举/压缩级别和路径校验；保存规范化快照，按调用顺序执行；单次失败不阻塞后续保存 |
| 设置输入未计入 dirty/save/cancel/reset；默认值晚到覆盖用户路径 | 同步输入和模型，保留手动选择，依赖设置未完成时禁用操作；销毁后忽略晚到回调 |
| 监听器晚注册泄漏，启动 promise 拒绝无人处理，界面永久 loading | 注册/卸载闭环、清理 debounce；启动、权限/能力、设置、版本和拖放错误走可见错误路径 |
| 入口扩展名不一致，About 版本陈旧，最近文件数据可破坏 keyed rows | 共用扩展名模块；从 backend 读取版本；校验、去重并限制最近记录；恢复主题/语言响应性 |
| 拖出暂存目录可被任意 cleanup 路径删除，同名并发拖动互相清理 | 随机私有暂存目录；清理仅接受 temp 根下专属命名空间的直接子目录，拒绝遍历/符号链接；前端失败也执行清理 |
| 次实例在转发前消费 shell action 文件；handoff 文件可能半写可见 | 在主实例存活后处理冷启动参数；同目录临时文件写完再发布，读取同一文件句柄并限制大小 |
| Windows 关联解绑用只读注册表句柄删除，Linux 恢复失败丢失备份 | Windows 写句柄和幂等清理；Linux 恢复失败保留备份供重试；Wine 注册表及 Linux helper 回归 |
| macOS 无法解除默认关联却报告成功 | 明确返回需要通过 Finder 选择替代应用的错误，界面不再收到虚假成功；macOS 原生执行仍待验收 |

Svelte 改动由专门代理完成，使用官方 MCP 文档和 autofixer；已检查文件均无 `issues`，有意保留的 imperative effect 建议未冒充为零建议。前端测试注入原生 API，不代替桌面验收。

## 质量检查

最终核验命令如下。测试基于完成格式化后的最终源码运行，具体结果见下表。

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings
cargo test --workspace --exclude geezipx-gui --all-features
cargo clippy -p geezipx-gui --all-targets --all-features -- -D warnings
cargo test -p geezipx-gui --all-features
cargo check -p geezipx --no-default-features

# crates/gui-tauri
npm test
npx tsc --noEmit
npm run build
```

| 检查 | 最终结果 |
| --- | --- |
| Workspace 格式检查 | 通过 |
| core/CLI/SFX Clippy，所有 target/feature，`-D warnings` | 通过 |
| 非 GUI workspace 测试 | 通过；core 674 项、CLI 335 项、SFX 7 项，另有通过的文档/辅助测试 |
| GUI Clippy，所有 target/feature，`-D warnings` | 通过 |
| GUI Rust 测试 | 183 项通过 |
| CLI `--no-default-features` 构建 | 通过 |
| 前端 Node 回归 | 66 项通过 |
| TypeScript 与 Svelte 生产构建 | 通过 |
| 独立 WIM 样本读写互操作、损坏摘要回归 | 通过 |
| Linux 实际 SFX 创建与执行、发布 shell 校验 | 通过 |
| `git diff --check` | 通过 |

原有标记为 ignored 的用例未强制运行；Windows/macOS 原生执行边界见后文。

CI 增加前端回归、TypeScript 检查和 Linux 最小 CLI feature 构建，避免新增测试仅在本地运行。发布流程另外修正：

- CLI/SFX 与 GUI 的 macOS `x86_64` 产物固定使用 `macos-15-intel`，避免 `macos-latest` 的 ARM runner 产出架构错标的文件。
- 各原生 CLI job 在打包前实际创建并执行 SFX，提取后比较原文件；不再把“文件存在”当作 SFX 验证。
- 汇总时检查归档和 `.sha256` 均非空并实际核验摘要，拒绝只有 checksum 的假成功；GUI 缺少要求的 bundle 也直接失败。
- `dry_run: true` 阻止 release 创建，包括从 tag 手动触发的场景。

workflow YAML 和 shell 语法已解析检查，原生 SFX 步骤在 Linux 实际运行通过；摘要脚本对正常、篡改、缺摘要、仅有摘要和空文件五类输入结果正确。现有 Windows GUI 测试流程已审查；本次未触发远端构建、发布或安装。

## 明确保留的限制与后续工作

| 优先级 | 项目 | 当前边界 |
| --- | --- | --- |
| 高 | Windows/macOS 桌面和安装验收 | Linux 已完成编译/测试；Wine 注册表与 Windows SFX 交叉构建不能证明 Explorer、NSIS、Finder、Launch Services 实际行为 |
| 高 | 单实例热启动交付 | 主实例建立但前端尚未订阅期间，直接 emit 仍可能丢事件；需要持久队列和确认协议；冷启动消费顺序修复没有覆盖此窗口 |
| 高 | WIM LZX/LZMS | LZX 的独立 wimlib 样本仍报解码错误；LZMS 未实现；不能承诺完整 WIM 支持。分割 SWM 与第一映像以外提取同样不支持 |
| 中 | 整体内存预算 | AES、ISZ 写入、UDF 映像读写、UU/XXE 和部分第三方适配器仍整体缓冲；WIM metadata/目录信息仍驻内存。当前变更不构成所有格式低于固定内存上限的保证 |
| 中 | AES 兼容边界 | 当前 writer 的非流式容器保持兼容；存在漏洞的 streaming 容器及超出 KDF 策略的外部容器明确拒绝。真正流式 AES 需要认证 framing 设计 |
| 中 | 注册表/系统关联恢复 | 多键注册尚无完整回滚；Linux 旧 handler 备份仅会话内有效，desktop ID 查找未完整遵循 XDG；安装关联表仍宽于 GUI 实际能力 |
| 中 | 组事务与断电持久性 | 多文件提取按条目发布；多卷发布不能整组原子提交。普通失败会清理，但不承诺进程强杀、断电或所有文件系统语义下的完整回滚 |
| 中 | 格式能力 | ALZ/Z 模块当前未接入构建；GUI 能力小于 CLI/core。文档已纠正，扩展名识别不代表每个界面可执行所有操作 |
| 后续 | 桌面性能和可访问性 | 缓存/复杂度优化已回归，但无桌面耗时或 RSS 对照基准；仍需超大归档交互、键盘焦点和原生拖放实测 |

本次没有把三方依赖的全部源码当作已完成的安全审计，也不把通过测试视为不存在其他缺陷。上述限制用于区分已经验证的修复与仍需专项实现/环境验证的工作。

## 变更入口

- core：`crates/core/src/io.rs`、`error.rs`、`archive/{mod,extraction,aes,gzip,isz}.rs`、`archive/wim/`、`volume.rs`、`sfx.rs`。
- CLI：`crates/cli/src/commands/`、`signal.rs`、入口与回归测试。
- GUI backend：`crates/gui-tauri/src-tauri/src/state.rs`、任务/预览/拖动/关联命令、`shell_action_file.rs`、`lib.rs`、`com_server.rs`。
- GUI frontend：归档/设置/app/locale stores，App、设置/压缩/提取/About 页面及相关组件，统一格式模块和 Node 回归。
- SFX：`crates/sfx-stub/` 与 `crates/core/stubs/` 的已更新目标。
- 文档与检查：README 中英文、AGENTS 中的能力状态、PRD、技术架构、GUI 计划、本报告和 `.github/workflows/{ci,release}.yml`。
