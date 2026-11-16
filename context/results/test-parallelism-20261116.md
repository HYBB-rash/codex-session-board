# 核对串行测试原因并启用已验证的并行

owner: side-conversation-01a1e3fb
execution_mode: isolated_write
allowed_write_set: scripts/rust_adapter.py、acceptance/test_rust_adapter.py、acceptance/bindings.json、并行说明及本任务记录
base_commit: 7ccdc644090ea917b158f9f780c5d54017e38679
worktree_path: /Users/hongyong/.codex/visualizations/2026/11/16/01a1e3fb-c015-7200-8dc9-1223b83cbb21/parallel-tests/source
coordinator: side-conversation-01a1e3fb
status: completed-local

用户要求追查单线程历史原因，若当前风险允许就开启并行。
主会话候选保持原样；本任务在独立 detached 工作树修改，不更新其共享 STATE 和 journal。
先对同一源码编译出的库与原生测试二进制比较一线程、两线程，再复测两线程。
保留全部测试、断言和超时，失败不自动降级串行或重试至通过。
只调整已实测的目标，不新增调度工具，不声称完成完整候选或目标环境验收。

## 历史原因

`eb2b580385f4afe1ce17bed5ff7bf7f7e5e1de0c` 首次引入 Rust 适配器时已经使用一线程，
提交说明没有解释最初原因，不能把后来的故障当成最初原因。
11 月 13 日的 `4be58e4b496e44db4b178f710583304f8c4236c7` 记录了两次并行原生隔离
超时，保留串行执行；`context/handoffs/blocked-optional-20261113.md` 另有并行库测试
报 Resource temporarily unavailable 的记录，同一故障用例串行复查通过。
11 月 14 日记录还出现过 1000ms 门槛实际 1018.55ms 的失败。
这些记录没有定位所有故障的根因；不能声称今天通过就证明历史问题已修复。

## 本机实测

源码为上述 base_commit，macOS 八逻辑核，独立 target，一次编译后运行同一批二进制。
环境沿用项目的 `/private/tmp`，没有设置全局 RUST_TEST_THREADS。
先分别跑一线程基线，再跑两次两线程；六次完整清单都通过，无跳过或失败后重试。

| 目标 | 用例数 | 一线程 | 两线程首轮 | 两线程复测 |
| --- | ---: | ---: | ---: | ---: |
| lib::codex_session_board | 344 | 119.21s | 87.44s | 87.94s |
| test::native_authority | 39 | 62.94s | 35.09s | 35.40s |

两组合计由 182.15s 降至 122.53–123.33s，省约 59–60s，约 32–33%。
这是两个测试组的执行时间，未包含其他组、CI 排队、编译、产包或整条 PR 流程。
原生性能门槛保持原值。库中形式化模型的单条长测试仍占用较长时间。
本机结果支持把这两组设置为两线程，不代表无限并行或 Linux 已验证。

逐轮 JSON 与原始输出在工作树父目录的 `lib-serial`、`lib-parallel-first`、
`lib-parallel-repeat`、`native-serial`、`native-parallel-first`、`native-parallel-repeat`
对应 `.json` 和 `.log` 文件。`probe.py` 是源码外诊断脚本，不是新增项目 CLI 或门禁证据。

## 改动与验证边界

现有适配器仅给库与 native_authority 增加两线程，业务组之间仍顺序执行。
其余组默认一线程；既有工具自测并行保持原样。没有自动串行回退。
现有适配器四项回归通过，绑定清单与发现的四项测试完全一致。
31 条需求、独立模型、锁和业务测试均未修改。
未推送、未改主会话候选、未部署；完整候选和 Linux 门禁留给正常集成流程。
独立工作树任务结果由主会话后续取用，不修改它正在维护的共享 STATE 和 journal。
