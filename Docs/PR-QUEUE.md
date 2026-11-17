# PR 合并队列与原会话修复

作者使用项目内 `pr-task` 手动交接或自动巡检时，队列清单设置 `"feedback_mode": "github"`。
协调者只留下 PR 反馈继续处理其他项，不直接唤起作者或等待聊天 ACK。
作者新头仍须独立审阅和验证。详见 [PR-TASK.md](PR-TASK.md)；
没有此字段的旧队列保留本文原生通知模式。

手动交接时由用户转发反馈链接，作者读取 PR 后修复，队列不等待聊天 ACK。
清单及审阅/验证路径由合并会话准备；用户无需操作各内部命令或手填 JSON。
手动作者的 route 沿用 task.json 中的作者身份与 repair_cwd，无需 socket/backend。
该身份只用于来源与独立审阅核对，不能用于原生通知。

作者交付最终头的代码、独立审阅和 PR 检查；协调者核对证据并机械合并。
冲突和失败反馈交回原作者，协调者不接手业务实现。最后复用 `integrate`、
`promote` 的双平台组合门禁推进 main；自动发布随后产包并隔离启动，客户端验收另行安排。

## 用户手动调度

首次安排说“按现有流程合并 PR #N，记录实际耗时”；遇阻塞后由用户安排作者或审阅者。
之后说“继续合并 PR #N”即可。用户不用准备清单、JSON 或报告路径。
合并会话准备原有 init、attempt、冻结及 finish 参数，正常编译、测试和机械操作在本轮执行。
反馈已保存、缺交付材料或外部 CI 未完成时，汇报并结束；不能连续调用 resume 维持轮询。
批量任务处理独立项后统一报告，不等待被打回项修好。

新入口复用同一队列记录：

```sh
/absolute/frozen-checker/scripts/pr-queue resume --repository OWNER/REPO --pr N
/absolute/frozen-checker/scripts/pr-queue status --repository OWNER/REPO --pr N --refresh
/absolute/frozen-checker/scripts/pr-queue status --repository OWNER/REPO --pr N --release
```

init 和写操作将记录位置登记在当前仓库 Git 公共目录的 `pr-queues/` 中，各工作树共用。
旧队列可先用 `resume --state /absolute/existing-queue` 登记，不扫描历史聊天或猜临时目录。
缺记录、目录丢失或多个活动记录冲突时明确返回错误，不创建新候选绕过旧记录。
来源、平台或冻结检查器不符时停下，由合并会话核对，不能改身份字段强行通过。

resume 刷新 GitHub PR、准确头、目标、CI、反馈及 main，核对独立审阅和交付报告。
同一有效候选继续原 run；记录 run 后只查询该 run，不能用另一个成功运行覆盖首次失败。
未冻结时返回后续机械组合或冻结动作，合并会话继续使用原命令，不另造发布入口。
新头使用已有 requeue 登记当前审阅和验证，可先准备 `--item` 再交给 resume；
未登记或不匹配的证据保持待补，不要求作者无故修改已经修好的源码。
未被打回而自行推进的新头也走这条入口，不凭空创建修复反馈或等待 ACK。
新头须延续已登记源码并通过准确头证据核验，再回到 submitted 进行机械组合；
旧候选和原运行仍保留，但不能替新组合签收。终态项目不能通过 requeue 重开。
main 漂移时保留旧候选，下一步交给合并会话按既有规则重新组合和验证。
外部已合并的 PR 还须确认合并提交属于当前 main；关闭未合并、重新打开分别处理。

| handoff.result | 本轮动作 |
| --- | --- |
| merged | 报告 main、检查及发布状态，返回 |
| waiting-ci | 保存候选、报告及 run，交接后返回 |
| needs-author | 交回现有 PR 反馈和实际错误，返回 |
| needs-evidence | 明确准确头缺少的审阅或验证，返回 |
| infrastructure-blocked | 保存网络、runner 或反馈送达错误，返回 |
| needs-user | 列出具体阻塞及 next_actor；main 漂移由协调者重新组合 |
| closed-without-merge | 确认未合并关闭，执行必要的幂等记录 |
| ready | 本地准备尚未结束，按 next_action 继续机械操作 |

status 默认仅看本地；--refresh 只读刷新 GitHub，不写记录或推进任务。
--release 显式查看既有发布流水线，不等待发布，不触发发布或修复。
状态查询失败退出 2，不能把旧观察当作新通过。写操作退出 0 也可能是 waiting-ci，
必须读取 handoff.result；其他阻塞退出 2，锁冲突退出 75。旧原生运输模式保持原入口。

队列保存每条 CLI 调用的开始、结束、结果、前后间隔、复用项与重新核对项。
合并会话在每次用户请求开始时记录带时区时间，将其传给本轮最后一步
`--started-at REQUEST_START_ISO`，得到 request_seconds；seconds 仅为该 CLI 调用耗时。
首次就绪时间来自 GitHub 时间线，最初非草稿则使用创建时间，修复不重置。
ready_to_main_seconds 包含用户交接、作者修复和外部 CI 等待，不拿 CLI 用时替代。
未知就绪或合并时间保留 null，不估算；进程被强杀的旧调用记录为 interrupted，
只记发现中断的时间，不虚构真实退出时间。

每轮报告包含结果、当前 PR 头和候选/main、对应检查、问题及反馈链接、下一步接手方、
“继续合并 PR #N”的指令、本轮用时和可确认的累计用时。
候选、作者源码、工作树、冻结检查器和首次失败证据保留到明确收尾。
子命令输出追加到候选旁的日志，即使初始化失败、尚未创建 integration.json 也可恢复错误。
首次失败按 run/attempt 保存不可覆盖的快照，每次调用记录对应工作流身份。
合并完成后本轮不持续观察作者或发布，也不替已有 automatic 作者任务改模式或删除定时器。

