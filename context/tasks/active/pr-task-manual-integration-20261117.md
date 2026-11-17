# 手动 PR 交接工具合入主线

owner: codex
execution_mode: isolated_write
allowed_write_set: 机械合并、发布版本清单、本任务记录及 context 状态
base_commit: 3ebc58649857807d92eae77ff86361e5f331cf27
worktree_path: /Users/hongyong/.codex/worktrees/integration-pr-task-manual-20261117
coordinator: 01a1e3fb-c015-7200-8dc9-1223b83cbb21
status: in_progress

用户授权将手动交接工具合入远端 main，之后另选真实业务任务手动体验。
固定作者头 240bd40b28d2c2cc78f1b3b6b51c005c5e3a67b5，保留四个原始提交；
与新增 PR19 的主线机械合并，无文本冲突或手工冲突修改。
复用 integrate/promote 的准确源码双平台组合门禁。
本次为已完成本地工具的直接集成，没有另开真实业务 PR。
版本由 check --prepare-release 按既有规则从 0.3.99 准备为 0.4.0。
集成准备计时起点为工作树创建时间 2026-11-17T10:04:17+08:00。
发布、安装、客户端验收与用户后续真实任务独立记录。
源码外证据目录为 manual-handoff-integration-20261117。
本机主工作树 main 另有未推送提交 c8a2f8a，保留其源码和历史。
