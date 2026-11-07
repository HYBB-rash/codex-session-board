# 本机原生位置读取优化

owner: codex
execution_mode: serial_write
allowed_write_set: src/, tests/, acceptance/bindings.json, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/native-read-performance-20261107/
base_commit: 3279763
worktree_path: /home/herman/文档/notion-program
coordinator: codex

远程接入会话已完成提交，原工作区干净。基于 0.1.59 实现本机直接读取，
替换状态修改、归档、标题校验与后台原生分区读取中的综合列表查询。
保留逻辑分区版本、写后确认、同会话操作锁和远程人工编辑分流。
验证映射失效、外部移动、丢回执、新输入，以及远程在线、断线、扫描中的隔离。
真实客户端仅只读取证；受控写操作使用隔离数据和协议端点，不修改日常配置。
按完整可验证改动分步提交，不推送、不部署。完整验收缺证如实保留。

已完成本机直读、缓冲映射读取和全部生产读取入口接入，远程分流保留。
真实客户端 119 条批量读取约 100 ms，30 个目标读取 p95 10.88 ms。
隔离 HTTP 在线、断线及两类扫描闸门验证通过，含重启恢复与同 UUID 隔离。
300 项自动检查通过，完整门禁退出 2，14 项既有外部证据仍待补齐。
说明见 Docs/NATIVE-READ-PERFORMANCE.md，证据在 outputs/native-read-performance-20261107/。
