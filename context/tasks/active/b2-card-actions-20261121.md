# B2 · 卡片操作资格

- owner: B2 作者
- execution_mode: isolated_write
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/98bd/notion-program`
- coordinator: 五轮重构主会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`
- allowed_write_set: 本卡、同名结果记录、卡片操作资格与调用处、对应的浏览器与隔离请求测试及其绑定。

分支 `codex/b2-card-actions`。领取时已 fetch 并读回远端。首件对照 B1 `96ad327` 的最终快照与现行网页、MCP 面板入口。维护者调整归档条件时，应能在同一处看见这条卡的调整、拖动、确认、归档和入选清单结果。根快照合流前的状态优先级与归列由 B1 决定。

先用相同卡片对照单卡按钮与批次清单。断开原本可写的主机后，使用原按钮的版本提交，核对请求结果与写入次数。对有新输入的卡，单卡确认只保存待归档；对待核对的卡，重复操作不得再次提交归档。

## 首件读回

B1 `96ad327` 的根 `/api/sessions` 返回十张卡；B1 与接单基线的 `index.html` 相同。隔离 Chrome 按下列按钮后，实际 POST 清单包含四项。快照、原清单、浏览器结果和运行脚本存于 `/tmp/b2-firstpiece-98bd/`。

| 最终快照或操作状态 | 调整和拖动 | 单卡按钮 | 进入新批次 |
| --- | --- | --- | --- |
| 本机待归档、在线远端待归档、旧 completed | 可用 | 归档 | 是 |
| 本机待归档，有新输入 | 可用 | 确认待归档 | 否 |
| 已断开、只有历史缓存、在线目录已归档 | 禁用 | 无 | 否 |
| 已认领，归档结果待核对 | 可用 | 无 | 否 |
| 旧批次仅登记，尚未提交 | 可用 | 归档 | 是 |
| 未归列 | 可用 | 无 | 否 |

挂起第一个归档请求时，重新创建的卡片已禁用，但批量谓词仍返回可提交。旧实现对原 DOM 的重画也会被保存锁挡住。待核对只限制再次提交归档，原有手动调整在 `/api/state` 实测可用。过期输入版本的调整请求被拒绝。

## 测试归属

- B2 与 T3 承接 `test::archive_flow::archive_mcp_resource_exposes_only_an_explicit_app_action` 中的按钮资格断言。MCP 可见性、破坏性标记及必填版本仍按工具契约核对。
- B2 的承接 ID 为 `lib::codex_session_board::remote::instances::identity_gate_tests::card_actions::card_buttons_and_batch_submit_only_eligible_revisions`，绑定 `MANUAL-003`。Chrome 点击单卡与列按钮，Rust 读回批次名单、状态版本、输入版本和原生写入次数。
- 离线桥由 `test::native_authority::remote_isolation::archive_batches::offline_panel_confirms_new_input_and_archives_only_the_ready_host` 承接。它从构建的 MCP 程序读取面板，验证缓存卡只读、批量名单、提交中的按钮和单卡确认，核对实际工具名、版本及目标主机写入次数。
- B2 扩展 `lib::codex_session_board::remote::instances::identity_gate_tests::managed_write_gate_requires_initialization_and_preserves_notes_after_home_change` 的来源变更拒绝，以及 `lib::codex_session_board::catalog::view_tests::catalog_metadata_preserves_notes_and_explicit_projectless_membership` 的发现前后资格对照。
- `test::native_authority::remote_isolation::remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations` 与 `test::native_authority::remote_isolation::backfill::remote_first_scan_backfills_legacy_rows_and_cold_maps_then_reconnects_without_old_phase_replay` 保留在线、断线和缓存的真实 HTTP 检查。
- E3 承接十卡恢复，原有 `test::native_authority::archive::archive_batch_recovers_lost_receipt_after_reopen_without_replay` 覆盖两卡、重开及丢回执。筛选作者负责 `test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry` 的主机、项目和可见范围，及 `test::ui_layout::host_catalog_filters_preserve_cached_rows_and_isolated_native_drafts` 的目录筛选。

依据为 `Docs/ARCHIVING.md`、五轮委托中的 B2 与 T3，以及 `Docs/REMOTE-SESSIONS.md` 中的缓存与来源边界。真实客户端验收由协调者另行安排。

B1 进入远端后已在作者树重基并重跑首件。首轮复审基线 `3de9e0f00f5458eda255b59e81cc54e63818f071` 包含 B1 `dce3536b70459c2962fa225320ecb88a8cde7522`。D 与四列告警 T3 的复验基线为 `f2a09ef9b685f1cddb1963e0ff885e9da1ad07fa`。最终采用协调者给定的 E1 提交 `fd6aeff666efa9f7e5164d7d008004ad19a4591f`，其中包含 A `72b0199`。分支、审阅与故障注入见[结果记录](../../results/b2-card-actions-20261121.md)。
