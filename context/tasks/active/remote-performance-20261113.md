# 远端连接与状态读取性能

owner: codex
execution_mode: isolated_write
allowed_write_set: src/rpc*, src/remote/connection_source*, tests/, examples/, scripts/, Docs/, acceptance/bindings.json, context/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json
base_commit: c15946e647b222029ee5938ec2ebdde740833155
worktree_path: /Users/hongyong/Developer/codex-session-board-remote-perf-20261113
coordinator: 当前会话

用户授权实现远端连接复用、状态摘要读取，并通过真实远端操作验证收益。
加入必须通过的性能阈值，防止后续改动撤销收益。保留来源、输入版本和归档保护。
在隔离实例中使用真实主机读取和看板状态操作，不安装或重载正式插件。
先实现并验证连接所有权与恢复，再验证摘要语义，最后记录实测和回归门禁。
完整客户端验收缺证继续如实报告，不用性能结果替代原 31 条验收。
