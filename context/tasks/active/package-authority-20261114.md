# PR 6 集成兼容与侧栏归属修复

- owner: codex-original-pr6
- execution_mode: isolated_write
- allowed_write_set: 本任务记录；fixture 托管、实际包启动验证、侧栏归属及相关回归
- base_commit: 865fb97
- worktree_path: /Users/hongyong/.codex/worktrees/package-authority-fix/codex-session-board
- coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: active

固定主线 04f7d58 与原 PR 头 9667957 无冲突组合为签名复现基线 865fb97。
先让包内 serve 与 MCP 均由监听端点的同一 fixture 托管，保留原有身份、就绪、协议与清理断言。
随后封闭实时后台与侧栏来自不同客户端的写入边界；不能证明归属时，在任何写入前拒绝。
不修改协调者工作区、共享 STATE、版本或日用客户端。仅执行针对性验证；完整双平台候选由协调者统一执行。
