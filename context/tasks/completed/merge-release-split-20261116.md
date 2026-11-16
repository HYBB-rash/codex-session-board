# PR 合并与自动发布分离

owner: codex
execution_mode: coordinated_write
allowed_write_set: 合并与发布CLI、工作流、固定checker缓存、对应测试文档及本任务上下文
base_commit: 1ba252ae5a9f01a34eb1579629a5e3ae64d41bcb
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

最终结果：PR17与0.3.98已进入远端main，双平台不可变候选插件已自动发布。
最终源码81fa140，完整CI为macOS1038/Linux1049项；未安装或重载日用插件。
详细计时、缓存、连续更新与失败恢复见context/results/merge-release-split-20261116.md。
以下保留开发期阶段记录；其中“待执行”不代表最终状态。

用户要求拆分PR合并和发布完成状态，main自动发布、保留在跑任务并合并pending，
失败保留上一可用包并交GitHub反馈；固定工具按源码及构建条件复用，
业务测试必须构建当前候选，不另建调度器，不操作日用插件或客户端。
远端0.3.97的库/native_authority两线程已机械合入，本任务不重复实现或覆盖。
保留31条需求、模型、锁及必要业务断言，缺少目标证据不宣称完整验收。

最近真实基线CI43861045024，integrate入口26分03秒。
Mac验证1194秒、fixture49秒、构建上传启动130秒，finalize命令22秒；
Mac验证结束到main推广302.565秒。不可将这些直接报作新流程节省时间。
checker在暂存快照及git_hooks独立fixture重复编译已确认，
旧记录缺少纯编译拆分，缓存收益通过同一工具受控实测。

协调者独占共享状态与发布改动；缓存独立隔离checkout开发，返回提交供审阅取用。
分别记录PR就绪到main、main到发布、缓存首次/命中/变更重建，
真实验证发布失败及连续main更新；不将模拟、单测或估算冒充真实流水线结果。

已实现并定向验证：
- candidate门禁保留项目规则、全目标Clippy及完整/按风险的组合回归，移走发布构建。
- integrate/promote及queue分别记录merge与release，closeout不等待发布或客户端。
- main push/dispatch/同工作流定时补偿自动发布，保留running并合并pending。
  API读main的时点固定源码，context记录复用原实现提交；旧实现可由已发布后继覆盖。
- 已有不可变双平台包+STARTUP后幂等复用，双渠道原子push失败不会部分前进。
- GitHub故障工单由发布成功独占关单，使用Release-Fix非自动关单标记。
  故障区分phase，确认merge-gate-gap才强制对应合并前回归；其余基础设施修复可单独恢复。
- 固定checker独立Cargo锁和内容缓存已合入；staged旧example入口改为相同scripts/check。
- 112项阶段回归、后续32项修正回归通过；337项Python清单与绑定无缺失。
  此处不是最终冻结头的组合通过证据；工具缓存快照/外部配置独立审阅问题正在修复。
  source-preflight通过，尚未进行真实新流水线计时，不宣称主线或发布完成。

## 最终验证

最终头81fa1408dba69d02104e5b33cf4c6d5d62a89786经独立复核，
准确PR头CI43898609880与最终组合CI43898595245均通过，Python355项全部纳入。
14:39:16Z首次ready到16:05:11.571042Z主线合并为85分55.571秒，包含修复与重跑；
最终integrate到main为26分29.629秒，其中macOS排队10分02秒。
自动发布43903547266于16:16:59.347256Z确认两个渠道，main到发布11分47.776秒。
与旧26分03秒记录的源码、runner、排队及测试数不同，不报整体提速比例。

真实推进74447dd与9e2f64e两个context主线提交，等待发布43904509825被后者替换，
正在运行的发布未取消且成功；最终等待任务43905335526选择最新main，
实现仍固定81fa140/0.3.98，直接复用原包，产包和发布任务按设计跳过。
这两次记录更新是队列演练样本，不是新增长期日常步骤。

缓存工具启动实测9.274秒冷构建、0.305秒复用、源码变化2.331秒重建；
跨树复用和两个并发请求仅一次编译通过，当前业务改坏仍正确失败。
原0.3.97的库/native_authority两线程未改；没有新增测试调度器。

发布前故障注入43906236333真实失败，演练工单18创建并读回；
两个渠道引用、包、STARTUP与SCENARIOS均保持原样，两平台重试证据逐字节复用。
原CLI dispatch启动正常恢复43908518671，流水线于16:32:27Z自动关闭工单，
并添加release-published。演练针对已存在的可用0.3.98，不制造作者业务故障；
不以此声称真实作者接单修复，正常失败作者反馈与门禁缺口保护已纳入回归。

旧UI工单14/15及草稿PR16保持，不把本轮通过冒充历史偶发故障根因修复。
普通全新业务PR的耗时样本仍待用户发起，不由本轮工具改动full门禁推算。
