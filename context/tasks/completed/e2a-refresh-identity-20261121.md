# 周期刷新保留会话卡片身份

- owner: rust_contracts
- execution_mode: isolated_write
- allowed_write_set: `scripts/check-light-ui.mjs`、`tests/ui_layout.rs` 与本任务卡
- base_commit: 77f282b8a055b6d2d793a99aee5784f9ba5509d8
- worktree_path: /home/herman/.codex/worktrees/e2a-refresh-identity/notion-program
- coordinator: Herman / T0

需求见 `context/results/refactor-e2a-card-display-20261121.md`。在已有的 1,500 张卡片夹具中，用户点击展开一张卡的项目详情。随后八次无变化刷新，每个会话仍使用原来的可见卡片节点，详情保持展开，键盘焦点仍在该会话的原控件上。

已随 `8fcc14d` 合入。现有浏览器测试逐张核对节点和会话 ID，八次刷新后可见性、已展开的详情及焦点均通过检查。
