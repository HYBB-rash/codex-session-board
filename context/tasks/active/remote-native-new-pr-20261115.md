# 远程原生新建 PR

owner: codex
execution_mode: serial_isolated
allowed_write_set: src/, scripts/native_new/, scripts/check-native-new.mjs, scripts/check-host-filter.mjs, scripts/check-transfer-feedback.mjs, scripts/package_migration.py, acceptance/bindings.json, acceptance/test_native_new.py, acceptance/test_gate.py, config.example.json, index.html, Docs/REMOTE-NATIVE-NEW.md, Docs/REMOTE-SESSIONS.md, 本任务文件
base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
worktree_path: /private/tmp/csb-remote-native-new
coordinator: 当前会话
status: awaiting_pr_validation

用户要求提交远程原生新建 PR，并在检查通过后才汇报 PR 完成。
将 2026-11-13 的独立候选移植到当前 origin/main，保留旧分支与交付包。
开发 PR 保持主线版本，不修改版本清单及共享 STATE。
不合并主线，不部署或重载日用客户端，不启动真实模型任务。

正常新建仅导航到 Codex 原生远程项目输入页，不发 thread/start。
旧 requestId 仅核对并恢复原会话，读连接禁止创建及命名。
客户端入口扩展绑定两个原生模块摘要、候选 archive 和明确启动命令。
源码包保留全部入口工具，测试解包后的独立执行。
现有 App::connection 及原生依赖身份校验继续生效。

移植冲突仅涉及新建时的 render 和源码包目录列表，保留主线的等待反馈、
现有交付命令及脚本，同时加入原生导航和其工具。
浏览器回归核对新请求没有旧创建 requestId，旧 pendingDrafts 的原 requestId
则经过真实快照载入并在恢复重试中保持。等待反馈及现有场景数量不减少。
移植后的三项入口及源码包测试、53 项主机筛选和 42 项等待反馈浏览器场景通过。
本地 PR 验证运行 scripts/validate --scope pr --base origin/main。
GitHub 的 Check PR changes 必须针对最终 PR 头成功；失败后修复并重跑，
没有检查或跳过检查不记为通过。具体报告留在源码外本任务交付目录。
原有 31 条完整验收及远程原生页面、首轮来源仍需目标环境证据；
PR 检查通过不代签完整验收，不改写历史会话来源。
