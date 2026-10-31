# Core/CLI 新实现专项验收

这是格式清单第一轮实现后的专项复查记录；兼容范围总表见 [FORMAT_PARITY_AUDIT.md](FORMAT_PARITY_AUDIT.md)。真实样本、独立工具和结构损坏回归分别记录，不把内部 round-trip 当作互操作证据。当前结果来自 Linux；Wine 中运行的外部工具不计作 Windows 原生验收。

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

## 最终验证与待办

本轮代码/样本提交为`670bfe2`、`66cb7a7`、`cd0e31a`、`8221139`、`07931bc`、`c5b03dc`、`77cd3b4`、`5ea7fa6`。最终Linux工作区检查通过：

- `cargo fmt --all --check`。
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`。
- `cargo test --workspace --exclude geezipx-gui --all-features`：**1135通过、9忽略、0失败**；逐目标检查了实际执行数量。专项core命令使用`-p geezipx-core`，CLI包`-p geezipx`的零匹配测试不计为core验证。
- 重建`cargo build -p geezipx --all-features`后，用实际CLI完成独立EGG文件树比较；另外复核63份AES官方明文输出、ISZ官方ISO逐字节一致以及19个样本校验值。

本轮性能措施是有界流式I/O、固定缓冲、在分配前检查字典/FAT/索引预算、按扇区缓存和避免重复解码。未测得统一吞吐或峰值RSS数据，不宣称速度提升百分比或总进程内存硬上限。

后续应分别处理：ALZ与CFB等累计元数据预算；现有整项/整包缓冲的创建和第三方读取路径；Windows/macOS原生构建与行为验证；EGG加密/solid/分卷/AZO、ISZ加密/分段/真实BZip2及其它未覆盖变体。PEA、NSIS、DAA、BH的新增实现仍按总表中的独立任务推进。
