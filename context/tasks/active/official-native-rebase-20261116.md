# 官方原生入口同步主线

owner: codex
execution_mode: serial_write
allowed_write_set: 官方入口相关源码、测试、文档与本任务记录
base_commit: 1ba252a
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: in_progress

用户要求将现有实现 rebase 到最新主线。原头 8c5b2f1，
备份引用 refs/codex-backups/official-native-before-rebase-20261116。
主线固定为 1ba252a，保留 0.3.97、PR 队列工具、测试并行设置及共享 STATE。
保留项目准备、来源/账户校验和稳定操作重试，导航改用官方 SSH 单目录入口。
移除主线新增的客户端修改脚本及其专属测试，替换为官方入口行为验证。
本任务不推送、不安装或重载日用插件。
