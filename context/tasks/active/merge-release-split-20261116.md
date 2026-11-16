# PR 合并与自动发布分离

owner: codex
execution_mode: coordinated_write
allowed_write_set: 合并与发布CLI、工作流、固定checker缓存、对应测试文档及本任务上下文
base_commit: 1ba252ae5a9f01a34eb1579629a5e3ae64d41bcb
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

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
