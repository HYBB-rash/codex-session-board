# 合并与验收分阶段及四 PR 重演

owner: codex
execution_mode: serial_write
allowed_write_set: 合并规则、候选门禁、包启动检查、相关测试绑定、任务上下文
base_commit: 1a5e7fa
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

按用户最新授权，合并任务只保证最终组合通过自动回归并产出可运行插件包。
在隔离环境检查实际包的启动，不安装、重载或修改日用客户端。
计时从开始重演四个固定 PR 头，到受保护主线确认同一候选且双平台不可变包可用。
规则实现、调查和后续记录分别计时；完整 31 条验收不在本次完成边界。
原四 PR 已在主线，此次正常重演历史合并，再合入当前主线和新规则，保留已有修复。
复用已审阅的冲突解决，不能把重演耗时当作首次未知故障的保证。
验收阶段的组织、记录、重试和完成条件作为后续研究问题，本任务不设计新验收流程。
