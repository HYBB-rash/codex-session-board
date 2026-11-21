# T2 第二组 · 候选验证的三个来源拒绝场景

- owner: T2 作者会话 `01a1fc65-7a91-7440-a579-bc28c74a235c`
- execution_mode: isolated_write
- allowed_write_set: `acceptance/test_candidate.py`、`acceptance/bindings.json` 内本组三个 ID、本卡和同名 `context/results/` 记录
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/t2-candidate-source/notion-program`
- coordinator: 五轮重构协调会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`

T0 盘点列出三个共用 `CandidateGateTests.run_candidate` 的 GATE-001 入口。首件在基线上分别调用三者，确认未提交改动、仅提交号变化、自动报告指纹不符都要求退出 1 且报告 `passed=false`。本组只调整这三项的组织和绑定，逐个验证失败能定位到对应场景，由协调者登记、归档并安排合入顺序。
