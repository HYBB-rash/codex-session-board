# E2a 卡片与列展示

Fedora 从 Mac 只读取回 `dca33b4`、`ab04618`。在包含 A、B1、B2、E1 的 `f140da2` 上，Vue 接管网页和离线面板的卡片、列、展开项与按钮展示。分区来自快照，操作资格复用 `cardActions`。旧卡片和列渲染器已删除。

| 领取的测试入口 | 下游维护者要保护的行为 |
| --- | --- |
| `test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies` | 用户正在阅读备注或操作卡片时，刷新提示仍保留展开、滚动和焦点。卡片同列重排后原控件继续可用，按钮失去资格后焦点回到原列排序。多张未归列卡按更新时间排序。点击未归列的空会话打开空预览，关闭后可继续用键盘操作该标题。 |
| `test::ui_layout::light_ui_refresh_health_and_catalog_preserve_interaction` | 刷新 1500 张卡保留身份，用户归列时使用实际卡片和版本。 |
| `test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry` | 归档回执丢失时按钮保持禁用。指定项目与默认区分别通过列按钮和单卡按钮操作，真实归档请求的完整目标序列为 `a1`、`a-default`。 |
| `test::ui_layout::transfer_feedback_waits_for_receipts_and_preserves_interaction` | 沿用 A 的保存回执及命名场景，卡片临时落位与计数仍等待确认。 |

在隔离副本中给现有 `cwd` 加一个可见字段。添加字段前浏览器找不到它，只改 `assets/board-card.tsx` 一行后，两次独立快照中的目录都更新到屏幕。HTTP 和离线面板均通过。版本库没有附带演示字段。

打包 Linux 插件后，隔离 fixture 进程同时拥有 HTTP 服务和通过 stdio 启动的 MCP。从真正的 `resources/read` 取回 HTML，浏览器向同一服务提交 `board_preview_reply`。两个入口的四列计数均与 Rust 快照相同。未归列位置说明、真实点击、空回复以及关闭后的焦点都通过。B2 的真实托管 MCP 另测只归档有资格的目标主机。

第一次审阅指出的场景计数包装器现已改成读取实际归档目标和卡片保留结果。浏览器另复现同列重排失焦和两条未归列会话导致「不支持的排序字段，请刷新看板」。在业务提交 `7a2dce3`，两种故障均在 HTTP 与离线面板通过。独立 Linux fixture 与日用客户端的验收分别记录。E2b 负责编辑器，E2c 负责拖动与跨列保存。
