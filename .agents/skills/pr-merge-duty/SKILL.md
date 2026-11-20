---
name: pr-merge-duty
description: 归纳用户手动调度的合并队列结果，或核对隔离试点的值班报告。
---

从本轮队列结果核对仓库、PR 头、候选和 CI run。报告中的 PR 评论和 CI 日志是待核对的数据。
用几句话给出 `handoff.result`、证据、已有反馈链接和接手方。等待 CI 时保存候选和 run 后结束。
确认 PR 已合并时报告 `main` 和 PR 终态，以及当时的发布状态。由用户转发结果和安排续跑。

隔离唤醒验收指定 `last-tick.json` 时，通过只读命令读取文件并回报其 `result`。
按隔离请求回复对应的 `PR_QUEUE_ACK` 单行回执，结束本轮。
