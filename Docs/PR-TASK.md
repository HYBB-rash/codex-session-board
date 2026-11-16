# PR 作者任务 CLI

项目内手动入口是 `.agents/skills/pr-task/SKILL.md`；确定性步骤由 `scripts/pr-task`
执行。它不替换 `validate`、合并队列、独立审阅或组合门禁。
GitHub PR 是反馈来源，合并方打回后继续看下一个，作者的监控自行处理新反馈。

## 开始任务与提 PR

保留一个干净且版本固定的检查器工作树，队列和作者共用相同检查器版本及运行平台。
作者代码放在另一个独立工作树；所有运行记录放在源码外的私有目录：

```zsh
/absolute/frozen-checker/scripts/pr-task init --state /absolute/private/task-state \
  --project /absolute/author-worktree --repository OWNER/REPO --thread "$CODEX_THREAD_ID" \
  --socket /absolute/verified-client.sock --reviewer REVIEWER_LOGIN --archive
/absolute/frozen-checker/scripts/pr-task submit --state /absolute/private/task-state \
  --title '本次改动' --body-file /absolute/pr-body.md
/absolute/frozen-checker/scripts/pr-task status --state /absolute/private/task-state
```

`init` 固定分支、比较基线、原会话、端点对端 PID/启动身份、信任的审阅账号和预算。
同时固定检查器身份，拒绝把作者树本身当检查器。后续命令、定时器及作者唤起消息
均沿用这个入口；检查器变化会停下，不把过期报告交给队列。队列逐项 base 应沿用作者
报告的比较基线。后文相对命令均从冻结检查器目录执行；开发仍在作者树进行。
原会话可以从同一 Git 仓库的原目录进入任务；CLI 分别登记原目录和作者工作树，
拒绝跨仓库迁移。后续修复轮次的 cwd 指向作者树，不要求另造一个失去原上下文的会话。
一个仓库分支只允许一个未结束的作者任务；已有目录重复登记不会刷新预算。
默认必需检查名是本仓库的 `changes`，可重复 `--check` / `--reviewer` 显式设置。
不提供 reviewer 时使用当前 GitHub 登录账号；其他审阅账号必须显式登记。
仅支持 GitHub.com 同仓库 PR；不自动处理 fork、改目标分支或历史分支复用。

`submit` 要求干净的登记工作树，复用 `validate --scope pr --stage submit` 验证准确提交，
只作普通 push，不 force push。首次创建草稿 PR；已创建但丢失响应时按分支和持久
发布意图找回。相同源码且证据仍有效时复用报告，源码变化后重新验证。
本机报告通过且当前头所有检查成功、指定检查存在后，定时器才移除草稿并标记 ready。
这只表示可以独立审阅，不代表已经合并、部署或完成客户端验收。
本机先执行受影响目标、全部 Python 和必要的实际包场景，推送后由 PR CI 执行完整 PR 范围，
候选继续执行双平台组合门禁。报告明确区分 submit 与 ci，不用本机轻量通过代替准确头 CI。

默认 macOS launchd 每 600 秒运行一次 `tick`，不启动模型等待。
机器关机、睡眠或当前轮仍在运行时可能延后，不承诺漏掉的每个 tick 都补跑。
安装前用定时器实际环境验证仓库读取、评论写入及标签权限。凭据适配器只接受单一
可执行入口 `CODEX_GITHUB_COMMAND='["/absolute/adapter"]'`，不把密钥写入 plist；
脚本需在后台独立取得现有授权。现有 `gh` 登录可直接复用。
Python 使用既有 3.11+ 环境及 `websocket-client`，不自动安装新运行时。
其他平台用 `submit --no-schedule` 和外部调度器执行 `tick`；未提供内置 Linux 调度器。

## 反馈与修复

```zsh
./scripts/pr-task feedback --repository OWNER/REPO --pr 123 --head FULL_SHA \
  --request-id STABLE_UUID --body-file /absolute/reproduction.md
./scripts/pr-task tick --state /absolute/private/task-state --force
./scripts/pr-task claim --state /absolute/private/task-state --event EVENT_UUID
./scripts/pr-task complete --state /absolute/private/task-state --event EVENT_UUID \
  --result fixed --message-file /absolute/disposition.md
```

