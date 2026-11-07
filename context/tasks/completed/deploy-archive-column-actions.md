# 部署待归档列一键归档

owner: codex
execution_mode: serial_write
allowed_write_set: Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/, bin/, 当前看板受管服务与已登记插件目录和缓存
base_commit: af96d47
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户明确要求部署到正在使用的看板。核实当前为 0.1.64，
使用现有构建、受管停止启动及客户端插件重载流程。
部署记录按仓库补丁规则准备 0.1.67，业务改动沿用 0.1.66。
备份原二进制、插件、配置和数据库，保留端口、数据目录与 MCP 环境注入。
读取真实服务与插件版本、构建和界面，检查原有备注与人工字段。
不点击真实归档按钮，不为验证修改工作会话，不推送。

已部署 0.1.67，日常程序、marketplace、缓存与运行进程 SHA256 一致。
客户端插件 installed/enabled，localVersion 与 MCP serverInfo.version 均读回 0.1.67。
原配置字节、端口、数据目录以及两份 MCP 环境注入清单保持不变。
停止期间备份的 126 条本地记录全部保留，note、selected_state、selection_turn、
selection_users、selected_at 无变化。服务显示 101 条本地及 41 条远程会话，
本机运行观察、原生位置读取和远程扫描健康；既有单条历史位置缓存提示保留。
真实 HTTP 页面在隔离 Chrome 桌面及 390px 窄屏验证并审阅截图，
待归档列头存在一个一键归档按钮，卡片调整状态按钮为零，无浏览器错误。
验证期间阻止浏览器发起任何写请求，未点击真实归档按钮。
300 项自动验收通过；完整门禁退出 2，14 项外部证据仍待补，release_ready=false。
备份、版本、构建、数据比较、真实页面截图及验收报告位于
outputs/archive-column-deployment-20261107/，目录权限 0700。
