# 测试并行改动合入主线

owner: side-conversation-01a1e3fb
execution_mode: isolated_write
allowed_write_set: f2eb297 的七个文件、候选版本清单及本任务上下文
base_commit: 0aeb0db7edab43db6f47d0a79be274c0673dbe32
worktree_path: /Users/hongyong/.codex/worktrees/test-parallelism-main-20261116
coordinator: side-conversation-01a1e3fb
status: active

用户授权将已实测的两线程改动合入远端主线。
仅机械移植 f2eb297，不引入主会话尚未通过的验证工具候选。
本地 main 工作树有独有提交，不重置、不移动该工作树。
远端 v0.3.96 草稿已属于 7ccdc64，保留其产物和原始故障记录。
通过已有升版入口逐次记录 0.3.96 和 0.3.97；前者不上传或发布，最终冻结后者。
两平台完整候选及不可变包隔离启动通过后，使用现有推广入口更新远端 main。
不安装、重载日用插件，不把定向实测当作完整候选验证。
