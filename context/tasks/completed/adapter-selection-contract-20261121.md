# 适配器选择 Rust 测试组

- owner: T1 原作者，独立认领适配器测试
- execution_mode: isolated_write
- coordinator: 五轮总协调，结果交 T0 与集成者
- base_commit: `0a47d49411e7cbfcdae9b6f84a950122a9388de9`
- worktree_path: `/home/herman/.codex/worktrees/adapter-selection-contract/notion-program`
- allowed_write_set: 本卡、`acceptance/test_rust_adapter.py` 中本项用例
- 分支: `codex/adapter-selection-contract`

**保留 `acceptance.test_rust_adapter.RustAdapterTests.test_selection_does_not_execute_unselected_groups`，改测作者实际调用的 `rust_adapter.run(request, prefixes=...)`。** 作者选择一组已有 Rust 测试时，适配器先清点全部用例，再执行指定组，将其余 ID 记入 `not_run`。错误启动其他测试组会增加合入验证的耗时，也会把无关测试的运行和失败带入本次操作。

| 查找出处 | 查到的行为及证据边界 |
| --- | --- |
| [作者合入步骤](../../../Docs/INTEGRATION.md) | 第 2 步授权作者运行与改动相关的测试并记录命令和结果。如何选择执行器由作者决定；文档没有指定 `prefixes` 或承诺稳定的对外 API。 |
| T2 标题回执集成，`f157541..808a0f5` | 源码外 `refactor-t2-title-old-ack-20261121/integration/verify_title.py` 调用 `run` 并传入 `test::board_contract`、`test::board_io`。`808a0f5…-tests.json` 记录 Linux 全清单 961、实跑 89、未跑 872，`main-integration.log` 确认远端读回同一提交。 |
| T2 看板与归档复验，`0fa71cf` | 源码外 `t2-board-archive/verify.py` 以同一入口选择上述两组及 `test::archive_flow`。该候选报告清点 960、通过 97、未跑 863。它补充作者使用证据；本记录以标题批次的远端回执确认合入实绩。 |
| T1 集成，`b794c8a` | `merge-queue/logs/t1-rust.txt`、`t1-document.txt` 记录 Cargo 运行目标二进制。T1 作者 `2190676` 的回执也按目标和用例过滤。T1 证明相关测试可以用另一入口执行，未据此认定它调用了 `prefixes`。 |
| [历史 PR 脚本](../../../scripts/verify_pr_tests.py) | 提交 `e01700a` 引入了这条隔离断言，历史调用曾按 PR 计划分组。`1f971bf` 删除 `scripts/pr_plan.py`。残留脚本在导入阶段即失败，现役 T2 作者直接导入适配器，无需经过该脚本。 |
| [验收协议](../../../acceptance/README.md)与[现役适配器](../../../scripts/rust_adapter.py) | 发布用标准输入入口按默认参数全选。`run` 明确实现选择、结果集一致性核对及 `not_run` 报告。GATE-001 继续登记该 ID；正式验收成功还需满足原有全量条件。 |

原测试给内部 `run_rust` 两个虚拟组，并替换 `run_target` 看调度次数。现测试从 `run` 传入完整请求和一个组前缀，用可观察的测试进程接口区分 `--list` 和执行用例。普通业务组被执行一次，未选的工具组可以参与收集但没有执行；返回值逐项核对通过和未运行的 ID。用例不限定组内线程数。相邻线程用例由检查器作者维护。
