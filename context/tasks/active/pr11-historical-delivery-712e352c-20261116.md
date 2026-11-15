# PR 11 历史交付修复

- owner: root
- execution_mode: isolated_write
- allowed_write_set: scripts/check-host-filter.mjs, context/tasks/active/pr11-historical-delivery-712e352c-20261116.md
- base_commit: 0a71260f2c1e8e6ecfa06d0f9689e606318f95a3
- worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author11-20261115/codex-session-board
- coordinator: historical-four-pr-queue
- status: validation_pending

用户授权的历史合并演练。只在指定修复树工作，不合入当前主线或其他完整组合，
不修改版本、共享 STATE、需求、参考模型和锁，不操作日用客户端。
比较 base 为 22b9dc4e45c872d91e921c2f636d256001241ac6；
固定 checker 为 ec4e9c6bd73f477a6baf5d1ff6bceea6ed0a5e3b。

最初的 delivery 失败为缺少源码外 review-pending.json，已保留读文件失败的复现日志。
协调者分派的独立审阅通过原头，作者没有自签审阅。
原头首轮固定检查器耗时 1000.970 秒，769/770 项通过；
主机筛选回归期望只读故障提示，却读到旧项目归属提示。
UI 三项独立复跑通过，不能替代首轮失败。

受控浏览器复现确认：启动两个 load，延迟较新响应，较早 load 因 requestSerial
过期返回时仍是旧快照；较新响应完成后提示才正确。
测试刷新辅助函数为 fixture 快照加版本标记，等待该版本生效后执行全部原断言。
故障读取仍直接调用 load 并检查连接错误；断言、用例数和业务实现不变。

源码外输出目录：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr-queue-20261115/author-11

首次失败保留在 validation-0a71260f2c1e8e6ecfa06d0f9689e606318f95a3-1；
竞态复现在 ui-snapshot-race-reproduction.log。提交后更新 ready.json，
由协调者再次分派绑定新头的独立审阅，使用新的源码外目录重跑固定门禁。
PR 9 所有权借用和 PR 10 新建请求的组合风险仍由精确组合候选验证。
