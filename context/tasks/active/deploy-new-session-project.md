# 提交并部署项目继承新建会话

owner: codex
execution_mode: serial_write
allowed_write_set: Cargo.toml, Cargo.lock, context/, bin/, outputs/, 现有看板受管服务与插件部署副本、浏览器看板选择和原生新建页
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
