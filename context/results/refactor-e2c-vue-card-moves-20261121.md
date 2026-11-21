# E2c 拖动与跨列保存

基线是 E2b 编辑器 `5bcdba2`。Vue 的卡片响应原生起拖与取消，列决定能否放下，看板接收列间隙的指针坐标。一次拖动只保存一份起始快照。进入阻塞列由 E2b 编辑器接管草稿，普通跨列走 A 的保存回执，范围与数量复用 B1/B2 的判断。旧卡片处理器、列处理器和全局 `document.dragover` 监听已删除。

| 测试 ID | 受益者、触发及结果 | 需求出处 |
| --- | --- | --- |
| `test::ui_layout::transfer_feedback_waits_for_receipts_and_preserves_interaction` | 拖动途中有新输入，发起请求仍带起拖时的版本；保存被拒绝时还原列和计数，已有备注保留。拖到列间隙停留会横向滚动，取消后立即停止。归档结果核对在拖动期间完成时，松手后批量按钮恢复可用。拖动时更换筛选不会改变当前主机。 | `Docs/DRAGON-FEEDBACK.md` 的保存反馈与可继续操作约定，`acceptance/requirements.json` 的 `MANUAL-002` |
| `test::board_contract::store_tests::test_new_input_during_drag_rejects_stale_confirmation` | 即使状态版本相同，新增用户输入也使旧确认失效。拒绝前后的数据库记录完全相同。 | `MANUAL-002` |
| `test::board_contract::runtime_tests::test_new_phase_rejects_a_stale_drag_without_erasing_manual_note` | 执行进入新阶段后，旧拖动提交失败，原先人工备注保留。 | `MANUAL-001`、`MANUAL-002` |
| `test::observer_contract::same_phase_runtime_input_rejects_stale_input_version_without_changing_state_version` | 原执行阶段再来一条输入，旧版本经实际应用入口拒绝。重开数据库仍保留人工状态和备注。 | `MANUAL-002` 及观察者需求 |

Fedora 实际打包的 HTTP 页面和无网络权限的 MCP iframe 分别用 Chromium 原生鼠标起拖。鼠标已经按下时，向临时 fixture 的 `Store::observe` 交付一条用户输入。Rust 快照的 `inputVersion` 增加而 `version` 保持不变。普通跨列和拖入阻塞后保存均收到「会话有新输入，请刷新并核对后重试」。前者保留原列、原备注，后者保留编辑器中的草稿；取消后焦点返回本卡，重开仍能恢复草稿。HTTP 服务和 stdio MCP 均由该隔离 fixture 启动。

初审发现的列间隙坐标与取消时按钮问题分别有红测。修复后，同一浏览器检查读取实际滚动偏移、按钮资格、筛选值和拖动后消失的反馈。新建或保存占用写入口时，检查发送带有效 `DataTransfer` 的事件；闲置时同类输入能真实起拖。E2c 沿用 E2a/E2b 的滚动几何、草稿隔离和焦点恢复测试。