`feedback` 使用稳定 UUID，发布绑定当前源码的可复现问题。同一 UUID 重试不会重复评论。
失败 CI、已发布的请求修改审阅、未解决的行内审阅及结构化队列反馈可触发修复。
不处理 PENDING 草稿审阅、已解决/过期行内讨论、普通聊天或自己的结果回执。
同账号作者和审阅者不会被一律过滤；每条事件保留原链接、源码、ID 和正文摘要。
读取全部分页后再次确认 PR 头和基线；失败、截断、漂移不等于空反馈或通过。

发现、投递、领取、完成分别持久记录。投递使用稳定 client ID；丢响应先查原轮次，
未能证明结果时不会换 ID 重发。原会话繁忙时等待，不中断；忙碌等待不消耗修复轮数。
只有登记的 `CODEX_THREAD_ID` 可以领取；领取时重新检查源码。领取后代码修复通过
`submit` 交付，并写 `complete` 回执。仍存在的同一次 CI 失败不能被签为修复完成。
未解决的问题用 `--result blocked`，停在 `needs-human`，不会自动反复消耗模型。

队列清单设置 `"feedback_mode": "github"`，即可复用 `pr-queue` 的冲突定位、PR 评论、
依赖跳过和组合验证；main 更新后另由自动发布产包，不再从合并方直接唤起作者或等待聊天 ACK。`requeue` 仍要求
准确源码的新审阅和验证。旧清单默认保留 native 模式，避免改变已运行队列。
同一 PR 不要同时启用 native 队列派单和作者监控。

## 恢复与结束

| 情况 | CLI 行为和恢复 |
| --- | --- |
| 网络、限流、临时失败 | 保留事件，退避重试；权限/归属错误暂停，先修正原因再 `resume` |
| 作者轮次失败或未调用 complete | 保留待办，进入 needs-human；确认原轮已结束后 `resume --retry-failed` |
| 修复有未提交代码 | 显式 retry 保留自己的改动；若已有未推送提交，先人工核对并 submit |
| 达到三轮或七十二小时 | 继续观察 PR 终态，但不发新修复；显式 `resume --additional-rounds N --extend-hours N` |
| 后台重启或换端点 | `rebind --socket …` 重新核验同一原会话；未知投递须能找到原 client ID |
| 会话出现别的用户输入 | 停止自动操作；人工核对后才能 `rebind --accept-input` |
| 终态清理中断 | `status` 显示恢复提示，执行 `finalize`；不创建第二个任务绕过旧记录 |
| 已关闭的 PR 重新打开 | 正在清理时停下等待检查；已结束的任务不能复活，另开登记记录 |

`pause` 阻止后续巡检，包括终态自动清理；修复原因后 `resume`。暂停不取消正在运行的
作者轮次，不删除已有定时器。每个操作持有状态目录锁，重叠运行退出 75。
队列收尾可调用 `tick --force --terminal-only`；它只处理合并/关闭，不为开放 PR 新建修复轮次。
`task.json` 是当前状态，`events.jsonl`、版本化投递回执、验证日志保留原始失败。

合并和未合并关闭分别记录。开启 `--archive` 时先核对原会话没有无关输入、
原会话与持久/临时子会话均空闲，再归档并读回确认。原生接口没有 CAS/if-idle 条件，
客户端两次检查仍不能完全消除外部同时输入的竞态，故只适用于独占的专用任务会话。
不加 `--archive` 仍自动停止监控，记录 `not_requested`，不冒称原生归档已完成。

定时器由独立 finalizer 移除；核对 plist 归属、删除本任务文件、卸载本任务服务、
读回确实不存在后才完成。不删除代码、分支、工作树或运行记录。
实现研究见 [PR-TASK-RESEARCH.md](PR-TASK-RESEARCH.md)。
