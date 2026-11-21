# 标题同步业务测试

- owner: T3 标题同步作者
- execution_mode: isolated_write
- base_commit: `f15754129ac64a1dd02ec93abb00f9237bf0c9f8`
- worktree_path: `/home/herman/.codex/worktrees/refactor-t3-title-behavior/notion-program`
- coordinator: 五轮总协调，Herman 指定合入顺序
- allowed_write_set: 本卡、`examples/remote_parity_lab/{remote_title_tests,remote_archive_tests,surface}.rs` 的测试和夹具、`tests/board_io.rs`、`tests/board_contract.rs`、`src/app.rs` 的测试模块、`acceptance/bindings.json` 对本批的绑定。

分支 `codex/refactor-t3-title-behavior`。T0 的 `b859880` 版工作簿「标题同步作者／随业务替换断言」第 901 至 906 行共六个原 ID。T2 首批修改的是 `test::board_io::old_ack_cannot_delete_newer_title_job` 和 `test::board_contract::title_store_tests::old_ack_and_old_failure_cannot_delete_or_damage_a_newer_job`，这两条均由 T2 维护。

## 六项处置

| 工作簿原 ID | 决定及维护者可以改变的实现 | 用户应继续观察到的结果、承接测试 |
| --- | --- | --- |
| `example::remote_parity_lab::remote_title_tests::delayed_old_receipt_cannot_delete_a_newer_title_intent` | 替代。可改变实验适配层和队列结构。 | TITLE-004。第一版原生改名后有了第二次人工选择，第一版回执不能清掉第二版，重试写出第二版正文。生产路径 `test::board_io::title_ack_during_newer_manual_selection_leaves_new_job_for_retry` 检查写入顺序及两次结果。旧回执返回拒绝由 T2 承接测试核对。 |
| `example::remote_parity_lab::remote_title_tests::incorrect_connection_host_is_rejected_before_any_write` | 保留。实验同时覆盖改名、归档及移列，原生写入计数是三条断言共用的观察点。 | 连接主机错误时三种操作均拒绝，原生零写。正式标题写入连接以实际初始化身份为准，见 `lib::codex_session_board::rpc::framing::tests::initialize_codex_home_is_independent_and_required_before_business_methods`；正确身份可写见 `lib::codex_session_board::rpc::framing::tests::verified_proxy_strips_banner_initializes_and_binds_peer_before_writing`。 |
| `example::remote_parity_lab::remote_title_tests::latest_complete_title_survives_truncated_discovery_and_repeated_sync` | 替代。列表可选择任意摘要长度，原生正文仍为权威来源。 | TITLE-001、TITLE-002。`test::board_io::title_sync_reads_full_fresh_name_and_avoids_duplicate_native_write` 给发现入口短摘要，为原生 RPC 提供同一标题的长正文，检查完整输出及两次入队仅一次原生改名。 |
| `example::remote_parity_lab::remote_title_tests::lost_title_receipt_survives_restart_and_readback_avoids_duplicate_write` | 替代。补偿调度可以改变，持久化、读回和原生写入次数必须可核对。 | TITLE-003、TITLE-004。`test::board_io::title_failure_persists_and_lost_ack_is_reconciled` 在正式同步中丢弃写成功回执，随后重开数据库，验证任务、备注、最终标题及重试前后写入次数。 |
| `example::remote_parity_lab::remote_title_tests::metadata_updates_do_not_become_new_user_input` | 替代。发现可更新名称和时间；用户输入另行识别。 | MANUAL-003。`test::board_contract::store_tests::discovery_updates_metadata_but_never_removes_missing_records` 重新发现已人工确认的会话，读回新名称、原输入版本及待复核标志，并保留已有缺项保护。 |
| `lib::codex_session_board::app::tests::busy_title_sync_does_not_wait_for_background_batch` | 改写原 ID。调度速度可变，取消固定 150 毫秒断言。 | TITLE-003。后台持有标题锁期间，前台状态保存就能返回，备注已保存，当前版本仍在补偿队列。测试先取得前台回执再释锁，超时仅用于结束阻塞测试。 |

需求见[冻结契约](../../../acceptance/requirements.json)和[远程接入说明](../../../Docs/REMOTE-SESSIONS.md)。[实验报告](../../../Docs/REMOTE-PARITY-LAB.md)记录标题、主机隔离及输入识别的历史来源。各项实测和独立审阅回执保存在源码外 `~/.local/state/codex-session-board/t3-title-20261121/`。

首轮独审发现三处没有区分正确与错误实现的断言。主机实验现在对每种可执行操作仅切换连接报告的身份，先核对错主机零写，再用相同数据从正确主机写成一次。丢回执重开后必须实际读取目标会话。元数据改名和非空批次遗漏旧会话分两轮发现，各自读回。五个独立业务故障对照覆盖旧回执误清队列、原生正文截断、恢复时重复改名、元数据误增输入版本及前台保存被后台锁阻塞。
