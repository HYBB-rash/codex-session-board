# 提交并部署项目继承新建会话

owner: codex
execution_mode: serial_write
allowed_write_set: Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, bin/, outputs/, 现有看板受管服务与插件部署副本、浏览器看板选择和原生新建页
base_commit: 1978198aa4304e56237ac7b5c7c6c392e095ace0
worktree_path: /home/herman/.codex/worktrees/99a4/notion-program
coordinator: codex

用户明确授权提交现有改动，部署到正在使用的看板，并实际验证选中项目后新建页带入项目。
按补丁升版与每批最多 13 文件、400 行的规则分批提交，保留提交钩子。
先备份现有程序、配置、数据库与插件，再通过身份核验停止、更新并启动日常服务。
保留生产配置、端口及全部已保存状态和备注，同步插件副本。
从实际浏览器选择两个已有项目并点击新建，只验证输入页的项目，不发送消息或执行模型任务。
实测使用 Computer Use 操作真实页面和客户端，保存截图证据。
完整 31 条发布验收仍按现有门槛处理，不用本次功能实测替代其他缺失证据。

调研记录提交 b31a205，功能提交 b443874；提交前检查、Clippy 和测试钩子通过。
功能提交后 ./scripts/check 通过。部署记录补丁准备为 0.1.33，
同步两份源码插件清单，避免打包继续复制旧的 0.1.15 元数据。
备份位于 outputs/new-session-project-live-20261107/，权限 0700，
含原二进制、配置、SQLite 一致性备份、HTTP 快照和两个实际插件目录。
上次 locations.json 的缓存路径为部署前路径；本轮已核实当前为 0.1.30。

已部署 0.1.33，日常程序、marketplace、新安装缓存及运行进程的 SHA256 一致。
生产配置逐字节不变；120 条既有会话全部保留，备注、后续事项及人工状态无变化。
扫描、运行观察和状态分区同步正常；插件重载读回版本 0.1.33，
board_new schema 已包含 projectId。
完整验收报告 outputs/new-session-project-live-20261107/acceptance.json：
257 项自动检查通过，完整验收退出 2，release_ready=false。

真实 Chrome 看板选择“Notion任务看板管理”并点击新建，显示请求已发送。
独立读回客户端 selected-project，项目 ID 为 f5102374-66cd-4cd4-8d0c-4e5e024addb4，
与选择一致，但两个可见原生窗口仍显示全局 MCP 看板，未观察到新建输入页。
原生点击和快捷键均超时；诊断用的客户端深链导航被浏览器协议策略拒绝。
未绕过策略，不将请求成功或持久化项目选择等同于输入页实测通过。
未发送模型消息。第二项目切换及输入页最终项目仍待验证，任务保持活动。

用户随后授权部署 main，组合版本 0.1.39 已部署并重载插件；
新建入口及 projectId schema 均存在。没有重复受限的原生导航验证，
输入页和第二项目切换的待验证结论保持，部署证据见 context/tasks/completed/main-live-deployment.md。
