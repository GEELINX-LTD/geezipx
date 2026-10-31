# Core/CLI 格式对齐审查

目标：按用户提供的 Bandizip 创建/提取清单逐个复核。每项完成实现、修复或阻碍分析后独立提交。GUI 接入另行处理；识别扩展名、原样复制不等于支持提取。未发布能力以本分支源码为准。

性能原则：优先有界缓冲的流式 I/O；可寻址容器优先磁盘暂存而非整文件驻内存；不以未校验的输出换取速度。编码器字典占用随压缩等级变化。专项测试后执行 workspace fmt/clippy/test 门禁；独立工具互操作与内部 round-trip 分开记录。

## 第一轮结论与验证（截至 `3a886ae`）

用户清单中的创建与提取项已逐个完成一轮排查；有变化的实现、性能修复及暂缓分析分别提交。创建清单中的 ZIPX/XZ 已补齐，SFX 仍要求对应目标 stub；其它指定创建方法已核对。BH、DAA 1.0、PEA、NSIS 提取本轮暂缓，ALZ/EGG、MSI、光盘映像、WIM、SFX 和 ZIPX 的子集限制见逐项表；本轮完成不代表与 Bandizip 全变体兼容。

最终 Linux 本地检查：

- `cargo fmt --all --check` 通过。
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings` 通过。
- `cargo test --workspace --exclude geezipx-gui --all-features`：1119 通过、9 忽略、0 失败。
- `cargo test -p geezipx-core -p geezipx --no-default-features --locked`：998 通过、4 忽略、0 失败。
- `cargo deny check advisories bans licenses sources` 通过；重复依赖等警告按现有策略报告。

独立互操作包括 ZIPX/ZIP 与 7-Zip、`.Z` 与 ncompress、PMA 独立语料、ACE/ARJ 真实样本、RAR4/RAR5 与 bsdtar、WIM XPRESS 与 wimlib；详细范围在对应行记录。第二轮已补真实EGG、UltraISO ISZ、AES Crypt v0–v3独立验证；第三轮已完成x86-64三平台原生CLI与SFX验收，见[专项验收](FORMAT_PARITY_ACCEPTANCE.md)。

性能改进集中在消除整包驻内存和重复解码；未建立新的统一吞吐基线。ISO创建的整包缓冲已在第四轮移除；UDF/ASAR/LZH/ZPAQ创建与部分第三方读取API的剩余性能债务保留具体原因和后续路线。

## 第二轮：新实现专项验收

- ALZ：严格要求Deflate流结束、声明区间、CRC和长度；固定32KiB工作缓冲，拒绝输出越界。再次修复重复规范化名称指向首项的问题。
- EGG：7份真实样本通过上游记录比对；独立Go解包器验证其中6份、22个文件的路径/长度/SHA-256。修复合法无EOS的LZMA块误拒绝，新增64MiB索引预算和父引用/重复ID/截断检查；POSIX样本不能计为独立Go工具通过。
- ISZ：官方UltraISO 9.7.6.3829生成的默认zlib归档，ISO还原与官方输出逐字节一致并通过7-Zip校验；修复非final/缺Adler32的误接受，流式/seek视图共用严格块校验。
- AES Crypt：官方Linux CLI 4.7.0交叉验证63份v0–v2向量，官方新建v3含中文密码通过；保留边界fixture，验证认证失败不写目标，并明确旧格式长度字节不受MAC保护。
- Lzip / TLZ / LZMA Alone：严格Lzip成员封装，拒绝空输入/残缺头/IO错误/尾随垃圾；固定缓冲及256MiB解码预算，GNU Lzip 1.25等级0/6/9与拼接成员互操作通过。
- IMG / BIN：修复短Read误识别，按需校验每个raw CD扇区的同步/模式/子头并缓存当前扇区；CFB在解析前限制FAT估算预算64MiB，避免稀疏文件导致大额分配。

完整Linux门禁：fmt、workspace Clippy `-D warnings`、workspace all-features测试均通过，**1135通过、9忽略、0失败**。复查范围、8个代码/样本提交、样本来源/许可/摘要及未覆盖变体，详见[专项验收](FORMAT_PARITY_ACCEPTANCE.md)。

## 第三轮：三平台候选构建与验收

候选源码`8f5e9f53780ec5502efc9fda5bf8708a29288eb7`的[CI](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159081353)和[Audit](https://github.com/GEELINX-LTD/geezipx/actions/runs/41159057553)均成功。三平台最小、基础与扩展feature测试及Clippy通过；macOS沿用ZPAQ限制，使用Intel runner和RAR/WIM/SFX组合。新增离线候选验收脚本实际调用release CLI，每平台39项通过，覆盖新增格式、Unicode路径、损坏/密码失败保护及实际SFX执行。

三份CLI与三份报告已下载，源码SHA、x86-64架构和各自可执行文件SHA-256均匹配；提取文件哈希跨平台一致。下载的Linux CLI复跑39项通过。远端Rust1.99新增Clippy检查促使文本解码器改为数组分块，并补UU/UUE/XXE候选往返。系统版本、feature配置、逐项数量、产物链接/哈希及ISO内存诊断见[专项验收](FORMAT_PARITY_ACCEPTANCE.md#第三轮三平台原生候选验收)。此处的CLI原生验收不代替GUI及其它架构验收。

## 第四轮：ISO 创建性能与布局

实现提交`418cd013ee0fd5038bae4095653b2a27edac8513`移除原文Vec/建树内容clone/估算输出Vec，以单输入暂存文件、FileSource区间和缓冲镜像文件完成创建；按实际镜像长度输出。独立MIT支持库`geezipx-iso`使用显式版本+path，发布core前须先发布所需支持库版本。

同时修复实文件补零、目录记录跨扇区、SUSP CE向后指向/跨扇区及目录在内容之后的问题；7-Zip与bsdtar独立提取内容一致，后者保留多扇区长名集合的完整Rock Ridge名称。64MiB输入峰值RSS从205936KiB降至8520KiB，512MiB输入为8500KiB；暂存使用真实磁盘，空间约为原文加镜像，元数据仍随项数增长。Linux门禁1141通过/9忽略，支持库130项单测和独立package验证通过。实测范围、取消/回滚和原生候选证据见[第四轮专项验收](FORMAT_PARITY_ACCEPTANCE.md#第四轮iso-创建内存与兼容性修复)。

同一源码的[CI 41166748855](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166748855)与[Audit 41166752086](https://github.com/GEELINX-LTD/geezipx/actions/runs/41166752086)全部成功。三平台各feature矩阵/Clippy与实际CLI/SFX每平台39项通过，六份产物/报告已下载，源码SHA、架构及可执行文件哈希匹配；提取内容跨平台一致，Linux下载包复跑39项通过。macOS沿用RAR/WIM/SFX与Intel runner范围，不包含ZPAQ；具体数量/产物摘要见上述专项记录。

## 逐项记录

| 项目（含别名） | 本轮结果 | 验证/限制 |
| --- | --- | --- |
| ZIP / ZIPX / XPI | 已完善 | ZIPX 使用 XZ method 95；ZIP 级别透传；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；专有 JPEG/WavPack 方法未覆盖 |
| 7Z / 7Z(LZMA2) | 已优化 | solid 按 64 MiB/1024 项阈值分块（单项可超阈值），单项 >8 MiB 暂存磁盘；加密保留自定义字典；jobs=0 自动识别；读取使用库的顺序提取避免逐项重解 solid |
| EXE / SFX | 已完善 ZIP SFX | 新增可寻址探测：MZ/ELF/Mach-O 前缀后验证 ZIP 中央目录，不将任意 EXE 当压缩包；CLI 自动 list/test/decompress。创建仍需 feature 和对应 stub；7z/RAR/NSIS SFX 待独立载荷路由 |
| TAR | 已复核 | 读写流式；`visit_entries` 单次顺序扫描，整包提取/校验不再逐项重扫。保留随机单项提取从头扫描的 API；GNU/PAX 路径与危险路径由 tar crate/共享提取层处理；现有专项回归通过 |
| TGZ / TAR.GZ | 已复核 | 已有 gzp 并行创建、多成员读取与单次顺序提取；访问流尾时 drain 以验证 gzip trailer；现有单/多线程、取消/损坏专项回归通过。单项 extract 提前返回不校验后续项，整包 test 才覆盖全流 |
| LZH / LHA / lh7 | 已优化 | 新增顺序 visitor，整包提取/校验避免每项从头扫描，文件 EOF 前验证 CRC16；同样覆盖 PMA。lh7 编码和现有回归通过。创建端仍逐项缓冲原文和压缩结果，后续需流式编码器 |
| ISO / Joliet | 已优化创建内存并修复布局 | Joliet Level3 + Rock Ridge；读取缓存树并seek。创建改为磁盘暂存/64KiB级缓冲，FileSource区间避免内容clone，按实际长度输出；实文件补零、记录扇区边界、向前CE布局已修复，7-Zip/bsdtar独立验证。64/512MiB输入峰值RSS均约8.3MiB；目录元数据仍随项数增长，临时空间约原文+镜像，tmpfs会占系统RAM；详情见[专项验收](FORMAT_PARITY_ACCEPTANCE.md#第四轮iso-创建内存与兼容性修复) |
| GZ | 已复核并修复 | 多成员解码与每成员 trailer/CRC 校验已具备；修复非法压缩等级在编码前返回结构化错误。独立 GZ 的 jobs 当前不生效（TAR.GZ 已支持并行），后续需保持非 static writer API 兼容 |
| XZ / TXZ | 已优化 | `--jobs` 接入 liblzma MtStreamBuilder；默认单线程；TAR.XZ 各读取路径统一多流解码；无效选项返回错误 |
| ACE | 已修复列表、CRC、密码与 solid 前驱处理 | 列表显式跳过 payload，修复有效档报错；ACE CRC 元数据转为标准 CRC32；CLI 接受并在打开底层解码器前传递密码（补充 Blowfish 密码向量回归，避免 unified setter 仅更新 options 导致丢失）；单项提取先解码 solid 前驱以恢复字典，传播跳过错误。ACE v1/v2 真实样本 CLI test 通过。unarc-rs 0.8 解码 API 仍返回整项 Vec，逐项重新打开带来重复扫描，流式 writer API 需上游改造；不能将 read_to 包装误报为流式 |
| AES | 已接入 AES Crypt 读取 | aescrypt-rs 0.2.0-rc.18（锁定版本）支持 v0–v3；`.aes`/AES magic 路由，双遍验证后释放明文；通用非 seek API 将密文暂存磁盘，64 KiB 级缓冲。v0–v2全部63向量与官方4.7.0交叉比对，官方v3/中文密码通过；长度字节不受认证是旧格式限制，seek双遍源须不可变；原 `.enc` 仍非流式，拒绝以 `.aes` 创建旧容器 |
| ALZ | 部分实现 | 新增单卷Store/Deflate流式提取、CRC/长度及严格流结束校验、ZipCrypto密码；索引缓存避免逐项重扫并拒绝重复规范化名称。修改版BZip2库需要整压缩项缓冲，method3/分卷暂缓；路径需UTF-8，累计索引预算和真实加密样本待补 |
| ARJ | 已修复列表与密码 | 列表显式 skip payload 并传播错误；CLI 在打开解码器前传递密码（Garble/GOST 由依赖处理），Garble 正确/缺失/错误密码回归覆盖 setter 未更新 backend 的缺陷；真实 method0–4 样本 CLI test 通过，损坏 CRC 样本失败符合预期；unarc-rs 返回整项 Vec 和逐项重复扫描仍为性能债务，分卷需 volume provider 尚未接入 |
| BH | 暂缓：规范及独立语料不足 | BlakHole/ZipTV 专有容器，未找到足以验证 header/codec/CRC 的可用解析器与独立样本；不能用ZIP别名替代。后续先补来源可追溯的 Store/压缩/损坏/Unicode 档及规范，按索引+有界Read实现，见下方阻碍记录 |
| BIN | 已实现光盘内容读取 | CLI list/test/decompress 改为解析 ISO9660/Joliet/Rock Ridge/UDF；支持 2048-byte cooked 和单轨 2352-byte Mode1/Mode2 Form1，缓存索引/按需 seek/64KiB 缓冲，拒绝未知文件。音轨/CUE/混合轨、分区/FAT 等磁盘布局暂缓，需多轨/文件系统适配；旧 core bin_decompress 仍为显式原始复制 API |
| BR | 已复核 | 单流64KiB解码缓冲/流式编码，等级0..11校验；现有空流/截断/级别/往返测试通过。无魔数/原生校验和，依赖扩展名，无法保证检测所有位翻转；默认quality11偏慢，性能敏感调用宜显式--level 4..6，保留默认兼容性；jobs 当前忽略，后续可按用户需求评估多线程编码器 |
| BZ / BZ2 / TBZ / TBZ2 | 已复核并修复 | BZIP2 多成员流式读取/CRC 已具备；增加 `.bz`/`--format bz` 别名与大小写输出名处理，修复独立编码 API 非法等级 panic。`.bz` 仅指 BZIP2 容器，旧 BZIP/BZ0 算法未实现；TAR.BZ2 顺序 visitor 已接入共享提取/校验路径；旧 infallible TAR writer 构造器仍要求调用者校验等级 |
| CAB | 已优化创建内存并修复空包 | 创建不再整包 Vec，单临时文件暂存/64KiB 缓冲，限制单项32-bit长度；空输入输出合法 CAB，纯空目录返回明确错误。3MiB生成流/空包及现有 Store/MSZIP/路径回归通过。读取通过 cab 0.6 支持单卷 Store/MSZIP/LZX，单项 read_file 仍重建 folder decoder，整包重复解压债务需上游开放 folder 顺序读取器；多卷未接入 |
| 化合物 / MSI | 已实现容器流提取并补FAT预算 | cfb 0.15；识别CFB magic/.msi/.msp/.mst/.cfb/.ole；mini-stream/普通流Read+Seek按需复制。打开前64MiB FAT估算预算（512/4096字节扇区约8/64GiB容器），目录等其它元数据仍驻内存。提取原始OLE/MSI streams，不解释MSI数据库File表、不自动展开CAB |
| DAA 1.0 | 暂缓：无已验证适配器/语料 | 可参考 GPL-2.0 daa2iso，但本项目 MIT，未直接移植；尚无独立 DAA1.0 chunk/损坏/密码/分卷语料验证。后续独立规范实现或隔离转换器需先定依赖/许可策略；必须有界块索引+缓存，转换 ISO 后再提供内容视图，不能仅识别 DAA 算支持 |
| DEB | 已优化创建内存及顺序提取 | 创建从整包原文/TAR/压缩载荷 Vec 改为磁盘暂存/64KiB 缓冲，控制信息仅小 Vec；读取 gzip/xz/zstd/bzip2/lzma/plain TAR 复用单次顺序 visitor，整包不再逐项重复解压并验证 codec trailer。现有6 codec/路径/空目录回归与新增 TAR 终止符之后 gzip CRC 损坏回归通过。创建仍经原文/TAR/压缩暂存多遍磁盘I/O，后续可将 TAR builder 直接连到编码器；只提取 data.tar 内容，不解释控制信息 |
| EGG | 部分实现 | 单卷非 solid 未加密 Store/Deflate/BZip2/LZMA 流式提取，逐块 CRC32/长度及流结束校验；LZMA 字典内存上限 256 MiB。自有 seek 索引解析设置64 MiB累计元数据预算及路径父引用检查，unegg仅复用类型/名称编码；7份真实样本记录核对、6份/22个文件与独立Go工具哈希一致，含无EOS LZMA和POSIX元数据解析；AZO/solid/分卷/加密暂缓 |
| IMG / IMA | 已实现光盘内容读取 | 与 BIN 复用 cooked/raw 光盘 reader，实际提取 ISO/UDF 文件，非光盘数据拒绝。修复短Read探测并逐个验证被读取的raw扇区头/模式，缓存当前扇区且保持流式；不含EDC/ECC。FAT12/16/32、NTFS、分区盘、稀疏磁盘/多轨待补，不能宣称通用 IMG 支持；旧 core/CLI stdin copy API 保留显式透传 |
| ISZ | 已修复兼容性并优化内存/提取 | 按公开规范将 block_size 改为字节并接受 48-byte 头，兼容旧 GeeZipX 64-byte/sector 字段；创建使用 2MiB 输入块/磁盘暂存载荷和索引，Store/Zlib/Zero 自选。新增 Read+Seek 单块缓存（解压块/index 各最大64MiB），CLI list/decompress 直接提取 ISO 内文件，--stdout 仍还原 ISO，test 校验全部块。加密/分卷/非3-byte pointers暂缓；官方UltraISO 9.7.6.3829默认zlib样本互操作通过，修复StreamEnd/Adler32缺失误接受；规范 https://forum.imgburn.com/topic/14260-isz-images-supporting/ |
| LZ / Lzip | 已完善严格解码与等级校验 | 写入仍用lzma-rust2，读取改为严格Lzip v1成员封装+已有liblzma，拒绝空输入/残缺头/错误IO/尾随垃圾；CRC32、数据/成员长度检查；256MiB解码内存上限和64KiB输入缓冲；GNU Lzip1.25多等级/拼接成员通过并保留fixture。0..9等级错误返回，level0仍为压缩；编码字典随等级分配，单流jobs暂不生效 |
| LZMA | 已复核并修复共享 API | liblzma Alone 编解码均流式，等级0仍压缩；补充 LZMA/XZ非法等级回归，并修复共享 XZ简化API对非法等级panic。现有 LZMA 空流/截断/级别/往返测试通过。Alone无原生CRC，无法保证检测全部位翻转；解码已设256MiB内存预算，超限明确拒绝，可配置预算待补；字典占用与文件体积无关 |
| PMA | 已实现读取 | 升级 delharc 0.8，启用 pm0/pm1/pm2；复用 LHA 流式读取与 CRC16 校验；独立 PM1/PM2 样本及 CRC32 验证通过；拒绝以 PMA 名义创建 LZH |
| RAR / RAR5 | 已优化顺序校验并修复空文件计数 | 新增直接 visitor，CLI test/顺序访问一次打开、保留 solid 状态，避免逐项重扫；修复两种 extract_all 空文件未计数。RAR4/RAR5 solid/Unicode 等5个独立 unrar fixture 与 bsdtar 提取逐文件SHA256一致；新增可移植 Store/空文件回归。unrar 0.5 writer callback 未公开，仍整项 Vec（大单项/取消延迟债务）；后续需安全 ReadToWriter upstream API，不能直接把 native extract_to 用于通用流以避免链接/路径语义变化；分卷/密码仍受库支持范围限制 |
| TLZ | 已实现读取并补资源边界 | `.tlz` / `.tar.lzma` / `.tar.lz` 支持 LZMA Alone 与 Lzip 两种外层，逐项访问单次流式解码；复用严格Lzip reader，Alone/Lzip解码预算256MiB。修复短Read误识别外层，截断/拼接/超大字典回归通过；不支持创建 |
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
