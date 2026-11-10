# 去掉启动记录文件

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, build.rs, context/decisions/plugin-monolith.md, context/
base_commit: 8325506e4e841b262e3a4c778dff49b40d115d4a
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

看板不再读写 server.json。复用和停止只问配置端口上的 /api/info。进程活着但端口没有响应时，靠数据目录锁拒绝再开一份。形式化会话模型未改。不宣称 release_ready。
