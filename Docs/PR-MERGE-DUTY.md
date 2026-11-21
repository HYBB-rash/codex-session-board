# PR 合并值班

## 正式定时值班

原生定时任务 `pr` 每十分钟续接同一个 GPT‑6 Luna 会话，在独立的干净主线检出执行
`python3 -B scripts/pr_merge_duty.py tick --config /absolute/private/duty.json`。
配置为 `mode: production-duty`，并设置 `repository`、运行检出 `checker`、完整 `checker_head`、
原值班会话 `coordinator_thread`、源码外的私有 `state`。启用时核对运行入口与远端 `main`。
之后 `main` 前进时，值班源码必须仍包含在主线，组合门禁按本轮实际远端 `main` 重新准备。

正式定时任务从持久应用数据目录中的独立 Git 克隆加载脚本和 skill。
用 `registry_project` 指向现有作者任务与合并队列登记所在的持久项目仓库。
正式值班锁沿用该仓库的 Git 公共目录，领取与派单状态继续使用原 `state`。
单个 PR 的作者与候选仍使用自己的隔离工作树，生命周期跟随该 PR。

CLI 完整查询 GitHub 打开且目标为 `main` 的 PR。首领只认本仓库来源、非草稿且带
`ready-for-review` 的 PR。`scan` 预览待领取和在办项，`tick` 持久领取并跟进。
每轮最多首领一条，其他待领项继续显示，在办和新项轮流推进。
源仓库、源分支或目标变化时暂停该项。GitHub 查询错或分页重复时保存错误，下一轮重新完整查询。
同一仓库只允许一个值班 CLI 持锁，领取记录不因空结果、重启或回退消失。

优先通过作者仓库的 `pr-tasks` 登记核验提交及原会话，再复用同一 PR 的 `pr-queues` 登记。
没有作者任务的既有手动 PR，可以由协调者核对证据并在 `author_receipts` 按 PR 号指定私有 JSON。
该记录包含 `repository`、`pr`、`branch`、`base`、`base_commit`、`delivery_head`、
`validation`、`checker`、`checker_project` 和原作者 `route`。没有可信路径或准确头交付时保留待补状态。
已有的组合、候选、反馈和通知继续使用原队列和回执。刷新源码时保留旧头证据，新头独立审阅；
重新组合在独立候选中完成，旧组合和跑过的 CI 仍可查。

每个 PR 使用一个专用的独立审阅会话。`reserve-review` 按准确头原子预约，`begin-review` 返回
新建或续用的派单方式。`record-review` 将首次实际审阅会话固定到这个 PR，后续新头向同一会话派单。
升级前已有多个审阅会话的 PR 续用最近一次已登记的会话；历史记录保留。
开始派单后中断或响应遗失时先查原任务或工具记录。证实未发送才用 `recover-review` 恢复。
回执继续按完整头区分，每个新头都需要重新审阅并给出结论。
正式合并继续复用冻结检查器、作者交付、准确头 GitHub PR CI 和组合门禁。
后台写入异常以实际错误为准，合并终态和作者送达分别记录。

上线前备份原任务及只读配置。只在会话空闲时原地更新同一任务 ID。
回退在原任务恢复下面的 `production-scan` 入口，同步只读检出的 `checker_head` 到远端 `main`；
`production-duty` 的领取、审阅及队列状态保留在原目录。

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
