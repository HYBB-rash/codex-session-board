# 部署核验、现场冒烟与发布预检 CLI

owner: codex
execution_mode: serial_write
allowed_write_set: scripts、acceptance测试绑定、CI、Docs及本任务context
base_commit: 762939ab7863350a3e14439c7193e5500876d7f0
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: in_progress

用户授权落地部署生效核验、发布后功能冒烟、发布前快速预检三个CLI，
尽量复用已有实现，并实际运行计时，对比上轮人工操作。
保留当前0.3.75日用插件与配置，工具先对现有不可变安装实测，
不为部署工具本身重新构建或替换业务插件。
本次计时分别记录一次性开发与工具重复执行，不把上轮整个阶段算作可节省时间。
生效检查必须绑定指定客户端进程，不能拿无关守护进程或旧安装回执代签。
现场测试仅写明确指定卡片，使用服务原有版本校验恢复；并发修改时拒绝覆盖。
实际服务页面与原生容器证据分开，保留31条完整验收的外部待验状态。
