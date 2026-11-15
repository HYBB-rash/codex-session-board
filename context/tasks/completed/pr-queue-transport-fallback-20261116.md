# 原会话分页读取不支持时的精确回执核对

owner: /root/queue_security_review
execution_mode: isolated_write
allowed_write_set: scripts/pr_queue_transport.py; acceptance/test_pr_queue_transport.py; acceptance/bindings.json; context/tasks/active/pr-queue-transport-fallback-20261116.md
base_commit: 4e2a26151fc45598274725c463017a26fd0f9773
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-transport-fallback-20261116/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

协调者要求修复 PR12 真实派单暴露的兼容问题。建树前工具发布树干净，
固定为上面的 0.3.92 源码。冻结 checker、业务、版本及共享 STATE 均不修改。

只有 thread/items/list 返回 -32601 且说明该方法尚不支持时，才改用同一已核验
连接的 thread/read includeTurns。再次核对原会话 ID，只在准确轮次中识别助手
精确 ACK。未知送达只可用唯一 userMessage clientId 对账；没有证据继续 unknown，
绝不重新派单。其他 RPC 错误、错误会话或缺失/重复轮次均拒绝。

两个读取入口复用 ACK 与轮次状态处理。观察结果完整成功后才更新回执，
避免中途身份错误留下新的通过状态。没有原生完成时间戳时以观察时间为上界，
显式记录 ack_time_basis=observed_upper_bound，不伪造精确 ACK 延迟。

新增三项回归并接入已有 GATE-001 绑定。旧 4e2a261 源码在新增精确轮次
回归中因不支持方法而失败；修复后运输模块九项回归全部通过，0.034 秒。
最终组合 CI 由协调者执行。

2026-11-16 01:56:17 +08:00 使用 PR12 真实 route/request 和 sent 回执复制件，
仅执行 poll，0.120 秒得到准确原 thread/turn 的 item-171 助手 ACK。
实测调用只有 initialize、thread/read、thread/items/list，没有 turn/start/resume。
原始回执哈希前后相同。原生 item 没有完成时间戳，记录观察上界，
不把这次轮询耗时或派单至轮询间隔当作作者 ACK 延迟。

证据目录：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr-queue-20261115/transport-fallback/
其中 baseline-regression.log、targeted-tests.json、real-poll.json 与
real-poll-receipt-copy.json 分别保留旧失败、回归、只读实测和新回执复制件。

20029ec修复经独立审阅通过，最终随0.3.93、61d9d280进入真实主线。
CI43746211763双平台组合与实际包隔离启动通过。无时间戳的ACK仍只记录
观测上界，未据此声称精确通知延迟。
