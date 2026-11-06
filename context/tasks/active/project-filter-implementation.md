# 项目选择与搜索切换

owner: codex
execution_mode: serial_write
allowed_write_set: src/, tests/, index.html, scripts/, acceptance/bindings.json, Docs/, README.md, Cargo.toml, Cargo.lock, context/
base_commit: 1452dd7b595c0eafbe1748092e0a420766539304
worktree_path: /home/herman/文档/notion-program
coordinator: codex

实现已确认方案：读取当前本地 Codex 项目，按工作目录动态关联会话，
未关联归默认工作区；移除项目后归默认，重新添加后自动识别回来。
旁边按钮切换项目选择与标题、路径搜索，保留六状态与原有编辑刷新保护。
两种筛选模式分别生效，避免隐藏的项目条件影响全局搜索。

协调者统一写入。两名只读代理分别检查前端接线与原生工作树关联来源，
不得修改共享文件、Git 暂存区或运行配置。
沿用本轮已授权的检查与分批提交流程，保持每批 13 文件、400 行及补丁升版。
不修改冻结的需求、独立模型或锁，不降低验收门槛。
开发与界面验证使用隔离数据；本任务不改现有生产配置或会话成员关系。
