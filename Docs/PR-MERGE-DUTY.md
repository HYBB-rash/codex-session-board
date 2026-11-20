# PR 合并值班试点

## 正式试跑

由用户指定一条 PR 并手动开始一轮。合并会话准备交接清单和独立候选工作树，沿用作者任务登记的冻结检查器与平台。已有队列通过 `pr-queue status --repository OWNER/REPO --pr N --refresh` 定位和核对，在用户安排继续时调用同样参数的 `pr-queue resume`。首次入队和其余操作见 [手动合并](PR-QUEUE.md#用户手动调度)。

CLI 核对当前头、作者的 submit 报告和独立审阅，保留当前 PR CI 与候选验证的来源。完整 PR CI 可与候选构建并行，推进 `main` 前要核对当前头的完整 PR CI 和组合门禁。头变化要重新核对交付和审阅，`main` 变化要重新组合。

用户可在专用 Luna 会话手动调用 `$pr-merge-duty`，归纳 CLI 结果。使用 `feedback_mode=github`。冲突及可归属失败在 PR 留下实际错误、日志和复现入口，保存反馈后返回。等待 CI 时保存候选和 run 后返回。用户负责把结果或反馈链接转交作者，以及安排下一轮。保留已有作者任务各自的模式。

最终组合门禁、远端 `main` 和 GitHub PR 终态确认后报告已合并，并注明当时的发布状态。由现有流水线发布，后续可用 `pr-queue status --release` 单独查询。

## 正式只读巡检

在独立的干净 `main` 检出运行 `python3 -B scripts/pr_merge_duty.py scan --config /absolute/private/config.json`。
配置使用 `mode: production-scan`，设置正式 `repository`、当前检出的 `checker` 和完整 `checker_head`、
源码外的私有 `state`，以及 `handoff` 指向正式交接清单的绝对路径。
交接清单包含 `repository` 和下面格式的 `assignments`，由协调者录入已明确交接的 PR。
空清单返回 `idle`，清单缺失或检出的提交与远端 `main` 不符则报告错误。
每轮在原值班会话记录执行路径、提交和结果。正式模式拒绝 `tick` 及其他控制命令。

迁移 Codex 原生定时任务时，保存原提示和配置。会话空闲时原地更新任务的提示，保留原任务 ID、
目标会话、间隔和通知偏好。只修改同一任务的入口。用两次实际定时运行核对新命令、提交和实际模型。
需要回退时，在一轮结束后恢复该任务原来的只读提示和配置。

## 隔离试点

原隔离试点的 timer 和测试后台已停止。
`pr_merge_duty.py` 接受 `mode=isolated` 与独立 GitHub 仓库。隔离配置放在源码外的私有目录。
`scan` 只核对交接证据，`tick` 执行一轮，清空 `assignments` 可核对空入口。

```json
{
  "mode": "isolated",
  "repository": "OWNER/TRIAL-REPO",
  "checker": "/absolute/frozen-checker",
  "checker_head": "FULL_CHECKER_SHA",
  "state": "/absolute/private/duty-state",
  "assignments": [{
    "pr": 1,
    "head": "FULL_AUTHORIZED_PR_SHA",
    "candidate": "/absolute/candidate-worktree",
    "author_task": "/absolute/author-task-state",
    "review": "/absolute/independent-review.json"
  }]
}
```

候选独占 `codex/integration-*` 分支，首次从当前远端 `main` 创建，fetch 和 push
地址均指向配置的 GitHub 仓库。作者通过同一干净检查器提交并保留 `task.json`。
审阅使用队列已有的 `head`、`reviewer`、`passed` 记录。

`last-tick.json` 保存隔离试点的最近结果。队列目录保留组合源码、运行身份、反馈和失败证据。
隔离 `delivery-*.json` 保存测试会话、请求和轮次供复核。
