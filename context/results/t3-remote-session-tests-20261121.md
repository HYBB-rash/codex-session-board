# 远程会话测试处置

## T3 远程会话作者的 18 项

四个实验测试改由生产测试承接，三个保留原 ID 并调整断言，十一项保留原样。每项均核对仓库内的需求或历史任务。原型的受控 RPC 最初是用户指定的可复现交付，出处为 `context/tasks/completed/remote-parity-lab.md:10`。下表分别标出实验和产品入口。

| T0 原测试 ID | 最终处理 | 需求、场景及核对结果 |
|---|---|---|
| `example::remote_parity_lab::host_isolation_tests::malformed_thread_and_traversal_host_cannot_escape_partition` | 由生产测试取代 | `Docs/REMOTE-SESSIONS.md:76`。`lib::codex_session_board::remote::hosts::tests::host_paths_cannot_escape_and_ssh_options_cannot_be_injected` 检查带上级目录片段的 hostId 生成的路径仍直接位于主机根目录。`lib::codex_session_board::remote::scans::tests::failed_remote_page_keeps_its_cursor_while_another_host_completes` 令一条正确 UUID 后紧跟无效 UUID，整页拒绝写入、旧卡仍在、重试仍用原游标。 |
| `example::remote_parity_lab::host_isolation_tests::mixed_host_page_is_rejected_without_partial_import` | 保留 | `Docs/REMOTE-PARITY-LAB.md:110`。同页两条记录复用同一个 UUID，第二条属于别的主机，整页拒绝；另试整批声明的来源与分区不一致。生产扫描分别覆盖其他主机发回新 UUID 和已保存 UUID，同一页内的两条使用不同 UUID。 |
| `example::remote_parity_lab::host_isolation_tests::same_uuid_on_three_hosts_keeps_independent_durable_state` | 保留 | `Docs/REMOTE-PARITY-LAB.md:110`、`Docs/REMOTE-SESSIONS.md:76`。本机与两个远端共用 UUID，各写不同备注，重开三个库后分别读回本机备注与各自的一笔待写标题。生产归档批次检查同 UUID 的墓碑与回执，缺少三笔标题队列的区别。 |
| `example::remote_parity_lab::model_replay_tests::all_520_frozen_edges_replay_through_host_adapter_and_native_writes` | 去掉重复计数断言 | `acceptance/README.md:133`、`Docs/REMOTE-PARITY-LAB.md:29`。继续比较 40 个状态的 13 个动作、520 条边、1,417 步、40 次重启和 40 次查看，以及状态、备注、人工标记和版本增量。原先重复的下限 `steps >= 520` 已由精确的 1,417 覆盖。 |
| `example::remote_parity_lab::remote_discovery_tests::batch_scope_is_frozen_and_project_ids_are_scoped_by_host` | 保留 | `Docs/REMOTE-PARITY-LAB.md:124`。带项目的批次只选所属主机同名项目里的可归档行；生成批次后原卡版本改变，批次仍持有原版本。无项目筛选时三张合格卡均入选。生产归档回执和逐卡拒绝不能替代这组入选条件。 |
| `example::remote_parity_lab::remote_discovery_tests::catalog_failure_or_last_project_removal_does_not_erase_known_hosts` | 补齐缺少的成功空目录输入 | `Docs/REMOTE-PARITY-LAB.md:126`、`:127`。目录报错时，已登记及仅在缓存中的主机均保留并报告错误。原第二种输入同时移除了主机；新增成功返回空项目、仍登记该主机的对照，两台应继续存在。显式移除主机后只保留另一台。 |
| `example::remote_parity_lab::remote_discovery_tests::duplicate_cursor_and_cross_host_page_fail_without_erasing_prior_data` | 由两项生产测试取代 | `Docs/REMOTE-PARITY-LAB.md:129`、`Docs/REMOTE-SESSIONS.md:81`。`lib::codex_session_board::remote::scans::tests::remote_pagination_survives_restart_and_bad_pages_do_not_erase_records` 拒绝重复游标，再用原游标完成；`lib::codex_session_board::remote::scans::tests::failed_remote_page_keeps_its_cursor_while_another_host_completes` 核对错主机页无部分写入，已有卡片保持可读。 |
| `example::remote_parity_lab::remote_discovery_tests::failure_keeps_progress_and_does_not_prevent_another_host_page` | 由生产测试取代 | `Docs/REMOTE-PARITY-LAB.md:125`、`:128`。`lib::codex_session_board::remote::scans::tests::failed_remote_page_keeps_its_cursor_while_another_host_completes` 按顺序调用生产扫描：第一台续页断线后，第二台完成同 UUID 的扫描，第一台此时仍未完成；用原游标恢复后读回它自己的持久卡片。`tests/native_authority/remote_isolation.rs:348` 另测远程扫描阻塞期间本机 HTTP 写入可完成。 |
| `example::remote_parity_lab::remote_discovery_tests::next_page_progress_survives_process_restart` | 由生产测试取代 | `Docs/REMOTE-PARITY-LAB.md:128`、`Docs/REMOTE-SESSIONS.md:81`。`lib::codex_session_board::remote::scans::tests::remote_pagination_survives_restart_and_bad_pages_do_not_erase_records` 重开 App 数据目录，第一次请求必须是保存的游标，成功续页只补读一次首页，空末页后核对持久卡片、活动身份和完成状态。 |
| `example::remote_parity_lab::remote_runtime_tests::calibrated_host_clock_reuses_existing_rules_without_a_second_state_machine` | 保留 | `Docs/REMOTE-PARITY-LAB.md:120`、`:135`，`Docs/REMOTE-SESSIONS.md:95`。远端开始时间 50 早于本机观察起点 100，已确认的时差 100 将其投影到起点之后，最终进入进行中。采样误差、超时及未校准反例不能取代这个成功端到端判断。 |
| `example::remote_parity_lab::remote_runtime_tests::complete_user_ids_drive_review_and_survive_restart_without_metadata_false_alarms` | 补齐元数据对照 | `Docs/REMOTE-PARITY-LAB.md:121`、`acceptance/requirements.json:109`。人工确认后仅改标题和更新时间，输入版本与复核标记保持；随后同轮追加消息 ID 才提升输入版本并拒绝旧编辑。最后重开分区核对提示和原备注。原测试名称提到元数据，正文此前没有注入这项输入。 |
| `example::remote_parity_lab::remote_runtime_tests::execution_approval_input_recovery_and_completion_reuse_the_same_state_rules` | 保留 | `Docs/REMOTE-PARITY-LAB.md:118`、`Docs/REMOTE-SESSIONS.md:16`。同轮先执行、等待审批、恢复、等待交互、再恢复、最后完成。逐个核对两次阻塞、两次恢复和终态；冻结模型逐边比较，另保留同一远端会话连贯经历两种等待的场景。 |
| `example::remote_parity_lab::remote_runtime_tests::late_snapshot_and_runtime_from_a_previous_connection_are_rejected` | 保留 | `Docs/REMOTE-PARITY-LAB.md:130`、`context/tasks/completed/remote-sections-20261114.md:14`。重连使原代次的位置回包和执行回包均失效。两种迟到结果分别被拒，原列保留且未发原生移动。只测试保存后迟到的位置回包，会漏掉旧运行信号。 |
| `example::remote_parity_lab::remote_runtime_tests::manual_choice_survives_repeated_phase_but_new_phase_takes_effect` | 保留 | `Docs/REMOTE-SESSIONS.md:16`、`Docs/REMOTE-PARITY-LAB.md:118`。远程运行中手选待归档并加备注，同阶段重报保留人工列且不多写一次；转为等待审批时才进入阻塞且仍保留备注。原生写入次数在此用于查无意重复提交。 |
| `example::remote_parity_lab::remote_runtime_tests::pinned_threads_are_not_moved_by_new_automatic_phases` | 保留 | `Docs/REMOTE-SECTIONS-IMPLEMENTATION-20261115.md:50`、`Docs/REMOTE-PARITY-LAB.md:115`。读取远端已置顶的位置，注入新的活跃阶段，原生位置保持且零次写入。生产 `tests/native_authority/faults.rs:236` 检查置顶与自定义分区；实验额外观察远端 RPC 的写入。 |
| `lib::codex_session_board::remote::hosts::source::tests::empty_alias_uses_hostname_without_interpreting_shell_characters` | 保留 | `Docs/REMOTE-SESSIONS.md:60`、`context/tasks/completed/remote-connection-identity-worker.md:14`。已登记连接的别名为空时，`Host::command` 构造的参数数组将包含分号的 hostname 放在一个 SSH 参数中。普通别名测试不会覆盖回退分支。此用例服务现有连接，新建项目的原生别名是另一入口。 |
| `lib::codex_session_board::remote::hosts::source::tests::oversized_identity_is_rejected_before_publishing_an_unreadable_source_record` | 保留 | `Docs/REMOTE-SESSIONS.md:69`。第一次准备绑定便遇超长端点，必须报错，磁盘上也不能留下下次无法读取的来源记录；已有损坏记录保留是另一种起始条件。17,000 是触发体积限制的测试输入，需求没有规定这个数字。 |
| `lib::codex_session_board::rpc::capture::tests::ssh_g_capture_bounds_stdout_stderr_and_never_reports_raw_diagnostics` | 保留 | `context/tasks/completed/remote-connection-identity-worker.md:14`、`Docs/REMOTE-SESSIONS.md:61`。端点探测的标准输出过大、错误输出过大、命令以失败状态退出，三种情形均报错并保持来源记录未创建。日志哨兵检验故障文案没有透出原始诊断；未找到约定具体错误措辞或哨兵内容的产品条款。 |

