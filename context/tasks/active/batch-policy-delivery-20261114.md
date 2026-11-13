# 分层合并规则与四 PR 全链路演练

owner: codex
execution_mode: serial_write
allowed_write_set: scripts、tests、验收绑定、CI、项目规则、相关文档及本任务上下文
base_commit: 1683bd4a6d1a42eddc066f42355b328ea7a2f3c4
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: implementing

用户授权先落实提交、PR、集成候选与发布四层规则，再重演原四 PR 到正式插件部署。
普通开发提交保留 13 文件、400 行，完整测试改到候选，版本仅在冻结候选时更新。
保留原历史规则和全部验收断言，完整 31 条外部缺证不得代签。
部署使用不可变候选产物，保护正式配置、数据和日用会话，验证后恢复测试卡片。
分别记录规则实现和合并到部署两段耗时，后者包含 CI、故障处理、验证和收尾。
上次实验已核对快照后恢复，正式主线作为本次规则实现基线。
