# T3 远程会话测试

- owner: T3 远程会话作者
- execution_mode: isolated_write
- allowed_write_set: 本任务卡、唯一结果记录、远程实验分页与测试、生产远程扫描及目录连续性测试、相应验收绑定
- base_commit: f15754129ac64a1dd02ec93abb00f9237bf0c9f8
- worktree_path: /home/herman/.codex/worktrees/t3-remote-session-tests/clean-integration-main
- coordinator: 五轮重构协调会话 01a1fc2d-af6f-7d50-9759-33835b51eaeb

领取 T0 登记为「远程会话作者／随业务替换断言」的 18 个测试。以生产扫描
复核多主机隔离、续页、重试和重新打开数据目录后的状态，再移除已被生产回归
覆盖的实验实现。保留只在旧协议、实验夹具或冻结模型才能观察的独有场景。
另核对 T2 旧快照缺账户字段与归档恢复途中断线两项及其承接用例。
完整 ID、各项依据及处置见 `context/results/t3-remote-session-tests-20261121.md`。
