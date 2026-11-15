# 共用验收适配器的直接脚本入口修复

- owner: 原作者会话 01a1da15-3bcd-7e32-9033-8d22bdfb545f
- execution_mode: isolated_write
- allowed_write_set: scripts/rust_adapter.py、acceptance/test_rust_adapter.py、acceptance/bindings.json 及本任务文件
- base_commit: 316a1644b955a4c3d9739e78cbeebbdc6688a23f
- worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author8-20261115/codex-session-board
- coordinator: 历史四 PR 合并队列会话 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: in_progress

反馈 b1c1d53e-9069-411b-82b1-fc7c51a4478e 指定本会话为唯一修复者。
已从 8261a07 快进合入精确失败候选 316a164，保留全部已审 PR 及候选祖先；
候选携带的版本与共享记录仅继承，本轮不修改它们，也不合入真实 main。

CI 43742301591 双平台在 Rust 测试组通过后失败。历史基线 f45bbff 的
run_python 以 scripts 包导入结果校验器，但直接执行文件时没有初始化项目根路径。
独立证据位于源码外 history-ci-43742301591；不能据此归责其他业务 PR。

先新增真实子进程回归，再修改生产代码。清除 PYTHONPATH、使用独立目标目录，
直接脚本入口的成功/失败目标均复现 No module named 'scripts'，模块入口通过。
首次记录保留在 author-8/direct-entry-b1c1d53e-before.log。
回归只用固定 Rust 工具 fixture，Python 发现、执行及结果校验均使用真实入口；
它验证入口和失败结果传播，不冒充完整业务测试。新用例追加到既有 GATE-001 绑定。

生产修复只初始化适配器文件所属项目的导入根，目标目录仍由 cwd 决定。
不设置 PYTHONPATH，不改结果完整性校验、需求、参考模型、锁或已有断言。
按 CARGO_BUILD_JOBS=2、暂存快检及 agent-git 提交，提交后立即写 ready.json。
协调者另行分派独立审阅；使用固定 ec4e9c6 检查器及比较 base 22b9dc4 验证新头。
本作者完整验证与后续候选 CI 分别记账，最终证据写源码外 author-8/result.json。
不安装插件、不操作日用客户端，任务归档和共享状态由协调者维护。
