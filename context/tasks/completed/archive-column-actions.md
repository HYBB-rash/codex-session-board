# 待归档列批量归档与精简卡片

owner: codex
execution_mode: serial_write
allowed_write_set: index.html, tests/archive_flow.rs, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, README.md, outputs/
base_commit: f8368ba
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求在归档面板增加一键归档按钮，归档下方全部会话，并删除每张卡片上的调整状态入口。
归档范围为当前筛选下待归档列中的本机会话，复用现有显式单条归档接口与版本、新输入、原生位置校验。
远程归档仍不支持。操作期间冻结界面，逐项报告结果，失败卡片保留。
拖动改变状态，拖入已阻塞时保留原因表单；不删除底层状态与备注接口。
按两个可验证改动分步提交，不部署、不操作真实会话归档。
调研检查现有归档实现，查询 MDN async/await 与按钮禁用文档；现有接口已满足归档校验，无需增加批量后端协议。

移除卡片状态入口已验证并提交为 a893685，版本 0.1.65。
后续补丁 0.1.66 增加列头一键归档，单条与批量使用同一归档流程。
固定点击时当前筛选的本机待归档集合，按顺序调用已有接口，不扩大到隐藏、远程或其他状态会话。
显示进度，成功项移除，失败项保留并显示原因，继续处理后续项；保存、拖动、表单编辑期间拒绝重复操作。
Node VM 隔离测试覆盖范围、远程排除、部分失败继续、版本字段、重复点击、交互锁及单条归档。
隔离 Chrome 验证真实点击、搜索范围和部分失败，审阅桌面及 390px 窄屏截图，无浏览器错误。
证据和可重复运行的脚本位于 outputs/archive-column-actions-20261107/。
完整门禁 300 项自动检查通过，退出 2，release_ready=false，14 项既有外部验收仍缺证。
仅完成源码改动和隔离验证，未部署，未归档真实用户会话。
