# T0 待执行发布选择主线冻结提交

owner: T0 发布选择测试作者
execution_mode: isolated_write
allowed_write_set: acceptance/test_main_release.py 的同名用例、本卡
base_commit: b615d4d09851402378aa024c630fe2e371635b25
worktree_path: /home/herman/.codex/worktrees/t0-frozen-main-selection/notion-program
coordinator: 唯一合入队列与 T0 总协调

原 ID 为 `acceptance.test_main_release.MainReleaseTests.test_pending_requests_select_the_latest_frozen_main_commit`，绑定 `GATE-001`。受益人是选择发布源码的维护者。业务出处为 `b615d4d:Docs/INTEGRATION.md:35` 的待执行发布选源规则，以及 `b615d4d:Docs/INTEGRATION.md:45` 对 `verify_source` 主线来源的规定。T0 领取台账 `b859880` 将此 ID 归为保留。本次独审在原 126 项之外发现了对 Git 实参拼法的依赖。

处置为保留场景、删减局部断言。临时上游依次产生两次升版和一次不升版的清单修改。执行者从第一次升版时的克隆读取上游主线，取得第二次冻结的准确 SHA 和标签，并拒绝对象已存在但未合入主线的冻结提交。正式选源实现与验收绑定沿用原版。
