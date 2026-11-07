# Codex 会话看板

本仓库的当前功能是 Codex 会话状态管理。Rust 后端自动发现本机未归档交互会话，
日常模式以 Codex 原生分区读取和更新列状态，本地保留备注和人工确认。
真实执行阶段直接更新单个会话的原生位置，标题同步可单独配置。
界面沿用经过原型验证的 HTML 与随包 MCP App SDK。

状态按待判断、进行中、待审阅、已阻塞、待归档排列。
开始与恢复执行进入进行中；等待批准、交互输入、失败、中断及系统异常进入已阻塞；
正常轮次结束进入待审阅。人工选择保持到新的真实执行阶段。
待归档由人工确认，旧已完成记录也显示在待归档。点击卡片上的“归档”按钮直接
归档原 Codex 会话，成功后移出看板。列头“一键归档”处理当前项目或搜索筛选下本列的全部本机会话，
逐项复用版本、新输入及原生位置校验，失败项保留并显示原因，远程会话保留。
卡片不再提供“调整状态”按钮，通过拖动修改状态；拖入已阻塞时仍需填写原因。
状态调整本身不执行归档，不发送消息或启动任务。
可显式配置每 30 分钟自动归档本机待归档会话，见 [定时归档](Docs/AUTO-ARCHIVING.md)。
待归档列按卡片显示的会话更新时间正序排列，最早的在顶部；其他列保持最近更新在前。

```bash
cargo build --locked --offline
cargo run --locked --offline -- --help
cargo run --locked --offline -- launch
```

首次启动使用 XDG 配置和数据目录。配置示例见 `config.example.json`，
自定义部署使用 `--config /path/to/config.json`。
`serve` 前台运行，`launch` 复用或启动匹配服务，`stop` 核验身份并等待实际退出。
`mcp` 提供 stdio MCP App，`scan` 执行一次发现，`migrate --source-config PATH`
使用 SQLite 备份导入旧原型，保留原库并拒绝覆盖目标。

构建插件运行 `bash scripts/build-plugin`，插件清单指向生成的 Rust 二进制。
迁移说明见 [Docs/MIGRATION.md](Docs/MIGRATION.md)，
验收规范见 [acceptance/README.md](acceptance/README.md)。

```bash
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --all-targets --locked --offline
python3 scripts/verify_acceptance.py --scope automated --report /tmp/session-board-automatic.json
python3 scripts/verify_acceptance.py --evidence /path/to/evidence.json --report /tmp/session-board-release.json
./scripts/check
```

验收使用指定会话交付的 acceptance-v1，31 条需求与独立模型保持原始 SHA256。
业务测试调用 Rust 实现；Python 仅用于独立验收工具和参考模型。
自动检查通过不能替代原生 Codex 界面实测。完整验收退出 0 且
`release_ready=true` 才达到开发完成标准。

原 Notion 调度与知识归档源码保留在 Git 历史 `8f86014`，不再属于当前程序。
原运行配置与数据仍保留在工作目录的忽略路径中。
重构不安装或重启旧的 systemd 调度服务。

## 项目筛选与搜索

顶部默认显示项目下拉框，包含全部项目、默认工作区与本机 Codex 当前加载的项目。
旁边的按钮切换到标题、路径搜索；搜索覆盖全部会话，两种模式分别保留当前输入。
没有关联项目的会话归默认工作区。移除项目后会话自动归默认，重新添加相同目录后
自动识别，状态和备注保持原样。同名项目显示目录以便区分，空项目也保留入口。
多根目录、嵌套项目、符号链接与外部 Git 工作树的规则见
[Docs/PROJECT-FILTERS.md](Docs/PROJECT-FILTERS.md)。

## 安装为应用

使用 Chrome 或 Edge 打开本机 `launch` 返回的网页地址。
浏览器允许安装时，工具栏会显示“安装应用”；也可使用浏览器菜单中的安装入口。
安装后从系统应用列表打开“Codex 会话看板”，会使用独立窗口。
浏览器取消安装后仍可从菜单重新安装，原生 Codex 内嵌看板不提供此按钮。

应用仍需这台电脑的看板服务运行，安装不会创建后台服务。
服务不可用时显示重新连接页面，不离线保存会话、备注或待提交操作。
服务恢复后点击“重新连接”，重新读取当前页面令牌和数据。
回环地址可用于本机安装；其他设备不能通过本机回环地址访问。
应用身份绑定地址和端口，安装后应保持配置中的 `port` 不变。

## 新建会话

状态导航右侧的“＋ 新建会话”按钮打开 Codex 空白会话输入页，
由用户选择项目并输入消息。按钮复用配置中的 `open_command`，
不自动发送消息或启动模型；输入页尚未发送消息时不会生成看板卡片。
原生看板与备用网页均提供此入口。

## 状态分区

日常模式开启 `native_sections`，配置 `sidebar_socket`、调用方已有会话
`sidebar_caller`、客户端全局状态文件 `sidebar_state_file` 和五状态到分区 UUID
的 `status_sections`。全局状态文件仅用于读取已加载会话的内部别名，不改写它。
部署机器的 socket、调用方和分区 ID 均保存在外部配置，不写入源码。
旧 completed 分区仅投影到看板待归档列，不批量迁移或删除原生分区。
项目本身保留在项目列表；固定等内置入口由客户端维护。

后台按 `runtime_poll_seconds` 只读刷新原生位置，不将旧本地状态纠正回去。
人工保存先移动并独立读回，再提交备注和确认；真正的新执行阶段才发起自动移动。
同阶段重复观察、历史补读和重启不重放旧位置。不同会话的操作按 ID 隔离。
分区缺失、改名或读取异常保留最后确认快照，不据此猜测新的位置。
`sync_sections` 已停用，旧配置须改为 `false`；未开启 `native_sections`
时保留冻结契约的本地兼容模式，不会移动原生分区。
接口选择、并发边界及验收要求见 [Docs/NATIVE-STATE.md](Docs/NATIVE-STATE.md)。
五状态与真实归档扩展见 [Docs/ARCHIVING.md](Docs/ARCHIVING.md)。
