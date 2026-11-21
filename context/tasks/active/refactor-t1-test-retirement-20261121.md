# T1 首批测试精简

- owner: T1 作者，Codex 会话 `01a1fc65-41ae-7ba3-b5a0-1c7c35c97c8c`
- execution_mode: isolated_write
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/15ec/notion-program`
- coordinator: Herman 五轮委托协调会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`
- allowed_write_set: 本卡、`examples/remote_parity_lab/{remote_title_tests,remote_runtime_tests,surface}.rs`、`tests/board_contract.rs`、`scripts/document_policy_tests.rs`、`acceptance/bindings.json` 中本卡五个原 ID 的绑定。

分支 `codex/refactor-t1-test-retirement` 在独立、干净的工作树上领取。`git fetch origin main` 及 rebase 后，HEAD、FETCH_HEAD 和 `origin/main` 均为上述基线。相对 T0 旧基线 `565bfaa`，本批五项的测试定义文件和远程实验报告无差异。

## 首件：读后写之间的标题竞争

移除 `example::remote_parity_lab::remote_title_tests::exposes_title_overwrite_between_read_and_unconditional_write` 及它独占的 RPC 夹具字段。原来绑定 TITLE-001 至 TITLE-004。

旧实验在原生读返回后、`thread/name/set` 写入时模拟另一客户端改名；只有新正文消失才通过。实验原始断言见提交 `21770a7`。[远程实验的验证记录与风险表](../../../Docs/REMOTE-PARITY-LAB.md) 记载了缺少条件写的竞争窗口；基线重跑该例通过。日后上游支持条件写时，保留新正文符合标题同步的目的。

[TITLE-001 至 TITLE-004](../../../acceptance/requirements.json) 要求读取时取得最新完整正文、幂等投影、失败补偿和队列版本保护。`App::sync_title` 使用生产 `titles::sync`。下列 [board_io 测试](../../../tests/board_io.rs) 调用生产函数和真实 Store，原生 RPC 由夹具控制。

| 支持的故障场景 | 承接测试完整 ID |
| --- | --- |
| 列表正文过短，原生正文含长文本、表情及其他标签；重复保存沿用已有完整标题 | `test::board_io::title_sync_reads_full_fresh_name_and_avoids_duplicate_native_write` |
| 读标题时出现更新的状态，旧任务必须零写入，新任务使用最新前缀 | `test::board_io::newer_state_before_write_prevents_stale_title` |
| 连接失败后重开数据库，写入成功却丢回执；读回后清除任务，原生只写一次 | `test::board_io::title_failure_persists_and_lost_ack_is_reconciled` |
| 旧回执到达时已经有新状态，较新的队列项必须继续执行 | `test::board_io::title_ack_during_newer_manual_selection_leaves_new_job_for_retry` |

这些场景覆盖读取时的正文、看板状态与回执之间的竞争。跨客户端在读后改名仍有研究记录中的窗口。历史实验中的 33 项是 2026-11-07 当时的运行数量。
