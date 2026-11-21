# 候选验证命令无法启动

owner: 五轮重构总协调
execution_mode: isolated_write
allowed_write_set: scripts/dev_environment/processes.py、scripts/validation_process.py、acceptance/test_validation_cli.py、acceptance/bindings.json、本任务卡与结果
base_commit: 0a47d49411e7cbfcdae9b6f84a950122a9388de9
worktree_path: /home/herman/.codex/worktrees/refactor-validation-start-failure/notion-program
coordinator: 五轮重构总协调；由 Fedora 合入会话串行推进 main

发布维护者运行候选验证时，如果选中的命令不能启动，主报告应指明失败步骤、具体命令及原始错误，以退出码 3 结束。进程组清理结果另行保留。出处为 `Docs/VALIDATION.md` 的环境错误和首次失败约定。

旧 ID `acceptance.test_pr_delivery.PRDeliveryTests.test_command_start_failure_preserves_the_failed_step_and_original_error` 随旧 PR 入口退役。新入口由 `acceptance.test_validation_cli.ValidationCliTests.test_candidate_reports_an_unstartable_command_as_the_first_failure` 验证。首件在隔离源码包内将 `scripts/check` 的解释器改为不存在的文件，再从现役 candidate CLI 读取 `VALIDATION.json` 和清理回执。

原实现退出 3，但 `first_failure.message` 仅显示进程组失去归属，`FileNotFoundError` 只出现在子进程日志。新实现把启动异常写入受控子进程的状态，守护进程存活到正常清理后才退出。候选报告给出 `step-1`、`./scripts/check` 和 `FileNotFoundError`；会话没有命令 PID 或退出码，清理回执则可独立核对。开发环境的既有启动、超时、进程中断和进程组回收仍经原有入口。

Linux 上先用同一新回归观察旧实现失败，再运行 `acceptance.test_validation_cli`、`acceptance.test_validation_process` 和 `acceptance.test_dev_environment`，共 20 项通过。实际命令、结果和独立审阅另存于协调工作目录 `work/launch-failure/`。
