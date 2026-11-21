# E2b 卡片编辑与保存

卡片的编辑表单由 `assets/board-editor.vue` 管理。状态选择和备注输入在 `assets/board-edit-fields.tsx`，编辑器复用卡片已有的状态写入接口、等待反馈和操作资格。旧表单、逐字段赋值以及 `index.html` 内的提交、取消和关闭监听已随接入移除。

同一主机和会话的草稿可以是空字符串。用户输入时保存原文，重载页面后恢复；明确取消普通修改回到打开前的草稿。保存失败后重新核对，用户编辑过的备注保留，只改状态时显示另一窗口最新的备注。确认成功后以服务回执替换卡片，再清除草稿。存储可用时，隔离键由主机和会话共同组成；受限面板拒绝存储时，本页面仍保有草稿并在失败时说明。整页退出后的恢复需要浏览器会话存储可用。

## 需求与测试

| 既有 ID 或验收入口 | 使用场景、出处与处置 |
| --- | --- |
| `test::ui_layout::light_ui_refresh_health_and_catalog_preserve_interaction` | 用户编辑后重载页面、切换主机、回到同一会话，应看到本机自己的草稿和焦点。复用既有 ID，新增真实表单场景，覆盖另外一台主机同线程、空草稿、拒绝存储、两个窗口、过期与错主机回执。`Docs/LIGHT-UI.md:13`、`Docs/NATIVE-STATE.md:72`、`acceptance/requirements.json:86`。 |
| `test::ui_layout::transfer_feedback_waits_for_receipts_and_preserves_interaction` | 远程确认期间重复提交、按 Escape、保存失败重新打开、再次成功后位置和提示均可见。继续使既有等待场景通过，用真实输入、按钮及重开后的备注验收，退出旧函数调用。`Docs/DRAGON-FEEDBACK.md:30`、`:33`、`acceptance/requirements.json:95`。 |
| `scripts/check-blocked-optional.mjs` | 隔离 Rust HTTP fixture 的 11 项编辑动作，包括取消、打开不写、普通拖放省略备注、表单原样提交空白、空草稿恢复。故障后重开还需看到草稿恢复提示，保存后再次重开已无该提示。`Docs/NATIVE-STATE.md:72`。 |
| `test::native_authority::entrypoints::built_binary_distinguishes_omitted_and_empty_notes_over_http_and_mcp`、`test::native_authority::transport::external_move_rejects_an_unobserved_stale_save_and_fresh_save_succeeds`、`test::native_authority::faults::input_arriving_during_a_native_move_cannot_be_confirmed_by_the_old_save` | 回归 A 已有的空备注、新输入和外部位置变化。E2b 消费 A 的写入回执，沿用原 ID 和原作者处置。`Docs/NATIVE-STATE.md:50`、`acceptance/requirements.json:95`、`:104`。 |
| `scripts/check-card-panel.mjs --editor`、`checkPackagedEditor` | 从实际 Linux 包启动进程，用同一浏览器动作在 HTTP、离线 MCP 资源分别提交带空格的备注和显式空串，读取服务最终状态、版本和备注。MCP 宿主只允许脚本，按钮也要校验表单并发出工具调用。 |

## 已复现的边界

- 受限 MCP iframe 中，仅靠脚本 `.click()` 和普通表单提交会分别失去真实焦点或没有发出写入。改用真实指针验证，在按钮激活时执行标准表单校验。
- 用户只改状态时，冲突后沿用打开表单时的旧备注会覆盖另一窗口的新备注。区分真正编辑过的内容后，原草稿和新读回分别得到保留。
- 已确认的写入会递增状态版本。同版本、较低版本、错主机或错会话的成功回执不能结束操作或清掉草稿。慢保存时连续三次 Escape 后仍只发一笔写入。
- 临时包试跑发现两次独立快照的 `seenAt` 会随轮询推进。用源回执和后续读回比较身份、状态、备注、新输入及版本字段。

最终以远端主线 `f06bc4ee75277f007caa88cd61cf7ade63e09a98` 为基线。Linux 隔离证据位于 `/home/herman/Documents/Codex/2026-11-21/wo/work/e2b-card-editor/`。最终交付回执单独关联准确提交、原始日志和独立审阅。部署、macOS 运行以及日用 Codex 宿主不在本机夹具证明范围内。
