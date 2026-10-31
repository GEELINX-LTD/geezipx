# Core/CLI 格式对齐审查

目标：按用户提供的 Bandizip 创建/提取清单逐个复核。每项完成实现、修复或阻碍分析后独立提交。GUI 接入另行处理；识别扩展名、原样复制不等于支持提取。未发布能力以本分支源码为准。

性能原则：优先有界缓冲的流式 I/O；可寻址容器优先磁盘暂存而非整文件驻内存；不以未校验的输出换取速度。编码器字典占用随压缩等级变化。专项测试后执行 workspace fmt/clippy/test 门禁；独立工具互操作与内部 round-trip 分开记录。

## 本轮结论与验证

用户清单中的创建与提取项已逐个完成一轮排查；有变化的实现、性能修复及暂缓分析分别提交。创建清单中的 ZIPX/XZ 已补齐，SFX 仍要求对应目标 stub；其它指定创建方法已核对。BH、DAA 1.0、PEA、NSIS 提取本轮暂缓，ALZ/EGG、MSI、光盘映像、WIM、SFX 和 ZIPX 的子集限制见逐项表；本轮完成不代表与 Bandizip 全变体兼容。

最终 Linux 本地检查：

- `cargo fmt --all --check` 通过。
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings` 通过。
- `cargo test --workspace --exclude geezipx-gui --all-features`：1119 通过、9 忽略、0 失败。
- `cargo test -p geezipx-core -p geezipx --no-default-features --locked`：998 通过、4 忽略、0 失败。
- `cargo deny check advisories bans licenses sources` 通过；重复依赖等警告按现有策略报告。

独立互操作包括 ZIPX/ZIP 与 7-Zip、`.Z` 与 ncompress、PMA 独立语料、ACE/ARJ 真实样本、RAR4/RAR5 与 bsdtar、WIM XPRESS 与 wimlib；详细范围在对应行记录。真实 EGG、UltraISO ISZ、AES Crypt v0–v2 独立样本及 Windows/macOS 原生验收仍待补，不能用合成结构或 Linux 测试替代。

性能改进集中在消除整包驻内存和重复解码；未建立新的统一吞吐基线。ISO/UDF/ASAR/LZH/ZPAQ 创建与部分第三方读取 API 等性能债务已保留具体原因和后续路线。

## 新实现专项验收（后续轮次）

- ALZ：复现旧实现接受缺少 BFINAL/流结束标记的 Deflate（原始长度与 CRC 均匹配，CLI `test` 错误返回成功）。改用固定 32 KiB 输入/输出缓冲，要求 `StreamEnd` 且精确消耗声明的压缩区间；写入前限制原始长度，打开时校验 payload 范围。新增非 final 块、尾随垃圾、输出越界回归；专项测试与 core Clippy 通过。没有通过忽略 CRC 或扩大缓冲绕过损坏检测。

- EGG：真实 `alkegi/unegg` CC0 样本暴露 LZMA 无 EOS 块被误判截断，已使用块的已知原始长度解码；独立 Store/Deflate/BZip2/LZMA/混合方法、Android POSIX 元数据、无块空文件共 7 份样本通过。新增 64 MiB 累计索引预算、范围和父引用校验，拒绝重复 ID/名称、循环/缺失父项、截断及尾随压缩数据；四项回归与 core Clippy 通过。保留 3 份最小真实样本及来源/许可；验收详情见 [专项验收](FORMAT_PARITY_ACCEPTANCE.md)。

- ISZ：官方 UltraISO9.7.6.3829生成的真实zlib ISZ通过；还原ISO与官方结果逐字节一致并通过7-Zip校验，已保留自有内容fixture。修复“非final/缺Adler32但输出长度匹配”的zlib误接受；流式和seek视图共用StreamEnd/精确输入长度验证。详见[专项验收](FORMAT_PARITY_ACCEPTANCE.md)。

## 逐项记录

| 项目（含别名） | 本轮结果 | 验证/限制 |
| --- | --- | --- |
| ZIP / ZIPX / XPI | 已完善 | ZIPX 使用 XZ method 95；ZIP 级别透传；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；专有 JPEG/WavPack 方法未覆盖 |
| 7Z / 7Z(LZMA2) | 已优化 | solid 按 64 MiB/1024 项阈值分块（单项可超阈值），单项 >8 MiB 暂存磁盘；加密保留自定义字典；jobs=0 自动识别；读取使用库的顺序提取避免逐项重解 solid |
| EXE / SFX | 已完善 ZIP SFX | 新增可寻址探测：MZ/ELF/Mach-O 前缀后验证 ZIP 中央目录，不将任意 EXE 当压缩包；CLI 自动 list/test/decompress。创建仍需 feature 和对应 stub；7z/RAR/NSIS SFX 待独立载荷路由 |
| TAR | 已复核 | 读写流式；`visit_entries` 单次顺序扫描，整包提取/校验不再逐项重扫。保留随机单项提取从头扫描的 API；GNU/PAX 路径与危险路径由 tar crate/共享提取层处理；现有专项回归通过 |
| TGZ / TAR.GZ | 已复核 | 已有 gzp 并行创建、多成员读取与单次顺序提取；访问流尾时 drain 以验证 gzip trailer；现有单/多线程、取消/损坏专项回归通过。单项 extract 提前返回不校验后续项，整包 test 才覆盖全流 |
| LZH / LHA / lh7 | 已优化 | 新增顺序 visitor，整包提取/校验避免每项从头扫描，文件 EOF 前验证 CRC16；同样覆盖 PMA。lh7 编码和现有回归通过。创建端仍逐项缓冲原文和压缩结果，后续需流式编码器 |
| ISO / Joliet | 已复核，创建性能债务暂缓 | Joliet Level 3 + Rock Ridge 已启用，读取缓存树并 seek 提取；现有 Unicode/嵌套/路径回归通过。hadris-iso 1.2.1 的 File::File 仅接受 Vec，当前创建整包驻内存且建树 clone；下一步应为依赖增加可寻址数据源或更换写入器，不能将整包缓冲归因于 ISO 格式本身 |
| GZ | 已复核并修复 | 多成员解码与每成员 trailer/CRC 校验已具备；修复非法压缩等级在编码前返回结构化错误。独立 GZ 的 jobs 当前不生效（TAR.GZ 已支持并行），后续需保持非 static writer API 兼容 |
| XZ / TXZ | 已优化 | `--jobs` 接入 liblzma MtStreamBuilder；默认单线程；TAR.XZ 各读取路径统一多流解码；无效选项返回错误 |
| ACE | 已修复列表、CRC、密码与 solid 前驱处理 | 列表显式跳过 payload，修复有效档报错；ACE CRC 元数据转为标准 CRC32；CLI 接受并在打开底层解码器前传递密码（补充 Blowfish 密码向量回归，避免 unified setter 仅更新 options 导致丢失）；单项提取先解码 solid 前驱以恢复字典，传播跳过错误。ACE v1/v2 真实样本 CLI test 通过。unarc-rs 0.8 解码 API 仍返回整项 Vec，逐项重新打开带来重复扫描，流式 writer API 需上游改造；不能将 read_to 包装误报为流式 |
| AES | 已接入 AES Crypt 读取 | aescrypt-rs 0.2.0-rc.18（锁定版本）支持 v0–v3；`.aes`/AES magic 路由，双遍验证后释放明文；通用非 seek API 将密文暂存磁盘，64 KiB 级缓冲。v3 内部测试通过；v0–v2 兼容由依赖实现，长度字节不受认证是原格式限制；原 `.enc` 仍非流式，拒绝以 `.aes` 创建旧容器 |
| ALZ | 部分实现 | 新增单卷 Store/Deflate 流式提取、CRC/长度校验、ZipCrypto 密码；索引缓存避免逐项重扫。修改版 BZip2 库需要整压缩项缓冲、method 3 以及分卷暂缓；路径需 UTF-8 |
| ARJ | 已修复列表与密码 | 列表显式 skip payload 并传播错误；CLI 在打开解码器前传递密码（Garble/GOST 由依赖处理），Garble 正确/缺失/错误密码回归覆盖 setter 未更新 backend 的缺陷；真实 method0–4 样本 CLI test 通过，损坏 CRC 样本失败符合预期；unarc-rs 返回整项 Vec 和逐项重复扫描仍为性能债务，分卷需 volume provider 尚未接入 |
| BH | 暂缓：规范及独立语料不足 | BlakHole/ZipTV 专有容器，未找到足以验证 header/codec/CRC 的可用解析器与独立样本；不能用ZIP别名替代。后续先补来源可追溯的 Store/压缩/损坏/Unicode 档及规范，按索引+有界Read实现，见下方阻碍记录 |
| BIN | 已实现光盘内容读取 | CLI list/test/decompress 改为解析 ISO9660/Joliet/Rock Ridge/UDF；支持 2048-byte cooked 和单轨 2352-byte Mode1/Mode2 Form1，缓存索引/按需 seek/64KiB 缓冲，拒绝未知文件。音轨/CUE/混合轨、分区/FAT 等磁盘布局暂缓，需多轨/文件系统适配；旧 core bin_decompress 仍为显式原始复制 API |
| BR | 已复核 | 单流64KiB解码缓冲/流式编码，等级0..11校验；现有空流/截断/级别/往返测试通过。无魔数/原生校验和，依赖扩展名，无法保证检测所有位翻转；默认quality11偏慢，性能敏感调用宜显式--level 4..6，保留默认兼容性；jobs 当前忽略，后续可按用户需求评估多线程编码器 |
| BZ / BZ2 / TBZ / TBZ2 | 已复核并修复 | BZIP2 多成员流式读取/CRC 已具备；增加 `.bz`/`--format bz` 别名与大小写输出名处理，修复独立编码 API 非法等级 panic。`.bz` 仅指 BZIP2 容器，旧 BZIP/BZ0 算法未实现；TAR.BZ2 顺序 visitor 已接入共享提取/校验路径；旧 infallible TAR writer 构造器仍要求调用者校验等级 |
| CAB | 已优化创建内存并修复空包 | 创建不再整包 Vec，单临时文件暂存/64KiB 缓冲，限制单项32-bit长度；空输入输出合法 CAB，纯空目录返回明确错误。3MiB生成流/空包及现有 Store/MSZIP/路径回归通过。读取通过 cab 0.6 支持单卷 Store/MSZIP/LZX，单项 read_file 仍重建 folder decoder，整包重复解压债务需上游开放 folder 顺序读取器；多卷未接入 |
| 化合物 / MSI | 已实现容器流提取 | cfb 0.15；识别 CFB magic/.msi/.msp/.mst/.cfb/.ole；目录/mini-stream/普通流使用 Read+Seek 按需复制，元数据驻内存。提取原始 OLE/MSI streams，不解释 MSI 数据库 File 表，不自动展开内嵌 CAB |
| DAA 1.0 | 暂缓：无已验证适配器/语料 | 可参考 GPL-2.0 daa2iso，但本项目 MIT，未直接移植；尚无独立 DAA1.0 chunk/损坏/密码/分卷语料验证。后续独立规范实现或隔离转换器需先定依赖/许可策略；必须有界块索引+缓存，转换 ISO 后再提供内容视图，不能仅识别 DAA 算支持 |
| DEB | 已优化创建内存及顺序提取 | 创建从整包原文/TAR/压缩载荷 Vec 改为磁盘暂存/64KiB 缓冲，控制信息仅小 Vec；读取 gzip/xz/zstd/bzip2/lzma/plain TAR 复用单次顺序 visitor，整包不再逐项重复解压并验证 codec trailer。现有6 codec/路径/空目录回归与新增 TAR 终止符之后 gzip CRC 损坏回归通过。创建仍经原文/TAR/压缩暂存多遍磁盘I/O，后续可将 TAR builder 直接连到编码器；只提取 data.tar 内容，不解释控制信息 |
| EGG | 部分实现 | 单卷非 solid 未加密 Store/Deflate/BZip2/LZMA 流式提取，逐块 CRC32/长度及流结束校验；LZMA 字典内存上限 256 MiB。自有 seek 索引解析设置64 MiB累计元数据预算及路径父引用检查，unegg仅复用类型/名称编码；真实7份独立样本验收通过，含无EOS LZMA和POSIX元数据。AZO/solid/分卷/加密暂缓 |
| IMG / IMA | 已实现光盘内容读取 | 与 BIN 复用 cooked/raw 光盘 reader，实际提取 ISO/UDF 文件，非光盘数据拒绝。FAT12/16/32、NTFS、分区盘、稀疏磁盘/多轨待补，不能宣称通用 IMG 支持；旧 core/CLI stdin copy API 保留显式透传 |
| ISZ | 已修复兼容性并优化内存/提取 | 按公开规范将 block_size 改为字节并接受 48-byte 头，兼容旧 GeeZipX 64-byte/sector 字段；创建使用 2MiB 输入块/磁盘暂存载荷和索引，Store/Zlib/Zero 自选。新增 Read+Seek 单块缓存（解压块/index 各最大64MiB），CLI list/decompress 直接提取 ISO 内文件，--stdout 仍还原 ISO，test 校验全部块。加密/分卷/非3-byte pointers 暂缓；真实 UltraISO 样本互操作待补；规范 https://forum.imgburn.com/topic/14260-isz-images-supporting/ |
| LZ / Lzip | 已复核并修复等级校验 | lzma-rust2 LzipReader/Writer 流式处理、CRC32与成员长度检查；新增0..9等级错误返回，避免preset静默裁剪/溢出；修正文档level0仍为压缩。新增真正CRC损坏回归，现有多级别/往返/空流测试通过。编码字典随等级分配；单流jobs暂不生效 |
| LZMA | 已复核并修复共享 API | liblzma Alone 编解码均流式，等级0仍压缩；补充 LZMA/XZ非法等级回归，并修复共享 XZ简化API对非法等级panic。现有 LZMA 空流/截断/级别/往返测试通过。Alone无原生CRC，无法保证检测全部位翻转；解码目前内存limit不限，后续应新增可配置memlimit而非悄然拒绝合法大字典；字典占用与文件体积无关 |
| PMA | 已实现读取 | 升级 delharc 0.8，启用 pm0/pm1/pm2；复用 LHA 流式读取与 CRC16 校验；独立 PM1/PM2 样本及 CRC32 验证通过；拒绝以 PMA 名义创建 LZH |
| RAR / RAR5 | 已优化顺序校验并修复空文件计数 | 新增直接 visitor，CLI test/顺序访问一次打开、保留 solid 状态，避免逐项重扫；修复两种 extract_all 空文件未计数。RAR4/RAR5 solid/Unicode 等5个独立 unrar fixture 与 bsdtar 提取逐文件SHA256一致；新增可移植 Store/空文件回归。unrar 0.5 writer callback 未公开，仍整项 Vec（大单项/取消延迟债务）；后续需安全 ReadToWriter upstream API，不能直接把 native extract_to 用于通用流以避免链接/路径语义变化；分卷/密码仍受库支持范围限制 |
| TLZ | 已实现读取 | `.tlz` / `.tar.lzma` / `.tar.lz` 支持 LZMA Alone 与 Lzip 两种外层，逐项访问单次流式解码，截断回归通过；不支持创建 |
| UDF | 已优化读取内存 | 移除 fs::read 全镜像；独立 64KiB 缓冲文件句柄导航目录/extent，目录元数据缓存，保留 ICB/分区/范围校验，8GiB 稀疏镜像小文件提取回归。仍仅覆盖当前 hadris 读取支持的 2048-byte 单物理分区/常规文件和短/长/embedded allocations；metadata/VAT/多分区、创建整包缓冲及逐项目录导航为后续债务 |
| WIM | 已复核，LZX/LZMS暂缓 | 资源按chunk范围解码、<=512KiB工作块、SHA1核验，37专项测试通过；独立 wimlib XPRESS两文件490003B通过。wimlib LZX样本仍复现 `encountered empty decode tree`，需核对 lzxd 的 LZXD头/状态与WIM变体差异，已修正模块能力说明；需WIM-specific LZX及E8处理/独立XPRESS-LZX-LZMS语料，不把内部roundtrip当互操作。LZMS、SWM、第一映像以外提取未实现；创建仅无压缩；元数据仍整体驻内存 |
| Z / Unix Compress | 已实现读取 | 流式 LZW，9–16 位有界字典；修复 `.Z` 大小写检测并加入魔数；ncompress 10/12/16 位共 15 例互操作通过；格式无校验和，不能保证发现所有截断 |
| ZPAQ | 已复核，流式改造暂缓 | 11专项测试通过；确认 zpaq_rs1.0.4 每次按路径提取都 fs::read 整包且返回整项Vec，创建同时驻留所有原文/压缩档，CLI整包逐项重复扫描。公共 decompress_stream 不暴露路径/日志最新版本选择，不能直接替换；下一步上游增加 archive-aware Read+Write/visitor，再实现磁盘暂存创建及一次顺序提取。当前UTF8路径/等级>5裁剪、非流式大包均为债务；不使用绕过共享路径校验的 native extract 命令替代 |
| PEA | 暂缓：容器/认证未实现 | 已查到 public-domain PEA1.6 规范，非“无开放规范”；Store/Deflate、对象/流/卷多层校验及AES/Twofish/Serpent认证模式需分别接入，不能以现有Deflate/AES代替。下一步先单卷未加密Store/Deflate+校验，独立PEA语料核验，再设计认证后输出 |
| UU / UUE | 已优化 | CLI 编解码改为逐行流式，固定 45 字节输入块，最大 1024 字节行；保留需要返回 Vec 的兼容 API；校验截断和终止符 |
| XXE | 已优化 | 复用有界逐行编解码器，CLI 不再整文件驻内存；截断回归测试通过 |
| CPIO（额外既有格式） | 已优化创建内存并修复长度截断 | newc/odc 创建最多保留1MiB项数据，超过后磁盘暂存，头部长度超过32-bit返回错误而非截断；odc 使用 reader API 避免重新读为 Vec。3MiB生成流/有界读取双格式回归及既有CLI/特殊文件/路径/往返测试通过。读取/校验原为流式，但整包提取逐项重扫仍有I/O债务；bin/crc变体暂未接入 |
| ARC / PAK（额外既有格式） | 已修复列表、密码和 CRC 元数据 | 列表显式 skip payload，支持 XOR 密码并在打开底层解码器前传入；CRC16 不再误标为 CRC32。带伪 header 的双文件/密码/损坏 CRC 回归及 CLI 加密 list/test/extract 通过；unarc 返回整项 Vec 和逐项重扫仍为债务 |
| ASAR | 已优化读取 | 原为每项 `fs::read` 整包且重建索引；现在只缓存 JSON 索引，seek + take 流式读 payload，保留 unpacked/符号链接路径校验；512 MiB 稀疏档与截断/恶意索引回归通过。索引上限 64 MiB；创建端仍缓存全部项，待改磁盘暂存 |
| ZSTD | 已复核并修复溢出 | libzstd 流式、多帧读取、可选多线程编码；新增0..22等级验证，阻止u32MAX转换成负fast等级，jobs转u32不再静默截断。现有多线程/截断/级别/往返及新增溢出回归通过。创建默认不启用可选内容校验和，输入无checksum时不能保证检测所有位翻转；字典/worker占用依设置变化 |
| NSIS | 暂缓：安装器载荷parser未实现 | 7-Zip NSIS handler 可作参考；需区分NSIS2/3、Unicode字符串/指令表、solid/non-solid及修改版Deflate/BZip2/LZMA，现有ZIP SFX探测不覆盖。下一步用makensis生成6 codec/solid组合基线并实现单次顺序解码/安全路径映射；不执行安装器代替提取 |
| LZ4 | 已复核 | lz4_flex FrameEncoder/Decoder 固定块缓冲/流式，无按文件体积Vec；现有往返/空流/无效数据测试通过。创建默认无内容校验和，依赖输入frame可选checksum（启用时库验证）；CLI level/jobs未映射LZ4，保留当前fast编码行为，后续若引入HC应单独测吞吐/内存；skippable frame/legacy LZ4专项互操作待补 |

## 共享顺序处理

- 共享提取和 `test` 入口使用 reader 声明的直接 visitor，TAR 各变体、LHA/PMA、ZIP 和 7z 不再逐项重新解码；随机访问 reader 保留原路径，避免无谓磁盘暂存。
- 补齐 EGG、Compound 在 CLI `test` 的路由；完整流访问保留解码器的尾部校验。
- CLI 帮助同步创建能力、XZ/7z 线程、solid 分块阈值及密码适用范围；格式错误提示包含新增及原有别名，避免把 CPIO 误报为只读。

## 暂缓格式的证据与后续路线

### BH / BlakHole

- 核对对象是 ZipTV 的 BlakHole `.bh`，不是同名的现代 BlackHole compression 项目；参考 [格式目录](http://fileformats.archiveteam.org/wiki/BlakHole)及 [ZipTV 格式说明](https://filext.com/file-extension/BH)。
- 本轮库/源码检索未取得能验证方法编号、文件头边界、CRC 和密码语义的适配器与独立样本。没有把 `.bh` 直接映射到 ZIP 或声称“无 Rust crate 就无法支持”。
- 下一步：取得可合法分发的 ZipTV 实际生成语料和可核对的布局；先实现单卷未加密 Store，再接具体 codec；限制名称/索引大小，按需 seek/take，独立校验 CRC/长度并与原工具比较文件哈希。没有这些证据前不合入猜测性的解码器。

### DAA 1.0

- 找到 [daa2iso](https://github.com/rsenn/daa2iso)参考实现：能够把 DAA 转为 ISO，公开仓库为 GPL-2.0；它是 C 转换器，不是现有 Rust archive reader。本项目声明 MIT，本轮未把其实现直接复制或链接进 core。
- 缺口是 DAA 1.0 容器头、块索引/偏移、压缩块及校验的实现和独立语料。参考转换器可帮助建立预期 ISO 字节，但不能把调用转换器当作已经完成跨平台底层实现。
- 下一步先取得 DAA1.0 标准样本、不可压缩块、尾块、损坏/截断样本；依据可用规范独立实现 Read+Seek 单块缓存，验证生成 ISO 哈希，再复用 ISO/UDF reader。分卷/密码另列能力范围。若选择接入 C 参考实现或外部转换器，需明确许可/分发及 Windows/macOS/Linux 构建策略。

### PEA / Pack Encrypt Authenticate

- [官方 PEA 1.6 格式说明](https://peazip.github.io/pea-file-format.html)声明规范为 public domain，压缩为 PCOMPRESS0 Store、PCOMPRESS1–3 Deflate；不能再以“格式封闭/没有 Rust 库”为永久排除原因。
- 其对象、流、卷层级分别携带可选校验/认证，支持多种 hash、AES/Twofish/Serpent EAX/HMAC 以及多卷；本项目尚无这套容器 parser、独立 PEA 生成语料或认证流边界实现。现有 `.enc`/`.aes`/Deflate codec 都不足以替代。
- 后续先限定单卷未加密 Store/Deflate，获取官方 Pea 可执行程序生成的空文件/目录/Unicode/多项/损坏校验语料，验证长度与所有声明的检查层。内存采用固定块，索引上限明确；对认证加密先验证再发布（必要时仅暂存密文），不能将未验证明文交给调用方。其它加密和多卷逐项扩展。

### NSIS 安装器

- 已找到 [7-Zip 的 NSIS handler](https://github.com/ip7z/7zip/tree/main/CPP/7zip/Archive/Nsis)作为可核对的载荷/指令解析参考；该处理器明确区分 Deflate/BZip2/LZMA 与 solid/non-solid。项目构建 NSIS 安装包的能力与读取安装器内部文件无关。
- 缺少的是 PE 后附载荷定位、NSIS2/3 及 Unicode 字符串/指令表解释、文件名到数据偏移映射和对应 codec 变体。现有 ZIP SFX 和通用压缩 codec 无法直接覆盖；仅在 EXE 中搜索 LZMA 魔数会误识别，也无法恢复文件列表。
- 下一步用 makensis 构建 Store/各 codec、solid/non-solid、NSIS2/3/Unicode 语料，先支持固定且可验证的子集，再扩覆盖；solid 共享一个顺序解码状态并必要时磁盘暂存，避免逐项从头解压。所有路径仍经共享验证，必须比较提取哈希；不执行安装器或安装脚本作为解压实现。若复用 C++ handler，先决定静态依赖与三平台构建方式。

## ZIP / ZIPX

- 创建：ZIP 保持 Deflate；`--format zipx` 或 `.zipx` 输出选择 XZ。`--level 0..9` 传给实际编码器，非法值在创建输出前拒绝。
- 读取：启用 ZIP crate 的 XZ、LZMA、BZIP2、Deflate64、ZSTD、PPMd；不引入外部子进程，不整文件缓冲。
- ZIPX 的 magic 仍为 ZIP，读取可走共用 reader；新 `ArchiveFormat::Zipx` 用于创建选择，GUI 尚未暴露此创建能力。
- 验证：workspace fmt/clippy（`-D warnings`）/all-features tests 通过；7-Zip 生成的 LZMA/BZip2/Deflate64/PPMd ZIP 均正确提取；GeeZipX XZ ZIPX 经 `7z t` 验证通过。
