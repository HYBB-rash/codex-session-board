# 排序路由字面断言裁决

原 ID `test::column_sort::column_sort_app_and_mcp_expose_separate_settings_operation` 保留。删除其中对压缩 HTML 字串 `"/api/column-sort":"board_sort"` 的匹配。应用将待归档列保存为降序、快照读回 `desc`、MCP 注册 `board_sort` 的断言均保留。测试 ID 和需求绑定无需变动。

| 承接证据 | 实际输入与可观察结果 |
| --- | --- |
| `test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme` | 隔离 Chromium 的沙箱 iframe 装载构建后的桥接。调用 `boardRequest("/api/column-sort", {id:"fixture", version:3, hostId:"host-fixture"})` 后，宿主须收到 `tools/call`，工具名为 `board_sort` 且参数逐项相同。页面须读回 `{ok:true, received:原参数}`。这是传输路由的自动承接。 |
| `test::column_sort::column_sort_real_http_and_stdio_mcp_share_persistent_settings` | 独立 stdio MCP 发送 `review / updated_at / asc / version=0` 给 `board_sort`。首次回执版本为 1，原版本重放返回 `isError=true`。随后 HTTP 快照和重开的数据库均读到 `asc`。 |
| 排序作者头 `ed44b9e50ebfcc6b9a337cca502c523fdbed6845` 的包内浏览器实测 | 通过 `resources/read` 导出的真实离线菜单在模拟宿主中发出两次 `board_sort`。待审阅列提交 `asc, version=4` 成功，顺序为 `a,b,z,invalid`。随后提交 `desc, version=5` 被夹具拒绝，页面显示“排序版本冲突”并保持升序。 |

只在源码外副本给映射的冒号加空格，旧字串消失，桥接浏览器的 57 项检查仍全部通过。另一份副本仅把该路由目标改为 `fixture_wrong_sort_tool`，同一浏览器测试在宿主收到的工具名上失败，预期 `board_sort`。外部消费的是工具调用及参数，构建产物的序列化空白没有格式要求。
