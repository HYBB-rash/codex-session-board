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
