# PR 合并队列与原会话修复

作者交付最终头的代码、独立审阅和 PR 检查；协调者核对证据并机械合并。
冲突和失败反馈交回原作者，协调者不接手业务实现。最后复用 `integrate`、
`promote` 的双平台组合门禁、不可变包和隔离启动，客户端验收另行安排。

## 入口

所有队列记录放在源码外的新目录，原始失败不覆盖：

```sh
./scripts/pr-queue init --manifest /tmp/queue-input.json --state /tmp/queue-run
./scripts/pr-queue attempt --state /tmp/queue-run
./scripts/pr-queue notify --state /tmp/queue-run
./scripts/pr-queue requeue --item /tmp/repaired-pr.json --state /tmp/queue-run
./scripts/pr-queue status --state /tmp/queue-run
./scripts/pr-queue finish --state /tmp/queue-run
./scripts/pr-queue retry-candidate --state /tmp/queue-run
```

`attempt` 检查固定输入并尝试机械合并。失败项保留 PR 评论、反馈编号与原始日志，
有依赖的项等待，独立项继续。`notify` 对同一反馈补发或读取原会话回执，不创建新反馈。
`requeue` 验证作者交回的来源、审阅和检查。所有目标项准备好后，协调者按既有规则
冻结版本并提交，再调用 `finish`。该命令复用 `integrate`，不安装插件。
冻结必须紧接已接纳前缀，只提交发布版本字段和精确发布记录；不得夹带新业务代码。
代码失败回到作者新头、审阅和验证后重新冻结。基础设施失败可以通过既有 GitHub 工具
重跑原 workflow，再用 `retry-candidate` 核对同一源码、同一 run 的新成功 attempt，
继续原候选；首次失败保留，不需要空提交或另造版本。

输入清单包含仓库、干净的独立候选工作树、固定主线与按依赖排序的 PR：

```json
{
  "repository": "OWNER/REPO",
  "project": "/absolute/candidate-worktree",
  "base": "FIXED_MAIN_SHA",
  "items": [{
    "number": 1,
    "head": "FIXED_PR_SHA",
    "base": "FIXED_MAIN_SHA",
    "dependencies": [],
    "review": "/absolute/review.json",
    "validation": "/absolute/pr-validation",
    "route": {
      "socket": "/absolute/private-app-server-socket",
      "backend": {"pid": 123, "started": 456},
      "coordinator_thread": "COORDINATOR_UUID",
      "thread_id": "ORIGINAL_AUTHOR_UUID",
      "expected_cwd": "/absolute/original-thread-cwd",
      "repair_cwd": "/absolute/isolated-repair-worktree"
    }
  }]
}
```

审阅文件至少包含 `head`、`reviewer` 和 `passed`，审阅者须与作者不同。
修复返回文件包含 `number`、`head`、`base`、`review` 和 `validation`。
新头须保留原头祖先关系；源码变化后，审阅及验证重新绑定，旧报告不能放行。
PR 检查通过统一入口执行，检查器在本轮队列中固定。作者工作树和检查器都须保持干净；
固定后不在检查器工作树追加收尾提交，否则会使本轮证据失效。

## 原会话通知

运输复用已安装的 `websocket-client` 与现有共享后台进程身份检查。
端点必须来自原客户端后台的实际连接；不从 `daemon version` 猜地址。
核对私有 socket、实际 RPC 连接的 peer PID 与启动身份、协调者已加载及原会话 ID/cwd，
随后才可恢复未加载会话或发起作者轮次。当前实现覆盖本机 Unix socket；
没有可核验的原后台连接时保留待通知，不换后台冒充成功。

每条反馈绑定唯一 request ID 与固定回执文件。收到 `turn/start` 返回值只算已发送；
只从该原会话、该轮次的 assistant 消息读取精确 ACK。轮次结束只表示作者处理结束，
不能证明业务修复通过，仍须核对返回的新源码与证据。
观察到繁忙状态时等待，不主动中断。外部客户端并发启动仍由后台约束，预检查不是锁。
派单会将原会话的后续默认目录设为修复工作树；收尾保留该目录及其来源记录，
直到会话明确迁走，不能因演练结束就删除。已发送反馈的只读回执不依赖作者后续目录。

开始发送前保存 `delivery_unknown`。响应丢失后查询原消息的 client ID，
不盲目重发；没有找到时仍保持未知。连接或身份变化需要重新核验。
同一反馈不要换输出目录重试；评论使用反馈标记去重。
凭据沿用已授权的 GitHub CLI 登录或 `CODEX_GITHUB_COMMAND` 参数数组，
不写入清单、评论、消息或回执。

## 计时与历史演练

从队列固定输入开始计时，保留冲突、通知等待、作者修复、失败与重跑。
结束点是目标主线确认同一候选且双平台不可变插件包可用。
开发工具、准备历史样本及事后收尾分别记录；不能删除失败时段再计算成绩。
独立子集交付不等于整队完成，报告逐项列出仍等待的 PR。

已合并的历史 PR 在当前主线上会被 Git 判为已存在，不能以此作为演练。
应在历史基线重放原头，固定原会话与独立修复工作树，PR 评论明确标注历史演练。
演练主线限定为 `refs/heads/codex/rehearsal-main-*`，候选使用
`codex/integration-rehearsal-*`。演练产物与正常安装渠道分开，
仍运行同样的组合、实际包及启动门禁；不回退真实主线或修改日用客户端。
清单设置 `rehearsal: true` 和 `reference`；最终冻结同时添加
`context/integration-target.json`，内容必须精确等于清单中的 `reference` 和 `base`。
演练 Release 使用 `rehearsal-<完整候选SHA>`，不更新候选或稳定安装渠道。
