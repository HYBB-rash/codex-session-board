# 2026-11-13 Singleton 本地集成

从 10d1217 的独立 worktree 集成 Linux 可选的 Chromium singleton 保护。
Guard 关联当前 profile、进程启动身份、监听 socket 和临时目录，共享 flock 保护合作清理器。
导航前仅原子恢复缺失的临时 cookie；错误内容、实例换代或目录/socket 丢失均拒绝跳转。
后台运行时、重叠运行时与异步启动辅助进程各自保留目录锁，退出释放。
12 项新增 Rust 回归及当前库的隔离 Electron 转交、持锁/释放清理、真实重启换代通过。
正式配置默认关闭；不推送、不部署、不改真实 profile 或 timer。
隔离实验不代替真实 Codex GUI 跳转，目标环境证据仍待验收。
设计与边界见 Docs/SINGLETON-GUARD.md；研究记录在本机 singleton-20261112 研究目录。
