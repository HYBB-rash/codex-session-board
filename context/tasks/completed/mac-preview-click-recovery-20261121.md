# macOS 嵌入回复回放首次指针点击

- owner: 嵌入回归作者
- execution_mode: isolated_write
- coordinator: 五轮重构总协调
- base_commit: `73da0c3031f3f78846b89ad4263303e7f8bb6e11`
- worktree_path: `/home/herman/.codex/worktrees/mac-preview-click-recovery/notion-program`
- allowed_write_set: 本卡、同名结果、`scripts/check-reply-preview.mjs` 的物理输入与浏览器页面生命周期

源运行 `44880119583` 验证 `b4d8b23`，Linux 通过，macOS arm64 的 MCP 首击超时。原 HTTP 场景已完成。核对准确源码与 E2a/E1，保留两种入口、真实指针、迟回执和原有断言。对照操作限隔离 fixture、专属浏览器和候选副本。
