# C1 远程归档的数据归属

owner: codex/refactor-c1-archive-ownership
execution_mode: isolated_write
allowed_write_set: src/app/archive_batches.rs, src/app/archive_batches/, tests/native_authority/remote_archive_batches.rs, tests/native_authority/remote_isolation.rs, acceptance/bindings.json, context/tasks/active/refactor-c1-archive-ownership-20261121.md, context/results/refactor-c1-archive-ownership-20261121.md
base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
worktree_path: /Users/hongyong/.codex/worktrees/c1f3/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb

2026-11-21 已 fetch origin/main，在上述基线建立命名分支并 rebase，无上游差异。

维护者需要简化的动作：修改远程批次提交或核对时，可以直接看清根库的批次结果、
目标主机库的会话状态及绑定来源的原生连接，不需追踪含义不断变化的 self/app。
沿用现有持久批次、停止和只读核对入口，不增加恢复流程。

首件：隔离双主机 fixture 追踪 mac 上一条会话，根库记录名单和 pending，
目标主机原生成功但回执丢失，重开仍 pending，只读核对只更新原目标库，
累计 thread/archive 为 1。相同 UUID 的本机和 other 会话保留。

疑点：现有端到端批次回归仅覆盖本机。先验证远程来源绑定、重开读回和核对归属，
再决定最小结构调整。SQLite 多个独立连接没有跨库事务承诺，保留先写根库 pending
和独立读回恢复，不将目标库与批次结果伪装成一个原子提交。

范围仅 C1 远程归档批次与核对，不改保存回执、T0 清单、C2/E3、版本、
31 条需求、独立参考模型、锁或现有故障断言。使用已有连接层和临时 fixture，
不操作日用 Codex，不推送、不合入 main、不部署。

认领测试完整 ID：

- test::native_authority::remote_isolation::archive_batches::remote_batch_owner_recovers_a_lost_receipt_once_and_preserves_same_uuid_on_other_hosts
- test::native_authority::remote_isolation::archive_batches::remote_batch_owner_persists_new_input_refusal_without_a_native_write
- test::native_authority::remote_isolation::archive_batches::remote_batch_owner_keeps_pending_when_the_bound_source_changes
- test::native_authority::remote_isolation::archive_batches::remote_batch_owner_persists_a_success_receipt_in_root_and_confirms_only_the_target

最终提交要求：每步至多 13 文件、400 行，审阅暂存快照并执行
./scripts/check --pre-commit，使用 agent-git commit。最终准确头执行 ./scripts/check、
相关测试与独立只读审阅，修复后重审。任务登记、共享状态及归档由协调者处理。

首件已在未改动业务实现的上述基线运行，三项测试通过，14.14 秒。
使用 TMPDIR=/private/tmp cargo test --locked --offline --test native_authority
remote_isolation::archive_batches:: -- --nocapture --test-threads=1。
原沙箱 Unix socket 监听失败保留 /private/tmp/c1-archive-first-fixture.log，
授权隔离运行证据为 /private/tmp/c1-archive-first-fixture-unrestricted.log。

查看归档结果的用户依赖丢回执后的 pending 跨重开保存，核对只读且只更新原目标。
准备归档的用户依赖原生新输入拒绝成为固定结果，重开和核对不重新发送写入。
切换账号的用户依赖来源不符时保持 pending，恢复原账号后才确认。
正常收到回执的用户依赖根库固定 archived 结果，只确认原目标主机库，重开不重放。

首件作为独立本地提交 1ae672f 保存。实际操作已集中到 HostArchive，
构造时分别传入 root_batches 和 target_host。提交使用目标主机连接读取来源，
根库 claim 后调用一次已有归档实现；核对只用 CatalogReader，
目标主机库 confirm_archive 与根库 record_archive_result 在真实调用处分别表达。
改动后的首轮相关测试 16 项 native_authority 归档及 11 项 archive_flow 通过，
相关 lib/test Clippy 通过。正常回执用例及最终提交验证继续执行。
