# 插件进程不再拉起后台

插件工具改为在本进程内监听；端口已被占用时直接使用，不再派生子进程。
build.rs 会在编译阶段拒绝插件代码调用 service::launch、再执行 serve，或在允许的文件之外使用 process_group(0)。
向 src/mcp.rs 写入 service::launch 后，cargo check 以「插件不得启动后台服务」失败。
已通过的检查包括看板与栏目排序里的 MCP 桥接，以及原生入口的 HTTP 与 MCP 共用。
旧的 launch 子命令未改。形式化会话模型未改。不宣称 release_ready。
