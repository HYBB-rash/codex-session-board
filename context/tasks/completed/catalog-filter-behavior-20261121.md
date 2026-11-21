# 远程目录筛选断言

- owner: 筛选作者 `01a1fc68-930a-7cc1-b249-e1c7041b5535`
- execution_mode: isolated_write
- allowed_write_set: `scripts/check-remote-catalog.mjs`、`tests/ui_layout.rs`、本卡与同名结果
- base_commit: eb9b179470af72d1058fa4eb0fe9e61eb05a32eb
- worktree_path: /home/herman/.codex/worktrees/catalog-filter-behavior/notion-program
- coordinator: T0 `01a1fc5f-040e-7a21-b4ce-6d94b255128b`

裁决 Rust 测试 `host_catalog_filters_preserve_cached_rows_and_isolated_native_drafts` 的固定计数。用实际目录缺项后的选择与可见卡片、归档目标和失败草稿的重试请求验证，做等价与故障对照，交付作者提交和原始回执。

已随 `f5c3426` 合入。浏览器测试验证了目录移除后的可见卡片和草稿重试的请求目标。[对照结果](../../results/catalog-filter-behavior-20261121.md)
