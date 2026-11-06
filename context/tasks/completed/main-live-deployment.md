# 部署 main 最新看板

owner: codex
execution_mode: serial_write
allowed_write_set: Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, bin/, outputs/, 已登记日常程序与插件副本、受管服务和浏览器看板刷新
base_commit: 9a67c5a1544a4942ad72d0d4631eef8c0d5a1b82
worktree_path: /home/herman/.codex/worktrees/pwa-support/notion-program
coordinator: codex

用户明确要求把 main 最新版本部署到正在使用的看板。
main 为 0.1.38，日常服务及缓存为 0.1.33；开始时工作区干净。
按仓库规则为部署记录准备补丁 0.1.39，业务代码沿用 main 的组合实现。
备份日常程序、配置、数据库及实际插件目录，保留生产配置和 MCP 环境注入，
通过受管停止及启动更新服务，并安装和重载插件。
读回版本、构建身份、工具 schema、五状态和数据保留。
本轮不执行真实归档、不发送模型消息，不把部署验证替代尚缺的真实环境验收。

部署完成为 0.1.39，日常程序、marketplace、缓存与运行进程二进制一致，
客户端重载读回版本 0.1.39，projectId 参数和 board_archive 工具均存在。
生产配置及两份 MCP 环境注入逐字节保留；120 条既有会话全部保留，
数据库中的 note、selected_state、selection_turn、selection_users、selected_at 未变。
服务扫描、运行观察与分区同步正常，分区一次 RPC 超时后自动重试清空待同步队列。

刷新实际 Chrome 看板，确认五列、20 个归档按钮和新建入口，截图已保存。
没有点击真实归档按钮或发送模型消息，新建页最终项目仍待原生活动验证。
268 项自动验收通过，完整验收退出 2，release_ready=false。
备份和证据位于 outputs/main-live-20261107/，权限 0700，
包括 deployment-verification.json、acceptance.json、browser-verification.json
和 deployed-board.png。提交保留检查及测试钩子，提交后核对上下文与干净工作区。
