# 定时归档待归档会话

owner: codex-auto-archive
execution_mode: serial_write_with_readonly_review
allowed_write_set: src/, tests/, config.example.json, index.html, Docs/AUTO-ARCHIVING.md, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/auto-archive-20261107/, 日常配置及部署副本
base_commit: 1235e6d
worktree_path: /home/herman/文档/notion-program
coordinator: codex-auto-archive

用户授权设计实现每 30 分钟清空待归档并直接部署，随后由用户验收。
沿用已有研究与归档接口；仅本机待归档参与，保留原生位置、输入和执行状态保护。
采用整点和半点扫描，默认关闭，日常配置显式启用 1800 秒；启动不立即清空。
失败保留记录，逐条继续，下轮重试；原生分区和远程记录保持既有权威边界。
主代理负责所有写入，两名子代理只读审查安全条件和部署路径。
冻结契约、模型及锁不变，扩展专项验证与完整产品验收分别记录。
