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
