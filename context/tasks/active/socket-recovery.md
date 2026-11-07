# 客户端中断后的 socket 恢复

owner: codex-socket-recovery
execution_mode: isolated_write
allowed_write_set: src/rpc.rs, src/rpc/, src/store/runtime_native.rs, tests/native_authority/, acceptance/bindings.json, Docs/NATIVE-STATE.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 5f95c6b
worktree_path: /home/herman/.codex/worktrees/socket-recovery/notion-program
coordinator: codex-socket-recovery

用户要求根治客户端闪退后旧 socket 残留导致自动移列持续失败的问题。
先修复安全重发现，再修复看板服务重启后首次待处理阶段被历史保护吞掉的问题。
连接层覆盖旧文件残留、地址重建、多存活实例、权限边界、共享超时与禁止写重放。
不发送任务消息、不归档真实会话，不修改日常服务配置与数据。
