# 隔离演练目标

owner: pr_origin_mapping
execution_mode: isolated_write
allowed_write_set: scripts/integrate_candidate.py, scripts/promote_candidate.py, scripts/validate_context.py, .github/workflows/plugin-candidate.yml, acceptance/test_rehearsal_target.py, 本任务卡
base_commit: 224666b6bfe1e0df55f2b5a1007925c22dd2c963
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-rehearsal-target-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

仅显式演练候选允许选择受限演练主线。配置、CLI、固定远端基线及祖先关系一致，
完整双平台验证、不可变包及启动门禁保持。普通主线忽略演练配置。
不推送、不发布、不改共享状态和测试绑定；协调者合并后补充绑定并执行历史四PR演练。

实现由候选分支决定是否读取受检目标配置，演练强制走双平台候选路径。
演练提升校验固定基线祖先，并使用精确基线租约阻止校验后并发推进覆盖。
入口定向回归覆盖普通主线、配置不符、非祖先、远端推进/删除、恢复显式选择和实际本地快进。
最终检查与提交结果随子任务回执交给协调者，测试绑定由协调者登记。
