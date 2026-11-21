# T0 · E1 嵌入资源与 D 桥接的两项销账

| T0 原 ID | 处置 | 需求与承接测试 |
| --- | --- | --- |
| `test::board_io::native_resource_and_tools_expose_global_app_only_actions` | 并入已有 stdio 握手与完整界面回放，删除对压缩路由、JavaScript 全局变量、按钮 ID 及图片属性排列的源码匹配。 | `APP-001`、`MCP-001` 原本均已绑定 `test::board_io::real_stdio_mcp_handshake_and_resource_round_trip`。本次把两项需求也绑定到 `test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies`。 |
| `test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme` | 保留 ID。合并重复的快照成功调用，删除对 57 次内部检查的断言。读取浏览器实际主题，逐项比对宿主收到的工具调用与 D 的既有登记。 | `MCP-001`。同路径不同方法、参数、未知请求的零调用、预览专有回执、常规结果和错误都由原 ID 继续核对。 |

## 出处

- `APP-001` 要求原生全局入口打开看板。`MCP-001` 要求握手、全局入口、自包含资源、随包 SDK 和受保护的桥接。[冻结需求](../../acceptance/requirements.json)
- E1 的 Mac 作者提交 `15973aeb717c2568b978a3a29810ba3964728aa8` 与 Linux 合入点 `48482a6d3b5d7a2e0b42837f93af355958c8d6c9` 均有实际插件启动与 `resources/read` 记录。E1 使用 stdio 资源回放同一预览场景。原设计还要求初始宿主主题、后续通知及安全边距。[E1 交付](refactor-e1-vue-preview-20261121.md) · [主题约定](../../Docs/LIGHT-UI.md)
- D 合入点 `5e2eaa046b9e9ff5731bc6510556c93cebb5d249` 统一了方法、路径与工具名。D 的历史独审用“只改登记、保留旧离线包”证明需要比对实际调用。完整比对当时刚好是第 57 个断言。[D 交付](../tasks/active/refactor-d-shared-operations-20261121.md)

## 可观察的故障

**入口和工具。** 握手测试从实际二进制接收 `tools/list`、`resources/read` 和私有端点的拒绝回执。`board_view` 是唯一同名入口，对模型和应用可见并链接到全局资源。其他工具仅应用可见。`board_new` 的写操作标志、可选项目及 UUID 参数由工具描述验证。实际可选项目、有项目及过期项目由 `test::board_io::mcp_new_session_forwards_optional_project_to_http` 继续验证。

**离线资源和新建。** 握手检查 MIME、资源 URI、空网络域声明及 HTTP 模板标记清除。完整资源回放在临时浏览器里限制资源子页的网络加载，检查实际加载违规、嵌套页面、PWA manifest 和内嵌图片解码。选择本机后，点击可见的新建按钮，隔离宿主必须收到一次参数为 `{}` 的 `board_new`。D 的简化 iframe 独立验证带项目和操作 UUID 的新建分派。发布包重演复用既有 `verify_package_startup.py` 输出的 `.resource.html`。[包内回放](../../Docs/REPLY-PREVIEW.md)

**宿主和分派。** 初始深色、切浅色、再切深色，以及只更新安全边距时保留深色，都从隔离 iframe 的计算样式读回。桥接逐项调用既有 `assets/board-operations.json`，从宿主实际收到的消息检出漏调用、重复或错工具。`test::board_io::archive_batch_tools_share_http_history` 另以真实 HTTP 和 stdio 回读归档，`test::native_authority::entrypoints::preview_tool_keeps_reply_out_of_model_content` 回读预览工具的实际专用回执。

按原 ID、承接 ID、提交、包启动与浏览器结果归档的交接包存放在本轮共享输出目录。测试使用隔离宿主与浏览器。日用客户端和完整发布验收另有取证范围。
