# PR 合并手动交接与恢复

owner: codex
execution_mode: isolated_write
allowed_write_set: scripts/pr_queue*、scripts/integrate_candidate.py、相关回归、流程文档和本任务上下文
base_commit: b66656a63e81581ef1947c4223b74fb76915980d
worktree_path: /Users/hongyong/.codex/worktrees/merge-manual-rounds-20261117
coordinator: current-session
status: active

用户手动调度。每轮推进到合并或明确交接后返回，用户再次要求继续时刷新远端事实，
复用仍有效的候选、审阅及验证。不创建协调者或作者定时器，不等待作者 ACK 或修复。
保留必要编译、业务测试、准确头 CI、独立审阅及组合门禁。
已有作者任务模式、资源和自动发布不迁移；不安装或重载日用插件。

2026-11-17T11:23:56Z 开始实施准备，基线工作树干净后创建独立工作树。
先补现有 integrate 的短轮次返回，再补队列定位、远端复核和交接计时。
隔离回归覆盖返回后恢复、重复调用、变头、main 漂移、查询失败和发布独立。
真实 GitHub 演练对象由用户之后指定，本轮不执行真实 PR 评论、合并或主线推送。
