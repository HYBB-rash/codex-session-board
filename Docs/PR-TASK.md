# PR 作者任务 CLI

项目内手动入口是 `.agents/skills/pr-task/SKILL.md`；确定性步骤由 `scripts/pr-task`
执行。它不替换 `validate`、合并队列、独立审阅或组合门禁。
GitHub PR 是反馈来源，合并方打回后继续看下一个。项目 skill 默认手动交接，
用户通知作者读取反馈；自动模式才由监控处理新反馈。

## 手动交接

仍使用同版本、同平台的冻结检查器和独立作者工作树；记录放在源码外。
项目 skill 负责准备这些路径和参数，用户只安排任务并转发 PR 链接。

```zsh
/absolute/frozen-checker/scripts/pr-task init --mode manual \
  --state /absolute/private/task-state --project /absolute/author-worktree \
  --repository OWNER/REPO --reviewer REVIEWER_LOGIN
/absolute/frozen-checker/scripts/pr-task submit --state /absolute/private/task-state \
  --title '本次改动' --body-file /absolute/pr-body.md
/absolute/frozen-checker/scripts/pr-task status --refresh --state /absolute/private/task-state
/absolute/frozen-checker/scripts/pr-task ready --state /absolute/private/task-state
```

`init --mode manual` 不连接客户端，不需要 socket，也不允许 `--archive`。
`--thread` 或 `CODEX_THREAD_ID` 仅用于作者身份；未提供时记录 `manual:分支名`，
不是客户端会话 ID。队列沿用该记录判断作者和独立审阅者，不向此身份发送消息。
手动 `submit` 不安装定时器，复用下文的准确头验证、普通推送和草稿 PR 创建。
同一任务的模式不可更改；旧记录没有 mode 字段时仍按 automatic 处理。
CLI 的 init 默认值仍为 automatic，以兼容既有调用；新 skill 显式选择 manual。

`status` 读取本地记录；`status --refresh` 额外读取 GitHub，并输出
`handoff`：链接、准确头、CI、反馈正文与来源链接、下一步。刷新不改 PR、任务记录、
标签或草稿状态，不连接客户端、不推进生命周期。`phase` 仍是上次本地操作的结果，
当前远端结果以带观察时间的 `handoff` 为准。请求失败直接报错，不当作无反馈或通过。
反馈沿用下文的受信审阅账号与结构化评论规则；普通讨论仍需作者阅读 PR。

`ready` 是显式写操作，要求本机已验证并提交当前 PR 头、工作树干净、CI 全部符合规则、
没有未解决反馈，然后复用已有入口移除草稿并公布 ready。重复执行仍复核证据，
复用原评论；重新转为草稿或修改标签后，不因旧 ready 标记跳过显式交付。
ready 只代表作者交付，可交给独立审阅者；不表示业务审阅或组合门禁通过。
CI 等待、失败或出现新反馈时刷新仍不执行修复。用户在原会话通知后，
作者修复并再次 submit，在 PR 回应处置及新头。仍有请求修改审阅时由审阅者复核，
不要为了 ready 擅自移除反馈，也不要求作者再修改已经修好的代码。

| 用户安排 | 会话停止位置 |
| --- | --- |
| 开发并提交 PR | 作者交付 ready 或明确阻塞，返回链接和当前头 |
| 独立审阅 PR | 将审阅结果写入 PR；阻塞项带源码与复现 |
| 处理 PR 反馈 | 修复、验证并回应，交回审阅者；不自我签收审阅 |
| 合并指定 PR | 复用队列核对证据、机械合并和组合门禁；合并或明确交接后返回，发布独立 |

确认 PR 已合并或关闭后执行 `finalize --state …`。只更新任务记录，
区分 merged 与 closed_without_merge，保留源码、会话及工作树，不调用客户端归档或调度器。
查询发现终态仍不自动 finalize。已关闭 PR 重新打开时需检查，不沿用完成任务重新开发。
手动模式拒绝 tick、schedule、probe、claim、complete、rebind、pause、resume，
防止误用自动运输入口；不存在需要后台续期的预算。网络或操作失败后可重试同一显式命令。
不要通过编辑 task.json 转换既有自动任务。
合并方的手动恢复及每轮报告见 [PR-QUEUE.md](PR-QUEUE.md)。
用户负责交接和再次调用；合并方不为自己或作者创建定时任务，也不等待作者领取反馈。

## 自动模式及共用交付规则

保留一个干净且版本固定的检查器工作树，队列和作者共用相同检查器版本及运行平台。
作者代码放在另一个独立工作树；所有运行记录放在源码外的私有目录：

```zsh
/absolute/frozen-checker/scripts/pr-task init --mode automatic --state /absolute/private/task-state \
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
本机报告通过且当前头的指定必需检查实际成功后，定时器才移除草稿并标记 ready。
