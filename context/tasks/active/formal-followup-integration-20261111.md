# 逐提交整合形式化后续验证

owner: codex-release-integration
execution_mode: isolated_write
allowed_write_set: src/formal/, tests/, examples/remote_parity_lab/, acceptance/bindings.json, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 464108853c17381b9340489e6da826d7a86fe31c
worktree_path: /home/herman/.codex/worktrees/release92-main-integration/notion-program
coordinator: codex-release-integration

沿用用户要求两个任务沟通协作的安排，接入来源任务明确交付的已封板提交。
按6b02aa8、7097299、34c9ad9、8093775顺序合并，四项均已收到明确封板回执。
每个合并保留历史，按第一父提交推进版本，执行提交前检查和完整钩子。
保留main已有分区配置模型、全部业务边、真实重开与逐需求验收映射并集。
来源更名测试按同一语义映射更新，不保留已不存在的旧测试ID，也不减少覆盖。
冻结需求、参考模型及锁不变；不改运行数据，不推送或部署，不混入Mac未交付改动。
最终候选运行统一完整验收，外部缺证继续如实记录，再快进main并同步准确基点。
回执保存到本机integration/20261111-formal-followup目录，避免随工作区清理丢失。
复审确认既有未读分区验证接受任意错误，已反馈来源补明确拒绝原因检查。
该修正及最终文档待来源给出封板提交后按同样检查条件接入。
