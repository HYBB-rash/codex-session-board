# 远程会话自动分区与映射恢复

owner: codex
execution_mode: isolated_write
allowed_write_set: src/native_sections*、src/app*、src/remote*、src/store*、src/rpc*、src/config*、tests、acceptance/bindings.json、Docs、当前任务记录、源码外本任务实验资料
base_commit: 91a4fe847140ad54daa744519b0112e271527ab4
worktree_path: /Users/hongyong/Developer/codex-session-board-remote-sections-20261114
coordinator: codex
status: active

用户授权实现调查报告中的映射校验、缺失校准和写后读回方案，
覆盖已有远程会话、新主机、断线重连与分区重建，并执行相应验证。
不修改 Codex 桌面端，不添加待核对业务分区。
保留人工选择及未知/断连时的原状态，不按同名分区猜测绑定。
业务连接统一通过 App::connection；普通开发不安装或重载日用插件。

计划：
1. 明确映射、来源和已有会话的接入与恢复规则。
2. 实现可验证映射与受限空会话校准，保证失败清理和重启恢复。
3. 接入远程自动阶段与历史补齐，保护人工选择及外部分区。
4. 验证新主机、断线、丢回执、分区重建和重启。
5. 完成组合自动检查、隔离环境验证与交付记录。

只读分析并行分配给 mapping_architecture 与 history_and_tests；
两者不改文件，所有实现、暂存和提交由本任务完成。
开发提交保持 13 文件、400 行，不修改版本和共享 STATE。
