# 远程新建 SSH 别名配置

owner: codex
execution_mode: serial_write
allowed_write_set: 看板新建与连接设置、相关测试与文档、本任务与交接记录
base_commit: 769f1f0
worktree_path: /home/herman/.codex/worktrees/remote-ssh-alias-preview/notion-program
coordinator: 当前会话
status: active

首件提供三条可演示流程。已绑定别名的连接沿用原项目；全新连接展示修改预览及拒绝、重新启用；无别名旧连接引导用户从 Codex 原项目新建。
新连接设置由隔离 fixture 提供预览与决策回执，实际写入端由后续开发接入。首件提交后由委派人核对。
原装客户端、日用 SSH 配置、Codex 登记和真实会话遵循独立授权边界。
