# Issue 15：目录快照加载与刷新交错

owner: 01a1da86-d345-7f73-aa75-70fa97d46d41
execution_mode: isolated_write
allowed_write_set: scripts/check-remote-catalog.mjs, scripts/browser-fixture.mjs, scripts/check-catalog-load-wait.mjs, tests/ui_layout.rs, acceptance/bindings.json, context/tasks/active/issue15-catalog-wait-20261116.md
base_commit: 0aeb0db7edab43db6f47d0a79be274c0673dbe32
worktree_path: /Users/hongyong/.codex/worktrees/issue15-catalog-wait-20261116/codex-session-board
coordinator: pr-validation-efficiency-20261116
status: ready_for_validation

来源为 GitHub issue 15，失败候选 7ccdc644090ea917b158f9f780c5d54017e38679。
相关业务和测试源码与远端 main 基线相同。原 PR12 修复树完整保留，不安装或重载客户端。
保持原 65 项目录检查、断连禁止目标写入的断言、业务实现、版本及契约不变。
先使用原 await load() 等待语义，在两个受控响应下复现加载被刷新淘汰后提前继续。
随后使 fixture 加载只在对应快照真正应用后完成；回归须进入常规 Rust 入口和验收绑定。
不通过固定延迟、重复运行至绿或放宽写入数量消除失败。

证据目录：
/Users/hongyong/.codex/visualizations/2026/11/14/01a1da86-d345-7f73-aa75-70fa97d46d41/issue15-catalog-wait-20261116

本任务不启动 pr-task 定时器，不登记自动归档，不修改共享 STATE 或其他工作树。

baseline-frozen-fixture.json 记录最终同一夹具在原等待语义上的确定性失败：
退出 1，21.108 秒，原目录脚本 SHA256 与基线完全相同。
响应 1 的 serial=3 被轮询 serial=4 淘汰；页面 revision=0、connected=true，
随后实际产生一个 /api/new。两个响应载荷都已是 revision=1、connected=false。
这是独立夹具证明的加载机制，CI 原报告未包含逐请求轨迹，不冒称观测了其定时器。
新等待为每次 fixture 更新递增 revision，只在 data 应用该版本后完成。
保留原 65 项检查及写入断言，UI-001 新增绑定常规 Rust 回归入口。
