# T2 首批 · 旧标题回执的承接证据

TITLE-004 要求旧版本的回执不能清除新任务。用户连续改两次状态后，同步器还要能从重试队列读到第二版。原入口只调用 `Store`，没有发送原生改名请求。

| 角色 | 完整测试 ID | 原绑定 | 本次绑定 |
| --- | --- | --- | --- |
| 删除 | `test::board_io::old_ack_cannot_delete_newer_title_job` | TITLE-001、002、003、004 | 无 |
| 保留 | `test::board_contract::title_store_tests::old_ack_and_old_failure_cannot_delete_or_damage_a_newer_job` | TITLE-001 至 005 | TITLE-004 |

两条基线测试在 Linux 各通过一次。原入口的两个断言分别检查旧成功回执返回 `false` 和 `pending_titles(None)` 的第一条记录为第二版。承接者已有前一个断言。现在它在收到旧成功回执后、处理旧失败前，立即检查公开队列恰有一项且 `version == 2`。

| 故障 | 承接者怎样发现 |
| --- | --- |
| 新选择未替换旧任务 | 第一次保存后 `title_current` 接受第一版；第二次保存后只接受第二版。 |
| 错误接受旧回执 | `title_synced` 返回值必须为 `false`。 |
| 拒绝旧回执却删除新任务 | 公开队列长度必须为 1。注入按会话误删的故障后，该断言实测得到 0，退出 101。 |
| 队列仍在却返回旧版本 | 读回的任务版本必须为 2。仅让队列查询少报一版后，数量断言通过，版本断言实测得到 1，退出 101。 |
| 旧回执改写标题、旧失败污染重试记录 | 旧成功和旧失败处理后，比较会话、变更历史、标题队列和设置四张表与第二次保存后的完整快照。 |
| 新回执无法写入，或又接受重复回执 | 第二版回执须返回 `true` 并在会话中读回新标题。再次回执须返回 `false`，四表快照保持一致。 |

两次实验使用同一条命令。每次恢复 `src/store.rs` 后，同一测试都通过。故障补丁、原始失败及恢复日志在本机源码外的 `~/.local/share/codex-session-board-investigations/refactor-t2-title-old-ack-20261121/`。

```sh
cargo test --locked --offline --test board_contract -- title_store_tests::old_ack_and_old_failure_cannot_delete_or_damage_a_newer_job --exact
```

Linux 验收适配器实际收集 736 个 Rust 和 221 个 Python 测试，共 957 个，与绑定集合相同。对比基线，绑定差异仅涉及上表两个 ID。31 条需求、参考模型和契约锁均与基线一致。跨原生写入的回执交错仍由 `test::board_io::title_ack_during_newer_manual_selection_leaves_new_job_for_retry` 单独验证。
