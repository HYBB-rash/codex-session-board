# 五状态与原生分区一致性可行性调查

- owner: Codex
- execution_mode: coordinated_read_only
- allowed_write_set: Docs/STATE-SECTION-CONSISTENCY-INVESTIGATION-20261111.md；本任务独立记录；context/STATE.yaml；本任务 journal
- base_commit: 1a86c73988898b241977e87cfa4a155cbc0d4158
- worktree_path: /home/herman/文档/notion-program
- coordinator: 当前会话主代理
- status: investigation_complete

## 用户需求和边界

调查当前架构能否让正式看板按五状态工作，并让 Agent 修改状态时，页面、原生分区映射及部署配置保持一致。回答是否需要改变架构骨架、最小可行方案、技术债、实现限制及边界情况。

用户已删除原生“已完成”分区。此前只读检查显示正式 v0.2.73 的六项配置仅匹配五项，唯一账号缺少 completed，完整匹配数为零。

本任务只做调查与项目内记录，不修改业务实现、生产配置、原生分区或会话，不部署。两名子代理分别只读分析代码架构和发布验收，主代理负责现场核对、资料核实、报告与上下文收尾。

## 成功标准

- 区分当前源码、正式运行版本、配置及客户端现状。
- 给出可追溯证据、最小方案和明确的实施前提。
- 列出无法保证的行为和待实测项目，不把调查结论当作功能验收。

## 调查结果

正式六映射导致零匹配；真实 NativeReader 连接在内存五映射下通过 Catalog::load。无需改变架构骨架，推荐统一有效绑定、显式退役配置迁移、分项错误和必经发布预检。历史五状态验收仍保留 completed，没有覆盖删除后的目录。

完整报告：Docs/STATE-SECTION-CONSISTENCY-INVESTIGATION-20261111.md。
边界包括账号/主机、重名重建、旧数据与标题、旧 MCP、运行竞争、客户端格式变化及恢复后定时归档。
本机定时归档仍开启，生产配置与功能没有修改或部署。

## 验证和限制

- 隔离八场景探针与真实只读六项/五项目录探针完成，输出位于 /tmp/state-section-consistency-20261111/。
- cargo test --offline --locked --test native_direct --test legacy_mode --test native_authority --test board_contract：99/99 通过。
- sh scripts/verify_context.sh：任务归档后通过；git diff --check 通过。
- 本次调查不构成候选新实现的完整发布验收，没有新的 release_ready=true。
- ./scripts/check 退出 1：已有及本次未提交文件；HEAD 1a86c73 的版本历史规则失败。另一个任务的准备版本及报告已在暂存区，未混入提交或改写其历史。
