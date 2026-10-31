# Core/CLI 新实现专项验收

这是格式清单第一轮实现后的专项复查记录；兼容范围总表见 [FORMAT_PARITY_AUDIT.md](FORMAT_PARITY_AUDIT.md)。真实样本、独立工具和结构损坏回归分别记录，不把内部 round-trip 当作互操作证据。第二轮独立参考工具验收来自 Linux，包含隔离 Wine；第三轮补充了下述 x86-64 三平台原生 CLI 验收。

## EGG

独立样本取自 [alkegi/unegg tests/data](https://github.com/alkegi/unegg/tree/9929009ca4d010e8cd88bb796f7a1868162026d4/tests/data)，固定提交 `9929009ca4d010e8cd88bb796f7a1868162026d4`。上游明确将测试数据以 CC0 1.0 发布；原始归档由不同真实 archiver 生成，`posix.egg` 来自 Android archiver。提取长度、CRC 与上游独立记录对比，未用 GeeZipX 重新生成样本。

| 样本 | 字节数 | SHA-256 | 验收 |
|---|---:|---|---|
| store5.egg | 9777 | `6bae0f197a4132b570f65f010a18df552251d4421e0bb9c25af1d9b75964b073` | 5 项通过 |
| deflate5.egg | 4675 | `bc2711e1702b8fa185e60d04390c876f604e1a6672abf63f47a05ed8cab2bdfa` | 5 项通过 |
| bzip2_big.egg | 3915 | `3f947ad9445274d92c71e44df32b3991ffc1877b89de733feca8b8a4e04a0498` | 8,100,000 字节通过 |
| lzma5.egg | 4771 | `4e26a1507b1c74e3ae3c0bae3ea081b83d9d36cf01bb1c854c8cb0ba78d79933` | 5 项通过；修复无 EOS 块误拒绝 |
| mixed_store_deflate5.egg | 4670 | `8e643418502af5e15f7b956836921efd11e23ee1d1a8a7e5833672c4c873a821` | 5 项通过 |
| empty_noblock.egg | 74 | `999d7a9e3dab3e375a71996fef038e15fa3ceaf10a50215b5c8ea37d155f99d9` | 无块空文件通过 |
| posix.egg | 370 | `a83ff22dff2ad8fc627745afd3fe5fbf27d4320a1b48717670e07e0071db44c7` | POSIX 元数据和 3 项内容通过 |

修复与资源边界：

- LZMA 解码使用块的已知原始长度，接受真实 archiver 不附 EOS 的合法流，同时保留长度、CRC、输入消费和256 MiB字典内存限制。
- 索引按 seek 读取并跳过 payload，不按未验证 extra 长度分配；索引、名称扩展与块元数据设64 MiB累计预算。逐次读取仍有界，不把整个 EGG 载入内存。
- 校验全部范围、父 ID、循环、重复 ID/名称、缺失名称与文件/块长度；保留危险路径供共享安全路径层拒绝。
- Deflate 要求 StreamEnd 和声明的精确输入范围；BZip2/LZMA 也检查实际输入消费。输出经长度限制和逐块 CRC。
- 回归涵盖所有截断前缀、4 GiB 名称声明、循环/缺失父 ID、重复 ID、非 final Deflate、尾随垃圾、损坏 CRC。三份小型真实归档保留于 `crates/core/tests/fixtures/egg/`。

另构建独立 Go 解包器 [blurfx/unegg](https://github.com/blurfx/unegg/tree/53bd143885752d1c940bb318200773f792fe93fb)（固定提交 `53bd143885752d1c940bb318200773f792fe93fb`，Go 1.24.4）。六份 Store/Deflate/BZip2/LZMA/混合/空文件归档的22个文件，其相对路径、长度和SHA-256均与GeeZipX CLI相同。它对`posix.egg`返回`unknown signature 0x1ee922e5 in file extras`，故该样本只计入上游记录/本地回归验证，不计为独立工具互操作通过。该样本验证POSIX extra存在时的内容读取，不代表完整属性还原。

为便于复核，对每个文件建立`{"bytes":长度,"sha256":内容SHA256}`，以POSIX相对路径为键；`json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode('utf-8')`的SHA-256如下。忽略时间、权限和目录自身属性，仅比对文件树内容。

| 归档 | 文件数 | 两工具相同的文件树摘要 |
|---|---:|---|
| store5 / deflate5 / lzma5 / mixed_store_deflate5 | 每份5 | `b1433cb46f9d7159eb3ebffffa88384077b49e360bfadba4ff317a21ec4a44be` |
| bzip2_big | 1 | `3fa6cf07006fc7014823c330602e902ff11fd16c840c25d170dba111a4a48ffb` |
| empty_noblock | 1 | `f7bad21a20486d946585c387052333763f2daa9bae8f6f24eccdfb530d863ca2` |

不包含 AZO、solid、分卷和加密 EGG；没有扩大本轮声明范围。

## ISZ

使用官方 UltraISO 9.7.6.3829 trial，在隔离 Wine 中从本项目自有 `hello.txt`（36 字节）生成729字节的默认 zlib ISZ。再用 UltraISO `-bin2iso` 还原：GeeZipX 的53,248字节 ISO与官方输出逐字节一致，并通过7-Zip25.01的ISO校验；原始文件名和内容一致。

- ISZ SHA-256：`01747015aca4bca5d89ebf1578ea456adab97a82afaaeccd485a31e4b1a10994`。
- 官方 ISO SHA-256：`6cbbc1ef6290593355dab5cd3fa955d280f2cb98cea6a40b0a907d6b7a848db5`。
- 原始文件 SHA-256：`aa8ea632b219083c93ad46998bf3383d273d514f9807eb5127f6e8449e68c1b5`。
- fixture 和可重现命令见 `crates/core/tests/fixtures/isz/README.md`。仅存自有内容的归档，不分发官方工具。

专项复查复现原实现对“输出长度匹配、但非 final Deflate 块或缺失 Adler32”的 zlib 数据错误返回成功；新回归在修复前失败。流式还原与 seek 镜像视图现共用严格块解码，要求 StreamEnd、Adler32、精确输入/输出长度，输出预算不变，固定32KiB工作缓冲，不新增整镜像分配。所有ISZ专项测试和core Clippy通过。

本次独立样本覆盖单卷、未加密、默认zlib、Joliet ISO；真实BZip2、加密和分段ISZ仍需分别验收，不能由该样本推导。

## AES Crypt

官方 AES Crypt Linux CLI4.7.0与GeeZipX CLI对aescrypt-rs0.2.0-rc.18的v0/v1/v2全部63个向量（每版21项，明文0–257字节，含多组16字节边界）输出完全一致。向量来自所用依赖，独立证据是另行调用官方工具并逐字节核对，不能将依赖本身的测试当作独立生成。

官方4.7.0从自有30字节明文新生成v3归档，密码为`Unicode 密码 🔑`，两工具解密结果和原文一致。保留9份旧版本边界向量与官方v3作为离线回归；来源、许可、全部保留样本SHA-256和生成命令见`crates/core/tests/fixtures/aescrypt/README.md`。

- 回归验证每个旧版本的空输入、16/17字节明文、仅返回1字节的Read、错误密码、截断、破坏MAC，认证失败不向已有目标追加任何字节。官方v3验证UTF-8密码。全部AES专项测试和core Clippy通过。
- 通用Read入口将密文写入匿名临时文件，固定缓冲下先认证、再解密输出；不按密文长度分配。直接seek双遍API要求调用方保证两遍之间源不变化，已补API注释。
- v0–v2原格式的末块长度字节不在MAC内，此限制不能通过兼容解码器修复；v3长度受认证。旧`.enc`容器的全量缓冲仍是已知性能债务，本轮不混入容器格式迁移。

## Lzip / TLZ / LZMA Alone

复查复现旧Lzip依赖将空输入、残缺成员头、底层读取错误和尾随垃圾当作正常EOF；声明字典可在未验证预算时触发超大分配。新回归在修复前失败。现使用严格Lzip v1 framing与已有liblzma decoder，保持单遍流式、64KiB输入缓冲，要求完整EOS、CRC32、原始长度和成员长度，并验证后续每一成员头。无新增依赖或整输入缓冲。Alone和TLZ复用同一256MiB解码内存预算；合法但更大字典明确拒绝，后续如需放宽应提供可配置预算。

GNU Lzip1.25独立生成levels0/6/9，各262182字节明文以及拼接成员均与GeeZipX CLI输出逐字节一致；损坏尾部返回失败。保留403字节GNU fixture、来源和哈希于`crates/core/tests/fixtures/lzip/`。回归另含全部截断前缀、短Read/Interrupted、CRC、超大字典、失败后的重复读取；TLZ修复1字节短Read误识别外层。v0历史Lzip不在当前声明范围。

## IMG / BIN 光盘视图

回归复现两处问题：单次Read不足24字节时把raw CD误判为cooked；只验证第一扇区模式、后续混合模式/损坏同步头仍按同一偏移提取。现用read_exact取得探测前缀，每个被读取的raw扇区验证同步、模式及Mode2子头，直接seek到扇区中部也校验。缓存当前已验证扇区，使用已有BufReader相对定位；不缓存全盘或为每次短Read重复检查。

专项用例覆盖1字节Read、逐扇区和直接seek的坏头/混合模式，以及cooked、Mode1、Mode2 Form1的文件提取。未加入raw CD EDC/ECC验证；不支持的音轨、混合布局和CUE仍明确拒绝，不能宣称任意IMG/BIN文件系统支持。

## ALZ

原始回归复现缺BFINAL/EOS而长度和CRC匹配时被错误接受；已在`670bfe2`改为固定32KiB输入/输出缓冲、严格StreamEnd/输入区间/原始长度校验。再次复查发现名称索引的or_insert使重复名称及`a/txt`/`a\\txt`别名静默指向首项，导致第二项可能提取为第一项内容；新增双记录回归在修复前失败，现打开时明确拒绝该歧义。

单卷Store/Deflate范围内的截断、尾随数据、CRC和超长输出回归通过。真实加密ALZ互操作、modified BZip2/method3、分卷和非UTF-8名称仍未完成；当前依赖先完整收集目录，尚无统一累计索引预算，此处保留为资源边界后续项。

## CFB / MSI

源码复查确认cfb0.15会为每个物理扇区建立u32 FAT槽，即使只是稀疏尾部填充。Unix稀疏容器回归在修复前失败；现打开同一文件句柄、读取固定32字节头，并在依赖分配前验证扇区大小与64MiB FAT估算预算。该预算对应512字节扇区约8GiB、4096字节扇区约64GiB容器上限，超出返回明确错误；不改变按需流复制。预算不等于总进程内存上限，目录、miniFAT、路径和依赖辅助结构仍需后续统一计费。

覆盖V3/V4稀疏长度、嵌套storage、mini-stream及普通流；稀疏回归仅在Unix运行，避免Windows未设置稀疏属性时实际占用数十GiB空间。CFB自身不带内容CRC，不能将任意字节损坏都当作可检测错误；MSI数据库和CAB自动展开仍不在当前容器流提取范围。

## 其余复查与失败保护

- `.Z`：复查LZW引用、CLEAR、宽度切换及prefix链边界；9–16位字典上限65536项，prefix/suffix/stack共256KiB，输出64KiB。沿用ncompress独立fixture，未发现需要改动的新问题。原格式没有长度/CRC，不能可靠识别全部截断或内容损坏。
- TLZ：顺序visitor一次解码，提取/校验继续读完外层以验证尾部；`entries()`仍收集全部目录元数据，大目录统一预算是后续项。LZMA Alone共用256MiB预算，避免从未验证的字典参数直接大额分配。
- 共享CLI回归验证坏压缩流保留旧目标并清理新目标、错误AES密码不改目标、取消清理临时文件、部分归档提取失败返回失败状态。完整门禁执行了这些既有回归。

## 第二轮 Linux 门禁与待办

本轮代码/样本提交为`670bfe2`、`66cb7a7`、`cd0e31a`、`8221139`、`07931bc`、`c5b03dc`、`77cd3b4`、`5ea7fa6`。最终Linux工作区检查通过：

- `cargo fmt --all --check`。
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`。
- `cargo test --workspace --exclude geezipx-gui --all-features`：**1135通过、9忽略、0失败**；逐目标检查了实际执行数量。专项core命令使用`-p geezipx-core`，CLI包`-p geezipx`的零匹配测试不计为core验证。
- 重建`cargo build -p geezipx --all-features`后，用实际CLI完成独立EGG文件树比较；另外复核63份AES官方明文输出、ISZ官方ISO逐字节一致以及19个样本校验值。

本轮性能措施是有界流式I/O、固定缓冲、在分配前检查字典/FAT/索引预算、按扇区缓存和避免重复解码。未建立统一性能基线，不宣称速度提升百分比或总进程内存硬上限。

后续应分别处理：ALZ与CFB等累计元数据预算；现有整项/整包缓冲的创建和第三方读取路径；EGG加密/solid/分卷/AZO、ISZ加密/分段/真实BZip2及其它未覆盖变体。PEA、NSIS、DAA、BH的新增实现仍按总表中的独立任务推进。

## 第三轮：三平台原生候选验收

验证源码为 `8f5e9f53780ec5502efc9fda5bf8708a29288eb7`：

- [CI 41159081353](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159081353) 全部成功，包括三平台测试/Clippy、候选构建、原生CLI/SFX、独立interop、streaming-smoke、Rust文档及原有其它任务。
- [Audit 41159057553](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159057553) 同一源码提交成功。
- 配置/修复提交为 `2c68d47`、`62d0538`、`8f5e9f5`。第一轮远端Rust1.99 Clippy发现文本解码器`chunks_exact(4)`的新检查；改为`as_chunks::<4>().0`，本机Rust1.96和远端1.99均通过。宏验收增加UU/UUE/XXE实际字节往返。

| 原生系统 / 架构 | 最小配置：通过 / 忽略 | 基础配置：通过 / 忽略 | 扩展工作区配置：通过 / 忽略 | 实际CLI验收 |
|---|---:|---:|---:|---:|
| Ubuntu 24.04.5 LTS / x86-64 | 1014 / 4 | 1074 / 9 | 1135 / 9 | 39 / 39 |
| Windows Server 2025 10.0.26100 / x86-64 | 1006 / 4 | 1066 / 9 | 1127 / 9 | 39 / 39 |
| macOS 15.7.9 (24G830) / Intel x86-64 | 1014 / 4 | 1024 / 9 | 1122 / 9 | 39 / 39 |

最小配置为`cargo test --locked -p geezipx-core -p geezipx --no-default-features`。Linux/Windows基础配置使用默认feature，扩展工作区使用`--all-features`；macOS沿用LLVM/ZPAQ兼容限制，基础配置为`--no-default-features -F rar,sfx`，扩展配置及候选CLI为`--no-default-features -F rar,wim,sfx`。该结果不包含macOS的ZPAQ，也不覆盖其它CPU架构及Windows 10/11手工桌面验收。各feature组合分别运行，忽略用例不计为通过。

Runner image版本分别为`ubuntu-24.04 / 20261025.353.1`、`windows-2025-vs2026 / 20261021.278.1`、`macos-15 / 20260824.0482.1`。固定使用`macos-15-intel`，使产物所声明的x86-64、构建主机与嵌入SFX stub一致。

### 候选产物与可复核证据

`check-format-candidate.py`在每个系统中调用实际release CLI，检查ZIP/XZ method95、7z、TAR/TGZ、LZH/lh7、ISO、GZIP/XZ、UU/UUE/XXE；消费EGG/PMA/Unix Compress/GNU Lzip/UltraISO ISZ/AES Crypt离线fixture；覆盖TLZ、cooked IMG/BIN内容视图、合成ALZ/CFB、损坏流/错误密码目标保护，并创建和执行原生SFX。合成ALZ/CFB用例的边界与离线fixture来源见脚本说明及前文。

| CLI候选产物 | 字节数 | 可执行文件SHA-256 |
|---|---:|---|
| [geezipx-linux-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159081353/artifacts/13025959425) | 18123200 | `6247c42f9a973e8a9af342ad7706b853811b6280b43d01c99b2df76149c36af3` |
| [geezipx-windows-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159081353/artifacts/13027055442) | 13890560 | `2e58e6144daf5f041652283b3af0860ca5d21c1ac00e43b596cafd5bfdb2e070` |
| [geezipx-macos-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159081353/artifacts/13026818156) | 16390936 | `97e8319a7148fa8809c5bab57027562d7321453046e96a37b8e9ed12aca7ea23` |

每个CLI有同名`-acceptance`报告artifact，包含`candidate-report.json`，记录源码SHA、OS/架构、feature参数、CLI版本、可执行文件SHA-256和提取文件树哈希。六份artifact已下载，逐一验证源码SHA、ELF/Mach-O/PE x86-64架构、可执行文件与报告哈希匹配。三平台39项名称及提取文件哈希完全一致；下载后的Linux CLI在本机再次运行39项验收通过，结果与CI报告一致。

CLI仍显示`geezipx 0.7.4`，本轮未抬版本或发布；按完整源码SHA识别候选。Actions artifact保留7天。可在有效期内复核：

```bash
gh run download 41159081353 --dir candidate-8f5e9f5
chmod +x candidate-8f5e9f5/geezipx-linux-x86_64/geezipx
python3 scripts/check-format-candidate.py --binary candidate-8f5e9f5/geezipx-linux-x86_64/geezipx --sfx --report downloaded-linux-report.json
```

### ISO 创建性能观察

同一下载的Linux release候选包处理一个64MiB文件（`bytes(range(256))*256`的64KiB块重复1024次），用`/usr/bin/time`记录峰值RSS：

- 输入67,108,864字节；输出76,546,560字节（约73MiB）。
- 峰值RSS 205,936KiB（约201.1MiB）。
- PVD双端序卷大小字段均声明67,162,112字节（约64.05MiB）；输出还包含约8.95MiB预分配余量。

这是Linux、刚生成输入的热页缓存单例内存诊断。旧源码同时保留原始文件Vec、建树时的内容clone及估算长度输出Cursor，且回写整个输出Vec。该单例不构成统一吞吐基线或所有输入的RSS上限；修复与对照实测见下述第四轮。

## 第四轮：ISO 创建内存与兼容性修复

实现提交 `418cd013ee0fd5038bae4095653b2a27edac8513`。保留 core `Read` 输入 API，以磁盘暂存适配可寻址格式，不要求调用者一次性持有文件内容。

- 原文先以64KiB级缓冲写入一个匿名临时文件，输入树保存`FileSource`区间；建树/clone只复制句柄和元数据。格式化写入第二个缓冲临时文件，最后按实际长度顺序输出，消除整包Vec、内容clone和估算输出Vec。
- 新增独立MIT支持库`geezipx-iso 0.1.0`，保留`hadris_iso`依赖/库别名，原作者/许可与上游1.2.1清单保留。路径和版本同时声明；实际发布core前须先发布支持库，不能依赖仅本地生效的workspace patch。
- 修复真实文件末扇区只seek不写零的问题。输出长度与PVD两种端序的卷大小一致，不再带预分配余量。
- 目录记录不跨2048字节扇区；目录先于内容按广度布局，SUSP CE指向目录之后、文件内容之前的单扇区续接区。固定大小回填dot/dotdot、子目录和内容地址，保留Joliet Level3与Rock Ridge名称。
- 旧候选经7-Zip读取内容通过，但bsdtar返回`Invalid parameter in SUSP "CE" extension`；修复后两工具均通过。20个192字节长名、交替空/非空文件强制目录跨多个扇区，bsdtar保留完整RRIP名且所有内容哈希一致；7-Zip验证Joliet视图内容，超过其名称限制的长名按既有规则截短。

### 实测与资源范围

Linux、Rust1.96 release CLI；临时目录显式放在`/dev/sda1`，避开本机`/tmp`的tmpfs。输入是`bytes(range(256))*256`的64KiB块重复，刚生成后运行，用`/usr/bin/time -f '%M %e'`记录峰值RSS/墙钟。旧包来自上述`8f5e9f5`原生候选；新本地可执行文件SHA-256为`6270d2a90339a66b2e9d842d0d0cd74fc5ba324c345e34cd564725b6d6e0298a`，测试前后不变。

| 实现 | 输入字节 | ISO实际字节 | PVD声明字节 | 峰值RSS / KiB | 单次墙钟 / 秒 |
|---|---:|---:|---:|---:|---:|
| 旧候选 | 67108864 | 76546560 | 67162112 | 205936 | 0.06 |
| 本轮 | 67108864 | 67162112 | 67162112 | 8520 | 0.08 |
| 本轮 | 536870912 | 536924160 | 536924160 | 8500 | 0.59 |

64MiB对照中的峰值RSS降低约95.9%；输入从64MiB增至512MiB时仍约8.3MiB。64MiB内容SHA-256为`281e519df3077b557c6b03f5da83c4e8d397219259615dd7c3308f89cae8f2a6`，512MiB内容为`c047731a3c134f3d34286d608e9c173027d50f43ab9d2064f3c360939977e908`，独立提取后相同。以上是热页缓存单次内存诊断，不能据此推导统一吞吐提升或全部平台RSS上限。

暂存空间约等于原文加生成镜像；目录元数据内存仍随项数增长。`TMPDIR`/`TMP`/`TEMP`若指向tmpfs，临时文件仍占系统内存。最终输出仍为顺序Write，新增的是暂存磁盘I/O；本轮保留ISO创建属性默认值，不代表新增完整POSIX属性还原。

### 本地验证

- workspace fmt、Clippy全目标/全feature且`-D warnings`通过；workspace测试1141通过、9忽略、0失败。
- 支持库独立130项单元测试、全feature库检查及`cargo package`默认验证通过；排除于workspace的库单独验证，忽略项不计为通过。
- 新增6项回归覆盖拒绝大Read请求的生成流、失败读回滚、源截断、路径/重复项、输出/取消回调、PVD/物理长度、Unicode树和目录/CE扇区边界。扇区跨界断言在修复前确实失败。
- 7-Zip与bsdtar独立提取Unicode/嵌套/空目录、空文件、64/512MiB文件与多扇区长名集合通过；提取内容SHA-256一致。
- 实际release CLI/SFX候选39项通过；对已有目标执行SIGINT取消创建，返回非零且原内容保持不变，临时输出清理。

### 本轮原生矩阵

同一源码`418cd013ee0fd5038bae4095653b2a27edac8513`的[CI 41166748855](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166748855)和[Audit 41166752086](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166752086)全部成功。三平台Clippy与最小/基础/扩展测试实际执行，新增6项ISO回归包含在各配置中；候选构建重建目标SFX stub并执行实际CLI/SFX验收。

| 原生系统 / 架构 | 最小配置：通过 / 忽略 | 基础配置：通过 / 忽略 | 扩展工作区配置：通过 / 忽略 | 实际CLI验收 |
|---|---:|---:|---:|---:|
| Ubuntu / x86-64 | 1020 / 4 | 1080 / 9 | 1141 / 9 | 39 / 39 |
| Windows / x86-64 | 1012 / 4 | 1072 / 9 | 1133 / 9 | 39 / 39 |
| macOS / Intel x86-64 | 1020 / 4 | 1030 / 9 | 1128 / 9 | 39 / 39 |

feature配置与第三轮相同；macOS基础配置为RAR/SFX、扩展与CLI为RAR/WIM/SFX，不包含ZPAQ；其它架构、Windows桌面与GUI验收不能由此推导。实际系统分别为Ubuntu24.04.5 LTS、Windows Server2025 10.0.26100、macOS15.7.9 (24G830)；runner image分别为`ubuntu-24.04 / 20261025.353.1`、`windows-2025-vs2026 / 20261021.278.1`、`macos-15 / 20260824.0482.1`。

| 本轮CLI产物 | 字节数 | 可执行文件SHA-256 |
|---|---:|---|
| [geezipx-linux-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166748855/artifacts/13029469126) | 18147288 | `7296862590a8dd24b3cddc09af2e761d1825a28b1b26cd18d8a93537ff4b1408` |
| [geezipx-windows-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166748855/artifacts/13029492081) | 13955584 | `16c7ff275ec244e6b614568ef67b0eeb79bdcf233f69246d6dbcccf99a0a73df` |
| [geezipx-macos-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166748855/artifacts/13028809239) | 16434544 | `6d8e495f1e69f7d8faded9629d5fda39ab8a55bb53af3b83cc53fa688b9eb5f3` |

三份可执行文件与三份同名`-acceptance`报告已下载；逐一核对418cd01完整源码SHA、feature参数和二进制哈希。ELF64/Mach-O64/PE机器头均为x86-64。39项检查的名称、提取文件名/长度/SHA-256在三平台完全一致；下载的Linux CLI重新执行39项含新建SFX通过，报告与CI相同。CLI仍显示0.7.4，以源码SHA区分候选；Actions产物保留7天。

```bash
gh run download 41166748855 --dir candidate-418cd01
chmod +x candidate-418cd01/geezipx-linux-x86_64/geezipx
GITHUB_SHA=418cd013ee0fd5038bae4095653b2a27edac8513 GEEZIPX_CANDIDATE_FEATURES=--all-features python3 scripts/check-format-candidate.py --binary candidate-418cd01/geezipx-linux-x86_64/geezipx --sfx --report downloaded-linux-418cd01-report.json
```

## 第五轮：UDF 创建内存与互操作

实现提交`6bc8f88cc04dc12cff96ffbe779d78cdc60a685d`。移除原文Vec、树中内容clone、整镜像Cursor三份随载荷增长的分配，保留任意`Read`输入和顺序`Write`输出API。

- 输入以64KiB缓冲暂存到单个匿名文件，树仅保存`FileSource`共享句柄/区间及元数据。可seek镜像使用第二个临时文件，按格式化器返回扇区数设置实际长度，最后以有界缓冲顺序输出。
- 独立MIT支持库`geezipx-udf 0.1.0`保留`hadris_udf`依赖/库别名，上游1.2.1清单、作者与许可保留；显式版本+path，授权发布core前须先发布所需支持库版本。
- 按实际CS0编码长度分配目录FID，校正父FID信息长度和所在块地址，逐条写目录而非整目录缓冲。标准CS0/8按Latin-1读取，CS0/16按UTF-16BE读取，支持中文、重音及补充平面字符。超过255编码字节的名称在读取载荷前拒绝。
- 单文件数据拆为30-bit、按扇区对齐的多个短分配描述符，修复跨1GiB后把长度高位当作extent类型的问题；保持既有单项4GiB创建上限。布局算术检查溢出。
- 写入完整字符集、正确UDF revision、真实文件/目录计数及对应LVID CRC，补卷末锚点。默认仍为UDF1.02；同时纠正文档中旧的“2.01 write”描述。
- 旧版GeeZipX错误地将UTF-8放在CS0/8中。兼容解码只在旧`*hadris-udf` producer/空字符集标记下启用，避免将标准Latin-1中恰好也是合法UTF-8的字节误读。

### 内存、时间与临时空间

Linux、Rust1.96 release CLI，`TMPDIR`显式放在真实磁盘的`/root/.cache/geezipx-udf-validation/tmp`，`stat -f`为`ext2/ext3`；未使用本机`/tmp`的tmpfs。相同输入、相同命令以`/usr/bin/time -f '%M %e'`记录单次峰值RSS/墙钟。输入为64/512MiB稀疏文件，首部为固定ASCII标记、末字节为`Z`；测试前刚生成，属于热页缓存内存诊断。

基线是ISO优化后的`b1a9a2b`（与`418cd01`应用源码相同），可执行文件SHA-256为`6270d2a90339a66b2e9d842d0d0cd74fc5ba324c345e34cd564725b6d6e0298a`；本轮本地可执行文件为`4cd3939e41d93bcc7dd1a4d82eaab3367679628ff87acf19b6101c21d206ab79`。

| 实现 | 输入 / MiB | UDF实际字节 | 峰值RSS / KiB | 单次墙钟 / 秒 |
|---|---:|---:|---:|---:|
| 基线 | 64 | 67670016 | 205124 | 0.06 |
| 本轮 | 64 | 67672064 | 8224 | 0.08 |
| 基线 | 512 | 537432064 | 1581096 | 0.57 |
| 本轮 | 512 | 537434112 | 7968 | 0.60 |

512MiB输入下峰值RSS由约1.51GiB降至7.8MiB；64MiB输入下约200.3MiB降至8.0MiB。镜像比旧版增加2048字节，用于末尾锚点。这里没有证明吞吐提升或全部输入的固定RSS上限：新路径增加暂存磁盘I/O，目录元数据仍随项数增长，临时空间约为输入加镜像，tmpfs会占系统内存。

### 本地验证和独立样本

- workspace fmt、Clippy全目标/全feature且`-D warnings`通过；workspace测试1148通过、9忽略、0失败。独立支持库fmt、同步读写配置27项单测及`cargo package --features write`默认验证通过；未发布任何crate。
- 新增7项core回归覆盖拒绝大Read请求的生成流、精确镜像长度/非seek输出、Unicode和多扇区目录、失败读回滚、路径/重复项、源截断、输出失败及格式化阶段取消。候选脚本补UDF往返和复杂目录树，实际CLI/SFX本地41项通过。
- 7-Zip独立`test`/提取64MiB、512MiB、43文件复杂目录树通过，提取后SHA-256逐项一致；覆盖中文、重音、emoji、长名、空文件、空目录。`udfinfo`识别1.02、关闭完整性状态、两个锚点和准确43文件/5目录。
- 1GiB+17字节的实际文件跨30-bit extent边界，7-Zip和CLI分别校验/提取通过，长度和SHA-256均与原文一致；不是仅计算布局的模拟测试。
- genisoimage1.1.11独立生成的ISO/UDF混合镜像由`UdfReader`读取，中文、`café.txt`及CS0/8歧义名`Â©.txt`内容正确。另保留旧候选实际生成的中文命名镜像，确认兼容旧UTF-8编码。两个压缩fixture、生成命令、原文/镜像哈希见`crates/core/tests/fixtures/udf/README.md`。
- 实际CLI分别在输入暂存和镜像格式化阶段触发SIGINT，均退出130，已有目标字节保持不变，无临时文件泄漏；非法名称退出1并保护已有目标。

独立库额外执行`--all-features`检查失败，原因是继承的同步/异步生成类型及`UdfTimestamp::now`冲突；未经修改的上游1.2.1同样复现。core/CLI仅使用已验证的同步配置，workspace all-features不启用该库async功能。后续需分离共享描述符/时间类型与I/O绑定，逐个验证sync、async和合并组合；本轮将其明确保留为依赖技术债，不宣称完整async支持。读取的metadata/VAT/多分区范围、逐项目录导航债务也未在本轮扩展。

### 本轮原生矩阵与产物追溯

同一实现源码`6bc8f88cc04dc12cff96ffbe779d78cdc60a685d`的[CI 41172820828](https://github.com/GEELINX-LTD/geezipx/actions/runs/41172820828)与[Audit 41172824839](https://github.com/GEELINX-LTD/geezipx/actions/runs/41172824839)全部成功。各平台Clippy与最小/基础/扩展配置测试实际执行，候选CLI均重新构建并嵌入本机SFX stub，41项实际验收通过，新增UDF往返与复杂Unicode目录树。

| 原生系统 / 架构 | 最小配置：通过 / 忽略 | 基础配置：通过 / 忽略 | 扩展工作区配置：通过 / 忽略 | 实际CLI/SFX验收 |
|---|---:|---:|---:|---:|
| Ubuntu / x86-64 | 1027 / 4 | 1087 / 9 | 1148 / 9 | 41 / 41 |
| Windows / x86-64 | 1019 / 4 | 1079 / 9 | 1140 / 9 | 41 / 41 |
| macOS / Intel x86-64 | 1027 / 4 | 1037 / 9 | 1135 / 9 | 41 / 41 |

最小配置关闭默认feature；Linux/Windows基础配置使用默认feature、扩展与CLI使用all-features。macOS基础配置为RAR/SFX，扩展及CLI为RAR/WIM/SFX，不含ZPAQ。忽略项不计通过，未验证架构和GUI不在本轮范围。

| 本轮CLI产物 | 字节数 | 可执行文件SHA-256 |
|---|---:|---|
| [geezipx-linux-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41172820828/artifacts/13032152277) | 18197624 | `c52098477806ec176cfb98163919a9212b6939d311cd1985b4487affc4d1e65e` |
| [geezipx-windows-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41172820828/artifacts/13032337050) | 13963776 | `9c53487b012eb0f304b561cc9b9f7a5ec8f3211db0372ae15d22d0f2033c06ef` |
| [geezipx-macos-x86_64](https://github.com/GEELINX-LTD/geezipx/actions/runs/41172820828/artifacts/13032446939) | 16483592 | `8cca0ae71fd6ae32969869db647b4982e85484806029887278a68b543886e313` |

三份二进制和三份`-acceptance`报告已下载，逐一核对完整源码SHA、feature参数、实际ELF64/PE/Mach-O64机器类型及二进制哈希。三平台41项名称、提取文件名/长度/SHA-256完全一致；下载的Linux二进制再次执行全部41项（含新建SFX）通过，报告与CI一致。CLI仍显示0.7.4，以源码SHA区分候选，Actions产物保留7天。Linux下载ZIP也已核对GitHub artifact digest。

```bash
gh run download 41172820828 --dir candidate-6bc8f88
chmod +x candidate-6bc8f88/geezipx-linux-x86_64/geezipx
GITHUB_SHA=6bc8f88cc04dc12cff96ffbe779d78cdc60a685d GEEZIPX_CANDIDATE_FEATURES=--all-features python3 scripts/check-format-candidate.py --binary candidate-6bc8f88/geezipx-linux-x86_64/geezipx --sfx --report downloaded-linux-6bc8f88-report.json
```

## 第六轮：ASAR → LZH → ZPAQ → PEA → NSIS

按格式依次完成实现/性能复查与独立提交，最终候选统一执行三平台原生验收。当前先记录各项本地证据。

### ASAR：磁盘暂存创建与流式 integrity

- 创建改为64KiB读写缓冲、匿名文件暂存和仅包含元数据的树；按原文生成全文件与4MiB分块SHA-256，空文件/整块末尾空哈希与Electron一致。JSON与累计索引预算64MiB；临时空间约为原文大小，目录/校验块元数据仍随项数和文件长度增长。tmpfs会占系统内存。
- Electron官方`@electron/asar 3.4.1`双向创建/提取通过。Unicode（中文、重音、emoji）、空文件、空目录、恰好4MiB边界的整文件/分块integrity逐项一致；64/512MiB内容SHA-256与原文相同。
- 读取/`test`流式验证可选全文件/分块integrity，保留无integrity的旧包兼容，接受Electron末尾空块和Rust省略空块两种表示；拒绝错误块表和损坏载荷。CLI损坏test和强制覆盖提取均失败且已有文件不变。校验值来自归档本身，仅能检测不一致，不构成来源认证。
- 实际CLI输入暂存/输出阶段SIGINT均退出130，已有归档保持不变，无临时文件残留；单元回归含短Read、拒绝大请求的生成流、输入失败回滚、路径/重复冲突、输出失败、可选integrity与边界哈希。
- workspace fmt、全目标/全feature Clippy `-D warnings`、测试1153通过/9忽略及release构建通过。三平台结果在本轮最终候选生成后记录。

Linux Rust1.96 release，`TMPDIR=/root/.cache/geezipx-format-followup/asar/tmp`位于`ext2/ext3`真实磁盘。以相同64/512MiB新建稀疏输入、热页缓存、单次`/usr/bin/time -f '%M %e'`诊断内存，不是统一吞吐基线。基线为第五轮UDF代码，二进制SHA-256 `4cd3939e41d93bcc7dd1a4d82eaab3367679628ff87acf19b6101c21d206ab79`；本轮本地二进制 `e3538f928492e281ca15c6fed52997648ddfcbde33ccaecfe27f212352ffc6aa`。

| 实现 | 输入 / MiB | ASAR字节数 | 峰值RSS / KiB | 单次墙钟 / 秒 |
|---|---:|---:|---:|---:|
| 基线 | 64 | 67110152 | 204068 | 0.52 |
| 本轮 | 64 | 67110216 | 8072 | 0.50 |
| 基线 | 512 | 536879704 | 1580328 | 4.06 |
| 本轮 | 512 | 536879772 | 8288 | 3.96 |

512MiB输入峰值RSS约1.51GiB→8.1MiB；索引略增来自Electron兼容的末尾空块校验值。新路径增加暂存磁盘I/O，不据此承诺其它介质或输入的吞吐提升。
