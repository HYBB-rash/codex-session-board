# 远程会话测试处置

## T2 旧快照与恢复告警

| 原测试 ID | 处理与承接测试 ID | 原始需求、输入和读回 |
|---|---|---|
| `lib::codex_session_board::catalog::worker::continuity_tests::old_snapshot_without_account_identity_cannot_continue_membership` | 合入 `lib::codex_session_board::catalog::worker::continuity_tests::unknown_or_missing_account_identity_never_continues_previous_membership` | `Docs/REMOTE-CATALOG-DESIGN-20261114.md:127`。两次发布之间丢失后端项目时，两轮必须确认同一账户才可沿用旧归属。第五种输入从旧快照移除整个 `accountFingerprint` 键，另保留显式 `null`、单侧有效和相同无效值。每种输入均读取第二次发布后的项目 ID。 |
| `lib::codex_session_board::remote::scans::restore_tests::restoration_warning_is_durable_when_a_later_runtime_read_disconnects` | 独立保留。对照 `lib::codex_session_board::remote::scans::restore_tests::restoration_warnings_reach_root_and_survive_unrelated_pages_restart_and_expired_cursor` 同样保留。 | `Docs/NATIVE-STATE.md:128`。归档核对先失败，同页另一张卡的运行读取随即断线，扫描来不及完成。第一项重开数据目录，确认告警仍能显示且卡仍隐藏。对照项从正常完成的分页开始，覆盖根看板提示、无关页、重启、游标失效与确认后清除。 |

两次故障注入均已撤销。允许缺少整个账户键的旧快照继承后，合并测试读到旧 `known`，预期为当前 `guessed`。移除运行读取失败前的告警保存后，断线测试在检查持久告警时失败，另一条恢复测试仍通过。
