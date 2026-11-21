# Markdown 真实入口的空回执

- owner: E1 回复预览作者
- execution_mode: isolated_write
- coordinator: Linux 合入队列
- base_commit: `71c36f313bce2c7f8ff1ef801017ee87c8ab072c`
- worktree_path: `/home/herman/.codex/worktrees/preview-native-eof/notion-program`
- allowed_write_set: 本卡、同名结果记录、`tests/support/fixture.rs` 及其启动回执调用者、`tests/native_authority/entrypoints.rs`

复核 `eb9b179` 全量验收的预览 EOF。实跑原测试和对照，分别保留真实 `serve` 启动、HTTP 回复、stdio 工具回执的故障现场。独立核对同批 owner 错误的取证边界。

已随 `2fe0634` 合入。三个测试组共 85 项通过。原现场的触发原因仍未确定。[故障对照](../../results/preview-native-eof-20261121.md)
