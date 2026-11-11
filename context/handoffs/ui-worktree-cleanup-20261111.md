# UI 修复发布后的工作区清理交接

修复提交 fb9b697、合并 2caa1c6，正式服务及插件为 0.2.82。
正式包、五库备份、数据核对、原生资源核对及真实页面布局证据均位于：
`/home/herman/.local/share/codex-session-board/releases/0.2.82/`。
主仓库 `outputs/ui-stable-columns/` 保存完整输出、收尾检查及其他任务改动的保全证明。

当前会话 ID 为 01a1c695-578a-76b2-a55a-e294f1277751。
其受管工作树为 `/home/herman/.codex/worktrees/ui-stable-columns/notion-program`，
list_artifacts 返回的 identityKey 与该完整路径一致。
archive_worktree 实际返回 `The task's primary checkout cannot be removed.`。
当前工具 handoff_thread 明确限制 `The calling thread cannot move itself`。
修复已发布；仅该工作区的最终受管归档尚未完成，不能宣称已经移除。

先通过桌面端迁移本会话，或由得到用户明确授权的其他会话执行 handoff，
将当前工作位置迁回主检出 `/home/herman/文档/notion-program`。
迁移后重新读取上下文、list_artifacts，确认本 worktree 已不再是主工作区且提交都已合入，
再通过 archive_worktree 归档，保存工具回执并更新任务及状态。
勿自行向其他会话发送迁移指令；消息工具要求人类明确授权给指定任务发消息。
正式进程和正式插件使用稳定发布目录，不依赖本工作树，不需要再次部署。

其他三个历史远程工作区虽然已有阶段成果，但仍关联 active 的远程验收会话，
且验证 marketplace 仍引用 remote-parity-lab，本轮未移除。
其源码差异、Git 历史、outputs 和必要 target 原始记录已保全并逐文件核对：
`/home/herman/.local/share/codex-session-board/worktree-archives/20261111/`。
保留 state-section-consistency、remote-restore-acceptance 以及完成情况不明的九个 notion-exp-*。
