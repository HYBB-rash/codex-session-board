# Codex 会话看板

本仓库的当前功能是 Codex 会话状态管理。Rust 后端自动发现本机及已连接远程主机的未归档会话，
日常模式以 Codex 原生分区读取和更新列状态，本地保留备注和人工确认。
真实执行阶段直接更新单个会话的原生位置，标题同步可单独配置。
界面沿用经过原型验证的 HTML 与随包 MCP App SDK。

默认按待判断、进行中、待审阅、已阻塞、待归档显示五列。
开始与恢复执行进入进行中；等待批准、交互输入、失败、中断及系统异常进入已阻塞；
正常轮次结束进入待审阅。人工选择保持到新的真实执行阶段。
待归档由人工确认，旧已完成记录也显示在待归档。点击卡片上的“归档”按钮直接
归档原 Codex 会话，成功后移出活动列表。列头“一键归档”处理当前项目或搜索筛选下本列可操作的会话，
逐项复用版本、新输入及原生位置校验，失败项保留并显示原因，只读项保留。
可编辑的卡片支持拖动换列，也可点击“调整”修改状态或备注。未归列卡片的按钮显示“归列”。
拖入“已阻塞”列或在该列内拖放时，会弹出编辑框。原因可以留空。点击“保存状态”才会生效，取消会保留原状态和备注。
状态调整本身不执行归档，不发送消息或启动任务。
本机与远程定时归档分别显式配置，旧本机设置不会自动扩展到远程，见 [定时归档](Docs/AUTO-ARCHIVING.md)。
待归档列按卡片显示的会话更新时间正序排列，最早的在顶部；其他列保持最近更新在前。

卡片的“预览答复”在右侧显示本轮最终正文。长答复在面板内滚动，关闭后回到原卡片。
看板观察到最新轮次正常结束且有明确最终答复时提供入口，支持本机与已连接的远程主机。
点击时重新核对轮次和运行状态。运行中、连接不明或缺少最终标记时，请点卡片查看原会话。

```bash
cargo build --locked --offline
cargo run --locked --offline -- --help
cargo run --locked --offline -- serve
```

首次启动使用 XDG 配置和数据目录。配置示例见 `config.example.json`，
自定义部署使用 `--config /path/to/config.json`。
普通桌面安装由 `mcp` 插件进程持有看板运行时，退出时回收 HTTP 和工作线程。
配置仅在统一入口解析一次，安装和更新不再另外选择配置，见 [插件运行时](Docs/PLUGIN-RUNTIME.md)。
`serve` 是显式独立部署入口，在当前进程监听；`stop` 核验身份并等待实际退出。
`mcp` 提供 stdio MCP App，`scan` 执行一次发现，`migrate --source-config PATH`
使用 SQLite 备份导入旧原型，保留原库并拒绝覆盖目标。

构建插件运行 `bash scripts/build-plugin`，插件清单指向生成的 Rust 二进制。
迁移说明见 [Docs/MIGRATION.md](Docs/MIGRATION.md)，
验收规范见 [acceptance/README.md](acceptance/README.md)。
Mac 开发与上下文迁移见 [Docs/MAC-DEVELOPMENT.md](Docs/MAC-DEVELOPMENT.md)，
双平台构建、GitHub 发布、更新和回退见 [Docs/DEPLOYMENT.md](Docs/DEPLOYMENT.md)。

```bash
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --all-targets --locked --offline
python3 scripts/verify_acceptance.py --scope automated --report /tmp/session-board-automatic.json
python3 scripts/verify_acceptance.py --evidence /path/to/evidence.json --report /tmp/session-board-release.json
./scripts/check
```

验收使用指定会话交付的 acceptance-v1，31 条需求与独立模型保持原始 SHA256。
业务测试调用 Rust 实现；Python 仅用于独立验收工具和参考模型。
浏览器布局回归还需 Node.js 22.12 或更新版本及 Chrome/Chromium。
可通过 `CODEX_BOARD_BROWSER` 指定浏览器可执行文件。
单独运行 `node scripts/check-ui-layout.mjs /tmp/session-board-layout`，
会使用合成数据验证五/六列、长内容、项目筛选、搜索和窄屏滚动，并保存测量与截图。
自动检查通过不能替代原生 Codex 界面实测。完整验收退出 0 且
`release_ready=true` 才达到开发完成标准；远程接入还需同一最终构建通过
`scripts/verify_remote_release.py` 的五状态与远程专项。连接和可靠性边界见
[远程会话](Docs/REMOTE-SESSIONS.md)。

原 Notion 调度与知识归档源码保留在 Git 历史 `8f86014`，不再属于当前程序。
原运行配置与数据仍保留在工作目录的忽略路径中。
重构不安装或重启旧的 systemd 调度服务。

## 项目筛选与搜索