## 入口

所有队列记录放在源码外的新目录，原始失败不覆盖：

```sh
./scripts/pr-queue init --manifest /tmp/queue-input.json --state /tmp/queue-run
./scripts/pr-queue attempt --state /tmp/queue-run
./scripts/pr-queue notify --state /tmp/queue-run
./scripts/pr-queue requeue --item /tmp/repaired-pr.json --state /tmp/queue-run
./scripts/pr-queue status --state /tmp/queue-run
./scripts/pr-queue finish --state /tmp/queue-run
./scripts/pr-queue closeout --state /tmp/queue-run
./scripts/pr-queue retry-candidate --state /tmp/queue-run
```

`attempt` 检查固定输入并尝试机械合并。失败项保留 PR 评论、反馈编号与原始日志，
有依赖的项等待，独立项继续。`notify` 对同一反馈补发或读取原会话回执，不创建新反馈。
派单前用固定头执行机械合并预检，显式登记会冲突的依赖顺序。共享文件的修复先收齐
前置固定头，再开始最终验证；不能让作者分别对旧前置跑完完整检查后才告知下一处冲突。
共同候选失败先定位日志中的实际错误，再交给对应作者；通知全部参与者不等于要求全部修改。
`requeue` 验证作者交回的来源、审阅和检查。本轮可独立交付的接纳项准备好后，协调者按既有规则
冻结版本并提交，再调用 `finish`。该命令复用 `integrate`，不安装插件。
接纳、冻结与 `finish` 属于连续的执行步骤。结果已就绪时立即推进，过程计时保留每步开始、
结束与等待间隔，不能把协调者未及时推进的时间归为编译或 CI。
冻结必须紧接已接纳前缀，只提交发布版本字段和精确发布记录；不得夹带新业务代码。
本机 submit 报告允许先机械组合并启动候选，PR CI 同时运行。
`finish` 将接受项固定为 PR 门禁输入；候选通过后核对准确头的 PR 工作流及 changes job，
推进主线前再次检查。变头、失败、缺少工作流或未处置审阅反馈均拒绝。
未完成返回 waiting-ci，恢复继续核对同一组 PR 输入，旧记录不能因缺少字段跳过该门禁。
代码失败回到作者新头、审阅和验证后重新冻结。基础设施失败可以通过既有 GitHub 工具
重跑原 workflow，再用 `retry-candidate` 核对同一源码、同一 run 的新成功 attempt，
继续原候选；首次失败保留，不需要空提交或另造版本。

输入清单包含仓库、干净的独立候选工作树、固定主线与按依赖排序的 PR：

```json
{
  "repository": "OWNER/REPO",
  "feedback_mode": "github",
  "project": "/absolute/candidate-worktree",
  "base": "FIXED_MAIN_SHA",
  "items": [{
    "number": 1,
    "head": "FIXED_PR_SHA",
    "base": "FIXED_MAIN_SHA",
    "dependencies": [],
    "review": "/absolute/review.json",
    "validation": "/absolute/pr-validation",
    "author_task_state": "/absolute/private/author-task-state",
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
`author_task_state` 明确登记本机作者任务，可省略，但缺失时不能宣称作者监控已收尾。
同一源码只补交交付证据时，重新核验独立审阅和报告后，队列发布绑定原反馈全文摘要的
resolved 记录。原始失败保留；修改过的旧评论、新反馈及其他请求修改不因此解除。

## 收尾

主线确认合并后可显式运行 `closeout`，不等待发布。收尾 pending 不改变已合并事实，
也不要求合并会话常驻；保留记录后交接。它核对作者任务的仓库、PR、源码、原会话、
工作树和冻结检查器。手动任务调用 `pr-task finalize`，只结束任务记录，不归档或删除工作树；
自动任务复用 `pr-task tick --force --terminal-only` 处理终态，
由既有 finalizer 归档和移除定时器，不另写归档或删除实现。
开放或重新打开的 PR 不从这个入口启动修复模型；繁忙、离线和身份不符记录 pending。
已完成任务再次执行不会重复归档。不清理作者分支、源码、工作树或原始运行记录。
全部登记项完成退出 0；缺登记或待处理退出 2，重试保留原始收尾起点。
项目记录在独立收尾中一次提交，按授权复用 `integrate` 的 context 专用路径同步；
没有远端同步授权时保留本地记录，不把同步等待绑定在合并轮次。

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

若后台明确表示分页读取方法不支持，运输使用 `thread/read` 回读准确会话和轮次。
其他 RPC 错误不能据此降级，未知发送状态仍须对账，不能盲目重发。助手回执没有原生
时间戳时只记录观察上界，不据此计算精确接单延迟。

开始发送前保存 `delivery_unknown`。响应丢失后查询原消息的 client ID，
不盲目重发；没有找到时仍保持未知。连接或身份变化需要重新核验。
同一反馈不要换输出目录重试；评论使用反馈标记去重。
凭据沿用已授权的 GitHub CLI 登录或 `CODEX_GITHUB_COMMAND` 参数数组，
不写入清单、评论、消息或回执。

## 计时与历史演练

从队列固定输入开始计时，保留冲突、通知等待、作者修复、失败与重跑。
合并结束点是目标主线确认同一候选；发布另从 main 更新计时，到双平台不可变插件包发布完成。
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
