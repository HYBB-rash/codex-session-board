# T0 · 归列与观察六项销账

- owner: B1 与观察回归作者
- execution_mode: isolated_write
- coordinator: 五轮重构总协调
- base_commit: `5e2eaa046b9e9ff5731bc6510556c93cebb5d249`
- worktree_path: `/home/herman/.codex/worktrees/t0-placement-observer-closeout/notion-program`
- allowed_write_set: 本卡、同名结果记录、下列六项及其承接测试和对应绑定；生产文件只允许临时故障对照，交付前恢复。

首件以 Linux `work/t0-b859880/ledger.json` 的六个完整 ID 为起点，对照当前主线、B1 快照与 T3 告警。检查哪条测试实际覆盖界面、观察来源、读取预算或零副作用，找出缺少的反例。B1 与 T3 已交付的开发分支独立保留，本包交总协调串行合入。

- `lib::codex_session_board::app::four_columns::tests::only_current_remote_rows_offer_navigation`
- `lib::codex_session_board::observer::four_column_tests::empty_requires_complete_response_and_an_inactive_verified_backend`
- `lib::codex_session_board::observer::tests::idle_rechecks_limit_extra_reads_and_rotate_across_stable_rows`
- `test::board_contract::store_tests::invalid_state_and_oversized_note_leave_all_tables_unchanged`
- `test::observer_contract::runtime_active_threads_are_read_each_poll_while_unchanged_idle_is_cached`
- `test::observer_contract::runtime_incomplete_pages_fail_before_discovery_or_any_turn_observation`
