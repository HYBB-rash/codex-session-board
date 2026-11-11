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
开始整合时，发布候选f9c68da及记录顶端1ecde5f尚未进入main。

以1ecde5f为第一父链，分两次合入a94a67f与19c713d，保留原提交历史。
每次准备版本、审阅索引并运行提交检查及完整钩子；不改变单次提交规模规则。
保留分区独立模型和测试、原生138040条边、连续路径及双方验收映射。
冻结需求、参考模型和锁不变，不删除断言或跳过失败。
最终检查后只快进main，并向两个协作任务公布准确基点。
本任务不推送或部署；正式0.2.92及完整验收未通过的事实保持不变。

第一步01273ee已合入草稿验证，版本0.2.96，完整提交钩子及工作区检查通过。
第二步合入19c713d的输入历史、pending身份、时间边界和连续路径验证。
验收绑定保留双方逐需求集合的并集，包括普通三方合并不能自动恢复的旧映射。
静态核对确认31条需求、531个唯一测试、1458条需求测试映射。
第二步46d7efc已通过完整提交钩子和工作区检查，版本0.2.97。
两次原形式化提交及发布记录均是整合结果的祖先，没有改写已有历史。
最终0.2.98仅整理本任务与交接记录，main只允许从19c713d快进到通过检查的结果。

完整验收使用最终候选和独立构建目录，原外部证据不代签新指纹。
正式版本仍为0.2.92，外部证据不足时保持release_ready=false。
本机回执统一保存在：
/home/herman/.local/share/codex-session-board/integration/20261111-main/。
该目录包含两轮提交日志、绑定并集核对、最终验收和main读回结果。
后续形式化6b02aa8起的提交及Mac平台改动不在本轮范围内，按清单另行整合。
