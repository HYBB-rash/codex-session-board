# F1 · 退役侧栏同步诊断

- owner: F 作者
- execution_mode: isolated_write
- coordinator: Herman，五轮重构协调会话
- allowed_write_set: 健康快照、页面连接诊断、开发环境健康探测、直接相关的配置与诊断回归、本卡
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/379c/notion-program`
- branch: `codex/f1-sidebar-diagnostics`

维护者调整原生位置的错误提示时，可以从实际读回的 `nativeError` 追到页面状态分组诊断。
首件用隔离旧配置读取迁移提示，再让现役原生位置读取失败，核对页面摘要和技术诊断。
主要疑点是退役错误是否仍有生产者、健康探测是否读取该字段，以及哪些字段属于保存回执。
旧同步配置校验和现役原生读回各有自己的入口，保存回执归 A 作者。

认领 `HEALTH-001` 的侧栏诊断部分。配置和真实读回分别复用
`lib::codex_session_board::config::tests::native_sections_require_explicit_unique_complete_destinations`、
`test::native_authority::missing_direct_snapshot_entries_never_become_unsectioned`。
页面复用 `test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry`
中独立的 `scripts/check-host-filter.mjs` 诊断场景。
开发探测认领 `acceptance.test_dev_environment.DevEnvironmentTests.test_native_health_cannot_publish_readonly_cards_as_ready`。

最后一个写入侧栏同步错误的同步器在 `450c2a1` 退役。此后本机和远程健康快照、
本机状态保存回执只提供固定的兼容值。网页和开发探测器曾将 `sectionsError` 解释为现役故障。
保留 HTTP/MCP 已发布字段，删除这两处消费。配置文件加载和直接构造 `App` 共用
`Config::validate_modes` 的停用提示。现役读回及自动更新分别维护 `nativeError`、`nativeStageError`。
基线没有匹配退役字段或旧页面文案的源码断言。现役自动更新断言改为读取页面摘要和该主机的技术诊断。

在本工作树运行 `./scripts/dev setup`、`./scripts/dev start` 后，使用独立配置和隔离浏览器做了三次读回。
旧 `sync_sections=true` 配置启动扫描时提示 `sync_sections 已停用，请配置 native_sections`。
带 `completed` 的旧原生配置提示先迁移，`migrate-sections` 新配置只移除该映射，源配置仍可逐字读回。
将本任务 `projects.json` 中待审阅分区的 `hostSectionIds.local` 指向原生目录中不存在的 ID 后，
`/api/sessions` 返回“原生分区已移除：待审阅”，页面显示“状态分组暂不可读，保留上次位置”，
技术诊断给出同一错误。恢复原文件后读回“连接正常”，三个会话在故障前后的状态、分区和备注一致。
