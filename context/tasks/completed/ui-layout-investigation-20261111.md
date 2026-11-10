# 核查项目筛选后的看板布局差异

owner: codex-ui-investigation
execution_mode: coordinated_read
allowed_write_set: context/tasks/active/ui-layout-investigation-20261111.md, context/tasks/completed/ui-layout-investigation-20261111.md, context/journal/2026-11-11-ui-layout-investigation.md, context/STATE.yaml, Cargo.toml, Cargo.lock, outputs/ui-layout-investigation-20261111/
base_commit: 280b20b874e9491f638dfdf59285091a26ef7930
worktree_path: /home/herman/文档/notion-program
coordinator: codex-ui-investigation

用户提供两张截图，要求调查进入不同项目后看板界面明显不一致的原因。
第一张为全部项目，153 条会话，列宽较大并出现水平滚动。
第二张为单个项目，2 条会话，五列同时展示。
不预设版本差异或项目模板差异，分别核查部署来源、筛选实现与实际浏览器布局。
两个子代理只读分析布局代码和部署来源，主代理负责隔离浏览器复现及整合结论。
不修改业务源码、生产配置、用户会话和正式看板。
工作区启动时已有其他任务暂存内容和正在运行的提交，保留并等待其完成。

## 结论

两张截图的差异是同一页面中由内容触发的布局回归。
第一张选全部项目，第二张选 Notion任务看板管理。
筛选移除了长标题，内容驱动的最小宽度随之改变，五列一起变窄。
单个项目只要含足够长的标题也会发生，不由会话数量直接决定。

`index.html:12` 给全部按钮设置 `white-space:nowrap`。
标题在第 239 行创建为按钮，第 62、63 行未覆盖禁止换行。
两行截断因此不能按预期换行，长标题仍参与固有宽度计算。
第 39 行 `.board{min-width:min-content}` 要求整个看板容纳内容的固有最小宽度。
第 40 行各列 `flex:1 0 240px` 分配扩大后的空间，空列也随之变宽。
第 541 行仅筛选数据，第 586 行始终使用同一状态集合和列模板。

回归引入点是 `bd02af3edfdc4817c05c9e1700aff4aebb5ca961`，
2026-11-10 19:05:49 +08:00，按 ai-todos 改版。
该提交同时引入全局按钮 nowrap，并将 Grid 的列数最小宽度改为 Flex 的 min-content。
既有资源、排序和验收测试没有断言长标题下筛选往返的实际列宽。
此前桌面、窄屏预览记录也没有这组对照。

## 现场与隔离实验

正式 HTTP 18767 与全部五个存活 MCP 进程均使用 v0.2.73，同一二进制 SHA256：
`0b0a7137453b66de1c49940d82236ef9a3d8fdfaaec8b11db22718e50044a570`。
两个入口都内嵌相同 index.html，正式 HTTP 去掉令牌与 PWA 注入后，
与 HEAD、工作区、部署包和插件缓存逐字节一致。
当前代码来源核对没有发现错版，不倒推截图时的宿主缓存状态。

2026-11-11，Chrome 156.0.8078.47，隔离服务器使用正式 API 的只读快照，
窗口 1474px，看板可视区 1434px，未连接生产写入接口：

| 条件 | 每列宽度 | 横向内容宽度 |
| --- | --- | --- |
| 全部项目，154 条 | 558.95px | 2843px |
| 单项目，3 条 | 277.2px | 1434px |
| 切回全部项目 | 558.95px | 2843px |
| 全部 154 条保留，仅把标题换成短文本 | 277.2px | 1434px |
| 只保留一张最长标题卡片 | 427.6px | 2186px |
| 全部项目，仅临时覆盖 `.board{min-width:0}` | 277.2px | 1434px |
| 全部项目，仅临时覆盖 `.card-title{white-space:normal}` | 277.2px | 1434px |

同时应用两条临时样式，在 600px 窗口下保持每列 270px、横向内容 1398px。
这些只是隔离浏览器里的诊断实验，没有改动业务源码或正式页面。
实测数量比截图多一条，包含本次调查新出现的会话，不改变布局结论。

证据保存在 `outputs/ui-layout-investigation-20261111/`：
`measurements.json`、四张截图、原始 HTML、私有数据快照及 `reproduce.mjs`。
复现脚本默认只读正式 API，也可传入 HTML 和快照路径完全离线运行，
随后仅在隔离服务器和浏览器中实验，拒绝写请求。
修正应取消内容驱动的看板最小宽度，显式允许标题换行，
并验证长标题、长项目标签、筛选往返和窄屏横滚。
本任务只调查和验证原因，未实施修复，不宣称发布验收通过或 release_ready。
