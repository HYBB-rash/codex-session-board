# 上线进程内看板

owner: cursor
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, releases/0.2.70, config-0.2.70.json, marketplace session-board-rust, 18767 受管进程
base_commit: 91d8f166dff20092c612e44034fd5d61a40f6727
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

正式进程已换成 v0.2.70，源码 91d8f16。pid 2893885，自己执行 serve。
binary SHA256 为 9e4dcdc905920c14fedbe69c162c766e77f00701588323799dc83a4b533125ff。
package、cache 与进程字节一致。配置为 config-0.2.70.json，数据目录和 configId ce0664890c3b6c81 未改。
再执行一次 serve 返回 started false，同一个 pid。插件 mcp 2897352 和 2897354 没有另起看板进程。
5 个库的备注哈希与部署前一致。浏览器刷新后仍是 151 张卡片。完整 31 条外部签收未做，不宣称 release_ready。
