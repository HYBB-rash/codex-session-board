# 检查会话控制的三条性质

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, tests/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 5bcda56959351905671bf3e4696483f53cdfca15
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

在已穷尽的会话控制状态机上检查三条性质。
未读会话不能由 Store 写上列。
选择时间只由人工挪列写入，读回不能制造选择。
已归档会话不能留下待确认阶段。
性质失败时改 Store 和模型，再重放全部可达边。
不修改冻结的 acceptance/model.py 和契约锁。

三条性质都已落到 Store。未读会话直接写列会被拒绝。
读回不再写入选择时间，选择只来自人工挪列。
归档确认会清掉待确认阶段。
改完后从空白未读会话得到 5242 个可达状态，39 个事件，204438 条边，全部与 Store 一致。
实现提交 8a8d5ba。
