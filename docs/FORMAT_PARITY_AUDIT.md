# Core/CLI 格式对齐审查

目标：按用户提供的 Bandizip 创建/提取清单逐个复核。每项完成实现、修复或阻碍分析后独立提交。GUI 接入另行处理；识别扩展名、原样复制不等于支持提取。未发布能力以本分支源码为准。

性能原则：优先有界缓冲的流式 I/O；可寻址容器优先磁盘暂存而非整文件驻内存；不以未校验的输出换取速度。编码器字典占用随压缩等级变化。专项测试后执行 workspace fmt/clippy/test 门禁；独立工具互操作与内部 round-trip 分开记录。

## 第一轮结论与验证（截至 `3a886ae`）

用户清单中的创建与提取项已逐个完成一轮排查；有变化的实现、性能修复及暂缓分析分别提交。创建清单中的 ZIPX/XZ 已补齐，SFX 仍要求对应目标 stub；其它指定创建方法已核对。BH、DAA 1.0 仍暂缓，PEA 与 NSIS 已新增有独立语料验证的只读子集；ALZ/EGG、MSI、光盘映像、WIM、SFX 和 ZIPX 的子集限制见逐项表；本轮完成不代表与 Bandizip 全变体兼容。

最终 Linux 本地检查：

- `cargo fmt --all --check` 通过。
- `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings` 通过。
- `cargo test --workspace --exclude geezipx-gui --all-features`：1119 通过、9 忽略、0 失败。
- `cargo test -p geezipx-core -p geezipx --no-default-features --locked`：998 通过、4 忽略、0 失败。
- `cargo deny check advisories bans licenses sources` 通过；重复依赖等警告按现有策略报告。

独立互操作包括 ZIPX/ZIP 与 7-Zip、`.Z` 与 ncompress、PMA 独立语料、ACE/ARJ 真实样本、RAR4/RAR5 与 bsdtar、WIM XPRESS 与 wimlib；详细范围在对应行记录。第二轮已补真实EGG、UltraISO ISZ、AES Crypt v0–v3独立验证；第三轮已完成x86-64三平台原生CLI与SFX验收，见[专项验收](FORMAT_PARITY_ACCEPTANCE.md)。

性能改进集中在消除整包驻内存和重复解码；未建立新的统一吞吐基线。ISO/UDF创建的整包缓冲已分别在第四/五轮移除，ASAR创建已改磁盘暂存并按Electron语义生成/校验SHA-256，LZH创建已改按项暂存与持续字典/位流编码，ZPAQ创建已改按归档暂存与共享solid模型；部分第三方读取API的剩余性能债务保留具体原因和后续路线。

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

## 第五轮：UDF 创建性能与互操作

实现提交`6bc8f88cc04dc12cff96ffbe779d78cdc60a685d`移除原文/建树clone/整镜像Vec，采用单输入暂存文件、FileSource区间、可seek镜像暂存和64KiB有界输出。默认仍为UDF1.02，保留既有单项4GiB创建上限。修复FID精确布局/父项、CS0 Unicode、跨1GiB多段短AD、LVID计数/CRC和卷末锚点；标准CS0/8按Latin-1解码，旧UTF-8只按旧producer/空charset标记兼容。

