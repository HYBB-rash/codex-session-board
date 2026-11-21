# 回复预览的 Vue 入口

## 修改与构建

回复阅读状态和事件集中在 `assets/reply-preview.vue`。
模板组织布局，`assets/reply-actions.tsx` 提供刷新和进入原会话按钮。
`assets/reply-preview.ts` 定义看板与预览之间的类型边界并挂载组件。
看板只传递读取、跳转、链接、快照与拖动保护，不持有正文或请求选择状态。
卡片和编辑框继续沿用原实现，后续迁移单独安排。

```sh
node scripts/build-app-bridge.mjs
node scripts/build-app-bridge.mjs --check --offline
```

复用现有临时 npm 安装、锁定依赖和 esbuild 构建。
`vue-tsc` 对 `.vue`、`.tsx` 和 `.ts` 做严格类型检查，esbuild 使用 Vue 的 JSX runtime。
SFC 在构建时编译，运行时不下载依赖或编译模板。
首次构建省略 `--offline` 以准备 npm 缓存。
之后完整自动验收中的预览测试执行离线重建与逐字节比对。
发布验证先执行 `--check`，在相同冻结源码上准备依赖缓存，随后仍运行完整门禁。

预览 IIFE 写入 `index.html` 的 `reply-preview-code` 生成区。
HTTP 与 MCP 原有入口都读取这一份 HTML，不新增另一份页面装配规则。
不要手改生成区。预览源文件平放在 `assets/`，沿用源码包和实现指纹的覆盖。
锁文件每包一行，完整 Vue MIT 许可证和生成的来源声明随包提供。

当前锁定 Vue 3.5.46、vue-tsc 3.3.14 与 TypeScript 6.0.3。
TypeScript 7.0.2 虽在 vue-tsc 的宽泛 peer 范围内，实际运行报 `typescript/lib/tsc` 不导出。
保留此实编译结论，不因版本较新就更换组合。

## 验证范围

`test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies`
先检查类型与离线产物，再把 Rust `mcp::resource()` 返回的 HTML 交给浏览器回归。
两轮分别验证独立网页与隔离 sandbox iframe。
长回复遇到新回复时保持旧正文和 scrollTop，点击按钮后才切换。
切换卡片和关闭后忽略迟到结果，关闭保持看板几何与滚动，原卡可见时恢复焦点。
卡片已移除时焦点回到主机筛选器。
这些断言不要求旧手工 DOM 实现继续存在。

实际插件包启动验证额外保存同名 `.resource.html`。
它来自包内程序的 stdio `resources/read`，可通过下面的第三个参数重放：

```sh
node scripts/check-reply-preview.mjs /tmp/new-preview-evidence /tmp/startup.resource.html
```

证据目录须先创建。可选第四个参数覆盖独立网页 HTML，供等价改写和故障对照实验使用。
此类回归验证实际产物和桥接，不能代签真实 Codex 客户端容器验收。
日用插件的安装、重载和现场验收继续独立安排。

## 官方资料

- https://vuejs.org/guide/typescript/overview.html
- https://vuejs.org/guide/extras/render-function.html#jsx-tsx
- https://github.com/vuejs/core/tree/main/packages/compiler-sfc
- https://github.com/vuejs/core/blob/main/packages/vue/jsx-runtime/index.mjs
- https://esbuild.github.io/plugins/#on-load
