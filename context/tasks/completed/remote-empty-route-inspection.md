# 原生远程空会话入口核对

owner: Lorentz
execution_mode: isolated_readonly
allowed_write_set: outputs/native-new-inspection/, context/handoffs/remote-empty-route-inspection.md
base_commit: b99199b863b92e9688814ca9f7e92f7d9a4ad7cd
worktree_path: /home/herman/.codex/worktrees/remote-data-migration/notion-program
coordinator: codex

迁移交付后顺序只读核对已安装客户端与同版本 CLI 的空会话路径。
确认 codex://new 的项目参数是否原生支持远程项目，及 Raw thread/start
无输入返回 ID 的加载、续写和落盘边界；给出源码与隔离试验证据。
允许独有 CODEX_HOME 中的实际 CLI 空会话试验及清理，不启动模型。
追加授权仅对隔离 home 内自行创建的空 UUID 调用 thread/name/set，
对照命名后的实际落盘、独立恢复和自建 daemon 重启；不触碰真实会话。
不操作真实 SSH、默认 daemon、用户会话、UI、凭据或主协调树源码。
不修改版本、绑定或共享上下文，只交付独有结果和指定 handoff。

2026-11-08 完成并由协调者复核。两实际 CLI 的命名冷恢复及原生入口限制
见 context/handoffs/remote-empty-route-inspection.md；完整原始交付已打包保留。
正式接入任务继续等待真实 GUI、首轮输入与完整发布验收。
