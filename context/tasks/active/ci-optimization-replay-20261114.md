# CI 分层、分组计时与 PR #5 重演

owner: codex
execution_mode: coordinated_write
allowed_write_set: 候选验证与连续集成脚本、工作流、相关测试和绑定、规则文档、版本冻结及本任务上下文
base_commit: 2634e48153432dec4efcf87612ff0672fed86b48
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: implementing

用户授权优化分组调度、按依赖运行工具自测、按改动范围验证和连续执行入口。
实现后用 PR #5 的固定头54096cd、历史主线b593edd完整重演。
优化开发和首次工具变更的完整验证单列；计时演练从固定来源准备开始，
到受保护主线、双平台不可变实际包、记录同步和自有分支清理分别停止计时。
基线为上一轮核心1485.901秒、含收尾1795.529秒。

完整31条需求、参考模型、锁及完整验收入口保持不变。
分层合并报告单独声明范围，未执行不算通过；未知路径及门禁自身变化退回全量。
并行只用于已经核对隔离性的测试组，原生与性能敏感组保持独占和原断言。
沿用现有包验证、启动检查和受保护主线提升，不安装或重载日用客户端。
只读子代理分别审查范围门禁及连续入口，主代理独占代码和共享记录写入。

优化提交为42fd4bc、45ae192、89d358d和fa7bf37。
新增分组计时、受控工具组并行、保守范围规划、独立集成报告及连续候选入口，
promote复用实际包验证并保留双平台报告与包。33项相关工具回归通过。
复核发现并修复NUL路径被strip误分类及错误工作目录可能测试旧实现的边界，
补充特殊路径、真实重演、精确版本父提交、失败不提升和实际包身份回归。
PR #5远端仍为已合并，固定来源54096cd未变化；主线保护仍严格要求Actions Candidate gate，
管理员同样受限，禁止强推和删除。工具变化首先走完整双平台候选路径。

源码外证据：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/ci-optimization-replay-20261114/
