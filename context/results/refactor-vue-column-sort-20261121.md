# 栏目排序菜单迁移

## 首件与方案取舍

从 `735b368` 的隔离检出启动 Chromium，分别打开 HTTP 页面和实际 MCP `resources/read` 返回的页面。MCP 夹具使用 `sandbox="allow-scripts"` 并禁止页面网络连接。两边均读到相同行为：

- 待归档默认旧的在前，其他列新的在前；相同时间按完整 ID 升序，无效时间排末。打开时的选项获得焦点；方向键循环，Home/End 到首末，Enter 选择，Escape 返回按钮，Tab 继续到下一控件。
- 宽屏、窄屏和横向滚动后的菜单左上角依次为 `(651,239)`、`(208,172)`、`(181,172)`。当前方向不写入；一次成功和随后一次拒绝分别提交版本 4 和 5。拒绝后保留已保存顺序。

在锁定的 Vue 3.5.46、esbuild 0.28.2 生产构建中实际比较了 Reka UI 2.11.0 的单选菜单和原生 popover。两种方案都能离线使用。下表含同一份预览组件及各自的最小菜单夹具。

| 方案 | 原始字节 | gzip 字节 |
| --- | ---: | ---: |
| 原生菜单夹具 | 128327 | 50409 |
| Reka 单选菜单夹具 | 203628 | 73889 |

Reka 可删除手写定位和部分菜单导航逻辑，但新增 18 个安装包条目，其中 9 个贡献了产物代码。实测默认 ↑ 不开启，↓ 开启后聚焦首项，末项 ↓ 不循环，Tab 留在菜单。它在右下角翻到按钮上方，也与现行定位不同。保留原生 popover 和原 CSS，用 Vue 管理触发按钮、选项、焦点、选中状态和打开时的版本；工程没有新增 npm 依赖。迁移删除了 `makeSortControl`、原手工选项更新和全局常驻定位监听。原基线整个 `index.html` 为 454836 字节，首次迁移产物为 453800 字节。

## 实现与回归

在 E2c 主线 `b615d4d` 上，`assets/board-columns.vue` 直接使用 `assets/board-column-sort.vue`。`assets/board-sorting.ts` 对每列当前可见卡片按时间及同时间 ID 排序。宿主负责写入 `/api/column-sort`、应用服务端回执和更新看板，拖动状态来自同一 `board.drag`。菜单经 Vue Teleport 渲染到当前页面的 body，列换位时原生 popover 和焦点保留。关闭状态下已获焦的按钮换位后恢复焦点。

新增 `test::column_sort::sort_menu_keyboard_focus_position_and_version_survive_refresh`，绑定 MANUAL-002 与 UI-003。它用真实浏览器覆盖键盘、焦点、点击外部、定位、空列默认方向、刷新后拒绝旧版本、同时间与无效时间、仅重置主动排序列的滚动位置。原有排序 Store、HTTP、MCP 测试继续验证独立版本和持久化。创建及保存等待期间，交互测试通过实际排序控件尝试操作。

首件资料位于 `~/.local/share/codex-session-board-investigations/vue-column-sort-baseline/run.XhHm32/`。菜单库对照、最终提交、独立审阅和离线包的 HTTP/MCP 证据交付在 `~/.local/share/codex-session-board-investigations/vue-column-sort/`。本轮验证使用 Linux 隔离浏览器和插件包。
