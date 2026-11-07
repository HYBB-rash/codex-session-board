# 新建项目与加入已有目录

owner: codex-project-creation
execution_mode: serial_write_with_readonly_review
allowed_write_set: src/, tests/, index.html, assets/, Docs/PROJECT-CREATION.md, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, config.example.json, context/, outputs/project-creation-20261107/, 日常程序及插件部署副本、授权的专用测试项目
base_commit: 94a674cccaa0672b405adc8a574b778ed9ca47d5
worktree_path: /home/herman/文档/notion-program
coordinator: codex-project-creation

用户授权先验证原生项目登记链路，确认可行后实现新建目录和加入已有目录两个入口，
处理重复、并发、丢回执、路径、嵌套归属和刷新保护，然后直接部署等待用户验收。
主代理串行写入；两个子代理只读审查界面和原生接口，不修改共享工作区。
优先复用原生项目服务，不直接编辑全局登记表，不把 server ID 当作 Desktop ID。
不发送消息、不执行模型、不默认初始化 Git 或创建 worktree。
使用专用空目录验证登记与读回，保留既有会话字段、项目和部署配置。
冻结 31 条契约不变，扩展专项证据与完整验收分别记录。

原生探针已真实登记并独立读回，但现有外部工具受 caller workspace 限制。
当前会话只允许仓库目录，既有部署 caller 无本机根，无法交付通用本机目录入口。
需要用户确认是否扩展桌面客户端；最小接口设计见 Docs/PROJECT-CREATION.md，
后续交接见 context/handoffs/project-creation.md。业务功能及部署尚未开始。
