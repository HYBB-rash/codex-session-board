# 收紧会话控制状态机

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, tests/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: faae7e7
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

控制模型只保留列、座位、阶段、待确认移动、亲手放置、确认锚点、新输入复核和归档。
标题队列和备注不再占状态。选择时间并入手工放置。
锚点在自动挪列后保留，因为后续输入仍要复核。
冻结的 acceptance/model.py 和契约锁不改。收完后重放 Store。

收完后从空白未读会话得到 1077 个可达状态、37 个事件、39849 条边，与 Store 一致。
实现提交 5aecbe6。
