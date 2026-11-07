# 原生分区作为状态唯一来源

owner: codex
execution_mode: serial_write
allowed_write_set: src/, tests/, index.html, config.example.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, acceptance/bindings.json, Docs/, context/, outputs/, bin/, 已授权日常部署程序、配置和插件副本
base_commit: dcfa25f7c83b14eeb15edf025e6056e8e32ba397
worktree_path: /home/herman/.codex/worktrees/pwa-support/notion-program
coordinator: codex

用户授权实现、验证并部署：Codex 原生分区为看板列状态唯一来源，
移除本地状态持续回写分区的后台对账，备注及人工确认数据暂留本地。
保留自动执行阶段变化、过期编辑、新输入和归档保护；
外部原生分区变化必须能反映到看板，不能被旧本地状态纠正回去。
使用公开 app-server 分区接口，不直接编辑客户端数据库。
冻结需求、模型和锁保留；扩展按真实新路径增加自动和部署证据。
只读子代理核对协议与验收边界，主代理唯一写入共享上下文和实现。
按提交规模限制分步验证和提交；备份后更新日常看板。
