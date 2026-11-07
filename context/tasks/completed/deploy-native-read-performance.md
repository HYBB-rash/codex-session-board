# 部署本机读取优化与远程会话接入

owner: codex
execution_mode: serial_write
allowed_write_set: Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, Docs/NATIVE-READ-PERFORMANCE.md, context/, bin/, outputs/, 已登记看板插件目录与缓存、受管日常服务
base_commit: f094e8e
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户明确要求部署刚完成的优化。按提交版本规则准备 0.1.64，
业务代码沿用已验证的 0.1.63。先备份程序、配置、插件和 SQLite，
保留端口、数据目录与 MCP 环境注入，通过现有受管启动器停止并替换服务。
核对运行程序及插件构建、数据保留、本机原生读取和远程发现健康状态。
不为测速修改工作会话或执行真实归档，不推送。产品完整验收缺证如实保留。

已部署 0.1.64。日常程序、marketplace、缓存和运行进程 SHA256 一致，
客户端安装与重载成功，MCP serverInfo.version 读回 0.1.64。
生产配置、端口、数据目录及两份 MCP 清单保持原样；124 条本地记录保留，
note、selected_state、selection_turn、selection_users、selected_at 均未变。
真实服务显示 119 条本地及 41 条远程会话，远程发现和本机运行观察正常。
单目标位置只读 p95 10.70 ms，整批读取 96.25 ms，快照约 5 至 6 ms。
原生批量缺项提示来自一条已在原生归档、当前看板不可见的历史缓存；
经原生归档列表只读核对，保持旧缓存，不执行额外归档或恢复。

当前构建 300 项自动检查通过，完整门禁退出 2、release_ready=false，
14 项既有外部验收证据仍待补。远程在线、断线、项目扫描和历史扫描挂起
继续由隔离 HTTP 测试覆盖，不对真实远程连接注入故障。
备份、构建身份、数据比较、测速与门禁报告位于
outputs/native-read-deployment-20261107/。提交前后运行仓库与上下文检查。
