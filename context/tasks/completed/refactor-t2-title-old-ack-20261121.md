# T2 首批 · 标题同步旧回执

- owner: T2 作者会话 `01a1fc65-7a91-7440-a579-bc28c74a235c`
- execution_mode: isolated_write
- allowed_write_set: 本卡、同名 `context/results/` 记录、`tests/board_io.rs`、`tests/board_contract.rs`、`acceptance/bindings.json` 中本组两个 ID；`src/store.rs` 仅用于临时故障实验。
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/b38b/notion-program`
- coordinator: 五轮重构协调会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`

T0 已在 Linux 收集到这两个完整 ID。协调者复核 `565bfaa..b859880` 的相关定义和实现，作者在本工作树 fetch、rebase 到 `origin/main` 后领取此组。

| 角色 | 测试 ID |
| --- | --- |
| 原入口 | `test::board_io::old_ack_cannot_delete_newer_title_job` |
| 承接者 | `test::board_contract::title_store_tests::old_ack_and_old_failure_cannot_delete_or_damage_a_newer_job` |

首件核对 TITLE-004。当用户先后保存两次状态、第一版回执迟到时，回执应返回未采纳，重试队列应仍能读到第二版。将这个直接读回放在旧回执和旧失败之间。核对其他断言后删除原入口，更新两者的需求绑定，并通过误删任务的故障和恢复后的同一测试验证。协调者负责登记、归档及安排后续候选。
