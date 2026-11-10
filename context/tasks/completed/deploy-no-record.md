# 换上不读启动记录的看板

owner: cursor
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, releases/0.2.73, config-0.2.73.json, marketplace session-board-rust, 18767 进程, 插件缓存
base_commit: 8425673
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

正式进程已换成 v0.2.73，源码 8425673。pid 3296170，自己执行 serve。
binary SHA256 为 0b0a7137453b66de1c49940d82236ef9a3d8fdfaaec8b11db22718e50044a570。
package、cache 与进程字节一致。配置为 config-0.2.73.json，数据目录和 configId ce0664890c3b6c81 未改。
再执行一次 serve 返回 started false，同一个 pid。插件 mcp 3296916 和 3296920 没有另起看板进程。
5 个库的备注哈希与部署前一致。浏览器刷新后仍是 152 张卡片。完整 31 条外部签收未做，不宣称 release_ready。
