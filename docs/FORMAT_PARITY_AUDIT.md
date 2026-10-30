# Core/CLI 格式对齐审查

目标：按用户提供的 Bandizip 创建/提取清单逐个复核。每项完成实现、修复或阻碍分析后独立提交。GUI 接入另行处理；识别扩展名、原样复制不等于支持提取。未发布能力以本分支源码为准。

性能原则：优先有界缓冲的流式 I/O；可寻址容器优先磁盘暂存而非整文件驻内存；不以未校验的输出换取速度。编码器字典占用随压缩等级变化。专项测试后执行 workspace fmt/clippy/test 门禁；独立工具互操作与内部 round-trip 分开记录。

## 逐项记录

| 项目（含别名） | 本轮结果 | 验证/限制 |
| --- | --- | --- |
| ZIP / ZIPX / XPI | 已完善 | ZIPX 使用 XZ method 95；ZIP 级别透传；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；专有 JPEG/WavPack 方法未覆盖 |
| 7Z / 7Z(LZMA2) | 已优化 | solid 按 64 MiB/1024 项分块，单项 >8 MiB 暂存磁盘；加密保留自定义字典；jobs=0 自动识别；读取使用库的顺序提取避免逐项重解 solid |
| EXE / SFX | 已完善 ZIP SFX | 新增可寻址探测：MZ/ELF/Mach-O 前缀后验证 ZIP 中央目录，不将任意 EXE 当压缩包；CLI 自动 list/test/decompress。创建仍需 feature 和对应 stub；7z/RAR/NSIS SFX 待独立载荷路由 |
| TAR | 已复核 | 读写流式；`visit_entries` 单次顺序扫描，整包提取/校验不再逐项重扫。保留随机单项提取从头扫描的 API；GNU/PAX 路径与危险路径由 tar crate/共享提取层处理；现有专项回归通过 |
| TGZ / TAR.GZ | 已复核 | 已有 gzp 并行创建、多成员读取与单次顺序提取；访问流尾时 drain 以验证 gzip trailer；现有单/多线程、取消/损坏专项回归通过。单项 extract 提前返回不校验后续项，整包 test 才覆盖全流 |
| LZH / LHA / lh7 | 已优化 | 新增顺序 visitor，整包提取/校验避免每项从头扫描，文件 EOF 前验证 CRC16；同样覆盖 PMA。lh7 编码和现有回归通过。创建端仍逐项缓冲原文和压缩结果，后续需流式编码器 |
| ISO / Joliet | 待复核 | |
| GZ | 已复核并修复 | 多成员解码与每成员 trailer/CRC 校验已具备；修复非法压缩等级在编码前返回结构化错误。独立 GZ 的 jobs 当前不生效（TAR.GZ 已支持并行），后续需保持非 static writer API 兼容 |
| XZ / TXZ | 已优化 | `--jobs` 接入 liblzma MtStreamBuilder；默认单线程；TAR.XZ 各读取路径统一多流解码；无效选项返回错误 |
| ACE | 待复核 | |
| AES | 待复核 | `.enc` 不等于 AES Crypt |
| ALZ | 部分实现 | 新增单卷 Store/Deflate 流式提取、CRC/长度校验、ZipCrypto 密码；索引缓存避免逐项重扫。修改版 BZip2 库需要整压缩项缓冲、method 3 以及分卷暂缓；路径需 UTF-8 |
| ARJ | 待复核 | |
| BH | 待复核 | |
| BIN | 待复核 | 现有透传不算镜像提取 |
| BR | 待复核 | |
| BZ / BZ2 / TBZ / TBZ2 | 待复核 | |
| CAB | 待复核 | |
| 化合物 / MSI | 待复核 | |
| DAA 1.0 | 待复核 | |
| DEB | 待复核 | |
| EGG | 待复核 | |
| IMG / IMA | 待复核 | 现有透传不算镜像提取 |
| ISZ | 待复核 | |
| LZ / Lzip | 待复核 | |
| LZMA | 待复核 | |
| PMA | 已实现读取 | 升级 delharc 0.8，启用 pm0/pm1/pm2；复用 LHA 流式读取与 CRC16 校验；独立 PM1/PM2 样本及 CRC32 验证通过；拒绝以 PMA 名义创建 LZH |
| RAR / RAR5 | 待复核 | |
| TLZ | 已实现读取 | `.tlz` / `.tar.lzma` / `.tar.lz` 支持 LZMA Alone 与 Lzip 两种外层，逐项访问单次流式解码，截断回归通过；不支持创建 |
| UDF | 待复核 | |
| WIM | 待复核 | |
| Z / Unix Compress | 已实现读取 | 流式 LZW，9–16 位有界字典；修复 `.Z` 大小写检测并加入魔数；ncompress 10/12/16 位共 15 例互操作通过；格式无校验和，不能保证发现所有截断 |
| ZPAQ | 待复核 | |
| PEA | 待复核 | |
| UU / UUE | 已优化 | CLI 编解码改为逐行流式，固定 45 字节输入块，最大 1024 字节行；保留需要返回 Vec 的兼容 API；校验截断和终止符 |
| XXE | 已优化 | 复用有界逐行编解码器，CLI 不再整文件驻内存；截断回归测试通过 |
| ASAR | 已优化读取 | 原为每项 `fs::read` 整包且重建索引；现在只缓存 JSON 索引，seek + take 流式读 payload，保留 unpacked/符号链接路径校验；512 MiB 稀疏档与截断/恶意索引回归通过。索引上限 64 MiB；创建端仍缓存全部项，待改磁盘暂存 |
| ZSTD | 待复核 | |
| NSIS | 待复核 | |
| LZ4 | 待复核 | |

## ZIP / ZIPX

- 创建：ZIP 保持 Deflate；`--format zipx` 或 `.zipx` 输出选择 XZ。`--level 0..9` 传给实际编码器，非法值在创建输出前拒绝。
- 读取：启用 ZIP crate 的 XZ、LZMA、BZIP2、Deflate64、ZSTD、PPMd；不引入外部子进程，不整文件缓冲。
- ZIPX 的 magic 仍为 ZIP，读取可走共用 reader；新 `ArchiveFormat::Zipx` 用于创建选择，GUI 尚未暴露此创建能力。
- 验证：workspace fmt/clippy（`-D warnings`）/all-features tests 通过；7-Zip 生成的 LZMA/BZip2/Deflate64/PPMd ZIP 均正确提取；GeeZipX XZ ZIPX 经 `7z t` 验证通过。
