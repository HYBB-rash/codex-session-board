# 面板错误实例与账号分区读取恢复

owner: codex
execution_mode: sequential
allowed_write_set: src/native_sections.rs, src/native_sections/direct.rs, tests/native_direct.rs, acceptance/bindings.json, Docs/, context/, 四份版本清单, outputs/, 本机看板配置及插件部署
base_commit: cc76132a6fdfeba4d9ba7a01cdfaa8fbd7ec2c9b
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求先查清根因、时间线并出报告，然后直接恢复面板；长期防复发机制留待恢复后讨论。
两名只读子代理分别核查状态链路和安装记录，业务修改由本会话执行。
现场与含 WAL 的标准数据库备份位于 outputs/state-incident-20261112，报告见 REPORT.md。
确认 MCP 默认配置接入旧库，正式库仍保留记录；正式分区读取同时不兼容 electron-owner-atoms 当前账号存储。
只按当前账号读取映射和别名，兼容无账号标识的旧客户端；禁止降级串读其他账号。
修复不合并旧库、不批量修改状态、不重命名、不恢复已归档会话、不消耗真实归档验收额度。
恢复期间暂缓定时归档，保留原设置；部署后核对真实 MCP 后端身份、原生位置、两张归档样例及备注。
修复实现与回归结果见同日 journal；实际部署与恢复结果统一写 outputs/state-incident-20261112/recovery.json。
完整业务验收未完成，不发布 stable。
