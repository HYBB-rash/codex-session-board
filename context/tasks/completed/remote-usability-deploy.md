# 远程列改动换上正在跑的看板

owner: cursor
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, 正式看板插件与 marketplace 配置、受管服务
base_commit: 5e38d56
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

用户要求部署后看效果。正式进程已换成 v0.2.63，源码 5e38d56。
binary SHA256 为 67d242e8b963b942e46f6f44a838a8cfbee79032e8ac0def89cd156d5e4fa125。
package、cache 和 18767 进程的字节一致。配置复制为 config-0.2.63.json，字段和原数据目录未改。

5 个库已在线备份，本机 11 条备注的哈希与部署前一致。没有改用户会话备注。
M1 已按运行状态分列。bb macbook 握手仍是 unexpected end of file。
两台 Hermes 保持关闭自动连接。完整 31 条外部签收未做，不宣称 release_ready。
两个既有活动任务未动。v0.2.64 只登记这次部署，运行二进制仍是 0.2.63。
