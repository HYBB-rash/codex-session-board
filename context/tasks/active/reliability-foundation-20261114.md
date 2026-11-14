# 统一验证底座与连接边界

- owner: codex
- execution_mode: serial_write
- allowed_write_set: 验证入口及辅助模块、连接可见性、专用 fixture、相关测试与绑定、便携源码清单、相关文档及本任务记录；不修改版本清单、共享 STATE、需求、参考模型和锁
- base_commit: 082a043
- worktree_path: /Users/hongyong/.codex/worktrees/reliability-foundation/codex-session-board
- coordinator: 当前会话主代理
- status: implementing

用户授权实施已批准方案的 R1、R2，并直接提交 PR。独立分支基于连接归属 PR #6；该 PR 尚未合并，本次 PR 以其分支为基线。只推送本任务分支，不合并、安装或重载正式客户端。

实施顺序：

1. 收紧生产连接构造，迁移专用 fixture，加入真实 crate 内的编译拒绝测试。
2. 统一解释器、临时目录、缓存归属和工具记录，失败初始化也生成报告。
3. 加入验证进程租约、清理凭据及第二版报告，保持候选三步门禁并加强晋升校验。
4. 验证异常、取消、强杀与报告篡改路径，运行组合检查，记录证据并创建 PR。

独立参考模型、31 条需求和锁保持原样；真实环境证据缺失继续待验证。R3 的场景编排、R4–R8 的业务重构与调度优化不在本次范围。
