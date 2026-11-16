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
