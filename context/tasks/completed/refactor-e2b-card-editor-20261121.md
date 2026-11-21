# E2b 卡片编辑与保存

owner: python_scenarios
execution_mode: isolated_write
coordinator: 五轮重构协调者
worktree_path: /home/herman/.codex/worktrees/e2b-vue-card-edit/notion-program
base_commit: f06bc4ee75277f007caa88cd61cf7ade63e09a98
allowed_write_set: 卡片编辑 Vue 模板、交互 TSX、离线入口接线与生成产物、相关表单浏览器回归和本项独有交付记录

开发从 E2a 临时提交 `ab04618` 开始。最终基线取远端主线 `f06bc4e`，其中已经包含 A 的确认卡片、B2 的操作资格、E1 的离线 Vue 和 E2a 的卡片与列。编辑入口保留供拖放打开阻塞表单，拖动、列排序和卡片投影分别归原作者。

验收以真实表单和后续读回为准。编辑中的备注按主机和会话保留，容许留空。旧版本、并发输入、其他主机的回执不能覆盖当前卡片；失败后读回新版本，继续使用当前草稿。明确取消的普通修改遵守已有行为。关闭后焦点回到当前可见的发起按钮。

关联入口为 `scripts/check-blocked-optional.mjs`、`scripts/check-transfer-feedback.mjs`、实际 HTTP 与包内 MCP 页面。Rust 场景的业务覆盖由 A 保有，E2b 接管这些入口中与表单 DOM 访问有关的测试和新增的草稿场景。
