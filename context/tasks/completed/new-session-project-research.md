# 新建会话沿用当前项目调研

owner: codex
execution_mode: shared_readonly
allowed_write_set: context/tasks/active/new-session-project-research.md, context/tasks/completed/new-session-project-research.md, context/STATE.yaml, context/journal/2026-11-07-new-session-project-research.md
base_commit: 1978198aa4304e56237ac7b5c7c6c392e095ace0
worktree_path: /home/herman/.codex/worktrees/99a4/notion-program
coordinator: codex

用户只要求调研：看板选定项目后，新建会话是否能自动选择当前项目，
以及交互和实现中的边界情况。不实现功能，不部署，不改变客户端偏好。
两名只读代理分别检查前端筛选状态与后端项目目录模型；
协调者核对官方资料、本机客户端新建深链和整理建议。

## 结论

想法可行，已有新建入口可以扩展，不需要新建会话服务或直接写客户端偏好文件。
本次只完成调研，业务代码、部署、生产数据和客户端选择均未改变。

看板 index.html:386 发送空对象，src/app.rs:106 忽略请求内容并打开固定
codex://threads/new，因此当前项目选择完全没有进入新建链路。
HTTP 与 MCP 共用 App::request，但 board_new 的 MCP schema 当前禁止额外字段，
将来增加项目参数时必须同时修改 schema，不能只修改网页。

## 客户端证据

2026-11-07 只读检查 /opt/codex-desktop/resources/app.asar，
package.json 版本 26.1029.61003。证据仅代表这份本机安装包。
临时提取到 /tmp/session-board-new-project-research/，没有运行客户端跳转。

- .vite/build/early-bootstrap-339-305d03d30317c879.js 中，F 为深链分发，
  H 解析新建参数，G 解析可选正文与浏览器上下文。
- threads/new 分支只在存在 browserUrl、mode、originUrl、path、prompt 之一时调用 H。
  单独 projectId 不在触发列表，故仅给旧链接追加该参数会被忽略。
- codex://new 直接调用 H，支持只有 projectId 的项目定位。
- 使用提取的 F/H/G/L/U 代码运行三个纯解析探针，URL 目的地分类和项目 ID
  类型转换采用最小替身。未提供路径、模式、消息或浏览器上下文。
  codex://threads/new 与 codex://threads/new?projectId=probe-project
  都得到仅含 kind=newThread 的对象；
  codex://new?projectId=probe-project 得到含 projectId=probe-project 的对象。
  该探针证明分发差异，不证明桌面路由和界面端到端行为。
- .vite/build/early-bootstrap-45-628adb74a9c47df1.js 的 newThread 分支
  优先通过 projectId 查已有本地项目，并使用该项目 rootPaths 的原生选择逻辑；
  找不到项目时会尝试 path/originUrl，均无结果时仍可能导航到未指定项目的新建页。
- ePU 会更新 SELECTED_PROJECT，并向所有窗口发送 global-state-updated。
  因而这不是严格的单窗口临时选择，其他窗口的具体界面联动尚待实测。
- 路径分支 ePN 检查目录；ePU 在路径没有对应项目时可以 createProjectForRoot。
  因此不建议把任意会话 cwd 作为新建目标，更不能用它模拟默认工作区。

官方资料通过 Exa 搜索并实际读取：
https://developers.openai.com/codex/app/worktrees
https://developers.openai.com/codex/app/features
资料说明项目内仍需单独选择 Local/Worktree 和起始分支；
本轮未取得深链参数的公开稳定契约，参数判断以本机包为准。
初始 web 工具未返回可用正文，直接 HTTPS 读取失败，之后用 Exa 取得正文。

## 建议行为与边界

1. “当前项目”定义为点击按钮时，本看板页面明确选中的具体项目。
   每次请求携带该 ID，后端不保存跨标签页共享的当前项目。
2. 搜索模式跨项目搜索，但隐藏的 selectedProject 仍保留。
   不应暗中采用隐藏旧项目；建议显示创建目标，或者点击时先选项目。
   不根据搜索结果、首张卡片或会话数量推断目标。
3. 全部项目没有单一目标；默认工作区是未归属会话集合，也没有固定根目录。
   二者都不能冒充真实项目或用任一旧 cwd 代替。
   若要承诺“新建无项目聊天”，需单独验证客户端清除旧项目的入口。
4. 点击时捕获目标；后端重新读取完整登记表，核对 ID、读取错误及目录可用性。
   已删除、重新登记成新 ID、失效根目录或读取失败时明确报错。
   不按同名项目替代，也不静默退回上次项目。
   看板后台旧目录快照可以用于展示，不能直接作为新建的有效性证明。
5. 同名、嵌套、多根及外部工作树项目均以 ID 指定。
   优先交由原生项目流程处理根目录，不自行取 paths[0] 或最近会话 cwd。
   项目归属识别不等于选择具体工作树、分支或执行模式。
6. 多标签页选择各自独立。原生客户端却会更新全局项目，
   多窗口或近同时点击 A/B 时，目标窗口、草稿及最终选择需要真实验证。
7. 点击期间沿用重复点击保护，同时明确编辑、拖动和保存期间的交互。
   HTTP/PWA 与 MCP 容器应具有一致参数和错误语义。
8. 现有 open_client_url 仅确认启动命令，没有读回原生项目。
   成功提示应保持“已发送打开请求”，不能据此声称项目已选中。
   目录删除或项目移除仍可能发生在后端验证之后，不能承诺完全消除竞态。
9. 项目 ID 和路径应使用标准 URL 编码；参数只由后端登记表解析，
   继续沿用固定协议目标和参数数组启动，避免任意 URL/路径注入。
10. 不发送正文、不启动模型，不擅自创建项目或工作树。
    原生模式、分支、模型及权限仍需在新建页确认，不能从筛选结果推断。

## 后续验收建议

覆盖 A/B 连续切换、零会话项目、同名、多根、工作树、中文和特殊字符；
搜索及全部/默认模式；首次加载失败、登记表损坏、删除和重新添加；
点击后切换项目、两个标签页与多个客户端窗口、已有输入草稿；
分别在实际 HTTP/PWA 和 MCP 容器中核对最终项目，
并确认没有发送消息或额外登记项目。
客户端升级后应重新确认深链参数，不能只用 /bin/true 夹具签收。

本任务未实现功能，不运行新功能验收，也不改变既有 release_ready=false。
收尾执行 context 验证与 ./scripts/check；结果见对应 journal。
