# 栏目排序紧凑菜单

owner: codex-compact-sort
execution_mode: serial_write
allowed_write_set: index.html, Docs/COLUMN-SORTING.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/compact-sort-20261107/, 日常二进制与既有插件部署副本
base_commit: ac20517b1427daa8c732f6add15c6923ca3aecef
worktree_path: /home/herman/文档/notion-program
coordinator: codex-compact-sort

用户选定图片草图 A，授权实现并直接部署，之后自行验收。
栏目标题同排显示“更新 ↓”或“更新 ↑”，点击展开排序菜单，选完关闭。
沿用后端栏目偏好、版本冲突保护及独立保存接口。
验证键盘、点外关闭、轮询保持菜单、失败焦点、窄屏及配置数据保留。
主代理唯一写入，两个只读子代理分别审查交互与部署复用。
完整冻结验收仍需如实保留缺证项目，不代替用户验收。

已部署 0.1.87，等待用户验收。五栏标题和排序标签同排，菜单浮于卡片上方。
选择后收起，点击当前规则只关闭而不保存或重置滚动。
键盘方向键、Home/End、Escape、Space、Tab/Shift+Tab 和点外关闭实测通过。
轮询跨 44 秒保留菜单及选项焦点，另一页面改序后旧菜单提交被拒绝。
修复保存后的延迟刷新清除重新打开菜单版本快照的问题，18 项焦点及竞态检查通过。
真实浏览器验证刷新保留、空列保存、断网回退、失效令牌拒绝及 390px 窄屏。
最终完整冻结门禁执行 326 项自动检查全部通过，仍 17 条通过、14 条缺外部证据，
退出 2、release_ready=false。301 项既有比较及滚动检查通过。
HTTP、插件缓存和客户端 MCP 均为 0.1.87，四处二进制哈希一致。
配置与 MCP 环境逐字节保留，135 本地、42 远程人工字段及 3 条设置保留。
各栏目偏好的字段、方向、版本部署前后完全相同；正式页面未改动排序设置。
截图、备份、验证与回滚脚本位于 outputs/compact-sort-20261107/。
