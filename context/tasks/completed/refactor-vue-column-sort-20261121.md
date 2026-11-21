# 第五轮 · 栏目排序

- owner: 排序作者 `01a1fc65-7a91-7440-a579-bc28c74a235c`
- execution_mode: isolated_write
- allowed_write_set: 栏头排序组件及其 Vue 构建入口、`index.html` 的排序与生成区、排序交互测试及对应绑定、本卡和同名结果记录
- base_commit: `b615d4d09851402378aa024c630fe2e371635b25`
- worktree_path: `/home/herman/.codex/worktrees/vue-column-sort/notion-program`
- coordinator: 五轮重构协调会话 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`，主线合入者 `01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d`

首件从独立基线页面读取每列默认顺序、相同和无效时间、键盘焦点、边缘定位及打开后刷新时的版本。以现行原生 popover 为对照，实际构建 Vue 菜单库候选并比较离线成本。认领排序入口和排序行为测试，筛选、卡片和批量归档各由原作者负责。

原排序作者头 `ed44b9e` 的实测和对照资料继续保留。E2c 已在主线提供 Vue 列与拖动控制，排序由 `BoardColumns` 直接渲染。E3 与筛选按主线集成顺序复验。
