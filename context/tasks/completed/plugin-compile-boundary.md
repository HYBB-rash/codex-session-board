# 用编译边界卡住后台启动

owner: cursor
execution_mode: serial_write
allowed_write_set: AGENTS.md, build.rs, src/main.rs, src/managed_launch.rs, src/service.rs, src/config.rs, context/
base_commit: 5634ebd4e48042ea8bad6b41323f43ec1f1cbe50
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

后台启动移到只由二进制引入的 src/managed_launch.rs。库代码调用 service::launch 时编译器报找不到函数。
插件入口不是 mcp::serve，或库里再写 serve 启动，build.rs 让编译失败。
AGENTS.md 里那节插件进程守则已删除。形式化会话模型未改。不宣称 release_ready。
