# E1 回复预览 Markdown 断言裁决

**受益人**　在网页或 Codex 看板中点开卡片、阅读最终回复的用户。`b615d4d` 的原测试要求禁用链接后出现 `</a>`。改用 `<span>` 后读者看到相同文字，链接仍不可点击，原测试却失败。`5cc5630` 交付的浏览器预览使用预制 HTML；`07d7b95` 修复首击定位；随后 `abdb5fa` 的包入口交接仍将完整回复正文列为待验。这三处均未覆盖 Rust 渲染器的这个反例。

| T0 处置 | 完整 ID | 需求 |
| --- | --- | --- |
| 移除逐字比较的单元测试 | `lib::codex_session_board::app::preview::tests::preview_markdown_formats_text_without_loading_untrusted_content` | `NAV-001.common` 转交下一行 |
| 由现役真实入口承接 | `test::native_authority::entrypoints::preview_tool_keeps_reply_out_of_model_content` | `NAV-001.common`、`MCP-001` |

**依据。** `NAV-001` 要求用户点击卡片后能阅读完整回复。原设计另行约定了 Markdown 格式、原始 HTML 可读、图片只显示替代文字，以及仅无凭据的 HTTP(S) 链接可点击。MCP 正文通过 `_meta.previewReply` 返回应用。`NAV-001` 本身没有指定 HTML 标签或 MCP 封装。[冻结需求](../../acceptance/requirements.json) · [原设计与格式样例](../handoffs/card-preview-redesign-20261118.md) · [MCP 正文约定](../../Docs/LIGHT-UI.md)

**实跑。** 隔离原生后台提供一条带表格、代码、两种可点击链接和三个受限目标的 Markdown 回复。测试读回真实 HTTP 回执，再核对真实 stdio `board_preview_reply` 的 `content` 为空、`_meta.previewReply` 与 HTTP 回执一致。在浏览器打开网页与 stdio `resources/read` 的 MCP 面板，分别点同一张卡，逐项核对可见文字、格式、可导航链接、脚本和图片节点。图片地址指向独占回环端口，两个入口都关闭后该端口收到零连接。[原生入口](../../tests/native_authority/entrypoints.rs) · [两入口共同使用的 DOM 断言](../../scripts/check-card-panel.mjs)

| 隔离副本中的改动 | 实际结果 |
| --- | --- |
| 原版渲染器 | 原测试通过；承接测试在两个入口均通过 |
| 禁用链接从无 `href` 的 `<a>` 改为 `<span>` | 原测试仅因缺少 `禁用</a> 图示` 失败；承接测试在两个入口均通过 |
| 使普通 HTTP 链接失效，保留 HTTPS | 承接测试报告可点击链接列表缺少「索引」及其 URL |
| 丢弃原始 HTML 文字 | 承接测试报告正文缺少字面量 `<script>sample</script>` |
