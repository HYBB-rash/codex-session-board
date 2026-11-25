# 冻结模型对照

```bash
npm run build
python3 -B acceptance/verify-typescript.py --model-only
```

模型模式输出源文件和受测运行模块的指纹、全部模型对照结果。任何对照失败时退出 `2`。完整 31 项要求由[发布门槛](release-gate.md)核验。

模型对照沿用上游 `tests/board_contract.rs` 的 40 个可达起点、13 种事件、520 条边及 1,417 次操作，含重开、查看各 40 次。每步比较状态、人工选择、备注及版本。重开和查看额外比较记录文件、归档账本和原生写入次数。夹具只模拟 Codex 请求回执和侧栏分区，调用生产 `BoardService`、`createDesktopConnection`、`createBoardRecords` 和 `createBoardLedgers`，持久化写入位于回收的临时目录。

运行证据用生产 `runtimeState` 测试上游原始的缺 ID、缺时间输入，再用 `runtimePhase` 验证补齐原生身份的同项结果。后者要求有效轮次 ID，判断结束时还要求有效完成时间。标题沿用上游 `tests/board_io.rs` 的前缀、幂等、空正文和非法状态断言。输入版本对照检查同轮用户项顺序变化、助手输出和未加载项。

`candidate-map.json` 登记共享操作和验收范围。`typescript-evidence.json` 将本适配器的结果绑定到相应契约条目，并列出当期缺口。历史检查点不代表被测源码指纹下已经复验。

上游 Rust 形式模型的 138,040 条会话边、分区配置、账号组合、归档及远端草稿边，以及变化日志和标题任务的原子性断言仍待迁移。真实桌面、正式产物和外部环境证据也仍需补齐。

2026 年 11 月 22 日，修复同轮新阶段识别后，10 项对照全部通过，包含全部 520 条状态转换。31 项发布验收仍有待验证项，详见 [真实宿主及交付证据](../docs/session-board-typescript-acceptance.md)。
