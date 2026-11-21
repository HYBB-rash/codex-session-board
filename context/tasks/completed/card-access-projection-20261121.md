# 卡片只读字段的归属

- owner: Fedora B2 作者
- execution_mode: isolated_write
- allowed_write_set: src/app/remote.rs, src/app/placement_tests.rs, tests/remote_sessions/{discovery,routes}.rs, index.html, scripts/check-*.mjs 中涉及卡片资格的场景, 本卡和同名结果记录
- base_commit: 722a776c100f9bc22b81bd6e7fbd5f0938406f89
- worktree_path: /home/herman/.codex/worktrees/card-access-projection/notion-program
- coordinator: Herman / 串行合入队列
- branch: codex/card-access-projection

根 `/api/sessions` 与 `board_snapshot` 共用 `App::snapshot()`。核查本机、旧根库远程缓存、兼容六列及在线托管目录的只读和归档资格。界面采用服务端结果，保留保存中、批次待核对及新输入确认等交互保护。
