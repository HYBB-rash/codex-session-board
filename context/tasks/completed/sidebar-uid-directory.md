# 适配客户端按用户隔离的工具连接目录

owner: codex
execution_mode: serial_write
allowed_write_set: src/rpc/, Docs/NATIVE-STATE.md, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/, 当前看板连接配置与受管服务
base_commit: 69d4297b59574393fb770866d91483e9965b4b36
worktree_path: /home/herman/文档/notion-program
coordinator: codex

2026-11-09 用户授权适配已调查确认的客户端目录变化。
上游从 26.1029.61003 升到 26.1103.60933 后，Linux 工具连接从
codex-app-tools 改为 codex-app-tools-当前 UID，socket UUID 仍每次启动生成。
配置失效时兼容两个标准目录，保留有效首选、自定义目录范围、唯一性、
权限、对端用户、统一截止时间与写入不重放约束。增加隔离回归测试。
当前已验收 0.2.38 服务优先通过仅更新连接地址恢复；源码另行验证并提交，
不把旧版本的完整外部验收证据用于新构建。

源码 v0.2.40 增加 9 项隔离回归；487 项自动检查及严格 Clippy 通过。
完整验收实际退出 2，14 项外部证据待补，release_ready=false。
当前 0.2.38 已通过只读工具调用核对新地址，仅更改 sidebar_socket，
使用受管停止和启动恢复，服务 binary SHA256 与原部署相同。
HTTP 和原生持久分区均确认本会话回到进行中，nativeStageError 清除；
146 条会话备注逐项一致，原有 4 个历史位置缺项仍如实保留。
新源码未部署，不将本次配置恢复冒充新构建完整验收。
证据及私有备份在 outputs/sidebar-uid-directory-20261109/。
