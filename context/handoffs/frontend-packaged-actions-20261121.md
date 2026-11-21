# 前端收尾 · E1 合入后的交接

主线 `48482a6d3b5d7a2e0b42837f93af355958c8d6c9` 已包含 E1 与 D。作者分支 `codex/frontend-packaged-actions` 保留隔离 fixture 修正与两项 T0 测试处置。T0 的受益人、逐类断言、保留与承接 ID 见[首件及销账记录](../results/frontend-packaged-actions-20261121.md)。

在 `67a6b69d36d7eaa8c69d5b98873b02c99548bf0e` 的干净工作树重新构建、打包和解包，5 项标准包场景通过。用包内启动器、两个全新原生 fixture 和两个独立浏览器复跑相同操作。HTTP 和 MCP 均完成卡片预览、保存非空备注、用两个空格清空备注、设置待归档列排序、筛选归档和重开同一已完成批次。MCP 端将 E1 验证器导出的 HTML 与本次活连接 `resources/read` 结果逐字核对，然后转发真实 `tools/call` 回执；`board_preview_reply` 的 `_meta` 也按原样返回。

源码外证据位于 `frontend-packaged-actions/first-67a6b69/`。`http-actions.json`、`mcp-actions.json` 记录 UI 操作和后端读回，`http-package.json`、`mcp-package.json` 记录包来源和进程归属。同目录上一级的 `first-package.py` 与 `first-browser.mjs` 是可重放的首件驱动。`scripts/check-pwa-page.mjs` 已纳入 Rust 自动测试，两项 T0 各有独立审阅。

## 继续实施

1. E2、E3、筛选和排序进入远端主线后，获取最新 `origin/main` 并重基。检查各作者已经提供的包入口、fixture 造数、故障注入和浏览器操作。优先把首件中尚缺的检查接到现役脚本，避免再维护一套宿主路由表。
2. 保持实际包的 HTTP 首页与 MCP `resources/read` 原文，两个入口各用一套相同初始数据。用有完整回复的卡补一次实际正文读回；用同组有资格和无资格卡核对筛选、排序持久化、批次成员和版本。复用 E3 的十卡中断点，重开后逐卡核对未提交、待核对及成功结果和原生写入次数。当前只实证了已完成批次的重开。
3. E1 的展示与交互在 `assets/reply-preview.vue`，预览操作组件在 `assets/reply-actions.tsx`，挂载在 `assets/reply-preview.ts`。当前首页有一个 `mountReplyPreview` 调用，Vue 监听在挂载时登记、卸载时解除。E2/E3 合入后沿实际导入链检查卡片与批次旧 DOM、事件绑定和重画入口；只删除已被替代部分。
4. 当前原生位置、保存与归档由 Rust 校验，共享 HTTP/MCP 操作登记在 `assets/board-operations.json`。最终报告对照新组件与这些维护入口，注明每条行为的实际所有者和唯一入口。

嵌入首件使用允许表单提交的隔离 Chromium 宿主。正式客户端授予的权限、原生手势，以及 PWA 实际系统安装和独立窗口启动仍需目标环境证据。主线由串行合入协调者维护。
