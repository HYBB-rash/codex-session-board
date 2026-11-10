# 日常原生位置正式上线

owner: cursor
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json
base_commit: 68d0a7c
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

用户要求把当前源码换上正在跑的看板。正式进程已换成 v0.2.59，源码 68d0a7c。
binary SHA256 为 0289925b6f6f529834aec77f0247700fd13cb58e99559a5c17449df33f50c2cc。
package、cache 和 18767 进程的字节一致。配置复制为 config-0.2.59.json，字段和原数据目录未改。

打开新进程时，既有库的 `native_revision` 列改名为 `native_section`。5 个库的备注哈希与部署前一致。
本机桌面 socket 文件当时不在，这次没有完成一次本机位置读回，也没有重载 MCP。
完整 31 条外部签收未做，不宣称 release_ready。两个既有活动任务未动。
v0.2.60 只登记这次部署，运行二进制仍是 0.2.59。
