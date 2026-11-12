# Singleton guard 集成

owner: codex
execution_mode: sequential-isolated-worktree
allowed_write_set: src/singleton*, src/app.rs, src/config.rs, src/observer.rs, src/lib.rs, tests/singleton*, scripts/bind_rust_tests.py, Docs/SINGLETON-GUARD.md, acceptance/bindings.json, version files, context task/state/journal
base_commit: 10d1217
worktree_path: /home/herman/.codex/worktrees/singleton-integration/notion-program
coordinator: codex

## 目标与边界

用户授权趁另一会话搁置时继续集成。将已验证的目录持锁、实例校验和条件修复移植到当前运行时，验证退出与重载生命周期。
保留 SQLite 热日志恢复和其他任务成果。只做本地代码、测试与提交；不推送、部署或修改真实客户端配置。

## 进度

- 主仓库干净，基线 10d1217；已建立独立 worktree。
- 先集成核心身份校验与基本回归，再接入导航及后台生命周期。

## 实现与验证

- 核心 Guard 依据当前进程、监听 socket、profile 标记与目录 inode 校验实例身份，只恢复缺失的临时 cookie。
- 导航前持锁并校验；后台观察器持锁，多个运行时和异步启动进程分别保留其 Guard。
- 12 项新增自动回归通过。隔离 Electron 用本次集成库重新验证转发、tmpfiles 持锁/释放和真实重启换代，各项结果均符合预期。
- 实验记录位于 /home/herman/.local/state/codex-session-board/research/singleton-20261112/integration-results.json。
- 不推送、不部署、不修改正式配置；真实 Codex GUI 目标环境验收继续待验证。

## 本地集成结果

- 核心提交 d89be03（0.3.39），运行时与并发回归提交 df7ce6a（0.3.40），提交前检查、Clippy 和完整 Rust 测试通过。
- 补齐验收绑定生成器、设计文档和交接记录；完整验收报告在研究交付记录中保留，缺失目标环境证据不作为通过。
- 最终收尾提交后仅快进合入本地主分支；未推送或部署，正式配置仍关闭。
