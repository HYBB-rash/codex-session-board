# 远程列和单主机失败

owner: agent
execution_mode: serial_write
allowed_write_set: src/, tests/native_authority/, index.html, Docs/REMOTE-SESSIONS.md, Cargo.toml, Cargo.lock, context/
base_commit: 7fde143
worktree_path: /home/herman/文档/notion-program
coordinator: agent

远程卡片的列改回运行状态和手选。创建远程实例时不再沿用本机原生分区开关。
保存备注和手选只写该主机自己的库，不移动本机桌面侧栏。本机看板仍以桌面侧栏为列来源。

单张轮次响应超过单帧上限时，只记下这张卡当时的 updatedAt，重连后继续同一台主机的其他会话。
updatedAt 不变就跳过，避免下一轮又先读这条大响应。这个失败不把整台主机标成运行状态未核验。
不改 itemsView，不提高帧上限。

一台远程主机断开，或旧侧栏扫描不完整，健康条不再因此整条变红。
本机侧栏失败仍按本机错误显示。已经由 SSH 扫描的主机不再走旧的侧栏会话扫描。

客户端里关闭自动连接的主机，刷新不再继续拨号。明确连接失败后去掉本次请求并停止。
自动连接开启的主机仍按原间隔重试。

验证：0.2.61 与 0.2.62 的提交前检查、Clippy 和全部离线测试通过。
没有重启 18767，没有改正式配置和数据库。完整 31 条外部签收未做，不宣称 release_ready。
