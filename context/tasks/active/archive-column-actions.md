# 待归档列批量归档与精简卡片

owner: codex
execution_mode: serial_write
allowed_write_set: index.html, tests/archive_flow.rs, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, README.md
base_commit: f8368ba
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求在归档面板增加一键归档按钮，归档下方全部会话，并删除每张卡片上的调整状态入口。
归档范围为当前筛选下待归档列中的本机会话，复用现有显式单条归档接口与版本、新输入、原生位置校验。
远程归档仍不支持。操作期间冻结界面，逐项报告结果，失败卡片保留。
拖动改变状态，拖入已阻塞时保留原因表单；不删除底层状态与备注接口。
按两个可验证改动分步提交，不部署、不操作真实会话归档。
调研检查现有归档实现，查询 MDN async/await 与按钮禁用文档；现有接口已满足归档校验，无需增加批量后端协议。
