# 归档连接预算修复

owner: codex
execution_mode: serial_write
allowed_write_set: src/rpc.rs, tests/native_authority/, acceptance/bindings.json, Docs/ARCHIVING.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/, bin/, 已授权日常程序与插件副本
base_commit: b6f4a23ff84dd2fc4277165d96d3c86d537533e9
worktree_path: /home/herman/.codex/worktrees/pwa-support/notion-program
coordinator: codex

用户反馈 0.1.54 点击归档仍然 RPC 操作超时。
定位 DesktopConnection::connect 把所有写连接硬限制为三秒，
而原生分区资格读取实测约 5.7 秒，归档连接等待期间预算已经耗尽。
先用只读真实连接复现，再增加包含慢资格检查的真实传输回归，
保留标题三秒上限、归档共享截止时间和权限边界。
验证后提交并更新当前日常部署。不对真实用户会话执行归档。

最终部署为 0.1.55。连接层只对 TitleWriter 保留三秒上限；
ArchiveWriter 使用调用方预算并共享总截止时间，旧本地模式仍由调用方限制三秒。
真实只读复现修复前 5.98 秒后超时、修复后 5.96 秒通过。
实际待归档会话的全部归档前检查在 5.96 秒内通过，探针禁止最终归档写入。
新增真实传输慢资格检查回归在旧实现失败、修复后通过；
21 个原生路径测试和 10 个归档流测试通过，285 项自动验收通过。
完整门禁退出 2、release_ready=false，保留既有外部证据缺口。
程序、运行进程、marketplace 和缓存四处哈希一致，客户端读回版本 0.1.55。
123 条记录及备注保留，生产配置和两份 MCP 注入逐字节不变。
证据和私有备份位于 outputs/archive-timeout-20261107/：
real-before.json、real-after.json、real-preflight.json、
regression-before.log、regression-after.log、deployment.json 和 acceptance.json。
