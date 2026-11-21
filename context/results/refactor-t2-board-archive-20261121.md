# T2 看板和归档测试处置

四个原入口中，三个并入已有测试。首次观察旧失败保留独立入口。基线 `808a0f5` 上逐个运行四对原入口与预定承接者，八项均通过。

| 原 ID | 承接 ID 或保留理由 | 承接者直接验证的需求 |
| --- | --- | --- |
| `test::archive_flow::archive_rejects_malformed_user_items_even_when_other_inputs_match` | `test::archive_flow::archive_unverifiable_native_input_never_reaches_the_write` | MANUAL-002、BOUNDARY-001 |
| `test::board_contract::isolation_contract_tests::test_disabled_title_sync_saves_without_native_writes` | `test::board_io::disabled_title_sync_persists_without_native_jobs` | MANUAL-001、TITLE-005、BOUNDARY-001 |
| `test::board_contract::runtime_tests::empty_turn_identifier_does_not_create_runtime_state_or_title_work` | `test::board_contract::runtime_tests::irrelevant_or_unidentified_runtime_evidence_does_not_write` | AUTO-001、AUTO-004、AUTO-006 |
| `test::board_contract::runtime_tests::test_first_observation_does_not_reclassify_old_failure` | 独立保留。`test::board_contract::runtime_tests::old_failure_boundary_uses_observer_start_and_persists_deduplication` 继续覆盖显式起点和重启。 | 前者 AUTO-002、AUTO-006、TITLE-005；后者 AUTO-002、AUTO-006 |

## 保留的观察与故障对照

归档用户需要原生最新输入可核对。前三例仍用空预期核对缺少数据、缺少轮次字段和仍在执行的轮次。另三例使用预期匹配的旧轮次和有效消息，额外混入 ID 为 `null`、数字或空串的用户消息。断言区分无法核对和仍在执行，并核对三次调用依次为 `thread/list`、`thread/read`、`thread/turns/list`。对照实验仅给假 RPC 返回匹配且完整的消息，归档模拟成功，要求拒绝的断言失败。

关闭标题同步的用户仍能保存状态和备注。应用测试向 `/api/state` 提交备注，按会话 ID 读回 `completed`、原备注、原名，同时检查待同步队列为空、启用时间为 `None`。夹具中有两个同名会话。对照实验省略请求中的备注，读回空串，备注断言失败。

运行观察者收到 active/inProgress 和显式空串 ID 时，须返回 `false`，会话、变更、标题队列和设置四表应与调用前相同。缺失 ID 仍是单独一例。对照实验只给显式空串换上有效 ID，真实开始被接受，该行的返回值断言失败。

首次旧失败用开启标题同步的 Store，观察时不传起点。它检查状态仍为 unknown、版本为 0、审计和队列均为空，再验证实际开始后的失败进入 blocked。对照实验在独立临时库加入错误的标题任务，仅空队列断言失败。预定承接者关闭标题同步，传入观察起点，分别验证 failed/interrupted、重启去重和时间相等时进入 blocked。关闭同步时队列本就为空，且显式起点走的是另一种判断。分别保留两条路径便于定位。

四次对照均在源码外的测试副本执行，退出 101。原始夹具下五个保留入口均通过。相关 ID 位于 Linux 和 macOS 共用的绑定；本包实际运行平台为 Linux。源码外证据目录为 `~/.local/share/codex-session-board-investigations/t2-board-archive/`。
