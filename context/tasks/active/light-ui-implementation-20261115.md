# 轻量 UI 实施

- owner: codex
- execution_mode: isolated_write
- allowed_write_set: index.html; assets/*; src/mcp.rs; tests/ui_layout.rs; tests/board_io.rs; scripts/*bridge*.mjs; scripts/check-ui-layout.mjs; scripts/check-light-ui.mjs; scripts/check-remote-catalog.mjs; acceptance/bindings.json; Docs/LIGHT-UI.md; context/tasks/active/light-ui-implementation-20261115.md; context/results/light-ui-implementation-20261115.md
- base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
- worktree_path: /Users/hongyong/Developer/codex-session-board-light-ui-implementation-20261115
- coordinator: 后续集成协调者
- status: in_progress

用户于 2026-11-15 授权按已调查的轻量方案实施。
方案来源为 light-ui-plan-20261115 的 3d7308f；原型为 6a53ff7。
共享根工作树在建树前干净。本任务保持 0.3.85，不改共享 STATE，不推送或部署。
使用 coding-taste 和 unslop 技能，小步提交，并执行隔离 fixture 与项目回归。

## 顺序

1. 恢复可读 SDK 入口、依赖锁和可复现构建，接入宿主主题。
2. 移植轻量视觉，完成短窗口、全文备注、调整入口和刷新焦点保护。
3. 对照远程目录已提交字段，保留历史、缓存、只读和能力限制，统一健康判断。
4. 独立浏览器、WebKit、真实 Rust fixture 与自动回归，交付可审阅分支。

远程目录工作树仍有未提交开发。兼容参考固定为 9160e4b，不能视为最终头。
本任务基于 main 独立开发，不复制目录后端；后续整合须冻结并复验最终目录头。
原 31 条需求、参考模型和锁保持；独立探针不能签收日用原生容器。
