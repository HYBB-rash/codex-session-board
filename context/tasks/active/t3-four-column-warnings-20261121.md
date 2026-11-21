# T3 · 四列会话持续缺证告警

- owner: T3 四列告警作者
- execution_mode: isolated_write
- coordinator: Herman 的五轮重构主会话
- base_commit: `3de9e0f00f5458eda255b59e81cc54e63818f071`
- worktree_path: `/home/herman/.codex/worktrees/t3-four-column-warnings/notion-program`
- allowed_write_set: 本卡、同名结果记录、`src/observer.rs` 与 `src/observer/four_column_tests.rs`。

分支 `codex/t3-four-column-warnings` 从接单时核实的 `b859880` 建立。在 C1 `f157541` 上做故障对照，随后重基到含 B1 的 `dce3536`，最后吸收候选来源测试和 C2 调查。T0 对齐清单中，此项唯一认领的完整 ID 为 `lib::codex_session_board::observer::four_column_tests::warnings_require_two_complete_observations_and_thirty_seconds`，沿用 `AUTO-006` 绑定。

2026 年 11 月 18 日会话 `01a1eb27-ddca-7b13-bd84-78c698872d0e` 第 406 条提出至少两次完整观察、持续 30 秒仍缺证时提示，第 698 条用户引用整份方案，第 764 条授权按该方案验证和实施。保留 29/30 秒边界。

首件用临时库和受控轮次响应，让观察器处理同一张已经人工放到待归档列并写有备注的卡。每一步读取根快照，在第 0、29 秒看不到告警，第 30 秒看到；第 31 秒收到可靠旧轮次证据后清除。第二次缺证从第 60 秒重新计时，第 89 秒没有告警，第 90 秒再提示。再用相隔 30 秒的两次完整观察确认第二次就够。各步均核对人工列和备注。

旧测试直接向计数器传入集合，覆盖到第二轮的开始。新测试应能抓住告警未发布、提前或推迟、恢复后未重新计时等错误；计数器换成等价存储后仍通过。告警与重检共用本轮观察时间。此项不操作日用客户端。
