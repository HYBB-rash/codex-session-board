# 换上不读启动记录的看板

旧进程 2893885 已退出。它忽略了停止信号，所以对这个进程发了 SIGTERM。新进程 3296170 是 v0.2.73，直接执行 serve，端口 18767。
配置复制为 config-0.2.73.json，字段和数据目录未改，configId 仍是 ce0664890c3b6c81。
binary SHA256 为 0b0a7137453b66de1c49940d82236ef9a3d8fdfaaec8b11db22718e50044a570。
第二次 serve 复用了这个进程。插件已装进 0.2.73 缓存，并完成 skills/list forceReload 与 config/mcpServer/reload。
新的两个 mcp 进程没有再拉起看板。本机 147 条会话、11 条备注，备注哈希未变。
浏览器打开页面，刷新后仍是待判断 9、进行中 0、待审阅 134、已阻塞 2、待归档 7。
两台 Hermes 仍是手动连接。本机位置读回没有报错。
备份在 releases/0.2.73/backup。完整 31 条外部签收未做。
