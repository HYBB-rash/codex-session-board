# PR #8 与远程原生新建的连接入口冲突修复

- owner: 原作者会话 01a1da15-3bcd-7e32-9033-8d22bdfb545f
- execution_mode: isolated_write
- allowed_write_set: src/app.rs、src/app/connection.rs 及本任务文件；其他改动只继承指定前置提交
- base_commit: de804274acf3e8663e0f9716ecd4d78739651f22
- worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author8-20261115/codex-session-board
- coordinator: 历史四 PR 合并队列会话 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: in_progress

本轮反馈 eab3bfa9-d165-5f76-80bb-6002250cb779 只授权同步组合前置提交
b27e164b6b034e270aab2fcde319246c99230ef9。验证比较基线仍固定为
22b9dc4e45c872d91e921c2f636d256001241ac6，检查器仍为 ec4e9c6。
不合入当前主线或其他完整候选，不修改版本、共享 STATE、需求、独立模型或锁。

merge-tree 已复现 src/app.rs 冲突，证据为源码外 author-8/merge-tree-eab3bfa9-initial.log。
PR #10 把 DraftWriter 改成只允许 initialize/thread/read 的 DraftReader，
但其修改仍位于旧的 App::connection 定义。保留 R2 的私有连接模块与凭证构造，
仅在该模块同步 DraftReader 的兼容模式拒绝规则，不恢复旧构造入口或放宽 RPC 权限。
上一轮验证入口、候选 v2 严格报告检查及演练发布隔离不变。

复用现有草稿、角色权限、原生归属及十二处编译拒绝回归，不降低断言或跳过测试。
按 13 文件、400 行和 CARGO_BUILD_JOBS=2 执行提交快检；提交后写 ready.json，
由协调者另行分派独立审阅，并用固定检查器对新头完成 PR 交付检查。
上一轮 ready/result/review 已按 de804274 完整头另存，既有报告和首次失败不覆盖。
最终验证与审阅结果只写源码外 author-8/result.json，不代签组合或真实客户端验收。
