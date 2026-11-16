# PR 验证与收尾效率

owner: codex
execution_mode: coordinated_write
allowed_write_set: 验证计划、PR 作者交付与队列收尾入口、对应回归、文档和本任务上下文
base_commit: 0aeb0db7edab43db6f47d0a79be274c0673dbe32
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: blocked

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

只读真实 API 发现已合并 PR 的 Actions pull_requests 会清空，原43829946296确有此变化。
恢复路径仅在 PR 已合并时，以准确仓库、源码、原分支、工作流和实际job核验；开放PR仍要求
PR关联。新增边界回归通过，真实只读证明返回原run43829946296/attempt1。
手工直接运行Python全量308项首轮在系统默认临时目录清理.git时报目录非空，
残留info/refs和objects/info/packs，尚无后台写入者归因，不声称根因已修复。
失败保留于源码外python-tests.log；同用例在项目规定/private/tmp环境通过，完整标准环境复验待执行。
标准环境完整Python308项全部通过，用时76.59秒，原始失败保留，不以复验冒称后台写入者根因已修复。
实际调用integrate --help发现旧入口选择系统Python3.10，复用pr-task已有delivery-venv优先级修正，
补真实shell入口与错误默认Python隔离回归；后续新PR计时仍待用户发起。

## 交付阻塞

实现已提交并推至远端codex/integration-pr-queue-20261115，冻结候选
7ccdc644090ea917b158f9f780c5d54017e38679，版本0.3.96。
main仍为0aeb0db7edab43db6f47d0a79be274c0673dbe32/0.3.95，未推广或签收候选包。
本轮修改工具，实际使用full门禁，不以application/submit选择替代完整自测。

CI43850294643的Linux1003项全部通过；首轮Mac992项中两项失败：native_authority远端隔离总计1661.341ms
超过原1000ms阈值；check-host-filter.mjs:104预期project all、实际project:a1。
309项Python全部通过。本机定向远端四场景347–368ms通过；
筛选首轮在相邻106行失败、耗时4.66秒，第二轮原断言通过、耗时3.98秒。
独立复核确认业务源码、Rust测试、浏览器驱动、执行器和候选工作流均未改；
完整执行链也未进入新增选择分支。四秒load覆盖局部data的机制高度吻合，
但缺少CI当时的刷新trace，不能宣称根因已实锤或修复。

只恢复一次失败Mac，复用已通过的Linux。attempt2仍失败：原两项通过，
但check-remote-catalog.mjs:187在断连load后目标写入预期6、实际7。
992项中仅此项失败，309项Python再次全过。停止重复重跑和main推广，
不能拼接两轮通过项充当完整成功报告。

工单14绑定筛选失败，已回贴原PR1评论6914723624；
工单15绑定目录失败，已回贴原PR12评论6915308338。
工单OPEN及两条评论均读回确认，只证明反馈送达，尚未确认原会话接单。
原业务和测试由作者修复，本任务没有夹带业务修复、降低阈值或跳过断言。
交接见context/handoffs/pr-validation-efficiency-20261116.md。
本次新真实PR提速测试尚未开始，不能以此工具开发或失败候选耗时宣称优化收益。