顶部先选主机，打开看板时默认选中本机。选定一台主机后，项目下拉框列出“全部项目”、“默认工作区”和已加载的项目。
旁边的按钮切换到搜索。搜索在所选主机和会话范围内匹配标题、目录、主机名及项目名，覆盖全部项目及默认工作区。
选择“全部主机”时可跨主机搜索。切回项目筛选时保留选择，切换主机时重置为新主机的全部项目并保留搜索文字。
确认无项目归属的会话显示在默认工作区。移除项目后会话归默认，重新添加相同目录后自动识别，
状态和备注保持原样。同名项目显示目录以便区分，空项目也保留入口。
多根目录、嵌套项目、符号链接与外部 Git 工作树的规则见
[Docs/PROJECT-FILTERS.md](Docs/PROJECT-FILTERS.md)。

选择已连接的远程主机后，项目列表还会显示那台主机上的项目，包括空项目；
会话可以切换“活动／归档／全部／历史缓存”，无需在操作机重新登记目标项目。
主机项目、主机旧项目与操作机登记项目分别标注，不按相同目录合并身份。
归档、缓存和补充引用只供浏览；项目归属不明确时显示待确认，保留原有备注。
主机切换只刷新已连接目标，连接失败时显示上次快照和时间。完整目录每五分钟核对一次，
点击“刷新”可提前请求核对。已有会话按主机和会话 ID 打开，看板不提供移动或打开项目动作。
目录来源和分页协议见[目录首版设计](Docs/REMOTE-CATALOG-DESIGN-20261114.md)。

## 安装为应用

使用 Chrome 或 Edge 打开插件看板提供的本机网页地址。
浏览器允许安装时，工具栏会显示“安装应用”；也可使用浏览器菜单中的安装入口。
安装后从系统应用列表打开“Codex 会话看板”，会使用独立窗口。
浏览器取消安装后仍可从菜单重新安装，原生 Codex 内嵌看板不提供此按钮。

应用仍需这台电脑的插件运行，安装不会创建后台服务。
服务不可用时显示重新连接页面，不离线保存会话、备注或待提交操作。
服务恢复后点击“重新连接”，重新读取当前页面令牌和数据。
回环地址可用于本机安装；其他设备不能通过本机回环地址访问。
应用身份绑定地址和端口，安装后应保持配置中的 `port` 不变。

## 新建会话

原生看板和备用网页都可用状态导航右侧的“＋ 新建会话”打开 Codex 空白输入页，消息由用户输入并发送。
本机使用配置中的 `open_command`。选中具体项目时，看板先核对登记，再把项目带入新建页。
在本机的全部项目、默认工作区或搜索中打开时，看板交给 Codex 的链接省略项目参数。

远程新建要求目标项目有单个目录，主机有唯一的 SSH 别名。需要在 Codex 登记目录、开启自动连接
或导入远端旧项目时，看板先列出变更，确认后再继续。打开输入页后核对主机和目录，
由用户发送第一条消息，Codex 才创建会话。需要补登记时，在实际会话卡片选择“归入项目”并核对。
旧版创建请求只有记录了会话 ID 才能重试打开同一会话。详见[远程原生新建](Docs/REMOTE-NATIVE-NEW.md)。

## 状态分区

使用原生分区时，开启 `native_sections`，配置 `sidebar_socket`、调用方已有会话
`sidebar_caller`、客户端全局状态文件 `sidebar_state_file` 和状态到分区 UUID 的 `status_sections`。
看板从全局状态文件读取项目登记、会话别名和分区 ID 映射。
部署机器的 socket、调用方和分区 ID 均保存在外部配置，不写入源码。
默认绑定五个状态。按[四列部署说明](Docs/DEPLOYMENT.md#启用四列看板)启用 `four_columns` 后，
`status_sections` 只绑定进行中、待审阅、已阻塞和待归档，需归类的卡片显示在“未归列会话”。
旧记录中的 `completed` 状态值显示为待归档。加载新配置前，须通过
[分区配置迁移](Docs/NATIVE-STATE.md#日常状态与环境分区绑定)移除原生配置里的 `completed` 映射。
迁移只写新配置文件。项目列表和固定等内置入口仍由客户端维护。

后台按 `runtime_poll_seconds` 只读刷新原生位置，不将旧本地状态纠正回去。
人工保存先移动并独立读回，再提交备注和确认；真正的新执行阶段才发起自动移动。
同阶段重复观察、历史补读和重启不重放旧位置。不同会话的操作按 ID 隔离。
分区缺失或读取异常时保留最后确认快照。只改名称时，只要分区 ID 仍有效且读回一致，就继续使用该分区。
`sync_sections` 已停用，旧配置须改为 `false`；未开启 `native_sections`
时不会移动原生分区。冻结契约的本地六状态需要显式设置 `legacy_mode=true`，
该模式不启用远程连接、原生分区或真实归档，日常原生和远程路径需单独验收。
接口选择、并发边界及验收要求见 [Docs/NATIVE-STATE.md](Docs/NATIVE-STATE.md)。
状态与真实归档见 [Docs/ARCHIVING.md](Docs/ARCHIVING.md)。
