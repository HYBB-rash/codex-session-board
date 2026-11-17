# 修正项目操作的无依据限制

owner: codex
execution_mode: serial_write
allowed_write_set: src/, index.html, scripts/check-native-target*.mjs, scripts/check-remote-catalog.mjs, acceptance/bindings.json, Docs/REMOTE-NATIVE-NEW.md, 本任务记录、对应完成记录与日志
base_commit: f5b3ff5abf126003ba29c055443ee7852f93bc54
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: in_progress

用户要求修复没有需求与技术依据的限制，并检查同类问题，
保留必要的身份校验和数据保护。
本次只修改源码并使用隔离 fixture 验证，不推送、安装、重载或操作日用客户端。
开发提交保持版本与共享 STATE 不变。

## 范围

- 修复含空格产品名的 userAgent 版本解析，版本只承担诊断用途。
- 以实际来源查询证据替代固定版本白名单，未知可见性仍待核实。
- 拆分项目就绪与全目录完整，保留目标来源、账户、项目、目录及删除检查。
- 检查无关旧数据、固定名称和身份字符串造成的跨范围误拦。
- 保留操作身份与同 UUID 重试、过期编辑保护和写后独立读回。

## 验证

执行定向回归与统一组合验证，覆盖允许合法目标操作及拒绝身份、
账号、目录、删除、重复创建和错误读回的反例。
每次提交前运行暂存快检；结束前运行 ./scripts/check。
真实客户端安装、目标环境与完整 31 条验收不由 fixture 代签。
