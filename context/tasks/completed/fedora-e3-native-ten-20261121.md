# Fedora E3 原生十卡核对时间

owner: Fedora 原生十卡修订作者
execution_mode: isolated_write
allowed_write_set: tests/native_authority/ten_card_archive.rs、tests/native_authority/archive.rs、本任务卡
base_commit: c0db37aee261bd35a56cb5b56c4b464433ba7445
worktree_path: /home/herman/.codex/worktrees/fedora-e3-native-ten/notion-program
coordinator: 唯一合入队列与 T0

承接 Mac 作者 `58ec7d7` 在队列中的 Fedora 重放。原测试 `test::native_authority::remote_isolation::archive_batches::ten_cards::ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes` 在真实 HTTP 读回和根库直读间按条目状态核对 `checkedAt`。待核对和尚未提交须有键且为 null；已归档、已核实、新输入和拒绝须有有限正时间，两侧最多差一个可表示步长。已完成批次的创建与结束时间也须为有限正数。

两次重启、十张卡的逐项状态、归属和逐卡原生写入次数沿用原场景。原生 WebSocket 夹具用等价的文本消息构造发送响应，符合整合后的 Clippy 行数限制。

Fedora 定向实跑完成两次服务重启。本地和另一远端各写入零次，目标远端逐卡写入 `[1,1,1,1,0,0,1,0,0,0]`。独立验证在批次结束后直接改写三条已归档记录或三条尚未提交记录，两侧分别读到相同的 `null` 或 `12345.25`。两种注入在 `c0db37a` 均退出 0，在修订版均由真实读回断言拒绝。原 `MANUAL-001` 绑定与测试 ID 保留。
