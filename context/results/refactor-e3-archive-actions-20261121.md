# E3 十卡持久恢复首件

owner: codex/refactor-e3-ten-card-fixture-20261121
execution_mode: isolated_write
base_commit: 735b368cb08aa2ce3c3c6db8f6f643316b362f4c
worktree_path: /Users/hongyong/.codex/worktrees/53c2/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
integrator: 01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d

本分支仅交付十卡 Rust fixture、逐卡原生计数、验收绑定和本任务记录。
协调者已将 Vue 页面与真实点击验收分给 Fedora 独立作者，页面源未进入本分支。
此首件可独立合入，不代表 E3 页面、双入口或完整目标环境验收完成。

## 页面作者可消费的对照

测试完整 ID：
test::native_authority::remote_isolation::archive_batches::ten_cards::ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes。
已绑定到现有验收清单，不改 31 条需求、独立模型或锁。

十卡全部经过真实 HTTP、根库、mac 主机库和受控原生代理。
第七张在 thread/read 断点暂停，根库已 pending，而该卡原生写入为零。
此时按当前请求与持久结果区分为 1 提交中、2 已确认、2 待核对、2 明确拒绝、3 未提交。
提交中是当前页面请求的临时事实，根库仅保存 pending。
放行后服务重开不会恢复一个虚构的提交中状态。

| 卡片位置 | 初始处理结果 | 重开后的动作 | 最终原生写入 |
| --- | --- | --- | --- |
| 0、1 | 收到 archived 回执 | 重放返回原记录 | 各 1 |
| 2、3 | 原生成功但回执丢失，pending | 只读核对成为 verified | 各 1 |
| 4 | newInput，原生拒绝新输入 | 重放和核对保持拒绝 | 0 |
| 5 | rejected，原生会话 active | 重放和核对保持拒绝 | 0 |
| 6 | 受控提交中，放行成为 archived | 重放返回原记录 | 1 |
| 7、8、9 | awaiting | 结束批次后重放仍未提交 | 各 0 |

两次停止并重开真实看板服务，沿用同一原生服务，执行两轮逐卡重放及重复核对。
根库逐字段比较冻结目标、来源、原因、checkedAt、createdAt 和 finishedAt；
HTTP 返回的逐项详情与根库逐字段对照，非时间字段完全相同。
时间字段只容许 JSON 浮点往返带来的一个相邻表示差，且必须同时为空或同时是正数有效时间。
目标库比较每卡归档标记及备注，提交前即保存本机和 other 同 UUID 哨兵基线。
这些旁观库始终不变，mac 仅原目标得到确认。
重复提交不能改写任何批次字段或目标库内容。
最终原生 thread/archive 次数逐卡为 [1,1,1,1,0,0,1,0,0,0]。

## 原始证据与交付边界

首轮五项相关测试通过，30.48 秒；相关 Clippy 和独立只读复审通过。
日志为 /private/tmp/e3-ten-card-related.log、/private/tmp/e3-ten-card-clippy.log。
早期编译、沙箱 socket、启动后新增 fixture 卡片和空 checkedAt 断言的失败分别保留于
/private/tmp/e3-ten-card-{first,second,unrestricted,seeded}.log。
原首件提交 7e38dc8 已重基为 5ef8fb2，普通版本保持 0.4.19。
最终准确头及重基后实跑结果以 /private/tmp/e3-ten-card-final-head.json 为准。

T3 点击准备保留在另一分支 codex/refactor-e3-archive-actions-20261121 的 3d14531。
它保留原测试 ID，使用真实鼠标点击并严格核对当前筛选范围的 ID 和三个版本。
函数覆盖绑定改写通过，只读卡误入和监听积累旧范围都会失败。
该检查点只验证源码 HTTP 与模拟批次响应，不签收真实打包资源或原生写入次数。
页面作者可选择消费，不必复制其中临时 fixture 来替代本首件。

## Fedora 时间读回修复

集成者反馈其准确重基头 189757d 的整组结果为 47 项通过、十卡失败。
失败只在 HTTP checkedAt=1795246528.790944 与数据库 1795246528.7909439 之间。
原失败位于其 logs/e3-ten-189757d50cc88faca6fc97ab4e05cf268e32c016/difference.txt。
本机用同一值序列化再解析，直接复现一个相邻 f64 表示差，约 0.24 微秒。
没有修改生产序列化、依赖、持久格式或状态判断。

修复只用于 HTTP 时间对照，最多接受一个正数有限浮点的相邻表示差。
数据库的重开、重放和逐字段归属比较继续完全严格。
确定性负对照拒绝两个表示差、一秒偏移、零、负时间、空值变化、缺字段和状态变化；
pending/awaiting 仍要求空值，archived/verified/newInput/rejected 仍要求有效时间。
十卡复验通过，11.07 秒，相关 Clippy 通过，日志为
/private/tmp/e3-time-roundtrip-test.log 和 /private/tmp/e3-time-roundtrip-clippy.log。
最终准确头及独立审阅回执仍以本文件前述 JSON 为准。

没有操作日用 Codex，没有修改共享 STATE、任务注册或 journal。
没有推进 main、推送或部署；任务归档及共享登记由协调者处理。
