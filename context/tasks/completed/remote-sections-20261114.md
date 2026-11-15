# 远程会话自动分区与映射恢复

owner: codex
execution_mode: isolated_write
allowed_write_set: src/native_sections*、src/app*、src/remote*、src/store*、src/rpc*、src/config*、src/observer.rs、src/archive.rs、tests、examples、acceptance/bindings.json、Docs、当前任务记录、源码外本任务实验资料
base_commit: 91a4fe847140ad54daa744519b0112e271527ab4
worktree_path: /Users/hongyong/Developer/codex-session-board-remote-sections-20261114
coordinator: codex
status: completed
completed_at: 2026-11-15
delivery_phase: 实现与隔离验证完成，尚未合并、打包或部署

用户授权实现调查报告中的映射校验、缺失校准和写后读回方案，
覆盖已有远程会话、新主机、断线重连与分区重建，并执行相应验证。
不修改 Codex 桌面端，不添加待核对业务分区。
保留人工选择及未知/断连时的原状态，不按同名分区猜测绑定。
业务连接统一通过 App::connection；普通开发不安装或重载日用插件。

已完成：
1. 明确映射、来源和已有会话的接入与恢复规则。
2. 实现可验证映射与受限空会话校准，保证失败清理和重启恢复。
3. 接入远程自动阶段与历史补齐，保护人工选择及外部分区。
4. 验证新主机、断线、丢回执、分区重建和重启。
5. 完成组合自动检查、隔离环境验证与交付记录。

只读分析并行分配给 mapping_architecture 与 history_and_tests；
两者不改文件，所有实现、暂存和提交由本任务完成。
开发提交保持 13 文件、400 行，不修改版本和共享 STATE。

实现检查点：5d5804c、ba7e06c、9be8523、a5d4f30、82f7a6e、afd8672、
89f2c6c、6f6d3c9、334c6f6、7693cca、309e05d。
每次提交前均审阅暂存内容并通过快检和提交钩子，保持提交规模限制。

## 交付行为

远程原生分区已经接入扫描、状态观察和人工修改。写后复用已验证的后台目录，
读取真实位置确认，不等待全局状态文件防抖写回。
来源限定的持久映射通过实时目录校验；缺失映射使用一个有归属记录的原生空会话校准。
校准中断可恢复；移动或归档丢回执时核对实际结果，改名或有输入时保留并停止管理。
创建回执未知时不重复创建，显式恢复要求匹配请求、来源和确认标志。

旧默认区域会话按兼容人工状态及可信观察补齐分区，持久保存进度、输入版本和阶段边界。
保留备注、人工选择及已有原生状态、自定义和置顶位置，不重放旧完成事件。
同一逻辑 UUID 的后台分区重建会重新校准；逻辑 UUID 删除后不得按同名分区猜测绑定。
专用空会话不进入业务发现、观察、标题同步和看板列表。

## 自动验证

独立工作树使用自己的 target，未复用其他工作树的编译产物。
`cargo clippy --offline --locked --all-targets -- -D warnings` 通过。
`TMPDIR=/private/tmp python3 scripts/verify_acceptance.py --scope automated` 退出 0：
709 项检查全部通过，无失败或跳过。包括库测试 256、native_authority 37、
native_direct 9、提交门禁 19 和项目检查 73 项。
31 条需求仍为 17 pass、14 pending，`release_ready=false`；本任务不宣称完整真实环境验收。
需求文件、独立参考模型和锁未修改。

状态机覆盖新主机五项映射、无文件写回、重启复用、创建回执未知、
改名或输入接手、丢回执、跨时刻位置冲突、后台重建及逻辑 UUID 删除。
服务集成覆盖旧人工状态和备注、未确认输入、冷映射、移动和归档丢回执、
断线缓存只读、重连后的新完成事件、阶段基线及空会话隔离。

自动报告：`/private/tmp/csb-remote-final-acceptance.json`。
构建指纹：`0ea859c91381776bfea8bff6666f5214df3884e1decabd7f37e1933ffe0d71be`。
Clippy 日志：`/private/tmp/csb-remote-final-clippy.log`。
上下文检查 `sh scripts/verify_context.sh` 通过；未改共享 STATE 或版本清单。

## 实机验证

Fedora 192.168.6.100 的 Codex 客户端连接 Mac 192.168.6.103 的原生后台。
实验只使用私有配置、数据库及专用原生空会话，未发送消息或运行模型轮次。
私有状态副本去掉目标主机全部磁盘映射，业务会话行数始终为 0。

全新实验目录中，五项映射校准和空会话清理总计 13638.40ms；
关闭连接并重新建立后，复用验证 1089.08ms，没有再次创建空会话。
同一恢复日志的中断续跑另一次通过：首次补齐 13681.00ms，重启复用 1047.85ms。
两次均确认私有状态文件内容未变，证明校准不依赖客户端磁盘写回。
最终检查三个实验目录，五项映射完整且待清理记录均为空。
记录：`/private/tmp/csb-remote-impl-20261114/live-fresh-verified.log`、
`live-final-verified.log` 和 `live-ledgers-final.log`。
本任务独立 SSH 代理已经关闭，未改系统代理或登录配置。

实机发现并修正两项原生协议差异：
分页完整历史尚不支持 `list_turns`，改用摘要历史页；
未 materialized 时读取会合成当前时间，比较元信息时忽略三个合成时钟。
仅对同一 UUID 的指定原生首条消息前错误接受空历史证明，其他错误不回退。

真实验证限于空会话校准、清理和重启复用；业务迁移、后台分区重建和断线故障
通过隔离 fixture 验证，未删除用户真实分区、未批量移动真实业务会话。
完整行为说明见 `Docs/REMOTE-SECTIONS-IMPLEMENTATION-20261115.md`。
