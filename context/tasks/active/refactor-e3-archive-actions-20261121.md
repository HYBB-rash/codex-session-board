# E3 批量归档与恢复

owner: codex/refactor-e3-archive-actions-20261121
execution_mode: isolated_write
allowed_write_set: tests/native_authority/archive.rs, tests/native_authority/transport.rs, tests/native_authority/remote_archive_batches.rs, tests/native_authority/ten_card_archive.rs, tests/archive_flow.rs, scripts/check-archive-actions.mjs, scripts/archive-browser-fixture.mjs, index.html, frontend/archive/, acceptance/bindings.json, context/tasks/active/refactor-e3-archive-actions-20261121.md, context/results/refactor-e3-archive-actions-20261121.md
base_commit: dce3536b70459c2962fa225320ecb88a8cde7522
worktree_path: /Users/hongyong/.codex/worktrees/53c2/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
integrator: 01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d

2026-11-21 获取 origin/main，从上述提交建立独立分支。
C1 已在基线中。E2 作者位于 Mac worktrees/64e3，页面迁移等待 E2 真实组件接口。
仅本任务作者写工作树，不改共享 STATE、登记和 journal，不推进 main、推送或部署。

要简化的维护动作：调整批量归档的结果展示时，维护者只需要查看持久批次与当前请求，
无需再推导旧页面对不同回执的终态优先级。卡片资格消费 B2/E2，不复制判断。
页面业务布局使用 Vue 模板，交互组件使用 TSX，同次移除旧批量渲染及监听。

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
- test::archive_flow::archive_mcp_resource_exposes_only_an_explicit_app_action

消费 C1 的四项 remote_batch_owner_*，不重复 B2 的真实资格测试。
T3 保留工具可见性元数据断言，页面接线改为真实鼠标点击当前筛选范围内的一键归档，
严格比较批次卡 ID、version、inputVersion、sectionVersion 和逐项 batchId。
等价绑定改写继续通过，只读卡误入提交时必须失败。

两种入口分别操作。嵌入入口由真实包执行 resources/read，返回 HTML 原样进入浏览器。
隔离 fixture 与真实日用客户端证据分开，后者没有授权也不由本任务代签。
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
