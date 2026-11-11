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
复审确认既有未读分区验证接受任意错误，来源随后补齐严格拒绝与完整返回类别。
连续编辑/归档复用严格拒绝并保留完整快照；Retry保留真实确认调用，
应产生请求时不能通过提前提交并返回None冒充成功。
草稿保留30条原边，每条非Absent转换后追加同请求Begin，检查已有会话身份。

| 来源提交 | 整合提交 | 整合版本 |
| --- | --- | --- |
| 6b02aa8 | a661b9e | 0.2.99 |
| 7097299 | c0ce265 | 0.3.0 |
| 34c9ad9 | d5225bc | 0.3.1 |
| 8093775 | c600d0e | 0.3.2 |
| 801d1bf | 4fe6f7a | 0.3.3 |
| 4ed8147 | 8271732 | 0.3.4 |
| d39a040 | 本提交 | 0.3.5 |

前六轮各自提交前检查、完整钩子及提交后干净检查通过。
来源最终代码4ed8147和文档d39a040也分别完整通过，停止写入且无未跟踪成果。
两次独立复审确认最终代码、文档没有新增问题；静态复审不代替整合后验收。
来源39项业务和11项设施变异的事实、范围及局限在跟踪文档中保留。
来源曾有一次既有RPC测试偶发失败及一次为补充断言主动中断的钩子；
未修改该RPC测试，后续完整钩子通过，失败原因未确定。

逐需求对固定main 4641088与最终来源取并集，保留544个唯一测试和1473条映射。
持久化检查从TITLE-004改绑TITLE-003是有意修正，没有累加已取消的旧映射。
冻结三文件不变，保留main的build.rs指纹输入和1800秒适配器期限。
最终候选使用独立构建目录执行完整验收；结果与实际指纹保存到本机持久目录。
main只从已核对的干净4641088快进，之后核对HEAD、指纹、检查及bundle。
最后回执和双方基点交接以该目录中的final-main.json为准。
正式服务仍是0.2.92；Mac流程改动由另一个独立任务继续，不混入本轮。
