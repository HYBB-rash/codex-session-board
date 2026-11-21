# 第五轮 · 看板筛选

- owner: 筛选作者 `01a1fc68-930a-7cc1-b249-e1c7041b5535`
- execution_mode: isolated_write
- allowed_write_set: 筛选组件、Vue 离线入口、index.html 的筛选与生成区、涉及筛选的浏览器用例及绑定、本卡与对应结果
- base_commit: 735b368cb08aa2ce3c3c6db8f6f643316b362f4c
- worktree_path: /home/herman/.codex/worktrees/round-five-board-filter/notion-program
- coordinator: 总协调 `01a1fc2d-af6f-7d50-9759-33835b51eaeb`，合入者 `01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d`

筛选组件拥有主机、项目、历史范围、搜索、控件锁定、数量及范围文字。看板按筛选结果分列，传给卡片、排序、列归档的仍是同一批完整会话对象。共用主机目录、项目命名及写操作继续由现有接口提供。

与 E2 共用列渲染输入，与 E3 共用列范围及归档按钮说明，与排序共用 Vue 入口和生成区。各自在独立组件注册，合入时重新构建内嵌资源。E3 拥有旧主机测试里的归档丢回执、重开及核对段。筛选首件使用两个可写的同名主机、同一线程 UUID 和不同版本，按可见卡片与两次实际请求核对。

已对照 E2a `ab04618`、E3 `161e983` 和排序 `7b97982`。E2a 的 `board.update` 接受已分列的会话，E3 延用 `archiveSessions` 批次入口，排序拥有列头。筛选接入沿用 E2/E3 的 `board-*` 编译规则；重新生成资源时保留各入口的 import。E2a 改动了归档按钮标题和未读判断的所在函数，重基时两处分别取 `filters.view.scope` 和 `filters.state.host`。
