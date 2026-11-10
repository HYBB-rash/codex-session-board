# 看板只在当前进程里运行

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, build.rs, tests/board_io.rs, tests/column_sort.rs, tests/native_authority/entrypoints.rs, README.md, Docs/DEPLOYMENT.md, context/
base_commit: b36171950220923793822fcf175eef9053d87c3c
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

删除脱离进程的 launch。serve 和插件都调用 service::open。
匹配的服务直接复用，否则当前进程监听。形式化会话模型未改。不宣称 release_ready。
