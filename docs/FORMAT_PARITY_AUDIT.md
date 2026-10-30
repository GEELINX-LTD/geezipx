# Core/CLI 格式对齐审查

目标：按用户提供的 Bandizip 创建/提取清单逐个复核。每项完成实现、修复或阻碍分析后独立提交。GUI 接入另行处理；识别扩展名、原样复制不等于支持提取。未发布能力以本分支源码为准。

性能原则：优先有界缓冲的流式 I/O；可寻址容器优先磁盘暂存而非整文件驻内存；不以未校验的输出换取速度。编码器字典占用随压缩等级变化。专项测试后执行 workspace fmt/clippy/test 门禁；独立工具互操作与内部 round-trip 分开记录。

## 逐项记录

| 项目（含别名） | 本轮结果 | 验证/限制 |
| --- | --- | --- |
| ZIP / ZIPX / XPI | 已完善 | ZIPX 使用 XZ method 95；ZIP 级别透传；启用 XZ/LZMA/BZIP2/Deflate64/ZSTD/PPMd 读取；专有 JPEG/WavPack 方法未覆盖 |
| 7Z / 7Z(LZMA2) | 已优化 | solid 按 64 MiB/1024 项分块，单项 >8 MiB 暂存磁盘；加密保留自定义字典；jobs=0 自动识别；读取使用库的顺序提取避免逐项重解 solid |
| EXE / SFX | 待复核 | |
| TAR | 待复核 | |
| TGZ / TAR.GZ | 待复核 | |
| LZH / LHA / lh7 | 待复核 | |
| ISO / Joliet | 待复核 | |
| GZ | 待复核 | |
| XZ / TXZ | 已优化 | `--jobs` 接入 liblzma MtStreamBuilder；默认单线程；TAR.XZ 各读取路径统一多流解码；无效选项返回错误 |
| ACE | 待复核 | |
| AES | 待复核 | `.enc` 不等于 AES Crypt |
| ALZ | 待复核 | |
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
| PMA | 待复核 | |
| RAR / RAR5 | 待复核 | |
| TLZ | 待复核 | |
| UDF | 待复核 | |
| WIM | 待复核 | |
| Z / Unix Compress | 待复核 | |
| ZPAQ | 待复核 | |
| PEA | 待复核 | |
| UU / UUE | 待复核 | |
| XXE | 待复核 | |
| ASAR | 待复核 | |
| ZSTD | 待复核 | |
| NSIS | 待复核 | |
| LZ4 | 待复核 | |

## ZIP / ZIPX

- 创建：ZIP 保持 Deflate；`--format zipx` 或 `.zipx` 输出选择 XZ。`--level 0..9` 传给实际编码器，非法值在创建输出前拒绝。
- 读取：启用 ZIP crate 的 XZ、LZMA、BZIP2、Deflate64、ZSTD、PPMd；不引入外部子进程，不整文件缓冲。
- ZIPX 的 magic 仍为 ZIP，读取可走共用 reader；新 `ArchiveFormat::Zipx` 用于创建选择，GUI 尚未暴露此创建能力。
- 验证：workspace fmt/clippy（`-D warnings`）/all-features tests 通过；7-Zip 生成的 LZMA/BZip2/Deflate64/PPMd ZIP 均正确提取；GeeZipX XZ ZIPX 经 `7z t` 验证通过。
