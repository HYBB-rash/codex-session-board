# PR 合并手动交接与恢复

owner: codex
execution_mode: isolated_write
allowed_write_set: scripts/pr_queue*、scripts/integrate_candidate.py、相关回归、流程文档和本任务上下文
base_commit: b66656a63e81581ef1947c4223b74fb76915980d
worktree_path: /Users/hongyong/.codex/worktrees/merge-manual-rounds-20261117
coordinator: current-session
status: completed

用户手动调度。每轮推进到合并或明确交接后返回，用户再次要求继续时刷新远端事实，
复用仍有效的候选、审阅及验证。不创建协调者或作者定时器，不等待作者 ACK 或修复。
保留必要编译、业务测试、准确头 CI、独立审阅及组合门禁。
已有作者任务模式、资源和自动发布不迁移；不安装或重载日用插件。

2026-11-17T11:23:56Z 开始实施准备，基线工作树干净后创建独立工作树。
先补现有 integrate 的短轮次返回，再补队列定位、远端复核和交接计时。
隔离回归覆盖返回后恢复、重复调用、变头、main 漂移、查询失败和发布独立。
真实 GitHub 演练对象由用户之后指定，本轮不执行真实 PR 评论、合并或主线推送。

实现及隔离验证完成。有效源码 28c31f0f0f51ddfb00933ddc1e4e75da9d68e7b7，
版本保持 0.4.3；macOS 原统一 PR submit 检查 1079 项全部通过，396.049 秒。
CLI 三个独立进程依次返回待 CI、已合并、已合并，仅一次 push 和一次 promote；
这是外部操作模拟的隔离证据，不代表真实 PR 或 GitHub 耗时。
新增状态、PR 索引及恢复均复用 pr-queue、integrate、promote 和现有 GitHub 快照。
早期子命令错误日志丢失及终态字段不同步均有修复前后证据。
首次完整检查的未改动 Rust 插件退出超时保留，未宣称根因修复；
后续规范环境定向 8 项及两次完整检查均通过。
分支、工作树和全部证据保留供审阅，未推送或合入远端 main。
详细结果及边界见 context/results/merge-manual-rounds-20261117.md。
