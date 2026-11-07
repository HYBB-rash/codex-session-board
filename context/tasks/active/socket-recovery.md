# 客户端中断后的 socket 恢复

owner: codex-socket-recovery
execution_mode: isolated_write
allowed_write_set: src/rpc.rs, src/rpc/, src/app/native.rs, src/store/native.rs, src/store/runtime_native.rs, tests/native_authority/, acceptance/bindings.json, Docs/NATIVE-STATE.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 5f95c6b
worktree_path: /home/herman/.codex/worktrees/socket-recovery/notion-program
coordinator: codex-socket-recovery

用户要求根治客户端闪退后旧 socket 残留导致自动移列持续失败的问题。
先修复安全重发现，再修复看板服务重启后首次待处理阶段被历史保护吞掉的问题。
连接层覆盖旧文件残留、地址重建、多存活实例、权限边界、共享超时与禁止写重放。
不发送任务消息、不归档真实会话，不修改日常服务配置与数据。

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
