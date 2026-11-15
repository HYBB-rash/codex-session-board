# 远程主机完整目录落地设计

- id: remote-catalog-design-20261114
- status: completed
- owner: codex
- execution_mode: isolated_write
- coordinator: codex
- base_commit: 082a043116be3610013c25501862faa8c86fedf5
- worktree_path: /Users/hongyong/Developer/codex-session-board-remote-catalog-design-20261114
- allowed_write_set: Docs/REMOTE-CATALOG-DESIGN-20261114.md, context/tasks/active/remote-catalog-design-20261114.md, context/tasks/completed/remote-catalog-design-20261114.md, context/journal/2026-11-14-remote-catalog-design.md
- started_at: 2026-11-14
- completed_at: 2026-11-14

## 目标与授权

为主机切换后完整显示目标主机的项目与持久会话制定方案，立即实测关键接口与可行性。
本轮不实现业务代码，不部署或重载插件，不创建、修改、移动或归档日用会话。
两个只读代理分别核对数据合并、缓存和上游接口；只在各自源码外目录写取证文件。

## 基线

调查开始时源码基线为 966795746becd982ec1bdf1d2e6df735c369601e。
写文档前重新确认协调工作区干净，基线已前进到上述 base_commit，其间新增另一任务的设计记录。
本任务使用独立分支 codex/remote-host-catalog-design-20261114 和独立工作树，不更改共享 STATE、版本及原工作区。

## 进度

- 已实测 Mac/Fedora 当前 0.169 后台的项目分页、读取、全局状态文件和会话分页。
- 已查明旧 ID 映射、迁移 checkpoint、明确无项目、owner marker、来源及 provider 过滤。
- 已复现 sectionVersion=null 时旧缓存被误当在线会话的分支。
- 原样登录/转发 wrapper 和 Fedora→Mac 真正反向 SSH 均已通过，两端确认目标 0.169。
- 两端 account/read 的 userId/accountId hash 与 desktop owner 精确匹配，section 双来源分页通过。
- 无 host 的两个 Mac 旧分组引用已证实不在 Mac DB、实际出现在已连接 Fedora 的列表。
- 原样原生链接接收/导航/renderer 内存探针通过，项目依赖访问即抛错仍能用 hostId 路由；不要求项目登记。
- 设计已固定范围、字段投影、迁移状态机、读取角色、续读/缓存、动作矩阵及实施入口。
- 只读取证完成；业务实现、真实 GUI 打开和 31 条完整验收未执行。

## 检查与交付

设计：Docs/REMOTE-CATALOG-DESIGN-20261114.md。
context 检查已通过；提交前运行暂存快检，提交后运行会话末 ./scripts/check，结果以终端收据及最终回复为准。
仅提交本文档及本任务记录，不推送、合并、安装、部署、重载或更新共享 STATE。

## 取证

源码外目录：/Users/hongyong/.codex/visualizations/2026/11/14/01a1da30-d415-7413-aa72-bf742f3110af/remote-catalog-20261114/
