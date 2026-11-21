# F2 · 卡片调整与阻塞原因说明

- owner: F 作者
- execution_mode: isolated_write
- coordinator: Herman，五轮重构协调会话
- allowed_write_set: README.md 的卡片调整与阻塞说明、本卡
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/f2-readme-card-actions/notion-program`
- branch: `codex/f2-readme-card-actions`

维护者更新卡片操作说明时，以页面实际按钮、拖放结果和 `UI-002` 为准。
首件核对 README 第 14 行的两项说法。基线仍写着没有按钮、必须填写原因，两项均需改正。
主要疑点是不同卡片的按钮文字和可写权限，以及拖入阻塞列与在该列内拖放的差别。

本项认领文档中的调整入口与 `UI-002` 使用说明。只读复用
`test::ui_layout::light_ui_refresh_health_and_catalog_preserve_interaction`
和 `scripts/check-blocked-optional.mjs` 的隔离页面场景。
按钮和拖放由 `index.html` 的 `makeCard`、`makeColumn`、`openEditor` 实现。
保存规则与回归的修改归 A 作者。

隔离浏览器验证了“调整”打开编辑框、只读卡禁用、未归列卡显示“归列”。
隔离 Rust 服务的 10 项阻塞场景通过，读回覆盖留空保存、取消不写、同列编辑及清空已有备注。
复验入口是 `node scripts/check-light-ui.mjs`，以及该工作树 `./scripts/dev setup`、`start`
之后的 `node scripts/check-blocked-optional.mjs <源码外报告路径>`。