64MiB输入峰值RSS由205124KiB降至8224KiB，512MiB由1581096KiB降至7968KiB；磁盘临时空间约为输入+镜像，目录元数据仍增长。7-Zip与udfinfo验证64/512MiB、43文件Unicode/多扇区树；实际1GiB+17字节多extent文件提取哈希一致。genisoimage独立UDF视图与旧版GeeZipX镜像保留为回归fixture。取消阶段退出130并保护原目标。Linux workspace1148通过/9忽略、支持库同步27项单测和package验证通过；互操作/实测/原生验收见[第五轮专项验收](FORMAT_PARITY_ACCEPTANCE.md#第五轮udf-创建内存与互操作)。

独立支持库`geezipx-udf`需在授权发布core前先发布。额外async/write组合检查与上游一样失败，已记录类型分层改造路线；core/CLI使用同步配置，不将该组合计为通过。

## 第六轮：ASAR/LZH/ZPAQ 创建与 PEA/NSIS 读取

依次完成ASAR、LZH、ZPAQ创建改造及PEA、NSIS只读子集，独立提交并保留原生工具语料。512MiB专项样本的创建峰值RSS分别由约1.51GiB降至8.1MiB、532MiB降至8.8MiB、1.01GiB降至8.5MiB；新增磁盘暂存，元数据和模型内存仍随其预算增长，单次实测不构成通用吞吐结论。PEA由官方Pea生成19份语料，NSIS由makensis生成7种Store/Deflate/BZip2/LZMA变体并以7-Zip独立提取核对。PEA加密/ECC/分卷、NSIS旧版/动态结果、ZPAQ journal提取和读取缓冲及macOS ZPAQ限制仍保留，两个GUI未接入新增格式。

统一候选`76b65a60fa9478f2661256369968ee275d245501`的[CI 41201756766](https://github.com/GEELINX-LTD/geezipx/actions/runs/41201756766)与[Audit 41201760473](https://github.com/GEELINX-LTD/geezipx/actions/runs/41201760473)均成功。三平台最小/基础/扩展feature测试及Clippy通过，原生CLI/SFX验收Linux/Windows各77项、macOS73项；六份二进制/报告已下载，完整源码SHA、实际x86-64架构、二进制哈希与报告匹配，三平台共有73项内容哈希一致。下载LinuxCLI复跑77项通过。额外修复独立LZH新依赖解析和WIM损坏用例的偶发无修改，后者4096次重复通过；详细测试数、产物链接、SHA-256和测量范围见[第六轮专项验收](FORMAT_PARITY_ACCEPTANCE.md#三平台最终候选与产物复核)。CLI仍显示0.7.4，按源码SHA识别候选，产物保留7天。

## 逐项记录

| 项目（含别名） | 本轮结果 | 验证/限制 |
| --- | --- | --- |
| ZIP / ZIPX / XPI | 已完善 | ZIPX 使用 XZ method 95；ZIP 级别透传；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；专有 JPEG/WavPack 方法未覆盖 |
| 7Z / 7Z(LZMA2) | 已优化 | solid 按 64 MiB/1024 项阈值分块（单项可超阈值），单项 >8 MiB 暂存磁盘；加密保留自定义字典；jobs=0 自动识别；读取使用库的顺序提取避免逐项重解 solid |
| EXE / SFX | 已完善 ZIP SFX | 新增可寻址探测：MZ/ELF/Mach-O 前缀后验证 ZIP 中央目录，不将任意 EXE 当压缩包；CLI 自动 list/test/decompress。创建仍需 feature 和对应 stub；现代NSIS3 Unicode另走只读载荷路由，7z/RAR SFX待接入 |
| TAR | 已复核 | 读写流式；`visit_entries` 单次顺序扫描，整包提取/校验不再逐项重扫。保留随机单项提取从头扫描的 API；GNU/PAX 路径与危险路径由 tar crate/共享提取层处理；现有专项回归通过 |
| TGZ / TAR.GZ | 已复核 | 已有 gzp 并行创建、多成员读取与单次顺序提取；访问流尾时 drain 以验证 gzip trailer；现有单/多线程、取消/损坏专项回归通过。单项 extract 提前返回不校验后续项，整包 test 才覆盖全流 |
| LZH / LHA / lh7 | 已优化读写 | 顺序visitor避免整包提取/校验逐项重扫并验证CRC16，也覆盖PMA。创建使用按项匿名文件与64KiB缓冲，维护lh4–lh7连续字典/位流，保留不缩小则lh0回退、u32大小限制；输入暂存后可取消，源失败不改输出。38专项+89支持库测试与7-Zip互操作通过；512MiB创建RSS约532MiB→8.8MiB，详见第六轮 |
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
| UDF | 已优化读写内存并修复创建互操作 | 读取64KiB seek缓冲，缓存目录元数据，保留ICB/分区/范围校验和8GiB稀疏镜像回归。创建改为磁盘暂存/FileSource/有界输出，UDF1.02、单项4GiB；精确FID/多段短AD、CS0、LVID和尾锚点修复，独立7-Zip/genisoimage/udfinfo通过，64/512MiB峰值RSS约8MiB。支持库同步配置已验证，async/write继承冲突暂缓；metadata/VAT/多分区及逐项目录导航仍为债务，详见[专项验收](FORMAT_PARITY_ACCEPTANCE.md#第五轮udf-创建内存与互操作) |
| WIM | 已复核，LZX/LZMS暂缓 | 资源按chunk范围解码、<=512KiB工作块、SHA1核验，37专项测试通过；独立 wimlib XPRESS两文件490003B通过。wimlib LZX样本仍复现 `encountered empty decode tree`，需核对 lzxd 的 LZXD头/状态与WIM变体差异，已修正模块能力说明；需WIM-specific LZX及E8处理/独立XPRESS-LZX-LZMS语料，不把内部roundtrip当互操作。LZMS、SWM、第一映像以外提取未实现；创建仅无压缩；元数据仍整体驻内存 |
| Z / Unix Compress | 已实现读取 | 流式 LZW，9–16 位有界字典；修复 `.Z` 大小写检测并加入魔数；ncompress 10/12/16 位共 15 例互操作通过；格式无校验和，不能保证发现所有截断 |
| ZPAQ | 创建已优化，读取仍有债务 | 使用维护支持库的named Read+Write接口，原文磁盘暂存、64KiB I/O、64MiB元数据预算，一个solid块跨文件保留模型；内置1–3，兼容4/5映射3，修复旧4/5运行时报错。新包写段SHA-1，路径读取修复原先读到校验和却未核对的问题；FFI末次写失败/析构、短Read/Interrupted、取消与输入长度有回归。读取仍整包fs::read并缓冲成员、批量操作重复解码；真实zpaq7.15 journal可列出input.txt但提取报not found，样本已保留。需journal-aware回调或fragment/index重建，不能以native extract绕过共享安全输出 |
| PEA | 已接入单卷未加密子集 | PCOMPRESS0 Store、PCOMPRESS1–3块Deflate；13类校验及NOALGO覆盖对象/流/卷，64KiB缓冲和64MiB元数据预算，打开时顺序验证后建立索引。官方PeaZip11.4工具生成19份语料，原文哈希、截断/损坏/路径/取消/输出失败验证通过；仅UTF-8/ASCII名称，加密/HMAC、ECC、分卷、旧ANSI及属性恢复未实现，两个GUI未接入；详见专项验收第六轮 |
| UU / UUE | 已优化 | CLI 编解码改为逐行流式，固定 45 字节输入块，最大 1024 字节行；保留需要返回 Vec 的兼容 API；校验截断和终止符 |
| XXE | 已优化 | 复用有界逐行编解码器，CLI 不再整文件驻内存；截断回归测试通过 |
| CPIO（额外既有格式） | 已优化创建内存并修复长度截断 | newc/odc 创建最多保留1MiB项数据，超过后磁盘暂存，头部长度超过32-bit返回错误而非截断；odc 使用 reader API 避免重新读为 Vec。3MiB生成流/有界读取双格式回归及既有CLI/特殊文件/路径/往返测试通过。读取/校验原为流式，但整包提取逐项重扫仍有I/O债务；bin/crc变体暂未接入 |
| ARC / PAK（额外既有格式） | 已修复列表、密码和 CRC 元数据 | 列表显式 skip payload，支持 XOR 密码并在打开底层解码器前传入；CRC16 不再误标为 CRC32。带伪 header 的双文件/密码/损坏 CRC 回归及 CLI 加密 list/test/extract 通过；unarc 返回整项 Vec 和逐项重扫仍为债务 |
| ASAR | 已优化读写 | 读取缓存64MiB内JSON索引，seek + take流式读payload，并验证可选全文件/分块SHA-256；保留unpacked/符号链接路径检查。创建以64KiB缓冲暂存匿名文件，索引预算64MiB，载荷按顺序输出；Electron官方工具双向互操作/校验信息一致。512MiB创建RSS约1.51GiB→8.1MiB，取消/失败保护通过，细节见专项验收第六轮 |
| ZSTD | 已复核并修复溢出 | libzstd 流式、多帧读取、可选多线程编码；新增0..22等级验证，阻止u32MAX转换成负fast等级，jobs转u32不再静默截断。现有多线程/截断/级别/往返及新增溢出回归通过。创建默认不启用可选内容校验和，输入无checksum时不能保证检测所有位翻转；字典/worker占用依设置变化 |
| NSIS | 已接入现代Unicode子集 | 原生makensis生成Store/Deflate/BZip2/LZMA共7种solid/non-solid样本，7-Zip独立提取哈希一致。PE边界/可选CRC、64MiB元数据/字典预算与严格流结束校验；solid单次解码到匿名磁盘，安全映射静态路径，不执行脚本。ANSI/旧版/Park、BCJ、自定义打包stub、运行时文件/插件及卸载器重建暂缓；两个GUI未接入，详见第六轮 |
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
- 对象、流、卷层级的普通校验已接入；`crates/core/src/archive/pea.rs`使用Read+Seek和有界缓冲，先完整顺序核验、再发布索引，提取重读索引对象并复核。13类校验及NOALGO、Store/三个Deflate等级、多块及原样尾块由官方Pea工具生成的19份独立语料覆盖，来源与SHA-256见[fixture说明](../crates/core/tests/fixtures/pea/README.md)。NOALGO没有对应完整性保证。
- 加密/HMAC、AES/Twofish/Serpent EAX、ECC、多卷、旧ANSI代码页和日期/属性恢复仍未接入。加密后续必须先认证再发布明文；现有`.enc`/`.aes`不能替代PEA容器或认证边界。其余能力逐项补充独立语料和资源预算，不按扩展名声称完整兼容。

### NSIS 安装器

- 已接入NSIS 3 Unicode Store及Deflate/BZip2/LZMA的solid/non-solid读取；通过PE节表定位后附NSIS头，校验范围、可选容器CRC及流结束。使用Apache-2.0的维护支持库`geezipx-nsis`（上游nsis0.6.0）解释有界解码头和NSIS BZip2块，core管理索引、取消和输出；不调用上游整包载入/提取接口。
- solid载荷一次顺序解码至匿名文件，后续按偏移访问；non-solid按项seek。64KiB I/O、64MiB元数据/字典预算，先验证完整索引/载荷再允许提取。路径仅接受静态且相对`$INSTDIR`的可移植名称，拒绝穿越、设备名、别名冲突和未解析变量；关闭CRC的包明确报告未验证容器CRC。
- 原生makensis生成7份样本并由7-Zip独立提取核对，覆盖空文件/目录、Unicode、不可压缩内容与重复数据；来源、许可和哈希见[fixture说明](../crates/core/tests/fixtures/nsis/README.md)。64/512MiB LZMA-solid单项用例的list/test/extract峰值RSS约16MiB，具体记录见第六轮专项验收。
- ANSI、NSIS1/2、Park、BCJ-LZMA、自定义打包stub及卸载器重建尚未覆盖；任意分支/运行时变量、插件结果和安装时生成文件需要脚本执行语义，当前不模拟、不执行安装器。后续须先补对应独立语料及有界解码策略，不能把静态载荷读取宣称为完整安装结果还原。两个GUI需独立桥接。

## ZIP / ZIPX

- 创建：ZIP 保持 Deflate；`--format zipx` 或 `.zipx` 输出选择 XZ。`--level 0..9` 传给实际编码器，非法值在创建输出前拒绝。
- 读取：启用 ZIP crate 的 XZ、LZMA、BZIP2、Deflate64、ZSTD、PPMd；不引入外部子进程，不整文件缓冲。
- ZIPX 的 magic 仍为 ZIP，读取可走共用 reader；新 `ArchiveFormat::Zipx` 用于创建选择，GUI 尚未暴露此创建能力。
- 验证：workspace fmt/clippy（`-D warnings`）/all-features tests 通过；7-Zip 生成的 LZMA/BZip2/Deflate64/PPMd ZIP 均正确提取；GeeZipX XZ ZIPX 经 `7z t` 验证通过。
