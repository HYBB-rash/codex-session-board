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

首批实现与文档已提交 2e72d1f，版本 0.1.70，提交钩子和提交后检查通过。
补齐合并时所有父提交的切换点继承，提交前验证 MERGE_HEAD 全部可达历史，
增加真实祖先篡改、反向合并、冲突切换点、暂存规则隔离与磁盘幂等性回归。
检查器 67 项测试及严格 Clippy 通过；下一步重新执行完整验收并部署。

合并与暂存回归已提交 db04647，版本 0.1.71，低并发完整提交钩子通过。
提交后发现共享 Cargo 缓存复用旧检查器；清理本项目构建缓存后，
默认检查及 67 项检查器测试重新通过。最终验收使用独立构建缓存。
按部署记录准备 0.1.72，同步两份插件清单，部署前备份已完成。

0.1.72 独立缓存完整验收：311 项自动检查全部通过，完整门禁退出 2，
14 项既有真实环境证据待补，release_ready=false，未降低冻结契约。
已部署至日常看板，程序、marketplace、插件缓存及运行进程哈希一致，
客户端插件与 MCP 均读回 0.1.72，配置端口和数据目录保留。
健康复查发现原 sidebar_socket 已失效，目录内只有一个同用户、0600、
可连接的新接口；通过只读 list_projects 核验后，仅更新这一项外部配置。
其他配置及两份 MCP 环境注入逐字节保留，受管重启后扫描和原生阶段恢复。
130 条既有记录的备注、人工选择、输入确认和归档标记全部保留。
远程扫描完整读回 41 条会话且无错误，既有单条原生位置缓存提示保留。
未发送消息、替用户确认或执行真实归档，未推送。
备份、完整验收、socket 核验及部署读回位于
outputs/version-rollover-deployment-20261107/，目录权限 0700。
