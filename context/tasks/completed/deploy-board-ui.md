# 部署看板外观

owner: cursor
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, 正式看板插件与 marketplace 配置、受管服务
base_commit: bd02af3
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

用户要求把 v0.2.65 换上正在跑的看板。正式进程已换成 v0.2.65，源码 bd02af3。
binary SHA256 为 6fa79488a279fc8d62da1ac906512350e02da6873852c67a34e068ccd5276c1c。
package、cache 和 18767 进程字节一致。配置复制为 config-0.2.65.json，字段和原数据目录未改，configId 仍是 ce0664890c3b6c81。
进程 pid 2403546。旧进程 2208478 已退出。

部署前对 5 个库做了在线备份，备份在 releases/0.2.65/backup。
本机库仍是 147 条会话、11 条备注，4 个远程库的会话数和备注哈希与部署前一致。没有改用户会话备注。

浏览器打开 http://127.0.0.1:18767/ 已显示白底列、黑色新建按钮和打开原会话。
本机 9 条，远程 142 条。M1 自动状态已连接。bb macbook 这次握手仍是 unexpected end of file。
两台 Hermes 自动连接保持关闭，页面上是连接按钮。

桌面 socket 符号链接悬空，没能执行 skills/list forceReload 和 config/mcpServer/reload。
已经在跑的 MCP 2273298 仍是 0.2.63 缓存里的进程。
完整 31 条外部签收未做，不宣称 release_ready。两个既有活动任务未动。
