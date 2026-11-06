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

## 实现与验证记录

源码版本 0.1.26。项目源、动态归属、外部工作树和模式切换已接线，
补充九项 Rust 回归；同名、多根、嵌套、目录缺失、读取失败和归属歧义均有处理。
用本机真实项目登记表的只读副本验证四个项目，以及当前目录和外部工作树归属。
现场发现路由器项目的 Git 元数据残缺，已修复其干扰其他工作树归属的问题并补回归。

隔离浏览器验证切换记忆、跨项目搜索、默认与空项目、移除与不同 ID 重新添加、
损坏登记表保留旧分类、编辑草稿保护以及异常归属不混入默认工作区。
证据在 outputs/project-filter-ui/，验证用临时服务已停止，未修改现有部署。

完整验收执行退出 2，252 项自动检查通过，但真实环境证据未齐，
release_ready=false，发布验收保持待验证，不宣称开发完成。
报告：/tmp/session-board-project-filter-final-acceptance.json。
本记录归档实现和隔离验证；最终提交后仍须执行普通检查直到退出 0。
