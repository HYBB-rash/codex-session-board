# E3 十卡持久恢复首件

owner: codex/refactor-e3-ten-card-fixture-20261121
execution_mode: isolated_write
allowed_write_set: tests/native_authority/archive.rs, tests/native_authority/transport.rs, tests/native_authority/remote_archive_batches.rs, tests/native_authority/ten_card_archive.rs, acceptance/bindings.json, context/tasks/active/refactor-e3-archive-actions-20261121.md, context/results/refactor-e3-archive-actions-20261121.md
base_commit: 735b368cb08aa2ce3c3c6db8f6f643316b362f4c
worktree_path: /Users/hongyong/.codex/worktrees/53c2/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
integrator: 01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d

2026-11-21 初读 detached HEAD 为 dce3536，首次获取 origin/main 后建支基线为 e31cbe7。
本首件已重基到上述新主线，原生 fixture 可独立合入。
协调者随后将 E3 Vue 页面和真实点击验收交给 Fedora 独立作者，本树只交十卡回归及绑定。
仅本任务作者写工作树，不改共享 STATE、登记和 journal，不推进 main、推送或部署。

要简化的维护动作：页面作者依据十卡真实读回对照结果展示，无需猜测根库、目标主机库
和原生服务各自已经确认什么。卡片资格继续消费 B2/E2，本首件不复制判断。

首件使用真实 Rust HTTP、根库、目标主机库及原生代理建立十卡批次。
两张已确认、两张成功但回执丢失、两张明确拒绝、一张请求受控暂停、三张未提交。
在暂停点核对批次冻结目标、逐卡状态及原生写入。放行后停止并重开服务，
逐字段比较根库与目标库，重放每卡请求、重复只读核对，再次重开。
每张已经处理的卡最多一次原生归档，拒绝及未提交为零。
同 UUID 的其他主机和本机哨兵保留。

主要疑点：提交中只是当前页面的请求状态，服务重开后不能伪造继续提交。
持久 pending 才是待核对。旧内存浏览器 fixture 无法证明两处存储或原生写入次数。
首件先补逐卡原生写入记录，页面迁移以服务端完整读回为权威。

五类结果及动作：

| 类别 | 来源 | 动作 |
| --- | --- | --- |
| 提交中 | 本页面当前请求 ID | 等待，不再次提交 |
| 已确认 | archived / verified | 查看结果 |
| 待核对 | pending | 只读核对 |
| 明确拒绝 | newInput / rejected | 查看原因，新输入从原卡确认后另发新批 |
| 未提交 | awaiting | 活动批等待首次提交，已结束批从当前卡另发新批 |

认领测试：

- test::native_authority::remote_isolation::archive_batches::ten_cards::ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes
消费 C1 的四项 remote_batch_owner_*，不重复 B2 的真实资格测试。
页面的五类计数、详情、可做动作、实际点击、真实包 resources/read 及双入口证据由
Fedora 页面作者负责。日用原生客户端证据独立，本首件不代签。
每步至多 13 文件、400 行，暂存审阅后运行 ./scripts/check --pre-commit，
使用 agent-git commit。最终准确头运行 ./scripts/check、相关测试与独立审阅。
任务共享登记、归档及 STATE 更新留给协调者。

首件已完成，五项远程归档相关测试通过，30.48 秒，相关 Clippy 通过。
证据为 /private/tmp/e3-ten-card-related.log 与 /private/tmp/e3-ten-card-clippy.log。
暂停时为 1 提交中、2 已确认、2 待核对、2 拒绝、3 未提交。
两次服务重开、两轮十卡重放、重复只读核对后逐卡原生写入为 [1,1,1,1,0,0,1,0,0,0]。
根库包含完整冻结目标、来源、原因和时间，HTTP 逐项读回一致；核对只改变两张 pending。
独立审阅要求补上提交前哨兵基线和重放后完整字段不变，均已实现并复验。
最初编译生命周期错误、沙箱 socket 拒绝、fixture 启动后新增卡未被发现及空 checkedAt
断言失败分别保留于 /private/tmp/e3-ten-card-{first,second,unrestricted,seeded}.log。
页面动作、五类详情和实际打包嵌入资源仍等待 E2，不将首件视为 E3 完成交付。

原十卡检查点为 7e38dc8，重基后为 5ef8fb2。
已写出的 T3 实际点击准备单独保存在 codex/refactor-e3-archive-actions-20261121 的
3d14531，不包含在本交付分支中。Fedora 作者可选择消费，其页面工作由该作者继续。
普通 git 重基误走个人 1Password 签名失败，未写出提交，已撤回并使用 agent-git rebase
重新执行。重基后的提交者为 Agent，没有修改个人签名配置。
最终准确头、检查命令、结果及独立审阅由源码外 /private/tmp/e3-ten-card-final-head.json
记录；不能用早期日志代替重基后最终验证。

Fedora 准确重基头 189757d 发现一次 HTTP 时间 JSON 往返相差一个相邻浮点表示。
Mac 已用同一数值确定性复现，修复只调整 HTTP 时间对照，其他字段及两处数据库比较保持严格。
负对照保留状态、空值、有效时间、两个表示差和一秒偏移拒绝；十卡与相关 Clippy 已通过。
原失败位置与新的独立复审和最终准确头记录见本任务结果。
