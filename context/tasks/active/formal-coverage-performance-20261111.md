# 补齐形式化业务覆盖并优化重放

owner: codex
execution_mode: isolated_write
allowed_write_set: src/formal/, tests/board_contract.rs, examples/remote_parity_lab/model_replay_tests.rs, acceptance/bindings.json, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 19c713ded861beb3546aa61d83f09698527f76ed
worktree_path: /home/herman/.codex/worktrees/formal-coverage-performance/notion-program
coordinator: codex

用户授权先补齐业务验证，再优化数据库重放与有等价依据的剪枝。
主代理串行写入；两个只读代理分别审查状态抽象和连续版本/重启检查。
不修改冻结需求、参考模型与锁，不部署、不推送、不改生产数据。

## 验证目标

- 输入先于分区读回、同轮 pending 重试、真实持久化重启均有连续路径检查。
- 状态及输入版本冲突检查返回值、增量与拒绝后的数据保留。
- 草稿检查首次/重复创建返回值、请求身份与明确拒绝。
- 保留完整抽象边的参考重放，验证优化前后结果和业务断言一致。
- 比较相同构建条件的运行时间；运行完整验收，缺少外部证据如实记录。

## 基线

当前 1,077 状态、37 事件、39,849 条原生转换及 30 条草稿转换。
三项 formal 测试两次耗时 7.94 秒和 7.89 秒，不含编译。
只读审查已复现输入历史和 pending 同轮关系丢失导致模型与 Store 分歧。

## 进度

草稿 30 条边已检查首次/重复创建标志、返回记录、请求/项目/会话身份、
待核对索引及明确拒绝后的记录保留；定向测试通过。
用户补充正确性优先，可以接受无法提速。只采用不损失业务覆盖的优化。
原生模型增加输入历史、pending与已确认轮次关系、同轮重试及两个独立时间边界；
当前 2,465 状态、56 事件、138,040 条边，第一轮完整重放通过。
移除把 Restart 映射到 Same 的假重启事件，后续单独检查真实磁盘重启。
逐边比较状态/输入版本增量，人工保存和归档必须返回明确成功或拒绝；
拒绝后检查原有会话字段保留。连续输入、重新确认、同轮pending与新轮次路径已通过。
完整数据库副作用检查和真实磁盘重启仍在实施。
四项临时生产代码变异均被测试捕获：去掉输入读回保护、pending同轮识别、
已确认阶段去重、重复草稿误报首次创建。变异未写入工作区。
只读复审要求补充 pending 早于人工确认的豁免、完成时间跨人工确认及相等边界，
将在独立连续测试中核对。
模型修正已提交并交接干净 main；后续在独立工作区开发，
最终接入统一 main 时按合并父节点准备版本，不预占发布号。
已完成交接，分支 codex/formal-coverage-performance；main 的 19c713d 提交钩子、
全套测试及 ./scripts/check 通过，交接时工作区干净。后续不写 main。
六项独立版本/归档连续测试及 Clippy 通过，拒绝操作比较六张业务表全字段。
另有十个版本/归档生产行为变异被捕获，累计十四项；
见 context/results/formal-mutations-20261111.md。
整合任务需要两轮长检查，后续先在独立分支按本分支父节点进位并提交；
依 Docs/CHECKS.md 的并行分支规则，最终整合时按合并父节点重新准备版本。
这些版本仅用于本地验证，不是发布号，不部署或推送，已同步给整合任务。
