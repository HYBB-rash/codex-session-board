# 空归档名单的拒绝行为

- owner: empty-archive-behavior
- coordinator: 五轮重构协调者
- execution_mode: isolated_write
- base_commit: 77f282b8a055b6d2d793a99aee5784f9ba5509d8
- worktree_path: /home/herman/.codex/worktrees/empty-archive-behavior/notion-program
- allowed_write_set: tests/board_io.rs、tests/plugin_runtime/authority.rs、tests/support/fixture.rs、tests/support/read_only_backend.rs、本任务卡及同名结果记录。

T0 的 `final/remaining-history-and-new-audit.md` 复现了更换等义提示后产生的误报。
保留 MCP-001 的 `test::board_io::archive_batch_tools_share_http_history`，从两个实际入口核对有效名单与空名单。
空名单须返回业务错误及可读原因，HTTP 与 MCP 的历史仅包含有效批次，隔离原生后台只接收读操作。
对照组会更换错误措辞、放行空名单或尝试原生写入，比较原始退出码和触发断言。
