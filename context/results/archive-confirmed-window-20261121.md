# 最近二十批之外的归档回执

用户在看板上归档会话，Codex 已执行归档，目标主机的 SQLite 却拒绝写入归档标记。另有二十批结束后，原批次退出历史列表。基线 `7b904a7` 此时会重新显示原来的活动卡。

根库匹配完整会话身份、状态版本、输入版本以及启用原生分区时的分区版本。快照读和归档写入共用该判断。相同 UUID 的另一主机及归档历史仍可见。普通索引适用于已有数据库，批次列表仍返回最近二十批和所有待核对批次。

主机观察到曾经落标失败的会话再次活动时，从当前来源读取活动与归档列表，并复核账号、Codex Home 与对端身份。来源匹配、活动列表出现、归档列表完整且没有该 ID 后，目标库在事务中复核版本并提升一次。原有备注和分区保留。没有额外后台线程。

## 回归入口

- `cargo test --test native_authority remote_isolation::archive_batches` 覆盖即时成功及丢失原生回执后确认。目标库拒写、二十个新批次、服务重开、旧库多来源、断线、错误账号、原生恢复和直写/批次重放使用隔离服务。旧修订原生只写一次，恢复后的新修订可以归档。
- `cargo test --test native_authority archive::archive_batch_recovers_lost_receipt_after_reopen_without_replay` 也覆盖本机非原生分区，直接提交伪分区版本仍被拒绝，原生恢复后只提升一次版本。
- `cargo test --lib snapshot_respects_native_positions_and_remote_presence` 在同时持有本机与远程主机的独占生命周期锁期间读取快照。`cargo test --lib card_buttons_and_batch_submit_only_eligible_revisions` 分别变更三种版本、主机和历史标志。
- Fedora、项目锁定的 SQLite 3.51.1、1 万批和 10 万项。12 次热查询中，按 1,500 张卡批量查询的中位数为 1.40 毫秒，单卡为 0.0044 毫秒。计划按 `archive_receipt_by_id` 查找各卡，旧查询仅扫描待核对部分索引，单测量其 SQL 中位数为 0.0019 毫秒。
