# PR 队列源码包清单修复

owner: queue_security_review
execution_mode: isolated_worktree
allowed_write_set: scripts/package_migration.py; context/tasks/active/pr-queue-package-fix-20261115.md
base_commit: ec4e9c6bd73f477a6baf5d1ff6bceea6ed0a5e3b
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-release-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

工具发布运行 43731189754 的两个平台均在源码包 preflight 失败。
实现指纹包含 `.github/pull_request_template.md` 和 `scripts/pr-queue`，
归档清单未包含这两项，导致重新解包的指纹不一致。本次仅补齐已有打包清单。

冻结检查器 ec4e9c6 及四个作者工作树保持不变。版本由协调者随后冻结；
本任务不推送、不发布、不修改共享 STATE，不处理已分离的评论权限问题。

验证使用统一源码包 preflight、项目检查及暂存快检，所有 Cargo 检查限制为两个并行任务。
证据保存在源码外的 pr-queue-20261115/package-fix 目录。

首次统一 preflight 的实际打包步骤退出 0，收尾因工作树尚未提交而拒绝签收；
该失败保留在 preflight/VALIDATION.json。首次项目检查仅报告预期的未提交改动，
518 次提交的版本步进检查通过。提交后从干净源码再次运行统一 preflight 和项目检查，
最终证据分别写入 preflight-committed/VALIDATION.json 和 check-committed.log。
暂存快检记录为 pre-commit.log，任务归档和共享状态更新交由协调者完成。

最终随0.3.93、61d9d280进入真实主线。CI43746211763双平台门禁通过，
源码包清单、实际不可变包读回和隔离启动均通过，首次缺项失败继续保留。
