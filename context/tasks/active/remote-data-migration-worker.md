# 远程子库迁移修复

owner: Euclid
execution_mode: isolated_worker
allowed_write_set: src/service.rs, src/service/migration.rs, migration tests, context/handoffs/remote-data-migration-worker.md
base_commit: 918ee9e4b3d3e3c1b3ab00dcb0f9425f2ce950dd
worktree_path: /home/herman/.codex/worktrees/remote-data-migration/notion-program
coordinator: codex

创建前协调工作树干净。工作者只在独立工作树修改迁移实现及定向测试，
不修改共享登记、状态、日志、版本或测试绑定，不提交或操作真实数据。
协调者审阅补丁、整合、绑定测试并提交。

目标为迁移根库及全部持久主机子库，保留侧车身份与分页进度，
拒绝覆盖和符号链接，SQLite 使用在线备份，失败仅清理本次新建内容。
测试覆盖最新备注、历史、标题队列、输入确认、归档墓碑、目标冲突、
部分失败及重试。跨库快照的一致性边界须明确记录。
