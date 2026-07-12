# GeeZipX Phase 1 — CLI MVP 任务拆分（归档）

> **本文档为 Phase 1 CLI MVP 历史归档。Phase 1 已于 v0.1.0 完成，当前开发在 Phase 2 GUI (v0.7.0)。**
> 详见 `docs/PRD.md` 和 `docs/GUI_MVP_PLAN.md`。

---

## 里程碑总览

| 里程碑 | 主题 | 周期 | 产出 | 状态 |
|--------|------|------|------|------|
| M1 | 项目骨架 + 核心引擎库 | 第 1-4 周 | `geezipx-core` lib crate，zip/tar/gz 基础读写 | **已完成** |
| M2 | CLI 基本命令 | 第 5-7 周 | `geezipx` binary，三个子命令可用 | **已完成** |
| M3 | 流式/进度/兼容性打磨 | 第 8-10 周 | 进度条、管道、格式检测、跨平台测试 | **已完成** |
| M4 | CI/测试/发布 | 第 11-12 周 | CI 全线通过、crates.io 发布、覆盖率追踪、GitHub Release | **已完成** |

---

## M1：项目骨架 + 核心引擎库（已完成）

Cargo Workspace 初始化，`geezipx-core` 库架构落地。实现 ZIP、tar、tar.gz、gzip 及后续扩展的 bzip2、brotli、lz4、xz、zstd 单流与 tar 容器基础读写。

**关键交付：**
- Cargo Workspace（`crates/core/` + `crates/cli/`）
- `GeeZipError` 统一错误类型（`thiserror`），格式检测模块（魔数 + 扩展名）
- ZIP 读写（含 Zip Slip 路径穿越防护）、tar / tar.gz / gzip 读写
- bzip2 / brotli / lz4 / xz / zstd 单流及对应 tar 容器读写
- 单元测试覆盖 detect、archive、error 模块

## M2：CLI 基本命令（已完成）

完整 CLI 入口，`compress`（自动/指定格式、递归目录）、`decompress`（自动格式检测、覆盖策略）、`list`（表格/JSON/紧凑）三个子命令。密码支持（`--password` + `GEEZIPX_PASSWORD` 环境变量）。

**关键交付：**
- 三个子命令：`compress`、`decompress`、`list`，命令行帮助完整
- 覆盖策略：`--overwrite` / `--skip` / `--rename` / `--auto-rename`
- 格式输出渲染（表格/JSON/compact），密码支持与环境变量
- 集成测试（135 CLI integration tests）

## M3：流式/进度/兼容性打磨（已完成）

`ProgressReader`/`ProgressWriter` 流式封装、`indicatif` 进度条、Ctrl+C 取消（`CancellationToken`）、覆盖策略与路径穿越防护。大文件（5 GB+）流式处理验证通过（峰值 RSS ~4 MB）。

**关键交付：**
- `ProgressReader`/`ProgressWriter` 流式封装（可选 `dyn ProgressReporter`）
- 进度条 CLI 渲染（`-p`/`--progress`），`indicatif` 集成
- Ctrl+C 取消支持（`CancellationToken` + POSIX/WSARecv 信号处理）
- 覆盖策略与路径穿越安全防护
- 格式互操作测试脚本 `scripts/check-interop.sh`，损坏输入优雅报错

## M4：CI/测试/发布（已完成）

三平台 CI 全线通过、cargo-deny 审计、Criterion benchmark 框架、cargo-tarpaulin 覆盖率、Shell 补全、GitHub Release 自动化。`geezipx-core` + `geezipx` 已发布到 crates.io（v0.1.0）。

**关键交付：**
- 三平台 CI matrix（fmt + clippy + test + build），cargo-deny 审计
- Criterion benchmark 框架（advisory continue-on-error），tarpaulin coverage（informational only）
- Shell 补全：`bash` / `zsh` / `fish` / `powershell` / `elvish`
- GitHub Release workflow：三平台 artifact + SHA256SUMS + consolidate 校验
- crates.io 发布：`geezipx-core` 0.1.0 → `geezipx` 0.1.0，页面渲染确认正常

---

## 任务优先级与依赖图

```
M1-2 错误类型 ──→ M1-4 ZIP 读写 ──→ M1-6 单元测试
              │                    ┌─→ M2-2 compress
              └─→ M1-5 tar.gz ────┤
                                  └─→ M2-3 decompress ──→ M3-4 覆盖/安全
                                                        ┌─→ M3-3 取消
                  M1-3 检测 ←───────────────────────────┤
                                                        └─→ M3-2 进度条
                                                             ↑
                                  M2-1 CLI 参数 ─────────────┘
                                  M2-4 list ──────→ M2-5 集成测试
                                                        │
                                                        └─→ M3-1 流式封装 ←── M3-5 互操作
                                                                                  │
                                                                       M4-1 CI ←─┘
                                                                       M4-2 质量 / M4-3 基准
                                                                       M4-4 文档 / M4-5 补全
                                                                       M4-6 发布
```

**关键路径：** `M1-2 → M1-4 → M1-5 → M2-2 → M2-3 → M3-1 → M3-5 → M4-1 → M4-5`

---

## 附录：Phase 1 发布检查表

以下记录 v0.1.0 发布验证项。B 组（重型验证）和 C 组（发布步骤）对未来版本仍有参考价值。

### B 组：重型验证（跨平台 / 大文件 / 人工观察）

- 5 GB+ 大文件流式处理：5.0 GiB 压力测试通过（~9 秒，SHA256 一致，峰值 RSS ~4 MB）
- 完整性能基准：`cargo bench -p geezipx-core` 运行通过，无显著退化
- 完整互操作测试：`GEEZIPX_INTEROP_STRESS=1 scripts/check-interop.sh` → 15 PASS / 1 SKIP / 0 FAIL
- 跨平台 CI：三平台（ubuntu/macos/windows）全线绿色，6 条 workflow runs 全部成功
- crates.io 页面渲染确认：`geezipx` 和 `geezipx-core` README/许可证/链接正确
- `cargo install geezipx` 远端安装验证通过，CLI 帮助与补全人工确认通过

### C 组：真实发布步骤（人工执行）

1. 确认状态（发布验证 A/B 组全部通过）
2. 发布 `geezipx-core` 到 crates.io，等待索引
3. 发布 `geezipx` 到 crates.io，等待索引
4. 打 tag `v0.1.0` 并推送
5. 创建 GitHub Release（引用 CHANGELOG.md）
6. 验证远端安装：`cargo install geezipx`
7. 更新 crates.io 页面元数据（确认渲染正常）
