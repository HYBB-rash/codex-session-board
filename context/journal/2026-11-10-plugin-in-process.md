# 看板不再派生子进程

`launch` 子命令和 `managed_launch.rs` 已删除。`service::open` 在匹配的服务已运行时直接返回，否则由当前进程监听。
`serve` 与插件 `mcp` 都走这条路径。重复启动、配置或构建不匹配、以及 `stop` 仍按原身份规则验收。
源码里再写脱离进程的启动会被 `build.rs` 拒绝。
