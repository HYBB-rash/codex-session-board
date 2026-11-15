# PR 9：归档回执丢失后的清理恢复

owner: codex-pr9-author
execution_mode: isolated_write
allowed_write_set: 原生校准清理、隔离 fixture 和测试、验收绑定、本任务记录
base_commit: fbe2100583a713ccd08b84740f78f7dba43ba158
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author9-20261115/codex-session-board
coordinator: pr-queue-20261115
status: ready_for_review
started_at: 2026-11-15
implementation_completed_at: 2026-11-16

反馈 ec1c3b05-196c-54a8-96a7-ea21c14d840a。仅合入指定前置提交
22b9dc4e45c872d91e921c2f636d256001241ac6，保留原 PR 祖先关系。
不修改共享 STATE、版本、需求、参考模型或锁；不操作日用客户端或其他工作树。

先提交独立回归，不修改生产实现，实际运行并保存失败头、命令和原始日志。
场景为归档已经执行但回执丢失，归档列表暂时不可用，重开存储后会话读取返回缺失，
列表恢复后应独立确认归档，保留五个映射，不重复创建、命名、移动或归档。
列表仍不可用时保留清理记录。

修复后使用固定 ec4e9c6 检查器、固定比较 base，在源码外保存新报告。
干净提交后写 ready.json，由协调者分派独立审阅；不自行签发通过记录。
本任务全部 Cargo、check、validate 设置 CARGO_BUILD_JOBS=2。

补充派单：只读核对 PR11 原始头 0a71260 的 client_atoms 拥有所有权返回值，
不合入 PR11。加入由测试提供同值但拥有所有权 atoms 返回值的编译回归，
核对 seed 在借用和拥有所有权两种返回值下都能运行。
归档故障已在独立测试头 68476602534fd081843f59e597fe20cdcd791920 实际复现：
两项测试均报“专用空会话仍存在，保留清理记录”，退出 101。
首轮命令和日志保留于本作者源码外输出目录的 reproduction-* 文件。

拥有所有权返回值回归在 1a2a1b2f7c053af9ff347689b21063b00abebde6
实际编译失败，错误 E0716 指向 mapping.rs 的临时 atoms 借用。
命令和首次失败保留于 owned-atoms-* 文件；未合入 PR11。

修复统一用于清理和重新校准前的退休确认：活动列表必须完整且没有该 ID；
归档列表找到该 ID 即确认完成，两份列表都没有时仍要求再次独立读取确认缺失。
列表失败或不完整且无法确认时保留记录。已有归档不重复写入。
seed 以局部 atoms 变量保留所有权，兼容现有借用返回值及 PR11 的 Map 返回值。
原生分区相关 20 项测试全部通过，没有失败或忽略，日志为 targeted-fix-r1.log。

源码提交后冻结工作树并写 ready.json，固定检查器的实际结果保存在新验证目录。
独立审阅由协调者分派，最终结果以源码外审阅 JSON、VALIDATION.json 和 result.json
为准；本记录不预签独立审阅、组合检查或真实客户端验收。

首轮固定检查器在 741103a 被 Clippy 的 duplicate_mod 拒绝，原始报告保留。
取消重复加载模块，库测试通过同值适配器编译原 seed 的拥有所有权路径，
集成测试继续编译生产借用路径；不关闭 Clippy 规则，不重复实现 seed。
新源码头另建验证目录、另行审阅，不沿用首轮头的通过记录。
调整后全目标 Clippy 与相同 20 项回归均通过，分别保存在 clippy-fix-r2.log、
targeted-fix-r2.log；最终交付仍以新头固定检查器报告为准。
