# 归列与观察六项测试销账

对照 T0 的 `work/t0-b859880/ledger.json`，六项中一项并入已有目录测试，五项保留完整 ID 并改写断言。生产行为没有改动。首次在含 B1、四列告警和 T0 清单的 `0a47d49` 主线上重基。测试绑定只将原导航 ID 的 `STATE-001` 转给承接者。

## 卡片与目录

| 原完整 ID | 处置、出处和独有断言 |
| --- | --- |
| `lib::codex_session_board::app::four_columns::tests::only_current_remote_rows_offer_navigation` | **并入** `lib::codex_session_board::catalog::view_tests::cached_snapshot_requires_live_reverification_after_restart`。原测试手填 `canOpen` 的输入。承接者从持久缓存重开远程 App，通过根 `/api/sessions` 读同一复合 ID。目录缓存命中仍然不能打开，实时核实后可以，断开后再次禁用。三步均保留未归列卡，并核对目录与只读标志。原 `STATE-001` 和承接者原有绑定均保留。 |
| `lib::codex_session_board::observer::four_column_tests::empty_requires_complete_response_and_an_inactive_verified_backend` | **保留并换入口**。轮次列表缺失、带未完游标、后台活跃但返回空页时，按 ID 读回未归列卡；完整空闲空页才在四列隐藏，普通五列仍显示待判断。原断言读内部 `confirmedEmpty`。B1 归列测试直接提供空证据，本项继续覆盖观察器怎样认定证据。 |

2026-11-18 四列方案会话 `01a1eb27-ddca-7b13-bd84-78c698872d0e` 第 406 条明确写出空会话省略、详情未准备好时进入未归列，以及离线卡禁用打开。第 698 条用户引用整案，第 764 条授权实施。T3 的十次告警快照检查已有人工列和备注，无法替代这两条来源及目录用例。见[四列迁移](../../Docs/DEPLOYMENT.md)和[告警对照](t3-four-column-warnings-20261121.md)。

## 轮询与保存

| 原完整 ID | 处置、出处和独有断言 |
| --- | --- |
| `lib::codex_session_board::observer::tests::idle_rechecks_limit_extra_reads_and_rotate_across_stable_rows` | **保留每轮两卡预算，改查新回复**。三张空闲卡的列表时间均不变，底层回复均已更新。每轮按会话 ID 去重，最多额外核验两张，随后三张卡均能预览各自的新轮次。删除按名字指定读取先后的数组。修复 `0e01ab5` 的作者在会话 `01a1edee-4d2c-7470-87c4-9eb799544adb` 第 872 条交付两卡预算；[预览任务](../tasks/completed/final-reply-preview-20261118.md)允许状态传播延迟。这是作者的性能约定，用户关心的是能看到新回复。 |
| `test::board_contract::store_tests::invalid_state_and_oversized_note_leave_all_tables_unchanged` | **保留边界与拒绝零副作用**。公开 MCP 状态枚举及 `note.maxLength=2000` 可见于[接口模式](../../src/mcp.rs)，迁移基线包的 `mcp_server.py:28` 和 `store.py:266` 已有同一边界。[现场记录](light-ui-implementation-20261115.md)有 2000 字完整读回证据。本项拒绝未知状态和 2001 字输入后比较会话、变更、标题队列和设置四表；接受 2000 个 Unicode 字符后比较中文、表情的完整原文。 |
| `test::observer_contract::runtime_active_threads_are_read_each_poll_while_unchanged_idle_is_cached` | **保留跨轮业务结果**。同一活动卡连续两轮的列表元数据相同，第二轮输入版本仍须递增，实际结束进入待审阅，后续相同空闲观察保持原状态、版本、输入版本和备注。删除对单次 RPC 数量和立即重复零读的固定断言。`AUTO-001`、`AUTO-003` 和 `AUTO-005` 要求读到真实执行并避免重复流转；空闲复核预算由上一条多卡用例负责。 |
| `test::observer_contract::runtime_incomplete_pages_fail_before_discovery_or_any_turn_observation` | **保留分页拒绝和可见的零副作用**。未完页返回分页错误，零次轮次读取，已保存卡保持原状，部分清单中的新卡不入库。同一观察器随后拿到完整页，即使空闲卡的元数据与失败页相同，也必须读到已结束轮次并将卡放入待审阅。这一步承接原来的私有修订表检查。`DISCOVERY-001/002`、`HEALTH-001` 规定缺页与保留旧数据。零落库沿用迁移基线 `runtime.py:117` 的拒绝顺序。游标、RPC 参数及查询作用域继续由 `test::observer_contract::runtime_pagination_uses_full_declared_scope_before_reading_owner_turns` 核对。 |

冻结需求见[验收契约](../../acceptance/requirements.json)，原 Python 读法及公开参数见[迁移基线包](../../Docs/verification-baseline/codex-session-board-0.2.5-acceptance-v1.tar.gz)。本项使用 Linux 临时数据库、RPC fixture、正式快照和当前 Rust 实现；真实宿主验收独立。
