# 历史演练工具准备

owner: pr_origin_mapping
execution_mode: isolated_write
allowed_write_set: 演练目标与发布工具、直接工具测试、acceptance/bindings.json、本任务卡
base_commit: 2fcf151dd2c0e3739149d5c92f2c7ddd8f60c38d
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-history-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

准备始于 2026-11-15T23:13:00+08:00，独立于正式四PR演练计时。
仅回移94bf3ad及2c0822a的演练目标、独立不可变Release及直接工具测试，并绑定11项DELIVERY-001。
promote保留历史v1报告规则，未引入PR8的v2验证器及任何PR8至11业务修复。
业务、原自动门禁和版本0.3.86不变；没有推送、产包或修改共享STATE。
最终冻结再由协调者写integration-target.json，base使用本准备完成提交，不自引用。
定向工具测试及最终检查结果由子任务回执交给协调者；完整组合门禁仍在正式演练执行。
