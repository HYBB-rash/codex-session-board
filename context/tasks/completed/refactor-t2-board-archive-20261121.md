# T2 看板和归档测试

- owner: T2 作者会话 `01a1fc65-7a91-7440-a579-bc28c74a235c`
- execution_mode: isolated_write
- allowed_write_set: `tests/board_contract.rs`、`tests/board_io.rs`、`tests/archive_flow.rs`、`acceptance/bindings.json` 中本组原入口和承接 ID、本卡及同名结果记录
- base_commit: `808a0f518b06c052cb3bc98654b8b48506b84936`
- worktree_path: `/home/herman/.codex/worktrees/t2-board-archive-cases/notion-program`
- coordinator: 五轮重构协调会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`

按 T0 `pairs.json` 中 `test::board_contract` 和 `test::archive_flow` 的三个候选及一个有条件项，核对基线测试、需求绑定及触发条件。首件对照关闭标题同步、空轮次 ID、畸形归档输入和首次观察旧失败。两端平台绑定分别比对，本机 Linux 实跑，逐项结论交协调者登记。
