# 第五轮筛选验证记录

筛选由 `assets/board-filters.tsx` 管理。主机、项目、历史和搜索共同决定可见会话，分列与批量操作沿用这些会话的完整标识及版本。原控件、筛选事件和菜单重建逻辑已删除。

## T0 测试归属

| 现役 Rust 测试 | 筛选部分的行为证据 |
| --- | --- |
| `test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry` | 保留 48 项布局与 60 项原主机检查。新增从待归档列实际点击的双主机首件，比较页面卡片、批次清单、逐项请求，无需新增测试 ID。 |
| `test::ui_layout::host_catalog_filters_preserve_cached_rows_and_isolated_native_drafts` | 通过控件选择真实项目，再移除项目，检查目录不完整时保留选择、完整时回到全部。保留历史范围和缓存行为。 |
| `test::ui_layout::transfer_feedback_waits_for_receipts_and_preserves_interaction` | 移动时仍能按主机、项目和搜索切换。A 的回执到达时，B 的同 UUID 卡片仍在待归档列。 |

E3 继续认领丢回执、重开和手动核对段。筛选控件不发起归档、不重建版本。证据使用隔离浏览器和虚构目录。

## 已验证

- 两台可写主机显示为“工作主机”，各有同一线程 UUID 和同名项目。测试分别选中 A、B，并排除 A 上另一项目的可归档记录。网页卡片、批次登记和唯一逐项请求的复合 ID、`version`、`inputVersion`、`sectionVersion` 均匹配。将登记中的输入版本改为 0 时，首件在清单比较处失败。
- Linux 包 `278fcd884f65058c5dd7738af9cbd6b496f8c2c3` 隔离启动与清理通过。对包内 `index.html` 和该包二进制 `resources/read` 返回的原文分别执行同一首件，两端通过。证据在工作树旁的 `evidence/278fcd884f65058c5dd7738af9cbd6b496f8c2c3/`，含 `filters.json`、`startup.json` 及实际资源。
- 初审发现刷新时为未变目录重复计算项目标签。修复后独立复审通过，1,000 个项目的九次重绘中位数从 14.9 ms 降至 0.7 ms。改名后下一次刷新可见新标签。
- 已接入 A 主线 `911cfdf`。在 `f5db7d2ab6964d8840a02f10578de003a5a071bd`，用同一 Chromium 对照该主线和候选。1440、390 CSS 像素宽度下，项目与搜索两种模式共四组筛选栏和范围文字截图像素一致。对照图位于该提交证据目录的 `appearance` 子目录。

## 合入条件

F 在原包 `0b58e374ed9e39c3b18cbb79a37be6283e0fd393` 上三次观察到 HTTP 完成后，MCP 首次选择主机只显示零张卡。原始日志位于预演 `rehearsal-gxaMl3` 的 `filter-targeted-1.log` 至 `filter-targeted-3.log`。正式脚本已将 MCP 的快照、批次和请求记录放入独立测试服务，进场核对空批次。使用同一包和同一份实际 MCP 资源连续重跑三次，61 项原检查和双入口归档均通过。两个入口的输出分别保存页面卡片、登记清单和逐项提交。

分支 `codex/round-five-board-filter`。E2 的卡片和 E3 的归档组件进入主线后，由筛选作者重基，重新生成共用 Vue 产物，并对最终头复审和验证。合入者为 `01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d`。
