# 将正式发布分支与形式化验证合入 main

owner: codex-release-integration
execution_mode: isolated_write
allowed_write_set: Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, src/formal/, acceptance/bindings.json, context/
base_commit: 1ecde5f4c4c75bae222cae33512cdb1ffdd2d860
worktree_path: /home/herman/.codex/worktrees/release92-main-integration/notion-program
coordinator: codex-release-integration

用户要求与形式化验证和MacBook开发任务协调发布分支合并。
形式化任务交出干净main 19c713d，随后只写formal-coverage-performance工作区。
Mac任务在独立clone基于正式0.2.92开发，不修改共享main。
发布候选f9c68da及记录顶端1ecde5f尚未进入main。

以1ecde5f为第一父链，分两次合入a94a67f与19c713d，保留原提交历史。
每次准备版本、审阅索引并运行提交检查及完整钩子；不改变单次提交规模规则。
保留分区独立模型和测试、原生138040条边、连续路径及双方验收映射。
冻结需求、参考模型和锁不变，不删除断言或跳过失败。
最终检查后只快进main，并向两个协作任务公布准确基点。
本任务不推送或部署；正式0.2.92及完整验收未通过的事实保持不变。

第一步01273ee已合入草稿验证，版本0.2.96，完整提交钩子及工作区检查通过。
第二步合入19c713d的输入历史、pending身份、时间边界和连续路径验证。
验收绑定保留双方逐需求集合的并集，包括普通三方合并不能自动恢复的旧映射。
预期31条需求、531个唯一测试、1458条需求测试映射，待本次完整检查。
