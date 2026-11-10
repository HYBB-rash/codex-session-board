# 插件不得启动后台

owner: cursor
execution_mode: serial_write
allowed_write_set: AGENTS.md, build.rs, src/mcp.rs, context/
base_commit: b9c45eec5adbb37a9954a3cddf3c4bc2b150dc3c
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

插件是单体程序，跟随拉起它的 Codex 进程退出，不再调用 service::launch。
编译时 build.rs 拒绝插件代码再启动 serve 或另开进程组。
codex-session-board launch 仍留在 main.rs，插件不得调用。
会话控制的形式化模型未改。完整 31 条外部签收未做，不宣称 release_ready。
