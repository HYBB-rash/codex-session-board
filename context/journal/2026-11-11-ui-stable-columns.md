# 2026-11-11 看板等宽修复发布

用户授权隔离开发、合并、正式部署和清理已结束任务的工作区。
根因为标题按钮继承 nowrap，叠加看板 min-content，使内容决定全部列宽。
改正原规则，保留既有布局结构；新增真实 Chrome 回归并绑定 UI-003。

实现 fb9b697 与主线合并为 2caa1c6，正式版本 0.2.82。
20 组浏览器回归和提交钩子全部通过，旧页面会被新回归拒绝。
完整验收 500/500 自动通过，14 项外部证据待补，退出 2、release_ready=false。
部署前后五库原会话 ID、备注摘要、库完整性和原配置一致。
正式 MCP 已重载并核对版本及真实页面资源；正式 HTTP 页面 159/5 条会话筛选往返五列均为 240px。
没有操作其他会话正在展示的原生验收页面；原生宿主实屏不由 HTTP 截图代签。

部署与恢复材料：`/home/herman/.local/share/codex-session-board/releases/0.2.82/`。
本任务输出：主仓库 `outputs/ui-stable-columns/`，包含验收、测量、截图和发布核对。
本任务专用 worktree 在收尾合并后通过 archive_worktree 归档，实际回执记录在输出目录。
其他任务的未提交修改保留；没有推送或改写其提交。

三个历史远程工作区归属仍运行的验收会话，保留其关联关系及 marketplace 引用。
另备份源码差异、Git 历史、全部 outputs 和必要 target 原始记录，并逐文件核对 SHA256：
`/home/herman/.local/share/codex-session-board/worktree-archives/20261111/`。
两处当前开发/验收工作区及九处完成情况不明的历史实验工作区均未移除。
