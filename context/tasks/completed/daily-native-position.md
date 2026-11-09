# 收起日常原生位置

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, tests/, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 25ee934
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

日常模式判断旧事件要不要推动卡片，改成模型那一句，写在 `old_event_keeps_card`。
没有阶段时看观察起点，终态看亲手放置的 `native_choice_at`。已经接受的同一轮移动不当作旧事件吞掉。
不再调用 `preserve_runtime`。该函数、`observe_runtime` 和 `recommend` 仍留给冻结的六状态契约。

日常路径换列、放置时间、待确认移动和归档清除，都经过 `write_daily`。旧本地模式的选择写入留在 `save_selection`。
重放仍是 1077 个状态、37 个事件、39849 条边。

`version` 是本地行计数，`input_revision` 是本地输入计数，`native_section` 是上次读到的分区标识。旧列 `native_revision` 在打开数据库时改名。
侧栏没有分区修订号，移走再移回认不出来。没有补一套侧栏条件写。

活动列表完整、会话只在原生归档列表里时，不再计为未读回位置，本地行不改。
原生侧栏取消归档后，下一次本机读回恢复已确认卡片并保留备注，不把恢复写成亲手放置。
正在跑的正式进程仍是 v0.2.40。未部署，未改用户数据库，不宣称 release_ready。
两个既有活动任务未动。

实现提交 7cf4ce1、d2bd59d、b4c123d。
