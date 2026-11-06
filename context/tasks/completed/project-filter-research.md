# 项目下拉筛选调研

owner: codex
execution_mode: coordinated_read_only
allowed_write_set: context/tasks/active/project-filter-research.md, context/tasks/completed/project-filter-research.md, context/journal/2026-11-06-project-filter-research.md, context/STATE.yaml
base_commit: 836f734d095d7a308ef18959dbc7ebb1884e3eb0
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户提出将看板搜索框改成项目下拉框，列出本地已加载到 Codex 的项目，
未归项目会话放入默认工作区。本轮仅调查并提出边界问题，等待讨论。

协调者读取当前来源与真实项目、会话元数据；两名只读代理分别调查
看板筛选与验收约束，以及 Codex 项目、工作目录和原生分区的关系。
代理不修改文件；协调者只维护本轮上下文记录。

保留工作区已有未提交的状态分区实现，不修改业务代码、部署或运行数据。

调研完成。原生列表确认四个本地项目，另有远程与 ChatGPT 项目需要排除。
当前聊天 cwd 属于已登记项目，但原生 projectId 为空，默认工作区定义需讨论。
已整理目录匹配、工作树、同名项目、移除、失败、计数、标题搜索和新建按钮边界。
结果与来源记录于 context/journal/2026-11-06-project-filter-research.md。
没有实施新功能或改变冻结验收；检查仅因未提交工作区失败。
