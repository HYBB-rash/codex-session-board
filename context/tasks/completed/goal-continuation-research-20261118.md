# Codex 原会话自动推进调查

owner: codex
execution_mode: serial_write
allowed_write_set: 本次任务卡、项目上下文收尾、源码外隔离环境及测试证据
base_commit: 1116fe1f59f42a573c75a9b3adc5bf2441363f93
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

用分阶段标记任务检验 Composer Predictions 和原生 `/goal`。
分别取证目标读回、空闲同 UUID 新轮、任务终态和交互停止条件。
项目业务代码和正式环境不在开发或部署范围内。

2026 年 11 月 18 日在 Fedora 44 使用原装桌面 `26.1111.71025` 和随包后台
`0.169.0-alpha.11.1` 实测。源码外证据位于
`/home/herman/.local/share/codex-goal-continuation-20261118/`。

- 已加载空闲原会话目标可保存读回，并在同 UUID 启动新轮、跨轮和完成。
  桌面识别 `/goal`，限制预算的暂停目标经原版恢复按钮复验完成。
- `notLoaded` 时只保存目标，本次 26 秒内未起轮。显式 `thread/resume` 后观察到同 UUID 新轮。
- 审批等待保留在当前轮。普通追问又运行两轮才阻塞。100 tokens 试验实际用 1,428 后停止后续轮次。
- 现有看板角色在发送前拒绝三个 RPC，独立一次性探针经 `App::connection`
  通过目标设置、读回和恢复验证，后台预测仍返回 `-32601`。
  测试账号的桌面未显示预测，缺少图形建议与接口的配对证据。

建议首版只考虑目标状态与原会话入口。结果丢失先读回目标与轮次。
报告记录了两种断线结果、完成后被桌面清理的情况及正式实现的边界。