另按协调卡保留 `example::remote_parity_lab::host_isolation_tests::host_re_registration_requires_explicit_migration`。`Docs/REMOTE-PARITY-LAB.md:111` 要求主机重新登记时显式迁移。同一会话换新 hostId，旧备注只能在原登记下读回，新登记的备注为空。源记录损坏时禁止接管属于另一条故障路径。

实验分页器及其 SQLite 游标记录、模拟 `list_threads` 和排队回包均已删除。生产兼容协议 `tests/remote_sessions/discovery.rs:13` 仍单独核对 hostId 和首、续页参数。实际 SSH 与桌面没有参与本批离线验证。

## T2 旧快照与恢复告警

| 原测试 ID | 处理与承接测试 ID | 原始需求、输入和读回 |
|---|---|---|
| `lib::codex_session_board::catalog::worker::continuity_tests::old_snapshot_without_account_identity_cannot_continue_membership` | 合入 `lib::codex_session_board::catalog::worker::continuity_tests::unknown_or_missing_account_identity_never_continues_previous_membership` | `Docs/REMOTE-CATALOG-DESIGN-20261114.md:127`。两次发布之间丢失后端项目时，两轮必须确认同一账户才可沿用旧归属。新增输入从旧快照移除整个 `accountFingerprint` 键，另保留显式 `null`、单侧有效和相同无效值。每种输入均读取第二次发布后的项目 ID。 |
| `lib::codex_session_board::remote::scans::restore_tests::restoration_warning_is_durable_when_a_later_runtime_read_disconnects` | 独立保留，改正故障注入。对照 `lib::codex_session_board::remote::scans::restore_tests::restoration_warnings_reach_root_and_survive_unrelated_pages_restart_and_expired_cursor` 补查触发错误的游标。 | `Docs/NATIVE-STATE.md:128`。归档核对先失败，同页另一张卡的运行读取随即失败，扫描来不及完成。原夹具只发响应超帧。现针对第二张卡分别注入并核对连接重置、原有超帧错误，再重开目录确认告警可见、原卡仍隐藏。对照项从正常完成的分页开始，覆盖根看板提示、无关页、重启、原游标 `last` 失效，以及再次确认后的告警清除。 |

两次故障注入均已撤销。允许缺少整个账户键的旧快照继承后，合并测试读到旧 `known`，预期为当前 `guessed`。移除运行读取失败前的告警保存后，断线测试在检查持久告警时失败，另一条恢复测试仍通过。
