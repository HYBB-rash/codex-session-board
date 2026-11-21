# B1 · 卡片归列

- owner: B1 作者
- execution_mode: isolated_write
- coordinator: Herman 的五轮重构主会话
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/fdfd/notion-program`
- allowed_write_set: 本卡、同名结果记录、Rust 快照和归列涉及的存储读取、归列测试与对应验收绑定。

接单已 fetch 并 rebase 到上述 `origin/main`。分支是 `codex/b1-card-placement`。
维护者调整卡片归列时，应能在同一处看到哪些事实决定列，什么情况下保留为未归列，什么情况下隐藏。A 负责保存回执与编辑，B2 负责操作资格。存储原值、归档校验、标题队列和前端渲染仍遵循各自契约。

## 首件

用临时 SQLite 库准备相同的本机和托管远程会话，通过根应用的 `/api/sessions` 读回。冻结模式只在本机启用，远程历史数据通过根库的旧缓存读回。出处见 `acceptance/README.md`、`Docs/NATIVE-STATE.md`、`Docs/ARCHIVING.md` 和 `Docs/DEPLOYMENT.md` 的四列迁移。

| 输入事实 | 四列 | 五列 | 冻结六状态 |
| --- | --- | --- | --- |
| 原生位置为待审阅，同时有人工阻塞与历史建议 | 待审阅 | 待审阅 | 配置禁止原生分区 |
| 原生启用但尚无位置，同时有人工阻塞 | 未归列 | 待判断 | 配置禁止原生分区 |
| 人工阻塞覆盖历史完成建议 | 已阻塞 | 已阻塞 | 已阻塞 |
| 只有历史完成轮次且有最终答复 | 未归列 | 待审阅 | 待审阅 |
| 人工选过待判断且有旧备注 | 未归列，保留备注 | 待判断，保留备注 | 待判断，保留备注 |
| 默认区、新鲜确认空、无备注或工作痕迹 | 隐藏 | 待判断 | 待判断 |
| 远程归档、缓存或置顶，仍有空证据 | 保留为未归列 | 待判断 | 不连接托管主机 |
| 旧人工 completed | 待归档 | 待归档 | 已完成 |

首件核对远程“缓存中”与“当前在线”的区别，也核对原生默认区与置顶、未读到位置的区别。只读回显示结果，检查读取后旧值、备注及版本仍在。Linux 临时库与目录 fixture 可验证正式快照入口，真实宿主另行取证。

## 测试认领

以下新测试专属 B1，绑定 `STATE-001`。原有四列行为、五列归档投影、六状态兼容及远程目录测试作为回归运行。

- `lib::codex_session_board::app::placement_tests::snapshot_places_local_and_remote_cards_from_work_evidence`
- `lib::codex_session_board::app::placement_tests::snapshot_respects_native_positions_and_remote_presence`
- `lib::codex_session_board::app::placement_tests::snapshot_preserves_legacy_remote_cache_in_each_mode`
- `lib::codex_session_board::catalog::view_tests::four_column_board_keeps_a_confirmed_empty_archived_chat_in_the_live_catalog`

已有 `lib::codex_session_board::app::four_columns::tests::only_current_remote_rows_offer_navigation` 随原投影删除而接到当前调用点，保留原有导航断言；导航资格规则由 B2 负责。
