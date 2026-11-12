# 2026-11-13 远程原生分区耗时调查

读取 Fedora 实际安装的 Codex 26.1106.51428 资源，核对实际移动、映射写回及保存源码。
普通真实 M1 移动确认 191–942 ms，未复现固定五秒移动。
受控更新另一张专用逻辑分区时，原生移动 230 ms 已成功，
目标映射仍到 5689 ms 才落盘；确认共享原子状态 500 ms 防抖可被不断重置。

隔离同包 Linux app-server 的 10 次移动中位 0.84 ms；
抽取实际保存类及防抖函数的调度实验亦通过。
旧 5.2 秒事件没有逐阶段日志，本轮不冒称恢复了当时的确切触发序列。
报告及原始数据入口见 Docs/REMOTE-SECTION-LATENCY-20261113.md。

三张专用空会话已归档，测试分区及隔离缓存已清理。
未恢复远程分区、未改正式实现或部署；已有完整验收遗留继续待验证。

首次提交钩子中 218 项库测试通过，两项既有 RPC 测试失败：
`sidebar_disconnect_never_replays_a_write_on_another_endpoint` 报 RPC 超时；
`refused_desktop_socket_has_the_same_recovery_without_hiding_security_errors` 报握手未完成。
未修改相关业务实现、断言或超时，改为先串行复验 RPC 测试再重跑全量钩子。
直接运行 Cargo 时另遇 macOS `/var` 与 `/private/var` 别名路径断言；
随后沿用仓库检查脚本的 `TMPDIR=/private/tmp` 环境进行复验。
相同断言和超时下，RPC 组串行复验 66 项全部通过。
全量单线程钩子在既有 SQLite 写入者子进程的 `writer-ready` 输出协议上持续等待；
主动停止本次专属子进程后结束，219 项通过、该项中断失败，未跳过或修改测试。
后续改用低并发完整检查，避免单线程运行方式影响子进程输出。
