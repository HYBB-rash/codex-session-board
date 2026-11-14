# PR 交付前检查与历史故障演练

owner: codex
execution_mode: coordinated_write
allowed_write_set: PR 验证入口、按改动选择的风险检查、实际包隔离检查、相关测试和绑定、工作流及文档、本任务上下文
base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

用户授权复用现有工具，落实 PR 交付前通用检查、按改动追加相关场景、
固定最终源码的结果报告，并实际演练提前发现的问题与流程耗时。
开发提交继续只做暂存快检；交付前检查不代替最终组合门禁或真实客户端验收。
不修改 31 条需求、参考模型与锁，不升版、不发布或重载日用插件。

复用 scripts/validate --scope pr、现有打包、fixture、包启动验证和回归。
两个只读审查者分别核对入口复用与报告边界、历史故障和真实场景覆盖。
协调者独占代码及共享上下文写入。历史演练必须固定原始源码，
不得把修复后的实现覆盖旧实现后宣称已经检出旧问题。
方案开发和实际交付检查分别计时，失败与重试保留。
