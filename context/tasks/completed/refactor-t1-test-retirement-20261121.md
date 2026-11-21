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

## 未校准的远端时钟

移除 `example::remote_parity_lab::remote_runtime_tests::exposes_cross_host_clock_skew_in_wall_clock_baseline` 及原 AUTO-001 绑定。实验把远端开始时间 50 与本机观察起点 100 直接比较，要求刚开始的执行留在待判断。[实验风险表与时钟投影说明](../../../Docs/REMOTE-PARITY-LAB.md) 保留了这组数值所说明的限制。

[AUTO-001](../../../acceptance/requirements.json) 要求真实开始或恢复后进入进行中。[远程接入维护约定](../../../Docs/REMOTE-SESSIONS.md) 要求有界校时，并拒绝异常采样。保留 `lib::codex_session_board::rpc::runtime::tests::remote_clock_uses_conservative_bounds_and_rejects_unbounded_samples`，实际检查原生轮次时间投影和无效采样。保留 `example::remote_parity_lab::remote_runtime_tests::calibrated_host_clock_reuses_existing_rules_without_a_second_state_machine`，验证同一个开始时间经校正后进入进行中。

## 原生分区移走再移回

移除 `example::remote_parity_lab::remote_runtime_tests::exposes_aba_moves_without_a_server_revision` 及原 AUTO-001 绑定。实验在两次观察之间移走卡片再移回，要求旧表单成功保存。[原生并发保证](../../../Docs/NATIVE-STATE.md) 和[原始实验](../../../Docs/REMOTE-PARITY-LAB.md) 均记录了缺少服务端修订号时无法识别中间移动。

[MANUAL-002 与 UI-002](../../../acceptance/requirements.json) 保留版本和原生位置检查。保留 `test::native_authority::transport::external_move_rejects_an_unobserved_stale_save_and_fresh_save_succeeds`，通过 `/api/state` 验证读到不同原生位置时拒绝旧表单、零移列、备注不变，刷新后可保存。`lib::codex_session_board::formal::versions::stale_edits_reject_each_revision_without_persistent_side_effects` 继续检查状态或输入版本冲突后的数据。

## 本地发现批次中的无 ID 记录

移除 `test::board_contract::isolation_contract_tests::discovery_batch_failure_does_not_leave_partial_inserts`，清理 AUTO-004、MANUAL-001、TITLE-001、TITLE-003 四处绑定。用例直接给 `Store::discover` 一条有效新记录和一条缺 ID 记录，要求整库回到调用前的快照。

查阅了引入提交 `eb2b580`、[冻结迁移包](../../../Docs/verification-baseline/) 中的 `acceptance/test_contract.py` 和 [DISCOVERY-001、DISCOVERY-002](../../../acceptance/requirements.json)。这些出处支持按 ID 发现、失败保留已有记录、单条历史读取失败后继续；未查到要求整批撤销新增记录的决定或故障记录。[本机 RPC 的 `append_threads`](../../../src/rpc.rs) 在向观察器返回列表之前就拒绝缺少字符串 ID 的记录。[Store 的事务](../../../src/store.rs) 仍在原处。

保留 `test::observer_contract::history_list_failure_does_not_remove_or_reclassify_saved_sessions`，实际验证列表失败后原有人工状态保留且健康信息报告错误。`test::observer_contract::history_request_error_marks_only_that_session_and_continues_next_read` 验证同批后续会话仍可补读。两者均调用生产 `scan_history`。

## 文档检查器的根目录

移除 `example::project_check::document_policy::tests::invalid_roots_return_errors_instead_of_success` 及原 GATE-001 绑定。用例直接向内部函数传入不存在的路径、普通文件。[常规入口](../../../scripts/project_check.rs) 从 Git 取得仓库根目录，[暂存入口](../../../scripts/staged_check.rs) 扫描自己导出的目录。查阅引入提交 `39169af` 及[文档检查规范](../../../Docs/CHECKS.md)，未找到把这两个值作为文档扫描根目录的受支持调用或历史事故。

保留 `example::project_check::document_policy::tests::new_files_are_checked_without_a_git_repository_and_sorted_stably`，验证扫描实际目录时能发现新文档；`example::project_check::document_policy::tests::reports_name_and_docs_destination_without_changing_contents` 验证报告违规、给出移动目的地并保留文件。检查器继续向上传播读取错误。[GATE-001](../../../acceptance/requirements.json) 的验收失败保护仍由 `acceptance.test_gate.GateTests.test_skipped_checks_and_nonzero_adapter_exit_cannot_pass` 等用例验证。

## 交付核对

五项均在删除前实跑通过。三个限制反例可在基线提交检出后，以 `cargo test --locked --offline --example remote_parity_lab exposes_` 重放。原研究记录保留当时的实测结论。五项退役只涉及这五个完整 ID 及它们的 11 条绑定关系。

最终提交在 Linux 上执行 `./scripts/check`、`sh scripts/verify_context.sh`、关联测试及 Rust/Python 全量收集核对。平台绑定差异按 Linux 和 macOS 分别核对。源码外证据目录为 `~/.local/state/codex-session-board/t1-retirement-20261121/`，最终核对记录使用所验提交号命名。分支交协调者登记，合入顺序由 Herman 安排。
