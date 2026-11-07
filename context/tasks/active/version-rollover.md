# 自动进位版本规则

owner: codex
execution_mode: serial_write
allowed_write_set: scripts/, Docs/CHECKS.md, Docs/MIGRATION.md, project-rules.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, acceptance/bindings.json, context/, outputs/, bin/, 日常看板受管服务及已登记插件副本
base_commit: b7acaa72fb62e2ffaf0d44292956a31d34bee036
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户授权保留每次提交加一次，第三段满 100、第二段满 10 自动进位，
兼容旧历史，同步修改文档、检查脚本和测试，验证后提交并部署。
旧提交按当时规则验证，切换点固定为直接父提交，后续不可撤销或移动。
重复准备不重复加号，新规则拒绝人工阶段升版、跳号、回退和越界。
先修改并验证检查器，再补充边界回归与文档，按提交规模规则分步提交。
部署前备份程序、配置和数据；保留端口、MCP 环境及人工字段。
不发送消息或归档真实会话，不推送，不降低冻结验收。
