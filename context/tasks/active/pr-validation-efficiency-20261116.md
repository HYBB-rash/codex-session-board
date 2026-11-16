# PR 验证与收尾效率

owner: codex
execution_mode: coordinated_write
allowed_write_set: 验证计划、PR 作者交付与队列收尾入口、对应回归、文档和本任务上下文
base_commit: 0aeb0db7edab43db6f47d0a79be274c0673dbe32
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

用户批准复用现有工具，优化无关提交规则自测、本机串行完整验证及合并收尾。
完成后由用户提供全新真实 PR 测量，不重演旧 PR，也不将推算当作实测收益。

保留最终 PR 头 CI、冻结组合双平台门禁、不可变包隔离启动、全部测试绑定及完整验收入口。
普通新增业务文件不得仅因新增而带入无关 hooks 自测；实际检查器及其依赖变化仍须自测。
作者本机检查与 CI 证据分别记录，较小范围不得冒充完整 PR 或候选门禁。
只读子代理调查验证选择和交付收尾，协调者独占修改、提交与共享上下文更新。

已实现应用回归选择、submit/ci 阶段区分、准确头 Actions 工作流与 changes job 的双次核验、
中断恢复固定输入、同头补交证据的准确反馈解除、作者终态批处理及 context 自动推广。
本轮先后 111 项定向回归及新增时序/终态回归通过；完整门禁待最终冻结候选执行。
独立复核指出的新增业务混合路径、Python 纳入、resume 绕过、同头旧反馈与查询期间变头
均已修正并补回归。31条需求、参考模型及锁未改。
只读重算旧 PR13 的实际路径：候选与 PR CI 选878项，保留288项Python，
排除93项无关工具自测；submit选736项。这是选择范围回归，不是新耗时成绩或旧PR重演。
后续以用户的新真实PR记录完整耗时，不能将推算当实测。
