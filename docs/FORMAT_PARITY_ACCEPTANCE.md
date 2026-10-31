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

不包含 AZO、solid、分卷和加密 EGG；没有扩大本轮声明范围。
