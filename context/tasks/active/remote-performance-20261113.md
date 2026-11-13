# 远端连接与状态读取性能

owner: codex
execution_mode: isolated_write
allowed_write_set: src/app.rs, src/rpc*, src/remote/connection_source*, tests/, examples/, scripts/, Docs/, acceptance/bindings.json, context/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json
base_commit: c15946e647b222029ee5938ec2ebdde740833155
worktree_path: /Users/hongyong/Developer/codex-session-board-remote-perf-20261113
coordinator: 当前会话

用户授权实现远端连接复用、状态摘要读取，并通过真实远端操作验证收益。
加入必须通过的性能阈值，防止后续改动撤销收益。保留来源、输入版本和归档保护。
在隔离实例中使用真实主机读取和看板状态操作，不安装或重载正式插件。
先实现并验证连接所有权与恢复，再验证摘要语义，最后记录实测和回归门禁。
完整客户端验收缺证继续如实报告，不用性能结果替代原 31 条验收。

## 实现与实测

主机实例独立持有 SSH master，克隆 Binding 共享连接；每次继续核验当前目标和 peer。
支持截止时间、死连接重建、断开后禁止重开和子进程回收，不修改用户 SSH 配置。
观察连接在远端压缩完整响应，保留全部用户输入 ID、答复 phase 和轮次元数据。
Codex 内置 summary 会丢同轮追加输入，未直接使用；归档写连接仍接收完整结果。

真实 Fedora 192.168.6.100，基线 c15946e/v0.3.55 与最终 v0.3.60 使用同一测量入口。
最终预定两轮各保存二十次并读回：中位数 316.50→97.04/99.70ms，
p95 405.09→225.56/289.72ms。每轮同十二个结束轮次固定三批：
读取 1308.79→538.21/544.36ms，5,891,175→52,008 字节。
两轮同次完整/压缩配对改善 37.7%/49.0%，全部输入和状态证据一致。
基线被门禁拒绝，候选通过。所有写入只发生在临时看板数据库。
认证使用已有 1Password 委托身份及临时 agent，未在远端安装文件或服务。
临时基线工作树和认证进程已清理，候选工作树保留供审阅。

## 回归与边界

八条 Rust 入口覆盖连接性能、恢复、deadline、身份核验、压缩语义和性能、
读写连接分工、断开回收及就绪事件；均绑定原需求，正常测试和提交钩子强制执行。
真实门禁固定状态中位数 <200ms、p95 <350ms，三十六次摘要读取 <750ms、
返回 JSON 数据量 <10%，证据不等或连接失败同样非零退出。
初次单批 293ms 超标回执保留；三批采样不删异常点，单位操作预算未放宽。
完整检查的 1084ms 远端隔离失败与旧 socket 握手失败回执均保留。
隔离测试单项和三十五项整组复验通过；新增连接就绪反例先证明旧实现失败，
再修正为 deadline 内等可写事件后检查错误，未改变原有断言或预算。
退出管道反例及旧轮询三十三次重复读取反例均先失败，修复后通过。
真实退出对照为 36ms；旧 fixture 遗留进程已清理。最终自有连接固定默认流量分类，
两轮真实门禁均使用最终参数，未放宽预算。完整旧快照验收因修复而取消的回执保留。

完整说明见 Docs/REMOTE-PERFORMANCE.md。原始报告、单次失败及 comparison-final.json 位于
/Users/hongyong/.codex/visualizations/2026/11/12/01a1d1b8-c781-7b40-8f16-ceefdc90fa52/remote-performance-20261113/。
未部署到日用插件，未操作真实会话的内容、标题或归档。
完整客户端验收保持独立，缺少新实现指纹的外部证据不宣称 release_ready。
最终完整 release 范围验证退出 2：637 项自动检查全过，31 条需求中 17 条通过、
14 条缺少真实环境证据，automated_passed=true、release_ready=false。
实现指纹为 19393006d7c3983deab37d126860ecf8ad5583c43d4d45c795813bb650c18772，
完整报告保存在上述证据目录的 full-acceptance.json 和 full-acceptance.txt。
字节数统计解析后重新序列化的业务 JSON，不代表抓包测得的 SSH 密文流量。
本任务保留活动登记，待完整验收证据齐全后归档。
