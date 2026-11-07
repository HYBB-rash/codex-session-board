# 客户端中断后的 socket 恢复

owner: codex-socket-recovery
execution_mode: isolated_write
allowed_write_set: src/rpc.rs, src/rpc/, src/app/native.rs, src/store/native.rs, src/store/runtime_native.rs, tests/native_authority/, acceptance/bindings.json, Docs/NATIVE-STATE.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/socket-recovery-deployment-20261107/, 日常 bin 与已安装插件副本
base_commit: 5f95c6b
worktree_path: /home/herman/.codex/worktrees/socket-recovery/notion-program
coordinator: codex-socket-recovery

用户要求根治客户端闪退后旧 socket 残留导致自动移列持续失败的问题。
先修复安全重发现，再修复看板服务重启后首次待处理阶段被历史保护吞掉的问题。
连接层覆盖旧文件残留、地址重建、多存活实例、权限边界、共享超时与禁止写重放。
不发送任务消息、不归档真实会话。用户后续明确授权部署精确 0.1.76 到日常看板，
保留配置与数据，并验证闪退重启后自动恢复连接和状态更新。先一致性备份，再部署，
只做此授权范围内的运行验证，记录真实进程重启与故障注入证据的区别。

7b78f40（0.1.73）已修复连接重发现，完整提交钩子与提交后检查通过。
服务重启丢阶段的新回归先复现失败，修复后运行、完成、失败及中断四种阶段
跨数据库重开均能继续；外部后续移列均保留。继续核对丢回执后的延迟读回。

2c3ec67（0.1.74）已修复重启丢阶段，完整提交钩子与提交后检查通过。
延迟确认先复现卡在进行中，再补齐独立读回、后台刷新顺序、服务重开和外部移列
八种组合，并覆盖首次运行未落地、同轮已失败、重启后处理新失败的路径。
26 项原生权威测试与严格 Clippy 通过，准备本步提交。

8054156（0.1.75）已提交延迟确认与同轮新阶段接替，完整钩子及提交后检查通过。
当前准备 0.1.76 验证记录，执行冻结完整门禁后将结果写入本卡片与 handoff。
日常服务保持 0.1.72；本任务未获明确部署指令，不修改 live。
恢复方案及无法区分同目标外部操作的协议边界见 Docs/NATIVE-STATE.md。

0.1.76 完整门禁已执行，316 项自动检查通过，31 条需求中 17 条通过、14 条待外部
证据，退出 2，release_ready=false。报告与真实旧 socket 只读恢复证据位于
outputs/socket-recovery-20261107/。任务保持待部署及外部验收，交接见
context/handoffs/socket-recovery.md，不宣称完整验收通过。

部署阶段已开始：日常服务核对仍为 0.1.72，配置中首选 socket 为当前桌面实例地址；
保持配置字节不变。使用 a44bf20 对应的精确 0.1.76 构建，不将后续记录版本部署。

部署已执行，精确 0.1.76 的程序、HTTP、插件与 MCP 核验一致。
SQLite 一致性备份后停止旧服务，替换程序，保留原配置、数据目录和 MCP 环境。
130 条本地与 41 条远程记录及设置均无人工字段变化。真实看板 SIGKILL 后使用
受管 launch 重启，健康恢复；此前无 pending，不能据此证明 live pending 重放。
原桌面 socket 监听端点改名，原址留拒绝连接残留，配置和看板 PID 均不变，
约 5.54 秒恢复读取，随后恢复原址。release 模式 26 项测试与浏览器刷新通过。
证据位于 outputs/socket-recovery-deployment-20261107/。

任务保持 active：尚未强退桌面客户端，也未观察到这次故障注入期间真实新阶段
自动移列。地址换代测试不是完整桌面闪退验收。桌面退出会终止当前 app-server
和验收轮次，需在独立编排进程中执行并观察。0.1.77 只记录部署，不部署该记录版。
独立脚本 desktop_crash.py 已通过无副作用预检，当前无其他活跃本地用户聊天；
提交和检查完成后将交给 systemd 用户临时服务执行真实桌面强退与重启。
本轮预计由测试中断，恢复后先读 desktop-crash-recovery.json 再更新完成状态。

最终完成：独立服务已于 2026-11-07 18:22:22 强退真实桌面，18:22:28 请求重开，
18:22:44 报告通过。旧地址 ECONNREFUSED，新桌面 PID 与 UUID 地址换代；
看板 PID、构建和配置完全保持，约 15.40 秒恢复健康。同一真实轮次被标记中断，
看板自动移入已阻塞，原生独立读回和自动变更日志 607 一致。
验收结束时 130 本地与 41 远程人工记录保留，输入版本及既有复核标志保护通过。
已读实际报告并归档本任务；14 条既有外部验收待证据仍保留，不宣称全产品验收通过。
日常继续运行精确 0.1.76；0.1.78 仅提交本次真实恢复记录。
后续真实新轮次又自动进入进行中，原生独立读回与日志 609 一致，
恢复后的浏览器卡片及周期刷新通过。独立只读审阅确认专项通过，边界见 handoff。
