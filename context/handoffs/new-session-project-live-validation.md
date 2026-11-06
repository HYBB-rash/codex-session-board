# 新建项目带入的真实页面验证

日期：2026-11-07。活动任务：context/tasks/active/deploy-new-session-project.md。

用户授权提交、部署和真实验证。调研 b31a205、功能 b443874 已提交，
日常服务和插件已升级至 0.1.33。生产配置与原来的 120 条会话、备注、人工状态不变。
证据和回退备份在 outputs/new-session-project-live-20261107/，目录权限 0700。
deployment-verification.json 记录程序一致性、配置和数据保留；acceptance.json
记录 257 项自动检查通过，完整验收退出 2，release_ready=false。

真实 Chrome 看板 http://127.0.0.1:18767/ 选择“Notion任务看板管理”并点击新建，
页面反馈“已发送新建会话页面打开请求”。
客户端 /home/herman/.codex/.codex-global-state.json 的 selected-project
读回为 local / f5102374-66cd-4cd4-8d0c-4e5e024addb4，与选择一致。
board-project-a.png 是浏览器证据，native-before.png 显示原生仍为 MCP 看板。
没有观察到原生新建输入页，因此不能宣称项目带入已完整实测通过。
没有发送模型消息或创建有内容的测试会话。

Computer Use 原生点击与快捷键超时；浏览器禁止直接导航 codex 协议诊断链接，
未通过其他技术或间接路径绕过该限制。只读检查客户端导航逻辑尚在进行。
下一步需要可用的原生 UI 控制或用户实际操作来观察新建输入页，
再验证项目 A、项目 B 的切换和最终显示；不能用 HTTP 成功替代该证据。
未明确选择具体项目时仍使用原来的 codex://threads/new，已由自动测试覆盖。
