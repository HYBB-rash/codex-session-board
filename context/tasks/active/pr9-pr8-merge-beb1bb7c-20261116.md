# PR9：固定 PR8、PR10 前置的连接冲突

owner: codex-pr9-author
execution_mode: isolated_write
allowed_write_set: 本 PR 连接冲突、兼容模式连接回归、验收绑定、本任务记录
base_commit: eda3aa0d2788a7b7ae67ae18e7f8ba6cde5543c6
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author9-20261115/codex-session-board
coordinator: pr-queue-20261115
status: ready_for_review

反馈 beb1bb7c-48f2-5b37-bc32-719146797e18。
先在干净的指定原头实际合入 b27e164，复现 app.rs、rpc.rs 两处冲突。
用户随后明确授权固定 PR8 头 8261a07e77db36f5c8c0d5f32a2f61fb96bbe388，
它已包含 b27e164。保留第一次证据，撤销无自写改动的未完成合并，改以该固定头合并；
同样实际复现两处冲突。两轮命令、原头、返回码与日志均保存于本作者源码外目录。

保留 PR8 的私有 ConnectionRequest 构造边界和唯一 App::connection 入口，
保留 PR10 DraftReader 只读权限，另保留 PR9 CalibrationWriter 的空探针权限。
将兼容模式对校准连接的拒绝迁入专用连接模块，增加隔离回归核对三种角色。
不合入真实 main 或未接纳 PR11，不修改版本、共享 STATE、需求、参考模型或锁。

原归档故障的本轮修复闭环已在祖先 68476602534fd081843f59e597fe20cdcd791920
新增独立回归并实际失败，生产实现当时未改；reproduction-* 保存命令和首次失败。
拥有所有权 atoms 回归已在祖先 1a2a1b2f7c053af9ff347689b21063b00abebde6
实际编译失败，owned-atoms-* 保存证据。两项修复和回归继续保留并在合并头重验。

提交后先更新 ready.json，再冻结源码并使用固定 ec4e9c6 检查器，
比较 base 仍为 22b9dc4。独立审阅由协调者分派；PR8 的完整验证通过前不放行。
所有 Cargo、check、validate 设置 CARGO_BUILD_JOBS=2，全部日志和回执写到源码外。

新增兼容模式回归先保留 PR8 原连接模块的判断，在未修复合并快照上实际运行失败：
校准角色错误进入传输层，返回连接不可用，而不是兼容模式拒绝。隔离 socket 不存在，
未连接日用客户端。legacy-merge-before-fix.patch、legacy-merge-failure-r1.json 和日志
保留失败头、合入头、准确补丁、命令与源码哈希。随后补回 CalibrationWriter 限制。
最终审阅和验证结论以冻结提交对应的源码外报告为准，本记录不预签通过。
