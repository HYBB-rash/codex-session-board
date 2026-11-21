# E3 十卡浏览器验收

owner: E3 浏览器验收作者
execution_mode: isolated_write
allowed_write_set: scripts/check-ten-card-archive.mjs、scripts/ten-card-archive-fixture.mjs、tests/ui_layout.rs 和 acceptance/bindings.json 的测试入口、本卡
base_commit: 161e98342249d5830cff1f849fd453188d4f2e93
worktree_path: /home/herman/.codex/worktrees/e3-ten-card-browser/notion-program
coordinator: 五轮重构总协调

认领 `test::ui_layout::ten_card_archive_recovers_after_closing_the_submitting_page`，沿用 `browser-client.mjs` 的隔离 Chromium 与 `browser-actions.mjs` 的指针点击。HTTP 夹具在第七次归档请求保留响应，关闭标签页后保留服务端批次供新浏览器配置读回。

直接运行 `node scripts/check-ten-card-archive.mjs` 输出 JSON，其中有三个阶段的逐项详情、逐卡归档 HTTP 次数和核对请求流水。也可运行 `cargo test --locked --offline --test ui_layout ten_card_archive_recovers_after_closing_the_submitting_page -- --exact`。

产品首件 `161e983` 的上一版 `735b368` 将仍在提交的第七张卡显示为“待核对”。脚本的可选参数接受导出的页面文件，可以用同一场景复核这项差异。Mac 原生写入次数由原生验收作者提供。
