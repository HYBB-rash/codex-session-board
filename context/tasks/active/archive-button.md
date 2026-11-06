# 待归档与一键归档

owner: codex
execution_mode: serial_write
allowed_write_set: AGENTS.md, Cargo.toml, Cargo.lock, src/, tests/, index.html, README.md, Docs/, acceptance/bindings.json, scripts/, context/
base_commit: 48ee1e16bae9399cab9a37967f9887a509d2a973
worktree_path: /home/herman/文档/notion-program
coordinator: codex

2026-11-07 用户要求去掉界面中的已完成状态，只保留待归档，并提供直接真实归档的按钮。
保留冻结验收的需求、模型、锁和旧状态存储兼容性；新界面单独验证五状态与显式归档扩展。
旧 completed 记录在界面投影为待归档，不删除备注或历史。
仅修改源码和隔离测试，不归档用户真实会话，不部署。
2026-11-07 用户要求提交本次修改，并将本地提交改为 Agent 自主决定，无须逐次申请授权。

源码实现与自动验证通过，已按提交规模与补丁版本规则分批提交至 0.1.34，目标客户端实测与部署仍待验证。
新增原生最新轮次和用户消息 ID 核对，覆盖观察器尚未落库时的新输入。
265 项自动验收通过，严格 Clippy 和隔离 Chrome 的桌面、窄屏检查通过。
完整验收退出 2，release_ready=false。提交收尾后使用 ./scripts/check 核对干净工作区。
继续工作见 context/handoffs/archive-button.md。

随后按用户要求与 main 的 PWA 部署记录整合至 0.1.35，保留双方历史并快进 main。
新指纹下 265 项自动验收通过，完整验收仍退出 2；归档扩展未部署。

用户随后授权部署 main，现已部署组合版本 0.1.39。
实际 Chrome 看板已显示五列和归档按钮，插件读回 board_archive schema。
120 条既有会话、备注及人工状态保留；未点击归档按钮，真实归档执行仍待验证。
部署证据见 context/tasks/completed/main-live-deployment.md。
