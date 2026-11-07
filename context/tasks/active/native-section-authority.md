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
使用实际桌面侧栏工具接口，不直接编辑客户端数据库。
冻结需求、模型和锁保留；扩展按真实新路径增加自动和部署证据。
只读子代理核对协议与验收边界，主代理唯一写入共享上下文和实现。
按提交规模限制分步验证和提交；备份后更新日常看板。

实测发现 app-server 的 thread/section/move 成功及读回后，桌面工具
list_threads.sections 中当前会话仍在原位置。两套分区命名空间未联动。
测试已恢复原位置及日常服务。最终采用现有桌面工具 list_threads /
move_thread_to_sidebar_section 作为唯一读写来源，保留现有 UI 分区 ID。
读接口虽仍需轮询，但仅采集快照，不推送本地期望状态。
